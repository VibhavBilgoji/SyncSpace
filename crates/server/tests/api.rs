//! Integration tests for the HTTP skeleton: `/healthz`, the error format, the body limit,
//! CORS, panic handling, log hygiene and shutdown. Each test starts the real app on an
//! ephemeral port and talks to it over HTTP. Only tests marked "(mongo)" need a MongoDB.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::routing::{get, post};
use common::{
    ALLOWED_ORIGIN, AppOptions, LogCapture, MongoMode, TestApp, UNREACHABLE_URI, test_config,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use server::app::{self, AppState, MAX_BODY_BYTES};
use server::db;
use server::error::ApiError;
use server::extract::ApiJson;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Serialize, Deserialize)]
struct Echo {
    name: String,
    age: u32,
}

async fn panics() -> &'static str {
    panic!("boom with secret-token-123")
}

async fn internal_failure() -> Result<&'static str, ApiError> {
    Err(ApiError::internal(std::io::Error::other(
        "connect failed for user admin with password hunter2",
    )))
}

/// Test-only routes. They go through the same middleware as the real ones.
fn test_routes(routes: Router<AppState>) -> Router<AppState> {
    routes
        .route("/__test/ok", get(|| async { "fine" }))
        .route(
            "/__test/echo",
            post(|ApiJson(body): ApiJson<Echo>| async move { Json(body) }),
        )
        .route("/__test/panic", get(panics))
        .route("/__test/internal", get(internal_failure))
}

fn no_mongo() -> AppOptions {
    AppOptions {
        mongo: MongoMode::Unreachable,
        ..AppOptions::default()
    }
}

async fn body_json(response: reqwest::Response) -> (u16, Value) {
    let status = response.status().as_u16();
    (status, response.json().await.unwrap())
}

fn json_post(app: &TestApp, body: impl Into<reqwest::Body>) -> reqwest::RequestBuilder {
    app.client
        .post(app.url("/__test/echo"))
        .header("Content-Type", "application/json")
        .body(body)
}

fn assert_error(body: &Value, code: &str) {
    assert_eq!(body["error"]["code"], code, "{body}");
    assert!(body["error"]["message"].is_string(), "{body}");
}

// --- /healthz ------------------------------------------------------------------------------

/// (mongo)
#[tokio::test]
async fn healthz_reports_ok_when_mongo_is_reachable() {
    let app = TestApp::real().await;
    let response = app.client.get(app.url("/healthz")).send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert!(
        response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("application/json")
    );
    let (_, body) = body_json(response).await;
    assert_eq!(body, json!({"status": "ok", "mongo": "ok"}));
}

#[tokio::test]
async fn healthz_reports_degraded_when_mongo_is_unreachable() {
    let app = TestApp::without_mongo().await;
    let response = app.client.get(app.url("/healthz")).send().await.unwrap();
    assert_eq!(response.status(), 503);
    let text = response.text().await.unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap(),
        json!({"status": "degraded", "mongo": "error"})
    );
    // The answer must not say why: no address, port or driver message.
    for leak in ["127.0.0.1", ":1", "timeout", "Topology", "server selection"] {
        assert!(!text.contains(leak), "{leak} leaked: {text}");
    }
}

#[tokio::test]
async fn healthz_needs_no_session() {
    let app = TestApp::without_mongo().await;
    let response = app.client.get(app.url("/healthz")).send().await.unwrap();
    assert_ne!(response.status(), 401);
}

// --- the error format ----------------------------------------------------------------------

#[tokio::test]
async fn unknown_route_uses_the_error_format() {
    let app = TestApp::without_mongo().await;
    let response = app.client.get(app.url("/nope")).send().await.unwrap();
    assert!(
        response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("application/json")
    );
    let (status, body) = body_json(response).await;
    assert_eq!(status, 404);
    assert_eq!(
        body,
        json!({"error": {"code": "NOT_FOUND", "message": "not found"}})
    );
}

#[tokio::test]
async fn wrong_method_uses_the_error_format() {
    let app = TestApp::without_mongo().await;
    let response = app.client.post(app.url("/healthz")).send().await.unwrap();
    let (status, body) = body_json(response).await;
    assert_eq!(status, 405);
    assert_error(&body, "METHOD_NOT_ALLOWED");
}

#[tokio::test]
async fn malformed_json_is_400() {
    let app = TestApp::spawn(no_mongo(), test_routes).await;
    let (status, body) = body_json(json_post(&app, r#"{"name": "#).send().await.unwrap()).await;
    assert_eq!(status, 400);
    assert_error(&body, "BAD_REQUEST");
}

#[tokio::test]
async fn wrong_field_type_is_422_with_details() {
    let app = TestApp::spawn(no_mongo(), test_routes).await;
    let response = json_post(&app, r#"{"name": "Ada", "age": "old"}"#)
        .send()
        .await
        .unwrap();
    let (status, body) = body_json(response).await;
    assert_eq!(status, 422);
    assert_error(&body, "VALIDATION");
    let detail = body["error"]["details"]["body"].as_str().unwrap();
    assert!(
        detail.contains("age"),
        "the detail names the field: {detail}"
    );
}

#[tokio::test]
async fn missing_field_is_422() {
    let app = TestApp::spawn(no_mongo(), test_routes).await;
    let response = json_post(&app, r#"{"name": "Ada"}"#).send().await.unwrap();
    let (status, body) = body_json(response).await;
    assert_eq!(status, 422);
    assert_error(&body, "VALIDATION");
}

#[tokio::test]
async fn wrong_content_type_is_400() {
    let app = TestApp::spawn(no_mongo(), test_routes).await;
    let response = app
        .client
        .post(app.url("/__test/echo"))
        .header("Content-Type", "text/plain")
        .body(r#"{"name": "Ada", "age": 36}"#)
        .send()
        .await
        .unwrap();
    let (status, body) = body_json(response).await;
    assert_eq!(status, 400);
    assert_error(&body, "BAD_REQUEST");
}

#[tokio::test]
async fn valid_json_round_trips() {
    let app = TestApp::spawn(no_mongo(), test_routes).await;
    let response = json_post(&app, r#"{"name": "Ada", "age": 36}"#)
        .send()
        .await
        .unwrap();
    let (status, body) = body_json(response).await;
    assert_eq!(status, 200);
    assert_eq!(body, json!({"name": "Ada", "age": 36}));
}

// --- the body limit ------------------------------------------------------------------------

/// A JSON body of exactly `total` bytes.
fn echo_body_of(total: usize) -> String {
    let (prefix, suffix) = (r#"{"name":""#, r#"","age":1}"#);
    let filler = total - prefix.len() - suffix.len();
    format!("{prefix}{}{suffix}", "a".repeat(filler))
}

#[tokio::test]
async fn body_over_256_kib_is_413_in_the_error_format() {
    let app = TestApp::spawn(no_mongo(), test_routes).await;
    let body = echo_body_of(MAX_BODY_BYTES + 1);
    assert_eq!(body.len(), MAX_BODY_BYTES + 1);
    let (status, body) = body_json(json_post(&app, body).send().await.unwrap()).await;
    assert_eq!(status, 413);
    assert_error(&body, "PAYLOAD_TOO_LARGE");
}

#[tokio::test]
async fn body_of_exactly_256_kib_is_accepted() {
    let app = TestApp::spawn(no_mongo(), test_routes).await;
    let body = echo_body_of(MAX_BODY_BYTES);
    assert_eq!(body.len(), MAX_BODY_BYTES);
    let response = json_post(&app, body).send().await.unwrap();
    assert_eq!(response.status(), 200);
}

/// A chunked upload has no Content-Length to check up front, so the limit has to hold while
/// the body streams in.
#[tokio::test]
async fn chunked_body_over_the_limit_is_413_too() {
    let app = TestApp::spawn(no_mongo(), test_routes).await;
    let mut stream = TcpStream::connect(app.addr).await.unwrap();
    let head = "POST /__test/echo HTTP/1.1\r\nHost: test\r\nContent-Type: application/json\r\n\
                Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
    stream.write_all(head.as_bytes()).await.unwrap();
    let chunk = "a".repeat(16 * 1024);
    for _ in 0..(MAX_BODY_BYTES / chunk.len() + 2) {
        let framed = format!("{:x}\r\n{chunk}\r\n", chunk.len());
        // The server may answer and close before the last chunk is written.
        if stream.write_all(framed.as_bytes()).await.is_err() {
            break;
        }
    }
    let _ = stream.write_all(b"0\r\n\r\n").await;
    let mut response = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut response)).await;
    let text = String::from_utf8_lossy(&response);
    assert!(
        text.starts_with("HTTP/1.1 413"),
        "expected 413, got: {}",
        text.lines().next().unwrap_or("(nothing)")
    );
    assert!(text.contains("PAYLOAD_TOO_LARGE"), "{text}");
}

// --- panics and internal errors -------------------------------------------------------------

#[tokio::test]
async fn panicking_handler_is_500_in_the_error_format_and_server_keeps_serving() {
    let logs = LogCapture::start();
    let app = TestApp::spawn(no_mongo(), test_routes).await;

    let response = app
        .client
        .get(app.url("/__test/panic"))
        .send()
        .await
        .unwrap();
    let text = response.text().await.unwrap();
    let body: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(body["error"]["code"], "INTERNAL");
    assert_eq!(body["error"]["message"], "internal server error");
    assert!(
        !text.contains("boom") && !text.contains("secret-token"),
        "{text}"
    );

    // The operator sees what happened.
    assert!(logs.text().contains("boom with secret-token-123"));

    // And the server is still up.
    let again = app.client.get(app.url("/__test/ok")).send().await.unwrap();
    assert_eq!(again.status(), 200);
    assert_eq!(again.text().await.unwrap(), "fine");
}

#[tokio::test]
async fn internal_error_cause_is_logged_but_never_returned() {
    let logs = LogCapture::start();
    let app = TestApp::spawn(no_mongo(), test_routes).await;

    let response = app
        .client
        .get(app.url("/__test/internal"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 500);
    let text = response.text().await.unwrap();
    assert!(
        !text.contains("hunter2") && !text.contains("admin"),
        "{text}"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap(),
        json!({"error": {"code": "INTERNAL", "message": "internal server error"}})
    );
    assert!(logs.text().contains("password hunter2"));
}

// --- CORS ----------------------------------------------------------------------------------

fn header<'a>(response: &'a reqwest::Response, name: &str) -> Option<&'a str> {
    response.headers().get(name).map(|v| v.to_str().unwrap())
}

#[tokio::test]
async fn allowed_origin_gets_headers_with_credentials() {
    let app = TestApp::spawn(no_mongo(), test_routes).await;
    let response = app
        .client
        .get(app.url("/__test/ok"))
        .header("Origin", ALLOWED_ORIGIN)
        .send()
        .await
        .unwrap();
    assert_eq!(
        header(&response, "access-control-allow-origin"),
        Some(ALLOWED_ORIGIN)
    );
    assert_eq!(
        header(&response, "access-control-allow-credentials"),
        Some("true")
    );
    assert!(
        header(&response, "vary")
            .unwrap()
            .to_ascii_lowercase()
            .contains("origin")
    );
}

#[tokio::test]
async fn other_origin_gets_no_cors_headers() {
    let app = TestApp::spawn(no_mongo(), test_routes).await;
    for origin in [
        "https://evil.example",
        "http://app.example",
        "https://app.example.evil.example",
        "null",
    ] {
        let response = app
            .client
            .get(app.url("/__test/ok"))
            .header("Origin", origin)
            .send()
            .await
            .unwrap();
        assert_eq!(
            header(&response, "access-control-allow-origin"),
            None,
            "origin {origin} must not be allowed"
        );
        assert_eq!(
            header(&response, "access-control-allow-credentials"),
            None,
            "origin {origin} must not be granted credentials"
        );
    }
}

#[tokio::test]
async fn cors_never_answers_with_a_wildcard() {
    let app = TestApp::spawn(no_mongo(), test_routes).await;
    for origin in [Some(ALLOWED_ORIGIN), Some("https://evil.example"), None] {
        let mut request = app.client.get(app.url("/__test/ok"));
        if let Some(origin) = origin {
            request = request.header("Origin", origin);
        }
        let response = request.send().await.unwrap();
        assert_ne!(header(&response, "access-control-allow-origin"), Some("*"));
    }
}

fn preflight(app: &TestApp, origin: &str) -> reqwest::RequestBuilder {
    app.client
        .request(reqwest::Method::OPTIONS, app.url("/__test/echo"))
        .header("Origin", origin)
        .header("Access-Control-Request-Method", "POST")
        .header("Access-Control-Request-Headers", "content-type")
}

#[tokio::test]
async fn preflight_from_allowed_origin_succeeds() {
    let app = TestApp::spawn(no_mongo(), test_routes).await;
    let response = preflight(&app, ALLOWED_ORIGIN).send().await.unwrap();
    assert!(response.status().is_success(), "{}", response.status());
    assert_eq!(
        header(&response, "access-control-allow-origin"),
        Some(ALLOWED_ORIGIN)
    );
    assert_eq!(
        header(&response, "access-control-allow-credentials"),
        Some("true")
    );
    assert!(
        header(&response, "access-control-allow-methods")
            .unwrap()
            .contains("POST")
    );
    assert!(
        header(&response, "access-control-allow-headers")
            .unwrap()
            .to_ascii_lowercase()
            .contains("content-type")
    );
    assert_eq!(header(&response, "access-control-max-age"), Some("600"));
}

#[tokio::test]
async fn preflight_from_other_origin_is_not_allowed() {
    let app = TestApp::spawn(no_mongo(), test_routes).await;
    let response = preflight(&app, "https://evil.example")
        .send()
        .await
        .unwrap();
    // Browsers allow a cross-origin request only when Access-Control-Allow-Origin names their
    // origin, so its absence is what keeps this origin out. Credentials are not granted either.
    // (The static headers, methods, headers and max-age, are sent on every preflight and grant
    // nothing without an allowed origin.)
    assert_eq!(header(&response, "access-control-allow-origin"), None);
    assert_eq!(header(&response, "access-control-allow-credentials"), None);
    // The answer depends on the Origin header, so shared caches must keep the variants apart.
    assert!(
        header(&response, "vary")
            .unwrap()
            .to_ascii_lowercase()
            .contains("origin")
    );
}

#[tokio::test]
async fn error_responses_carry_cors_headers_for_the_allowed_origin() {
    let app = TestApp::spawn(no_mongo(), test_routes).await;

    // 404 from the router.
    let not_found = app
        .client
        .get(app.url("/nope"))
        .header("Origin", ALLOWED_ORIGIN)
        .send()
        .await
        .unwrap();
    assert_eq!(not_found.status(), 404);
    assert_eq!(
        header(&not_found, "access-control-allow-origin"),
        Some(ALLOWED_ORIGIN)
    );

    // 413 from the body limit.
    let too_big = json_post(&app, echo_body_of(MAX_BODY_BYTES + 1))
        .header("Origin", ALLOWED_ORIGIN)
        .send()
        .await
        .unwrap();
    assert_eq!(too_big.status(), 413);
    assert_eq!(
        header(&too_big, "access-control-allow-origin"),
        Some(ALLOWED_ORIGIN)
    );

    // 500 from a panic.
    let panicked = app
        .client
        .get(app.url("/__test/panic"))
        .header("Origin", ALLOWED_ORIGIN)
        .send()
        .await
        .unwrap();
    assert_eq!(panicked.status(), 500);
    assert_eq!(
        header(&panicked, "access-control-allow-origin"),
        Some(ALLOWED_ORIGIN)
    );
}

#[tokio::test]
async fn several_allowed_origins_are_each_honoured() {
    let app = TestApp::spawn(
        AppOptions {
            mongo: MongoMode::Unreachable,
            origins: "https://app.example, http://localhost:3000",
            ..AppOptions::default()
        },
        test_routes,
    )
    .await;
    for origin in ["https://app.example", "http://localhost:3000"] {
        let response = app
            .client
            .get(app.url("/__test/ok"))
            .header("Origin", origin)
            .send()
            .await
            .unwrap();
        assert_eq!(
            header(&response, "access-control-allow-origin"),
            Some(origin)
        );
    }
}

// --- logs ----------------------------------------------------------------------------------

#[tokio::test]
async fn request_logs_never_contain_the_query_string_or_credentials() {
    let logs = LogCapture::start();
    let app = TestApp::spawn(no_mongo(), test_routes).await;

    let response = app
        .client
        .get(app.url("/__test/ok?ticket=TICKET-SECRET-9f3a"))
        .header("Cookie", "ss_session=COOKIE-SECRET-77b1")
        .header("Authorization", "Bearer BEARER-SECRET-12c4")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let text = logs.text();
    assert!(text.contains("/__test/ok"), "the path is logged: {text}");
    assert!(text.contains("200"), "the status is logged: {text}");
    for secret in ["TICKET-SECRET", "COOKIE-SECRET", "BEARER-SECRET", "ticket="] {
        assert!(
            !text.contains(secret),
            "{secret} leaked into the log:\n{text}"
        );
    }
}

// --- shutdown ------------------------------------------------------------------------------

async fn slow() -> &'static str {
    tokio::time::sleep(Duration::from_millis(400)).await;
    "slow done"
}

#[tokio::test]
async fn graceful_shutdown_finishes_the_in_flight_request_and_stops() {
    let config = test_config(
        UNREACHABLE_URI,
        "syncspace_test_unreachable",
        ALLOWED_ORIGIN,
    );
    let state = AppState::new(
        db::connect(&config).await.unwrap(),
        std::sync::Arc::new(config.clone()),
    )
    .unwrap();
    let router = app::apply_layers(
        app::routes()
            .route("/__test/slow", get(slow))
            .with_state(state),
        &config,
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(app::serve(listener, router, async move {
        let _ = stopped.await;
    }));

    common::install_tls();
    let client = reqwest::Client::new();
    let url = format!("http://{addr}/__test/slow");
    let in_flight = tokio::spawn({
        let (client, url) = (client.clone(), url.clone());
        async move { client.get(url).send().await }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    stop.send(()).unwrap();

    // The request that was already running still gets its answer.
    let response = in_flight.await.unwrap().unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.text().await.unwrap(), "slow done");

    // The server then stops by itself, and no longer accepts connections.
    let result = tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .expect("the server should stop after the shutdown signal")
        .unwrap();
    assert!(result.is_ok());
    assert!(client.get(url).send().await.is_err());
}

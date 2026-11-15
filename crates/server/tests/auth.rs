//! Integration tests for accounts and sessions (`docs/TESTING.md`, "Auth"), against a real
//! MongoDB: register, log in, who am I, log out. Every test starts the real app.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use common::{ALLOWED_ORIGIN, AppOptions, KEY_A, LogCapture, MongoMode, TestApp};
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime, Document, doc};
use reqwest::{RequestBuilder, Response};
use serde_json::{Value, json};
use server::auth::session;
use server::db::users::{self, InsertError, User};

const REGISTER: &str = "/api/v1/auth/register";
const LOGIN: &str = "/api/v1/auth/login";
const LOGOUT: &str = "/api/v1/auth/logout";
const ME: &str = "/api/v1/auth/me";

const PASSWORD: &str = "correct horse battery staple";

/// The key the test app signs sessions with.
fn secret() -> Vec<u8> {
    STANDARD.decode(KEY_A).unwrap()
}

async fn post(app: &TestApp, path: &str, body: &Value) -> Response {
    app.client
        .post(app.url(path))
        .json(body)
        .send()
        .await
        .unwrap()
}

async fn register(app: &TestApp, name: &str, email: &str, password: &str) -> Response {
    post(
        app,
        REGISTER,
        &json!({"name": name, "email": email, "password": password}),
    )
    .await
}

async fn login(app: &TestApp, email: &str, password: &str) -> Response {
    post(app, LOGIN, &json!({"email": email, "password": password})).await
}

fn with_session(request: RequestBuilder, token: &str) -> RequestBuilder {
    request.header("Cookie", format!("ss_session={token}"))
}

fn set_cookie(response: &Response) -> String {
    response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .to_string()
}

/// The token inside a `Set-Cookie` value.
fn token_of(set_cookie: &str) -> String {
    set_cookie
        .strip_prefix("ss_session=")
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

/// Registers a user and returns their session token and the `User` JSON.
async fn sign_up(app: &TestApp, email: &str) -> (String, Value) {
    let response = register(app, "Ada Lovelace", email, PASSWORD).await;
    assert_eq!(response.status(), 201);
    let token = token_of(&set_cookie(&response));
    (token, response.json().await.unwrap())
}

async fn user_count(app: &TestApp) -> u64 {
    app.db
        .collection::<Document>("users")
        .count_documents(doc! {})
        .await
        .unwrap()
}

fn assert_error(body: &Value, code: &str) {
    assert_eq!(body["error"]["code"], code, "{body}");
    assert!(body["error"]["message"].is_string(), "{body}");
}

fn not_signed_in() -> Value {
    json!({"error": {"code": "UNAUTHORIZED", "message": "not signed in"}})
}

// --- register ------------------------------------------------------------------------------

#[tokio::test]
async fn register_creates_the_account_and_starts_a_session() {
    let app = TestApp::real().await;
    let response = register(&app, "Ada Lovelace", "ada@example.com", PASSWORD).await;
    assert_eq!(response.status(), 201);
    let cookie = set_cookie(&response);
    let body: Value = response.json().await.unwrap();

    // The answer is exactly the User of docs/API.md: nothing else, least of all a hash.
    let mut keys: Vec<_> = body.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(keys, ["email", "id", "name"]);
    assert_eq!(body["name"], "Ada Lovelace");
    assert_eq!(body["email"], "ada@example.com");
    let id = body["id"].as_str().unwrap();
    assert_eq!(id.len(), 24);
    assert!(ObjectId::parse_str(id).is_ok());
    assert!(!body.to_string().contains("password"));

    // The cookie is a session for that user.
    let token = token_of(&cookie);
    assert_eq!(
        session::verify(&secret(), &token).unwrap().to_hex(),
        id,
        "the cookie must belong to the new user"
    );

    // The stored row holds an argon2id hash, never the password.
    assert_eq!(user_count(&app).await, 1);
    let stored = app
        .db
        .collection::<Document>("users")
        .find_one(doc! {"email": "ada@example.com"})
        .await
        .unwrap()
        .unwrap();
    let hash = stored.get_str("password_hash").unwrap();
    assert!(
        hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"),
        "{hash}"
    );
    assert!(!hash.contains(PASSWORD));
    assert_ne!(hash, PASSWORD);
    assert_eq!(stored.get_str("name").unwrap(), "Ada Lovelace");
    assert!(stored.get_datetime("created_at").is_ok());
    assert_eq!(stored.get_object_id("_id").unwrap().to_hex(), id);
}

#[tokio::test]
async fn register_normalises_the_email_and_trims_the_name() {
    let app = TestApp::real().await;
    let response = register(&app, "  Ada  ", "  Ada@Example.COM ", PASSWORD).await;
    assert_eq!(response.status(), 201);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["email"], "ada@example.com");
    assert_eq!(body["name"], "Ada");

    let stored = app
        .db
        .collection::<Document>("users")
        .find_one(doc! {})
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.get_str("email").unwrap(), "ada@example.com");
}

#[tokio::test]
async fn register_with_a_taken_email_is_409_in_any_case() {
    let app = TestApp::real().await;
    sign_up(&app, "ada@example.com").await;

    for email in ["ada@example.com", "ADA@EXAMPLE.COM", " Ada@Example.com "] {
        let response = register(&app, "Someone Else", email, "another password").await;
        assert_eq!(response.status(), 409, "{email}");
        assert!(response.headers().get("set-cookie").is_none());
        let body: Value = response.json().await.unwrap();
        assert_eq!(
            body,
            json!({"error": {"code": "CONFLICT", "message": "an account with this email already exists"}})
        );
    }
    assert_eq!(user_count(&app).await, 1);
}

#[tokio::test]
async fn concurrent_registrations_of_one_email_create_exactly_one_account() {
    let app = TestApp::real().await;
    let (first, second) = tokio::join!(
        register(&app, "First", "race@example.com", PASSWORD),
        register(&app, "Second", "race@example.com", PASSWORD),
    );
    let mut statuses = [first.status().as_u16(), second.status().as_u16()];
    statuses.sort_unstable();
    assert_eq!(statuses, [201, 409], "one winner, one conflict, no 500");
    assert_eq!(user_count(&app).await, 1);
}

#[tokio::test]
async fn the_unique_index_is_the_final_word_on_a_duplicate_email() {
    // The handler checks first, to save a hash. When two requests get past that check at the
    // same moment, the index must still refuse the second one, and say so as DuplicateEmail.
    let app = TestApp::real().await;
    let user = |email: &str| User {
        id: ObjectId::new(),
        email: email.to_string(),
        name: "Ada".to_string(),
        password_hash: "$argon2id$placeholder".to_string(),
        created_at: DateTime::now(),
    };
    users::insert(&app.db, &user("same@example.com"))
        .await
        .unwrap();
    assert!(matches!(
        users::insert(&app.db, &user("same@example.com")).await,
        Err(InsertError::DuplicateEmail)
    ));
    users::insert(&app.db, &user("other@example.com"))
        .await
        .unwrap();

    // Lookups expect a normalised email: this is why the handlers lower-case first.
    assert!(
        users::find_by_email(&app.db, "same@example.com")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        users::find_by_email(&app.db, "SAME@example.com")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn register_validation_reports_every_problem_at_once() {
    let app = TestApp::real().await;
    let response = register(&app, "", "not-an-email", "short").await;
    assert_eq!(response.status(), 422);
    let body: Value = response.json().await.unwrap();
    assert_error(&body, "VALIDATION");
    let details = body["error"]["details"].as_object().unwrap();
    let mut fields: Vec<_> = details.keys().cloned().collect();
    fields.sort();
    assert_eq!(fields, ["email", "name", "password"]);
    assert_eq!(
        user_count(&app).await,
        0,
        "nothing is created on a validation error"
    );
}

#[tokio::test]
async fn register_rejects_bad_json_and_wrong_shapes() {
    let app = TestApp::real().await;

    let malformed = app
        .client
        .post(app.url(REGISTER))
        .header("Content-Type", "application/json")
        .body(r#"{"name": "Ada", "#)
        .send()
        .await
        .unwrap();
    assert_eq!(malformed.status(), 400);
    assert_error(&malformed.json().await.unwrap(), "BAD_REQUEST");

    for body in [
        json!({"name": "Ada", "email": "ada@example.com"}),
        json!({"name": 7, "email": "ada@example.com", "password": PASSWORD}),
        json!({}),
        json!([]),
    ] {
        let response = post(&app, REGISTER, &body).await;
        assert_eq!(response.status(), 422, "{body}");
        assert_error(&response.json().await.unwrap(), "VALIDATION");
    }
    assert_eq!(user_count(&app).await, 0);
}

#[tokio::test]
async fn an_oversized_registration_is_413() {
    let app = TestApp::real().await;
    let response = register(&app, &"n".repeat(300 * 1024), "ada@example.com", PASSWORD).await;
    assert_eq!(response.status(), 413);
    assert_error(&response.json().await.unwrap(), "PAYLOAD_TOO_LARGE");
}

#[tokio::test]
async fn unicode_names_and_passwords_work_end_to_end() {
    let app = TestApp::real().await;
    let password = "pässwörd-🙂-नमस्ते-ok";
    let response = register(&app, "नमस्ते 🙂", "unicode@example.com", password).await;
    assert_eq!(response.status(), 201);
    assert_eq!(response.json::<Value>().await.unwrap()["name"], "नमस्ते 🙂");

    assert_eq!(
        login(&app, "unicode@example.com", password).await.status(),
        200
    );
    assert_eq!(
        login(&app, "unicode@example.com", "password-🙂-नमस्ते-ok")
            .await
            .status(),
        401
    );
}

// --- login ---------------------------------------------------------------------------------

#[tokio::test]
async fn login_succeeds_and_starts_a_session() {
    let app = TestApp::real().await;
    let (_, registered) = sign_up(&app, "ada@example.com").await;

    // A different spelling of the same address: the lookup is normalised.
    let response = login(&app, "  ADA@Example.com ", PASSWORD).await;
    assert_eq!(response.status(), 200);
    let cookie = set_cookie(&response);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body, registered, "the same account, with the same shape");
    assert_eq!(
        session::verify(&secret(), &token_of(&cookie))
            .unwrap()
            .to_hex(),
        body["id"].as_str().unwrap()
    );
}

#[tokio::test]
async fn a_wrong_password_and_an_unknown_email_get_the_same_401() {
    let app = TestApp::real().await;
    sign_up(&app, "ada@example.com").await;

    let wrong_password = login(&app, "ada@example.com", "not the password").await;
    let unknown_email = login(&app, "nobody@example.com", PASSWORD).await;

    assert_eq!(wrong_password.status(), 401);
    assert_eq!(unknown_email.status(), 401);
    assert!(wrong_password.headers().get("set-cookie").is_none());
    assert!(unknown_email.headers().get("set-cookie").is_none());
    let (a, b) = (
        wrong_password.text().await.unwrap(),
        unknown_email.text().await.unwrap(),
    );
    assert_eq!(a, b, "the two answers must be byte for byte the same");
    assert_eq!(
        serde_json::from_str::<Value>(&a).unwrap(),
        json!({"error": {"code": "UNAUTHORIZED", "message": "invalid email or password"}})
    );
}

#[tokio::test]
async fn odd_login_inputs_get_the_same_401_and_never_a_500() {
    let app = TestApp::real().await;
    sign_up(&app, "ada@example.com").await;
    let long_email = format!("{}@example.com", "a".repeat(260));
    let cases = [
        ("", PASSWORD),
        ("ada@example.com", ""),
        ("   ", "   "),
        ("ada@example.com", &"p".repeat(129)),
        (long_email.as_str(), PASSWORD),
        ("ada@example.com\0", PASSWORD),
        ("åda@example.com", PASSWORD),
    ];
    for (email, password) in cases {
        let response = login(&app, email, password).await;
        assert_eq!(
            response.status(),
            401,
            "{email:?} / {} chars",
            password.chars().count()
        );
        assert_eq!(
            response.json::<Value>().await.unwrap(),
            json!({"error": {"code": "UNAUTHORIZED", "message": "invalid email or password"}})
        );
    }
}

#[tokio::test]
async fn login_with_a_missing_field_is_422_and_malformed_json_is_400() {
    let app = TestApp::real().await;
    let missing = post(&app, LOGIN, &json!({"email": "ada@example.com"})).await;
    assert_eq!(missing.status(), 422);
    assert_error(&missing.json().await.unwrap(), "VALIDATION");

    let malformed = app
        .client
        .post(app.url(LOGIN))
        .header("Content-Type", "application/json")
        .body("{")
        .send()
        .await
        .unwrap();
    assert_eq!(malformed.status(), 400);
}

// --- who am I ------------------------------------------------------------------------------

#[tokio::test]
async fn me_returns_the_signed_in_user() {
    let app = TestApp::real().await;
    let (token, registered) = sign_up(&app, "ada@example.com").await;
    let response = with_session(app.client.get(app.url(ME)), &token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.json::<Value>().await.unwrap(), registered);
}

#[tokio::test]
async fn me_without_a_session_is_401() {
    let app = TestApp::real().await;
    sign_up(&app, "ada@example.com").await;
    let candidates: [Option<&str>; 5] = [
        None,
        Some("ss_session="),
        Some("ss_session=not-a-token"),
        Some("other=value"),
        Some("SS_SESSION=abc"),
    ];
    for cookie in candidates {
        let mut request = app.client.get(app.url(ME));
        if let Some(cookie) = cookie {
            request = request.header("Cookie", cookie);
        }
        let response = request.send().await.unwrap();
        assert_eq!(response.status(), 401, "{cookie:?}");
        assert_eq!(response.json::<Value>().await.unwrap(), not_signed_in());
    }
}

/// A token that is formally fine but must not be believed.
fn forged_tokens(user: ObjectId) -> Vec<(&'static str, String)> {
    let day = 24 * 60 * 60;
    let genuine = session::issue(&secret(), user, session::now()).unwrap();

    let mut parts: Vec<String> = genuine.split('.').map(String::from).collect();
    let mut claims: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(&parts[1]).unwrap()).unwrap();
    claims["sub"] = json!(ObjectId::new().to_hex());
    parts[1] = URL_SAFE_NO_PAD.encode(claims.to_string());

    let none_header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
    let none_claims = URL_SAFE_NO_PAD.encode(
        json!({"sub": user.to_hex(), "iat": session::now(), "exp": session::now() + day})
            .to_string(),
    );
    let hs512 = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS512),
        &json!({"sub": user.to_hex(), "iat": session::now(), "exp": session::now() + day}),
        &jsonwebtoken::EncodingKey::from_secret(&secret()).unwrap(),
    )
    .unwrap();

    vec![
        (
            "expired",
            session::issue(&secret(), user, session::now() - 2 * day).unwrap(),
        ),
        (
            "expired by seconds",
            session::issue(&secret(), user, session::now() - day - 3).unwrap(),
        ),
        (
            "signed with another key",
            session::issue(&[0xAB; 32], user, session::now()).unwrap(),
        ),
        ("tampered payload", parts.join(".")),
        ("alg none", format!("{none_header}.{none_claims}.")),
        ("another HMAC algorithm", hs512),
        ("truncated", genuine[..genuine.len() - 4].to_string()),
    ]
}

#[tokio::test]
async fn me_refuses_expired_forged_and_none_algorithm_tokens() {
    let app = TestApp::real().await;
    let (token, registered) = sign_up(&app, "ada@example.com").await;
    let user = ObjectId::parse_str(registered["id"].as_str().unwrap()).unwrap();

    // The genuine token works, so each refusal below is about the token and nothing else.
    let ok = with_session(app.client.get(app.url(ME)), &token)
        .send()
        .await
        .unwrap();
    assert_eq!(ok.status(), 200);

    for (what, bad) in forged_tokens(user) {
        let response = with_session(app.client.get(app.url(ME)), &bad)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 401, "{what}");
        assert_eq!(
            response.json::<Value>().await.unwrap(),
            not_signed_in(),
            "{what}: every refusal looks the same"
        );
    }
}

#[tokio::test]
async fn a_session_for_a_deleted_account_is_not_a_session() {
    let app = TestApp::real().await;
    let (token, registered) = sign_up(&app, "ada@example.com").await;
    let id = ObjectId::parse_str(registered["id"].as_str().unwrap()).unwrap();
    app.db
        .collection::<Document>("users")
        .delete_one(doc! {"_id": id})
        .await
        .unwrap();

    let response = with_session(app.client.get(app.url(ME)), &token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    assert_eq!(response.json::<Value>().await.unwrap(), not_signed_in());
}

#[tokio::test]
async fn the_session_lasts_24_hours() {
    let app = TestApp::real().await;
    let (token, _) = sign_up(&app, "ada@example.com").await;
    let claims: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(token.split('.').nth(1).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        claims["exp"].as_u64().unwrap() - claims["iat"].as_u64().unwrap(),
        86_400
    );
    let issued = claims["iat"].as_u64().unwrap();
    assert!(
        issued.abs_diff(session::now()) < 30,
        "issued now, not at some other time"
    );
}

// --- logout --------------------------------------------------------------------------------

#[tokio::test]
async fn logout_clears_the_cookie() {
    let app = TestApp::real().await;
    let (token, _) = sign_up(&app, "ada@example.com").await;

    let response = with_session(app.client.post(app.url(LOGOUT)), &token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
    let cookie = set_cookie(&response);
    assert_eq!(
        cookie,
        "ss_session=; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Path=/; HttpOnly; SameSite=Lax"
    );
    assert!(response.text().await.unwrap().is_empty(), "204 has no body");
}

#[tokio::test]
async fn logout_without_a_session_is_401() {
    let app = TestApp::real().await;
    let response = app.client.post(app.url(LOGOUT)).send().await.unwrap();
    assert_eq!(response.status(), 401);
    assert_eq!(response.json::<Value>().await.unwrap(), not_signed_in());
}

#[tokio::test]
async fn anonymous_callers_are_refused_on_every_route_that_needs_a_session() {
    let app = TestApp::real().await;
    for (method, path) in [("GET", ME), ("POST", LOGOUT)] {
        let request = match method {
            "GET" => app.client.get(app.url(path)),
            _ => app.client.post(app.url(path)),
        };
        let response = request.send().await.unwrap();
        assert_eq!(response.status(), 401, "{method} {path}");
        assert_error(&response.json().await.unwrap(), "UNAUTHORIZED");
    }
    // And the two that are public really are public.
    assert_ne!(login(&app, "x@example.com", PASSWORD).await.status(), 403);
    assert_ne!(register(&app, "A", "a@b.co", PASSWORD).await.status(), 401);
}

// --- the cookie over the wire --------------------------------------------------------------

#[tokio::test]
async fn the_cookie_has_the_documented_attributes_and_no_secure_flag_over_http() {
    let app = TestApp::real().await;
    let register_cookie = set_cookie(&register(&app, "Ada", "ada@example.com", PASSWORD).await);
    let login_cookie = set_cookie(&login(&app, "ada@example.com", PASSWORD).await);
    for cookie in [register_cookie, login_cookie] {
        let attributes = cookie.split("; ").skip(1).collect::<Vec<_>>();
        assert_eq!(
            attributes,
            ["Max-Age=86400", "Path=/", "HttpOnly", "SameSite=Lax"],
            "{cookie}"
        );
    }
}

#[tokio::test]
async fn the_cookie_is_secure_when_the_setting_says_so() {
    let app = TestApp::spawn(
        AppOptions {
            cookie_secure: true,
            ..AppOptions::default()
        },
        |routes| routes,
    )
    .await;
    let registered = register(&app, "Ada", "ada@example.com", PASSWORD).await;
    let cookie = set_cookie(&registered);
    assert!(cookie.ends_with("; Secure"), "{cookie}");
    let token = token_of(&cookie);

    let logged_in = set_cookie(&login(&app, "ada@example.com", PASSWORD).await);
    assert!(logged_in.ends_with("; Secure"), "{logged_in}");

    let logout = with_session(app.client.post(app.url(LOGOUT)), &token)
        .send()
        .await
        .unwrap();
    assert!(set_cookie(&logout).ends_with("; Secure"));
}

// --- capacity ------------------------------------------------------------------------------

fn tight_capacity() -> AppOptions {
    AppOptions {
        hash_limits: Some((1, Duration::from_millis(150))),
        ..AppOptions::default()
    }
}

#[tokio::test]
async fn when_hashing_capacity_is_used_up_requests_get_429_and_recover_afterwards() {
    let app = TestApp::spawn(tight_capacity(), |routes| routes).await;
    sign_up(&app, "ada@example.com").await;

    let held = app.auth.semaphore().acquire_owned().await.unwrap();
    let refused = register(&app, "Grace", "grace@example.com", PASSWORD).await;
    assert_eq!(refused.status(), 429);
    assert_error(&refused.json().await.unwrap(), "RATE_LIMITED");
    assert_eq!(login(&app, "ada@example.com", PASSWORD).await.status(), 429);
    assert_eq!(
        user_count(&app).await,
        1,
        "a refused registration creates nothing"
    );

    drop(held);
    assert_eq!(
        register(&app, "Grace", "grace@example.com", PASSWORD)
            .await
            .status(),
        201
    );
    assert_eq!(login(&app, "ada@example.com", PASSWORD).await.status(), 200);
}

#[tokio::test]
async fn an_unknown_email_costs_as_much_as_a_known_one() {
    // With no hashing capacity left, a login for a user that does not exist is refused for the
    // same reason as one that does: both need a (dummy or real) verification. That is what
    // keeps their response times alike.
    let app = TestApp::spawn(tight_capacity(), |routes| routes).await;
    sign_up(&app, "ada@example.com").await;
    let held = app.auth.semaphore().acquire_owned().await.unwrap();

    assert_eq!(login(&app, "ada@example.com", "wrong").await.status(), 429);
    assert_eq!(
        login(&app, "nobody@example.com", "wrong").await.status(),
        429
    );

    // An over-long password is refused up front, without hashing, so it still gets its 401.
    assert_eq!(
        login(&app, "ada@example.com", &"p".repeat(200))
            .await
            .status(),
        401
    );
    drop(held);
}

// --- routing, CORS, audit, logs ------------------------------------------------------------

#[tokio::test]
async fn paths_under_api_v1_use_the_error_format() {
    let app = TestApp::without_mongo().await;
    let unknown = app
        .client
        .get(app.url("/api/v1/nope"))
        .send()
        .await
        .unwrap();
    assert_eq!(unknown.status(), 404);
    assert_error(&unknown.json().await.unwrap(), "NOT_FOUND");

    let bare = app
        .client
        .get(app.url("/api/v1/auth"))
        .send()
        .await
        .unwrap();
    assert_eq!(bare.status(), 404);

    for (request, what) in [
        (app.client.get(app.url(REGISTER)), "GET register"),
        (app.client.get(app.url(LOGIN)), "GET login"),
        (app.client.get(app.url(LOGOUT)), "GET logout"),
        (app.client.post(app.url(ME)), "POST me"),
    ] {
        let response = request.send().await.unwrap();
        assert_eq!(response.status(), 405, "{what}");
        assert_error(&response.json().await.unwrap(), "METHOD_NOT_ALLOWED");
    }
}

#[tokio::test]
async fn cors_covers_the_auth_routes_for_the_allowed_origin_only() {
    let app = TestApp::real().await;
    sign_up(&app, "ada@example.com").await;

    let preflight = |origin: &str| {
        app.client
            .request(reqwest::Method::OPTIONS, app.url(LOGIN))
            .header("Origin", origin)
            .header("Access-Control-Request-Method", "POST")
            .header("Access-Control-Request-Headers", "content-type")
    };
    let allowed = preflight(ALLOWED_ORIGIN).send().await.unwrap();
    assert!(allowed.status().is_success());
    assert_eq!(
        allowed.headers()["access-control-allow-origin"],
        ALLOWED_ORIGIN
    );
    assert_eq!(
        allowed.headers()["access-control-allow-credentials"],
        "true"
    );
    let foreign = preflight("https://evil.example").send().await.unwrap();
    assert!(
        foreign
            .headers()
            .get("access-control-allow-origin")
            .is_none()
    );

    // The real request from the allowed origin: the browser may read the answer and keep the
    // cookie. From any other origin it gets no CORS header at all.
    let from_app = app
        .client
        .post(app.url(LOGIN))
        .header("Origin", ALLOWED_ORIGIN)
        .json(&json!({"email": "ada@example.com", "password": PASSWORD}))
        .send()
        .await
        .unwrap();
    assert_eq!(from_app.status(), 200);
    assert_eq!(
        from_app.headers()["access-control-allow-origin"],
        ALLOWED_ORIGIN
    );
    assert_eq!(
        from_app.headers()["access-control-allow-credentials"],
        "true"
    );

    let from_evil = app
        .client
        .post(app.url(LOGIN))
        .header("Origin", "https://evil.example")
        .json(&json!({"email": "ada@example.com", "password": PASSWORD}))
        .send()
        .await
        .unwrap();
    assert!(
        from_evil
            .headers()
            .get("access-control-allow-origin")
            .is_none()
    );
    assert!(
        from_evil
            .headers()
            .get("access-control-allow-credentials")
            .is_none()
    );
}

#[tokio::test]
async fn accounts_and_sessions_write_no_audit_rows() {
    // The audit table in docs/DATA_MODEL.md has no auth actions; nothing here is audited.
    let app = TestApp::real().await;
    let (token, _) = sign_up(&app, "ada@example.com").await;
    login(&app, "ada@example.com", PASSWORD).await;
    login(&app, "ada@example.com", "wrong").await;
    with_session(app.client.get(app.url(ME)), &token)
        .send()
        .await
        .unwrap();
    with_session(app.client.post(app.url(LOGOUT)), &token)
        .send()
        .await
        .unwrap();

    let audit = app
        .db
        .collection::<Document>("audit_logs")
        .count_documents(doc! {})
        .await
        .unwrap();
    assert_eq!(audit, 0);
}

#[tokio::test]
async fn the_log_never_contains_passwords_tokens_hashes_or_personal_data() {
    let logs = LogCapture::start();
    let app = TestApp::spawn(
        AppOptions {
            mongo: MongoMode::Real,
            ..AppOptions::default()
        },
        |routes| routes,
    )
    .await;

    let canary_password = "Canary-Pa55word-xyzzy";
    let registered = register(
        &app,
        "Canary Person",
        "canary.person@example.com",
        canary_password,
    )
    .await;
    let token = token_of(&set_cookie(&registered));
    login(&app, "canary.person@example.com", canary_password).await;
    login(&app, "canary.person@example.com", "Canary-Wrong-Pa55word").await;
    with_session(app.client.get(app.url(ME)), &token)
        .send()
        .await
        .unwrap();
    with_session(app.client.post(app.url(LOGOUT)), &token)
        .send()
        .await
        .unwrap();

    let text = logs.text();
    // The log is not empty: these requests were logged...
    for path in [REGISTER, LOGIN, ME, LOGOUT] {
        assert!(text.contains(path), "{path} is not in the log:\n{text}");
    }
    // ...and none of the sensitive data came with them.
    for secret in [
        "Canary-Pa55word",
        "Canary-Wrong",
        "canary.person",
        "Canary Person",
        token.as_str(),
        "argon2",
        "password_hash",
    ] {
        assert!(
            !text.contains(secret),
            "{secret:?} leaked into the log:\n{text}"
        );
    }
}

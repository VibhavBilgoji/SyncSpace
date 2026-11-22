//! Sign in with GitHub (ADR-015), against a fake GitHub that runs in the test. Nothing here
//! talks to the real GitHub.
//!
//! The things that must hold: a person is signed in as the user their GitHub account belongs
//! to, found by the linked GitHub id first and by a *verified* email address second; a new user
//! is created only when there is a verified address; the `state` check stops a forged callback
//! before any request goes to GitHub; every failure is a redirect to the sign-in page with a
//! code; and neither the token nor the client secret is stored or shown anywhere.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use common::{Account, AppOptions, TestApp};
use mongodb::bson::{Document, doc};
use serde_json::{Value, json};

const TOKEN: &str = "gho_faketoken0123456789";
const CLIENT_SECRET: &str = "test-secret-value";

/// What the fake GitHub does next, and what it was asked.
#[derive(Clone)]
struct Scenario {
    /// The token request is answered with a token (otherwise with GitHub's error body).
    token_ok: bool,
    /// The status of `/user` and `/user/emails` when it is not 200.
    user_status: u16,
    emails_status: u16,
    profile: Value,
    emails: Value,
    /// The JSON bodies of the token requests, and how many `/user` requests came in.
    exchanges: Vec<Value>,
    user_calls: usize,
}

impl Scenario {
    fn new(id: i64, login: &str, name: Option<&str>, emails: Value) -> Self {
        Self {
            token_ok: true,
            user_status: 200,
            emails_status: 200,
            profile: json!({"id": id, "login": login, "name": name}),
            emails,
            exchanges: Vec::new(),
            user_calls: 0,
        }
    }
}

type Shared = Arc<Mutex<Scenario>>;

struct FakeGithub {
    url: String,
    state: Shared,
}

impl FakeGithub {
    async fn start(initial: Scenario) -> Self {
        let state: Shared = Arc::new(Mutex::new(initial));
        let router = Router::new()
            .route("/login/oauth/access_token", post(token))
            .route("/user", get(user))
            .route("/user/emails", get(emails))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            url: format!("http://{addr}"),
            state,
        }
    }

    fn set(&self, change: impl FnOnce(&mut Scenario)) {
        change(&mut self.state.lock().unwrap());
    }

    fn exchanges(&self) -> Vec<Value> {
        self.state.lock().unwrap().exchanges.clone()
    }

    fn user_calls(&self) -> usize {
        self.state.lock().unwrap().user_calls
    }
}

async fn token(State(state): State<Shared>, Json(body): Json<Value>) -> Json<Value> {
    let mut scenario = state.lock().unwrap();
    scenario.exchanges.push(body);
    if scenario.token_ok {
        Json(
            json!({"access_token": TOKEN, "token_type": "bearer", "scope": "read:user,user:email"}),
        )
    } else {
        Json(json!({"error": "bad_verification_code"}))
    }
}

fn authorised(headers: &HeaderMap) -> bool {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == format!("Bearer {TOKEN}"))
}

async fn user(State(state): State<Shared>, headers: HeaderMap) -> (StatusCode, Json<Value>) {
    let mut scenario = state.lock().unwrap();
    scenario.user_calls += 1;
    if !authorised(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"message": "Bad credentials"})),
        );
    }
    if scenario.user_status != 200 {
        let status = StatusCode::from_u16(scenario.user_status).unwrap();
        return (status, Json(json!({"message": "nope"})));
    }
    (StatusCode::OK, Json(scenario.profile.clone()))
}

async fn emails(State(state): State<Shared>, headers: HeaderMap) -> (StatusCode, Json<Value>) {
    let scenario = state.lock().unwrap();
    if !authorised(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"message": "Bad credentials"})),
        );
    }
    if scenario.emails_status != 200 {
        let status = StatusCode::from_u16(scenario.emails_status).unwrap();
        return (status, Json(json!({"message": "nope"})));
    }
    (StatusCode::OK, Json(scenario.emails.clone()))
}

fn email(address: &str, primary: bool, verified: bool) -> Value {
    json!({"email": address, "primary": primary, "verified": verified, "visibility": null})
}

async fn app_with(github: &FakeGithub) -> TestApp {
    TestApp::spawn(
        AppOptions {
            github: Some(github.url.clone()),
            ..AppOptions::default()
        },
        |routes| routes,
    )
    .await
}

/// A redirect answer.
struct Redirect {
    status: u16,
    location: String,
    cookies: Vec<String>,
    body: String,
}

impl Redirect {
    async fn from(response: reqwest::Response) -> Self {
        let status = response.status().as_u16();
        let location = response
            .headers()
            .get("location")
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default();
        let cookies = response
            .headers()
            .get_all("set-cookie")
            .iter()
            .map(|v| v.to_str().unwrap().to_string())
            .collect();
        let body = response.text().await.unwrap();
        Self {
            status,
            location,
            cookies,
            body,
        }
    }

    /// The value of the cookie `name` that this answer sets (`None`: not set).
    fn cookie(&self, name: &str) -> Option<String> {
        self.cookies
            .iter()
            .find_map(|c| c.strip_prefix(&format!("{name}=")))
            .map(|rest| rest.split(';').next().unwrap().to_string())
    }

    fn query(&self, key: &str) -> Option<String> {
        let url = reqwest::Url::parse(&self.location).ok()?;
        url.query_pairs()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.to_string())
    }

    /// Every header and the body as one string, for "this never shows up" checks.
    fn everything(&self) -> String {
        format!("{} {} {}", self.location, self.cookies.join(" "), self.body)
    }
}

async fn start(app: &TestApp, query: &str) -> Redirect {
    let response = app
        .client
        .get(app.url(&format!("/api/v1/auth/github/start{query}")))
        .send()
        .await
        .unwrap();
    Redirect::from(response).await
}

async fn callback(app: &TestApp, query: &str, cookies: &[(&str, &str)]) -> Redirect {
    let mut request = app
        .client
        .get(app.url(&format!("/api/v1/auth/github/callback?{query}")));
    if !cookies.is_empty() {
        let header = cookies
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ");
        request = request.header("Cookie", header);
    }
    Redirect::from(request.send().await.unwrap()).await
}

/// The whole round trip: start, then come back with a code. `session` is a signed-in caller.
async fn sign_in(app: &TestApp, session: Option<&Account>, next: Option<&str>) -> Redirect {
    let query = next.map(|n| format!("?next={n}")).unwrap_or_default();
    let begun = start(app, &query).await;
    let state = begun.cookie("ss_gh_state").expect("a state cookie");
    let next_cookie = begun.cookie("ss_gh_next");
    let mut cookies: Vec<(String, String)> = vec![("ss_gh_state".into(), state.clone())];
    if let Some(value) = next_cookie.filter(|v| !v.is_empty()) {
        cookies.push(("ss_gh_next".into(), value));
    }
    if let Some(account) = session {
        cookies.push(("ss_session".into(), account.token.clone()));
    }
    let borrowed: Vec<(&str, &str)> = cookies
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    callback(app, &format!("code=abc123&state={state}"), &borrowed).await
}

async fn users_collection(app: &TestApp) -> Vec<Document> {
    use futures_util::TryStreamExt;
    app.db
        .collection::<Document>("users")
        .find(doc! {})
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap()
}

async fn user_with_github_id(app: &TestApp, id: i64) -> Option<Document> {
    app.db
        .collection::<Document>("users")
        .find_one(doc! {"github_id": id})
        .await
        .unwrap()
}

fn octocat(emails: Value) -> Scenario {
    Scenario::new(583231, "octocat", Some("The Octocat"), emails)
}

// -- what the sign-in page may offer ---------------------------------------------------------

#[tokio::test]
async fn providers_say_whether_github_is_set_up() {
    let off = TestApp::real().await;
    let (status, body) = off.call("GET", "/api/v1/auth/providers", None, None).await;
    assert_eq!((status, body), (200, json!({"github": false})));

    let github = FakeGithub::start(octocat(json!([]))).await;
    let on = app_with(&github).await;
    let (status, body) = on.call("GET", "/api/v1/auth/providers", None, None).await;
    assert_eq!((status, body), (200, json!({"github": true})));
}

#[tokio::test]
async fn without_a_configuration_start_is_404_and_a_callback_goes_back_to_the_sign_in_page() {
    let app = TestApp::real().await;
    let (status, body) = app
        .call("GET", "/api/v1/auth/github/start", None, None)
        .await;
    assert_eq!(status, 404);
    assert!(body["error"]["code"].is_string());

    let back = callback(&app, "code=abc&state=xyz", &[("ss_gh_state", "xyz")]).await;
    assert_eq!(back.status, 302);
    assert_eq!(back.location, "/login?github_error=failed");
    assert!(users_collection(&app).await.is_empty());
}

// -- start -----------------------------------------------------------------------------------

#[tokio::test]
async fn start_sends_the_browser_to_github_with_a_fresh_state_and_only_the_identity_scopes() {
    let github = FakeGithub::start(octocat(json!([]))).await;
    let app = app_with(&github).await;

    let first = start(&app, "").await;
    assert_eq!(first.status, 302);
    assert!(
        first
            .location
            .starts_with(&format!("{}/login/oauth/authorize?", github.url)),
        "{}",
        first.location
    );
    assert_eq!(first.query("client_id").as_deref(), Some("test-client"));
    assert_eq!(
        first.query("redirect_uri").as_deref(),
        Some("http://localhost:3000/api/v1/auth/github/callback")
    );
    assert_eq!(
        first.query("scope").as_deref(),
        Some("read:user user:email")
    );
    // Never the broad repository scope.
    assert!(!first.query("scope").unwrap().contains("repo"));
    assert!(!first.location.contains(CLIENT_SECRET));

    let state = first.query("state").unwrap();
    assert_eq!(state.len(), 64);
    assert_eq!(first.cookie("ss_gh_state").as_deref(), Some(state.as_str()));
    let cookie = first
        .cookies
        .iter()
        .find(|c| c.starts_with("ss_gh_state="))
        .unwrap();
    for part in [
        "Max-Age=600",
        "Path=/api/v1/auth/github",
        "HttpOnly",
        "SameSite=Lax",
    ] {
        assert!(cookie.contains(part), "{cookie}");
    }

    let second = start(&app, "").await;
    assert_ne!(second.query("state"), first.query("state"));
}

#[tokio::test]
async fn start_only_remembers_a_plain_path_of_the_app_to_return_to() {
    let github = FakeGithub::start(octocat(json!([]))).await;
    let app = app_with(&github).await;

    let good = start(&app, "?next=/w/eng/repos").await;
    assert_eq!(good.cookie("ss_gh_next").as_deref(), Some("/w/eng/repos"));

    for bad in [
        "?next=//evil.example",
        "?next=https://evil.example",
        "?next=/w%3Fx%3D1",
        "?next=/w/%2e%2e",
        "?next=w/eng",
    ] {
        let answer = start(&app, bad).await;
        assert_eq!(
            answer.cookie("ss_gh_next").as_deref(),
            Some(""),
            "{bad} must not be remembered"
        );
    }
}

// -- the callback: a new account -------------------------------------------------------------

#[tokio::test]
async fn a_new_github_user_gets_an_account_and_a_session_and_no_token_is_kept() {
    let github = FakeGithub::start(octocat(json!([
        email("octocat-old@example.com", false, true),
        email("Octocat@Example.com", true, true),
    ])))
    .await;
    let app = app_with(&github).await;

    let done = sign_in(&app, None, None).await;
    assert_eq!(done.status, 302);
    assert_eq!(done.location, "/w");
    let session = done.cookie("ss_session").expect("a session cookie");
    assert!(
        done.cookies
            .iter()
            .any(|c| c.starts_with("ss_session=") && c.contains("HttpOnly"))
    );
    // The one-use cookies are cleared.
    assert_eq!(done.cookie("ss_gh_state").as_deref(), Some(""));
    assert_eq!(done.cookie("ss_gh_next").as_deref(), Some(""));

    let users = users_collection(&app).await;
    assert_eq!(users.len(), 1);
    let user = &users[0];
    assert_eq!(user.get_str("email").unwrap(), "octocat@example.com");
    assert_eq!(user.get_str("name").unwrap(), "The Octocat");
    assert_eq!(user.get_i64("github_id").unwrap(), 583231);
    assert_eq!(user.get_str("github_login").unwrap(), "octocat");
    assert!(user.get("password_hash").is_none());
    // Neither the token nor the client secret is in the database.
    let stored = format!("{user:?}");
    assert!(!stored.contains(TOKEN) && !stored.contains(CLIENT_SECRET));

    // The session works and says who the person is on GitHub.
    let me = app
        .client
        .get(app.url("/api/v1/auth/me"))
        .header("Cookie", format!("ss_session={session}"))
        .send()
        .await
        .unwrap();
    assert_eq!(me.status(), 200);
    let me: Value = me.json().await.unwrap();
    assert_eq!(me["github_login"], "octocat");
    assert_eq!(me["email"], "octocat@example.com");
    assert!(me.get("password_hash").is_none());

    // The code, the client id and the secret went to GitHub, over the server-to-server request.
    let exchanges = github.exchanges();
    assert_eq!(exchanges.len(), 1);
    assert_eq!(exchanges[0]["code"], "abc123");
    assert_eq!(exchanges[0]["client_id"], "test-client");
    assert_eq!(exchanges[0]["client_secret"], CLIENT_SECRET);
    assert_eq!(
        exchanges[0]["redirect_uri"],
        "http://localhost:3000/api/v1/auth/github/callback"
    );

    // Nothing the browser sees contains the token or the secret.
    let seen = done.everything();
    assert!(!seen.contains(TOKEN) && !seen.contains(CLIENT_SECRET));
}

#[tokio::test]
async fn the_name_falls_back_to_the_login_and_is_cut_to_the_limit() {
    let github = FakeGithub::start(Scenario::new(
        1,
        "nameless",
        None,
        json!([email("n@example.com", true, true)]),
    ))
    .await;
    let app = app_with(&github).await;
    sign_in(&app, None, None).await;
    assert_eq!(
        users_collection(&app).await[0].get_str("name").unwrap(),
        "nameless"
    );

    let long = "x".repeat(200);
    github.set(|s| {
        s.profile = json!({"id": 2, "login": "longname", "name": long});
        s.emails = json!([email("l@example.com", true, true)]);
    });
    sign_in(&app, None, None).await;
    let user = user_with_github_id(&app, 2).await.unwrap();
    assert_eq!(user.get_str("name").unwrap().chars().count(), 80);
}

#[tokio::test]
async fn signing_in_again_finds_the_same_user_and_refreshes_a_renamed_login() {
    let github =
        FakeGithub::start(octocat(json!([email("octocat@example.com", true, true)]))).await;
    let app = app_with(&github).await;
    sign_in(&app, None, None).await;
    let first = users_collection(&app).await[0]
        .get_object_id("_id")
        .unwrap();

    // Renamed on GitHub, and the email changed there too: the linked id is what counts.
    github.set(|s| {
        s.profile = json!({"id": 583231, "login": "octocat-renamed", "name": "The Octocat"});
        s.emails = json!([email("brand-new@example.com", true, true)]);
    });
    let again = sign_in(&app, None, None).await;
    assert_eq!(again.location, "/w");
    assert!(again.cookie("ss_session").is_some());

    let users = users_collection(&app).await;
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].get_object_id("_id").unwrap(), first);
    assert_eq!(users[0].get_str("github_login").unwrap(), "octocat-renamed");
    assert_eq!(users[0].get_str("email").unwrap(), "octocat@example.com");
}

// -- the callback: linking by verified email -------------------------------------------------

#[tokio::test]
async fn a_verified_email_that_matches_an_account_links_to_it_and_the_password_still_works() {
    let github = FakeGithub::start(octocat(json!([email("Ada@Example.com", true, true)]))).await;
    let app = app_with(&github).await;
    let (status, registered) = app
        .call(
            "POST",
            "/api/v1/auth/register",
            None,
            Some(json!({"name": "Ada", "email": "ada@example.com", "password": "a long enough passphrase"})),
        )
        .await;
    assert_eq!(status, 201);

    let done = sign_in(&app, None, None).await;
    assert_eq!(done.location, "/w");
    let users = users_collection(&app).await;
    assert_eq!(users.len(), 1, "no second account");
    assert_eq!(
        users[0].get_object_id("_id").unwrap().to_hex(),
        registered["id"]
    );
    assert_eq!(users[0].get_str("github_login").unwrap(), "octocat");
    assert!(users[0].get("password_hash").is_some());

    let (status, _) = app
        .call(
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({"email": "ada@example.com", "password": "a long enough passphrase"})),
        )
        .await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn an_unverified_email_is_never_used_to_link_an_account() {
    let github =
        FakeGithub::start(octocat(json!([email("victim@example.com", true, false)]))).await;
    let app = app_with(&github).await;
    let victim = app.account("Victim").await;
    app.db
        .collection::<Document>("users")
        .update_one(
            doc! {"_id": victim.id},
            doc! {"$set": {"email": "victim@example.com"}},
        )
        .await
        .unwrap();

    let done = sign_in(&app, None, None).await;
    assert_eq!(done.location, "/login?github_error=no_email");
    assert!(done.cookie("ss_session").is_none());
    let users = users_collection(&app).await;
    assert_eq!(users.len(), 1);
    assert!(
        users[0].get("github_id").is_none(),
        "the victim was not linked"
    );
}

#[tokio::test]
async fn a_new_account_uses_the_first_verified_address_and_skips_unverified_ones() {
    let github = FakeGithub::start(octocat(json!([
        email("primary-unverified@example.com", true, false),
        email("second@example.com", false, true),
    ])))
    .await;
    let app = app_with(&github).await;
    sign_in(&app, None, None).await;
    assert_eq!(
        users_collection(&app).await[0].get_str("email").unwrap(),
        "second@example.com"
    );
}

#[tokio::test]
async fn no_verified_address_and_no_account_to_link_means_no_account() {
    let github = FakeGithub::start(octocat(json!([]))).await;
    let app = app_with(&github).await;
    let done = sign_in(&app, None, None).await;
    assert_eq!(done.location, "/login?github_error=no_email");
    assert!(users_collection(&app).await.is_empty());
    assert!(done.cookie("ss_session").is_none());
}

// -- the callback: a person who is already signed in -----------------------------------------

#[tokio::test]
async fn a_signed_in_person_links_github_even_when_the_addresses_differ_and_returns_where_they_were()
 {
    let github =
        FakeGithub::start(octocat(json!([email("elsewhere@example.com", true, true)]))).await;
    let app = app_with(&github).await;
    let me = app.account("Me").await;

    let done = sign_in(&app, Some(&me), Some("/w/eng/repos")).await;
    assert_eq!(done.location, "/w/eng/repos");
    let users = users_collection(&app).await;
    assert_eq!(users.len(), 1, "no new account");
    assert_eq!(users[0].get_i64("github_id").unwrap(), 583231);
    assert_eq!(users[0].get_str("github_login").unwrap(), "octocat");
    assert_eq!(users[0].get_str("email").unwrap(), me.email);
}

#[tokio::test]
async fn a_github_account_cannot_be_linked_to_a_second_user() {
    let github = FakeGithub::start(octocat(json!([email("a@example.com", true, true)]))).await;
    let app = app_with(&github).await;
    let first = app.account("First").await;
    let second = app.account("Second").await;
    assert_eq!(sign_in(&app, Some(&first), None).await.location, "/w");

    let refused = sign_in(&app, Some(&second), None).await;
    assert_eq!(refused.location, "/login?github_error=conflict");
    assert!(
        refused.cookie("ss_session").is_none(),
        "no new session for the person who was refused"
    );
    let owner = user_with_github_id(&app, 583231).await.unwrap();
    assert_eq!(owner.get_object_id("_id").unwrap(), first.id);
}

#[tokio::test]
async fn a_user_with_one_github_account_cannot_take_a_second() {
    let github = FakeGithub::start(octocat(json!([email("a@example.com", true, true)]))).await;
    let app = app_with(&github).await;
    let me = app.account("Me").await;
    sign_in(&app, Some(&me), None).await;

    github.set(|s| {
        s.profile = json!({"id": 999, "login": "other-account", "name": null});
    });
    let refused = sign_in(&app, Some(&me), None).await;
    assert_eq!(refused.location, "/login?github_error=conflict");
    let user = users_collection(&app).await;
    assert_eq!(user[0].get_i64("github_id").unwrap(), 583231);
    assert_eq!(user[0].get_str("github_login").unwrap(), "octocat");
}

// -- accounts that only have GitHub ----------------------------------------------------------

#[tokio::test]
async fn a_github_only_account_has_no_password_to_guess_and_its_email_cannot_be_registered() {
    let github = FakeGithub::start(octocat(json!([email("only@example.com", true, true)]))).await;
    let app = app_with(&github).await;
    sign_in(&app, None, None).await;

    let wrong = app
        .call(
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({"email": "only@example.com", "password": "anything at all"})),
        )
        .await;
    let unknown = app
        .call(
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({"email": "nobody@example.com", "password": "anything at all"})),
        )
        .await;
    assert_eq!(wrong.0, 401);
    assert_eq!(wrong, unknown, "the same answer as for an unknown email");

    let (status, _) = app
        .call(
            "POST",
            "/api/v1/auth/register",
            None,
            Some(json!({"name": "X", "email": "only@example.com", "password": "a long enough passphrase"})),
        )
        .await;
    assert_eq!(status, 409);
}

// -- the callback: everything that goes wrong ------------------------------------------------

#[tokio::test]
async fn a_forged_or_stale_callback_is_stopped_before_anything_is_sent_to_github() {
    let github = FakeGithub::start(octocat(json!([email("a@example.com", true, true)]))).await;
    let app = app_with(&github).await;

    let cases: Vec<(&str, Vec<(&str, &str)>)> = vec![
        ("code=abc&state=wrong", vec![("ss_gh_state", "right")]),
        ("code=abc&state=right", vec![]),
        ("code=abc", vec![("ss_gh_state", "right")]),
        ("code=abc&state=", vec![("ss_gh_state", "right")]),
        ("code=abc&state=righ", vec![("ss_gh_state", "right")]),
        ("code=abc&state=rightt", vec![("ss_gh_state", "right")]),
        ("", vec![]),
    ];
    for (query, cookies) in cases {
        let back = callback(&app, query, &cookies).await;
        assert_eq!(back.status, 302, "{query}");
        assert_eq!(back.location, "/login?github_error=state", "{query}");
        assert!(back.cookie("ss_session").is_none(), "{query}");
    }
    assert!(github.exchanges().is_empty(), "GitHub was never asked");
    assert!(users_collection(&app).await.is_empty());
}

#[tokio::test]
async fn declining_on_github_goes_back_to_the_sign_in_page() {
    let github = FakeGithub::start(octocat(json!([]))).await;
    let app = app_with(&github).await;
    let back = callback(
        &app,
        "error=access_denied&state=s1",
        &[("ss_gh_state", "s1")],
    )
    .await;
    assert_eq!(back.location, "/login?github_error=denied");
    assert!(github.exchanges().is_empty());
}

#[tokio::test]
async fn a_callback_without_a_code_fails_without_asking_github() {
    let github = FakeGithub::start(octocat(json!([]))).await;
    let app = app_with(&github).await;
    let back = callback(&app, "state=s1", &[("ss_gh_state", "s1")]).await;
    assert_eq!(back.location, "/login?github_error=failed");
    assert!(github.exchanges().is_empty());
}

#[tokio::test]
async fn when_github_misbehaves_the_person_is_sent_back_with_a_code_and_nothing_is_created() {
    let github = FakeGithub::start(octocat(json!([email("a@example.com", true, true)]))).await;
    let app = app_with(&github).await;

    github.set(|s| s.token_ok = false);
    assert_eq!(
        sign_in(&app, None, None).await.location,
        "/login?github_error=failed"
    );
    github.set(|s| {
        s.token_ok = true;
        s.user_status = 500;
    });
    assert_eq!(
        sign_in(&app, None, None).await.location,
        "/login?github_error=failed"
    );
    github.set(|s| {
        s.user_status = 200;
        s.emails_status = 403;
    });
    assert_eq!(
        sign_in(&app, None, None).await.location,
        "/login?github_error=failed"
    );
    github.set(|s| {
        s.emails_status = 200;
        s.profile = json!({"id": 5, "login": "../../evil?x=1", "name": "x"});
    });
    assert_eq!(
        sign_in(&app, None, None).await.location,
        "/login?github_error=failed",
        "a login that is not a GitHub username is refused"
    );
    github.set(|s| s.profile = json!({"login": "no-id"}));
    assert_eq!(
        sign_in(&app, None, None).await.location,
        "/login?github_error=failed"
    );
    assert!(users_collection(&app).await.is_empty());
}

#[tokio::test]
async fn github_being_down_does_not_hang_the_callback() {
    let github = FakeGithub::start(octocat(json!([]))).await;
    let app = app_with(&github).await;
    // Nothing listens on the port any more.
    let dead = {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        format!("http://{}", listener.local_addr().unwrap())
    };
    let app_down = TestApp::spawn(
        AppOptions {
            github: Some(dead),
            ..AppOptions::default()
        },
        |routes| routes,
    )
    .await;
    let started = std::time::Instant::now();
    let back = sign_in(&app_down, None, None).await;
    assert_eq!(back.location, "/login?github_error=failed");
    assert!(started.elapsed() < std::time::Duration::from_secs(15));
    drop(app);
}

#[tokio::test]
async fn every_failure_still_clears_the_one_use_cookies_and_never_shows_a_secret() {
    let github = FakeGithub::start(octocat(json!([]))).await;
    let app = app_with(&github).await;
    github.set(|s| s.token_ok = false);
    let back = sign_in(&app, None, Some("/w/x/repos")).await;
    assert_eq!(back.cookie("ss_gh_state").as_deref(), Some(""));
    assert_eq!(back.cookie("ss_gh_next").as_deref(), Some(""));
    let seen = back.everything();
    assert!(!seen.contains(TOKEN) && !seen.contains(CLIENT_SECRET) && !seen.contains("abc123"));
    assert!(back.body.is_empty());
}

#[tokio::test]
async fn a_state_cannot_be_used_twice() {
    let github = FakeGithub::start(octocat(json!([email("a@example.com", true, true)]))).await;
    let app = app_with(&github).await;
    let begun = start(&app, "").await;
    let state = begun.cookie("ss_gh_state").unwrap();
    let query = format!("code=abc&state={state}");
    let first = callback(&app, &query, &[("ss_gh_state", &state)]).await;
    assert_eq!(first.location, "/w");
    // The browser dropped the cleared cookie, so a replay has nothing to match.
    let replay = callback(&app, &query, &[]).await;
    assert_eq!(replay.location, "/login?github_error=state");
    assert_eq!(github.exchanges().len(), 1);
    assert_eq!(github.user_calls(), 1);
}

#[tokio::test]
async fn two_github_ids_cannot_share_one_user_and_users_without_github_can_be_many() {
    let github = FakeGithub::start(octocat(json!([]))).await;
    let app = app_with(&github).await;
    // Many users without a GitHub account: the sparse unique index lets them all in.
    app.account("A").await;
    app.account("B").await;
    app.account("C").await;

    let users = app.db.collection::<Document>("users");
    let first = app.account("D").await;
    let second = app.account("E").await;
    users
        .update_one(doc! {"_id": first.id}, doc! {"$set": {"github_id": 7_i64}})
        .await
        .unwrap();
    let clash = users
        .update_one(doc! {"_id": second.id}, doc! {"$set": {"github_id": 7_i64}})
        .await;
    assert!(
        clash.is_err(),
        "the unique index must refuse a second user with this GitHub id"
    );
}

//! Sign in with GitHub (ADR-015, `docs/API.md` section 1).
//!
//! The browser goes to `GET /auth/github/start`, which sends it to GitHub with a random `state`
//! that is also kept in an HttpOnly cookie. GitHub sends it back to `GET /auth/github/callback`
//! with a `code`; the server trades the code for a token, asks GitHub who the person is, signs
//! them in (or links the account) and redirects to the web app. **The token is used for those
//! two requests and then dropped: it is never stored and never logged.** Only the numeric
//! GitHub id and the login are kept, because the public repositories are listed from the login
//! by the browser, without any credential.
//!
//! Every way this can fail ends in a redirect to `/login?github_error=<code>`, never in a JSON
//! error page, because the person is in the middle of a browser navigation.

use std::collections::HashMap;
use std::time::Duration;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::header::{COOKIE, LOCATION, SET_COOKIE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use mongodb::bson::DateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::validation::{MAX_EMAIL_CHARS, MAX_NAME_CHARS};
use super::{cookie, session};
use crate::app::AppState;
use crate::config::GithubConfig;
use crate::db::users::{self, InsertError, LinkError, User};
use crate::error::ApiError;

const STATE_COOKIE: &str = "ss_gh_state";
const NEXT_COOKIE: &str = "ss_gh_next";
const COOKIE_PATH: &str = "/api/v1/auth/github";
/// A person has ten minutes to approve on GitHub.
const COOKIE_SECONDS: u64 = 600;
/// What GitHub is allowed to take before the sign-in is given up.
const GITHUB_TIMEOUT: Duration = Duration::from_secs(10);
/// Only identity and the list of email addresses. No repository scope: the public repositories
/// need none.
const SCOPE: &str = "read:user user:email";
/// Where a finished sign-in lands when the person did not say where they came from.
const DEFAULT_NEXT: &str = "/w";

/// The answer of `GET /auth/providers`: what the sign-in page may offer.
#[derive(Debug, Serialize, TS)]
#[ts(rename = "AuthProviders")]
pub struct ProvidersResponse {
    pub github: bool,
}

pub async fn providers(State(state): State<AppState>) -> Json<ProvidersResponse> {
    Json(ProvidersResponse {
        github: state.config.github.is_some(),
    })
}

/// 64 random hex characters (two version 4 UUIDs: the operating system's random source).
fn random_state() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// Compares without stopping at the first difference.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |diff, (x, y)| diff | (x ^ y))
            == 0
}

/// A path in this app that the sign-in may return to: it starts with a single `/`, is short and
/// uses only letters, digits, `/`, `-` and `_`. Anything else is ignored.
fn safe_next(raw: Option<&str>) -> Option<&str> {
    let path = raw?;
    let fine = path.starts_with('/')
        && !path.starts_with("//")
        && path.len() <= 200
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_'));
    fine.then_some(path)
}

fn secure_flag(secure: bool) -> &'static str {
    if secure { "; Secure" } else { "" }
}

fn short_cookie(name: &str, value: &str, secure: bool) -> String {
    format!(
        "{name}={value}; Max-Age={COOKIE_SECONDS}; Path={COOKIE_PATH}; HttpOnly; SameSite=Lax{}",
        secure_flag(secure)
    )
}

fn clearing(name: &str, secure: bool) -> String {
    format!(
        "{name}=; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Path={COOKIE_PATH}; HttpOnly; SameSite=Lax{}",
        secure_flag(secure)
    )
}

/// The value of the cookie `name` among the `Cookie` headers.
fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|header| header.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, value)| key.trim() == name && !value.is_empty())
        .map(|(_, value)| value.trim())
}

fn redirect(location: &str, cookies: Vec<String>) -> Response {
    let mut headers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(location) {
        headers.insert(LOCATION, value);
    }
    for cookie in cookies {
        if let Ok(value) = HeaderValue::from_str(&cookie) {
            headers.append(SET_COOKIE, value);
        }
    }
    (StatusCode::FOUND, headers).into_response()
}

#[derive(Debug, Deserialize)]
pub struct StartParams {
    next: Option<String>,
}

/// Sends the browser to GitHub. 404 when sign in with GitHub is not configured.
pub async fn start(
    State(state): State<AppState>,
    Query(params): Query<StartParams>,
) -> Result<Response, ApiError> {
    let Some(github) = &state.config.github else {
        return Err(ApiError::not_found("sign in with GitHub is not set up"));
    };
    let random = random_state();
    let url = reqwest::Url::parse_with_params(
        &format!("{}/login/oauth/authorize", github.oauth_url),
        [
            ("client_id", github.client_id.as_str()),
            ("redirect_uri", callback_url(github).as_str()),
            ("scope", SCOPE),
            ("state", random.as_str()),
        ],
    )
    .map_err(ApiError::internal)?;

    let secure = state.config.cookie_secure;
    let mut cookies = vec![short_cookie(STATE_COOKIE, &random, secure)];
    cookies.push(match safe_next(params.next.as_deref()) {
        Some(next) => short_cookie(NEXT_COOKIE, next, secure),
        None => clearing(NEXT_COOKIE, secure),
    });
    Ok(redirect(url.as_str(), cookies))
}

fn callback_url(github: &GithubConfig) -> String {
    format!("{}/api/v1/auth/github/callback", github.web_url)
}

#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// Why a sign-in did not happen. The code goes to the sign-in page in `?github_error=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Failure {
    /// The person declined on GitHub.
    Denied,
    /// The `state` did not match: an expired page, or a forged request.
    State,
    /// GitHub or the database did not answer as expected.
    Failed,
    /// No verified email address on the GitHub account, and no existing account to link to.
    NoEmail,
    /// The GitHub account or the SyncSpace account is already linked to another one.
    Conflict,
}

impl Failure {
    fn code(self) -> &'static str {
        match self {
            Failure::Denied => "denied",
            Failure::State => "state",
            Failure::Failed => "failed",
            Failure::NoEmail => "no_email",
            Failure::Conflict => "conflict",
        }
    }
}

pub async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<CallbackParams>,
) -> Response {
    let secure = state.config.cookie_secure;
    let mut cookies = vec![
        clearing(STATE_COOKIE, secure),
        clearing(NEXT_COOKIE, secure),
    ];
    match finish(&state, &headers, params).await {
        Ok(user) => {
            let next = safe_next(cookie_value(&headers, NEXT_COOKIE)).unwrap_or(DEFAULT_NEXT);
            match session::issue(state.config.jwt_secret.expose(), user.id, session::now()) {
                Ok(token) => {
                    cookies.push(cookie::session_cookie(&token, secure));
                    redirect(next, cookies)
                }
                Err(error) => {
                    tracing::error!(%error, "could not issue a session after GitHub sign-in");
                    redirect("/login?github_error=failed", cookies)
                }
            }
        }
        Err(failure) => redirect(&format!("/login?github_error={}", failure.code()), cookies),
    }
}

/// The part of GitHub's answers the server needs.
#[derive(Debug, Deserialize)]
struct TokenAnswer {
    access_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GithubUser {
    id: i64,
    login: String,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GithubEmail {
    email: String,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    verified: bool,
}

/// A GitHub username: letters, digits and single hyphens, at most 39. It ends up in a URL in the
/// browser, so nothing else is accepted.
fn valid_login(login: &str) -> bool {
    !login.is_empty()
        && login.len() <= 39
        && login
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// The verified addresses of the account, the primary one first, lower-cased.
fn verified_emails(mut emails: Vec<GithubEmail>) -> Vec<String> {
    emails.sort_by_key(|e| !e.primary);
    let mut result: Vec<String> = Vec::new();
    for entry in emails.into_iter().filter(|e| e.verified) {
        let email = entry.email.trim().to_lowercase();
        if email.contains('@') && email.len() <= MAX_EMAIL_CHARS && !result.contains(&email) {
            result.push(email);
        }
    }
    result
}

async fn finish(
    state: &AppState,
    headers: &HeaderMap,
    params: CallbackParams,
) -> Result<User, Failure> {
    let github = state.config.github.as_ref().ok_or(Failure::Failed)?;

    let expected = cookie_value(headers, STATE_COOKIE).ok_or(Failure::State)?;
    let given = params.state.as_deref().ok_or(Failure::State)?;
    if !same(expected, given) {
        return Err(Failure::State);
    }
    if params.error.is_some() {
        return Err(Failure::Denied);
    }
    let code = params
        .code
        .as_deref()
        .filter(|c| !c.is_empty())
        .ok_or(Failure::Failed)?;

    let (profile, emails) = ask_github(github, code).await?;
    if !valid_login(&profile.login) {
        tracing::warn!("GitHub sent a login that is not a GitHub username");
        return Err(Failure::Failed);
    }
    let emails = verified_emails(emails);

    let current = cookie::find_session_token(
        headers
            .get_all(COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok()),
    )
    .and_then(|token| session::verify(state.config.jwt_secret.expose(), token).ok());

    resolve(state, &profile, &emails, current).await
}

/// reqwest is built without a TLS provider of its own; the process uses `ring`, the one the
/// MongoDB driver already uses. Installing it twice is harmless.
fn install_crypto_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Trades the code for a token and asks who the person is. The token lives in this function.
async fn ask_github(
    github: &GithubConfig,
    code: &str,
) -> Result<(GithubUser, Vec<GithubEmail>), Failure> {
    install_crypto_provider();
    let client = reqwest::Client::builder()
        .timeout(GITHUB_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("syncspace")
        .build()
        .map_err(|error| {
            tracing::error!(%error, "could not build the HTTP client for GitHub");
            Failure::Failed
        })?;

    let redirect_uri = callback_url(github);
    let exchange: HashMap<&str, &str> = HashMap::from([
        ("client_id", github.client_id.as_str()),
        ("client_secret", github.client_secret.expose().as_str()),
        ("code", code),
        ("redirect_uri", redirect_uri.as_str()),
    ]);
    let answer = client
        .post(format!("{}/login/oauth/access_token", github.oauth_url))
        .header("Accept", "application/json")
        .json(&exchange)
        .send()
        .await
        .map_err(|_| {
            tracing::warn!("GitHub did not answer the token request");
            Failure::Failed
        })?;
    if !answer.status().is_success() {
        tracing::warn!(
            status = answer.status().as_u16(),
            "GitHub refused the token request"
        );
        return Err(Failure::Failed);
    }
    let token = answer
        .json::<TokenAnswer>()
        .await
        .ok()
        .and_then(|t| t.access_token)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            tracing::warn!("GitHub gave no access token for the code");
            Failure::Failed
        })?;

    let get = |path: &str| {
        client
            .get(format!("{}{path}", github.api_url))
            .bearer_auth(&token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
    };
    let profile = get("/user").send().await.map_err(|_| Failure::Failed)?;
    if !profile.status().is_success() {
        tracing::warn!(status = profile.status().as_u16(), "GitHub refused /user");
        return Err(Failure::Failed);
    }
    let profile: GithubUser = profile.json().await.map_err(|_| Failure::Failed)?;

    let emails = get("/user/emails")
        .send()
        .await
        .map_err(|_| Failure::Failed)?;
    if !emails.status().is_success() {
        tracing::warn!(
            status = emails.status().as_u16(),
            "GitHub refused /user/emails"
        );
        return Err(Failure::Failed);
    }
    let emails: Vec<GithubEmail> = emails.json().await.map_err(|_| Failure::Failed)?;
    Ok((profile, emails))
}

/// Which SyncSpace user this GitHub account is: the one already linked to it; else, when
/// somebody is signed in, that person (a link); else the user with one of its verified email
/// addresses (a link); else a new user.
async fn resolve(
    state: &AppState,
    profile: &GithubUser,
    emails: &[String],
    current: Option<ObjectId>,
) -> Result<User, Failure> {
    let db = &state.db;
    let database = |error: mongodb::error::Error| {
        tracing::error!(%error, "database error during GitHub sign-in");
        Failure::Failed
    };

    if let Some(user) = users::find_by_github_id(db, profile.id)
        .await
        .map_err(database)?
    {
        if current.is_some_and(|id| id != user.id) {
            return Err(Failure::Conflict);
        }
        return linked(db, user, profile).await;
    }

    if let Some(id) = current {
        let user = users::find_by_id(db, id)
            .await
            .map_err(database)?
            .ok_or(Failure::Failed)?;
        return linked(db, user, profile).await;
    }

    for email in emails {
        if let Some(user) = users::find_by_email(db, email).await.map_err(database)? {
            return linked(db, user, profile).await;
        }
    }

    let email = emails.first().ok_or(Failure::NoEmail)?.clone();
    let name = profile
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(&profile.login)
        .chars()
        .take(MAX_NAME_CHARS)
        .collect();
    let user = User {
        id: ObjectId::new(),
        email,
        name,
        password_hash: None,
        github_id: Some(profile.id),
        github_login: Some(profile.login.clone()),
        created_at: DateTime::now(),
    };
    match users::insert(db, &user).await {
        Ok(()) => Ok(user),
        // A parallel sign-in made the account first: the person tries again.
        Err(InsertError::DuplicateEmail) => Err(Failure::Failed),
        Err(InsertError::Database(error)) => Err(database(error)),
    }
}

/// Links `profile` to `user` and returns the user as stored afterwards. A user that already has
/// a different GitHub account, or a GitHub account that another user has, is a conflict.
async fn linked(
    db: &mongodb::Database,
    mut user: User,
    profile: &GithubUser,
) -> Result<User, Failure> {
    match users::link_github(db, user.id, profile.id, &profile.login).await {
        Ok(true) => {
            user.github_id = Some(profile.id);
            user.github_login = Some(profile.login.clone());
            Ok(user)
        }
        Ok(false) | Err(LinkError::AlreadyLinked) => Err(Failure::Conflict),
        Err(LinkError::Database(error)) => {
            tracing::error!(%error, "database error while linking a GitHub account");
            Err(Failure::Failed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_compares_whole_strings() {
        assert!(same("abc", "abc"));
        assert!(!same("abc", "abd"));
        assert!(!same("abc", "abcd"));
        assert!(!same("", "a"));
        assert!(same("", ""));
    }

    #[test]
    fn next_must_be_a_plain_path_of_this_app() {
        for good in ["/w", "/w/eng/repos", "/w/my-team_1/docs"] {
            assert_eq!(safe_next(Some(good)), Some(good));
        }
        for bad in [
            "",
            "w/eng",
            "//evil.example",
            "/\\evil",
            "https://evil.example",
            "/w?x=1",
            "/w#frag",
            "/w/a b",
            "/w/%2e%2e",
            "/w/\u{e9}",
        ] {
            assert_eq!(safe_next(Some(bad)), None, "{bad}");
        }
        assert_eq!(safe_next(Some(&format!("/{}", "a".repeat(200)))), None);
        assert_eq!(safe_next(None), None);
    }

    #[test]
    fn logins_are_github_usernames() {
        for good in ["octocat", "a", "Some-One-1", &"a".repeat(39)] {
            assert!(valid_login(good), "{good}");
        }
        for bad in [
            "",
            "a b",
            "a/b",
            "a_b",
            "../x",
            "a?b",
            "\u{e9}",
            &"a".repeat(40),
        ] {
            assert!(!valid_login(bad), "{bad}");
        }
    }

    #[test]
    fn only_verified_addresses_count_and_the_primary_comes_first() {
        let list = vec![
            GithubEmail {
                email: "Other@Example.com".into(),
                primary: false,
                verified: true,
            },
            GithubEmail {
                email: "unverified@example.com".into(),
                primary: false,
                verified: false,
            },
            GithubEmail {
                email: "Main@Example.com".into(),
                primary: true,
                verified: true,
            },
            GithubEmail {
                email: "other@example.com".into(),
                primary: false,
                verified: true,
            },
            GithubEmail {
                email: "not-an-address".into(),
                primary: false,
                verified: true,
            },
        ];
        assert_eq!(
            verified_emails(list),
            vec![
                "main@example.com".to_string(),
                "other@example.com".to_string()
            ]
        );
    }

    #[test]
    fn the_state_cookie_is_short_lived_scoped_and_http_only() {
        let cookie = short_cookie(STATE_COOKIE, "abc", true);
        assert_eq!(
            cookie,
            "ss_gh_state=abc; Max-Age=600; Path=/api/v1/auth/github; HttpOnly; SameSite=Lax; Secure"
        );
        assert!(!short_cookie(STATE_COOKIE, "abc", false).contains("Secure"));
        assert!(clearing(STATE_COOKIE, false).contains("Max-Age=0"));
    }

    #[test]
    fn random_states_are_long_and_different() {
        let (a, b) = (random_state(), random_state());
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn a_cookie_is_found_among_several() {
        let mut headers = HeaderMap::new();
        headers.append(
            COOKIE,
            HeaderValue::from_static("a=1; ss_gh_state=xyz; b=2"),
        );
        assert_eq!(cookie_value(&headers, "ss_gh_state"), Some("xyz"));
        assert_eq!(cookie_value(&headers, "missing"), None);
        let mut empty = HeaderMap::new();
        empty.append(COOKIE, HeaderValue::from_static("ss_gh_state="));
        assert_eq!(cookie_value(&empty, "ss_gh_state"), None);
    }
}

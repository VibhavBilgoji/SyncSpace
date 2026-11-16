//! The four auth endpoints. Nothing here logs an email, a password, a token or a hash.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header::SET_COOKIE;
use axum::response::{IntoResponse, Response};
use mongodb::bson::DateTime;
use mongodb::bson::oid::ObjectId;
use serde::Serialize;

use super::extractor::{AuthUser, not_signed_in};
use super::validation::{
    LoginInput, MAX_EMAIL_CHARS, MAX_PASSWORD_CHARS, RegisterInput, validate_registration,
};
use super::{cookie, session};
use crate::app::AppState;
use crate::db::users::{self, InsertError, User};
use crate::error::ApiError;
use crate::extract::ApiJson;

/// The `User` of `docs/API.md` section 4. It never carries the password hash.
#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: String,
    pub name: String,
    pub email: String,
}

impl From<&User> for UserResponse {
    fn from(user: &User) -> Self {
        Self {
            id: user.id.to_hex(),
            name: user.name.clone(),
            email: user.email.clone(),
        }
    }
}

/// The answer to every failed login, whatever the reason.
fn invalid_credentials() -> ApiError {
    ApiError::unauthorized("invalid email or password")
}

/// `status`, the user, and a `Set-Cookie` that starts a session for them.
fn signed_in(state: &AppState, status: StatusCode, user: &User) -> Result<Response, ApiError> {
    let token = session::issue(state.config.jwt_secret.expose(), user.id, session::now())
        .map_err(ApiError::internal)?;
    let cookie = cookie::session_cookie(&token, state.config.cookie_secure);
    Ok((
        status,
        [(SET_COOKIE, cookie)],
        Json(UserResponse::from(user)),
    )
        .into_response())
}

pub async fn register(
    State(state): State<AppState>,
    ApiJson(input): ApiJson<RegisterInput>,
) -> Result<Response, ApiError> {
    let registration = validate_registration(input)?;

    // Check first, so a repeated registration costs a lookup and not a 19 MiB hash. The
    // unique index below still settles two requests that race.
    if users::find_by_email(&state.db, &registration.email)
        .await?
        .is_some()
    {
        return Err(email_taken());
    }

    let user = User {
        id: ObjectId::new(),
        email: registration.email,
        name: registration.name,
        password_hash: state.auth.hash(registration.password).await?,
        created_at: DateTime::now(),
    };
    match users::insert(&state.db, &user).await {
        Ok(()) => signed_in(&state, StatusCode::CREATED, &user),
        Err(InsertError::DuplicateEmail) => Err(email_taken()),
        Err(InsertError::Database(error)) => Err(ApiError::internal(error)),
    }
}

fn email_taken() -> ApiError {
    ApiError::conflict("an account with this email already exists")
}

pub async fn login(
    State(state): State<AppState>,
    ApiJson(input): ApiJson<LoginInput>,
) -> Result<Response, ApiError> {
    let email = input.email.trim().to_lowercase();

    // No registered password is longer than this, so there is nothing to compare against and
    // nothing worth spending 19 MiB on.
    if email.len() > MAX_EMAIL_CHARS || input.password.chars().count() > MAX_PASSWORD_CHARS {
        return Err(invalid_credentials());
    }

    let user = match users::find_by_email(&state.db, &email).await? {
        Some(user) => user,
        None => {
            // Take as long as a real check, so the response time does not reveal that the
            // account does not exist.
            state.auth.verify_dummy(input.password).await?;
            return Err(invalid_credentials());
        }
    };
    if !state
        .auth
        .verify(input.password, user.password_hash.clone())
        .await?
    {
        return Err(invalid_credentials());
    }
    signed_in(&state, StatusCode::OK, &user)
}

/// Ends the session on the client. The token itself stays valid until it expires: it is
/// stateless and cannot be revoked (see `session`).
pub async fn logout(State(state): State<AppState>, _user: AuthUser) -> Response {
    let cookie = cookie::clearing_cookie(state.config.cookie_secure);
    (StatusCode::NO_CONTENT, [(SET_COOKIE, cookie)]).into_response()
}

pub async fn me(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<UserResponse>, ApiError> {
    // A valid token for an account that no longer exists is not a session.
    match users::find_by_id(&state.db, user.id).await? {
        Some(found) => Ok(Json(UserResponse::from(&found))),
        None => Err(not_signed_in()),
    }
}

//! `AuthUser`: the signed-in user of a request, for routes that need one.
//!
//! The extractor only checks the token; it does not touch the database, so guarding a route
//! costs a signature check. A handler that needs the user's data loads it itself.

use axum::extract::FromRequestParts;
use axum::http::header::COOKIE;
use axum::http::request::Parts;
use mongodb::bson::oid::ObjectId;

use super::{cookie, session};
use crate::app::AppState;
use crate::error::ApiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthUser {
    pub id: ObjectId,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let cookies = parts
            .headers
            .get_all(COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok());
        let token = cookie::find_session_token(cookies).ok_or_else(not_signed_in)?;
        session::verify(state.config.jwt_secret.expose(), token)
            .map(|id| AuthUser { id })
            .map_err(|_| not_signed_in())
    }
}

/// One answer for a missing, expired, forged or malformed session: the client learns nothing
/// about which it was.
pub fn not_signed_in() -> ApiError {
    ApiError::unauthorized("not signed in")
}

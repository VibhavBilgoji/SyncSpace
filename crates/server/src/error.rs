//! The one error type for REST handlers, and the one error format (`docs/API.md` section 1):
//! `{"error": {"code": "...", "message": "...", "details": {...}?}}`.
//!
//! Handlers return `Result<_, ApiError>`. Client-caused errors carry a message written by the
//! server code. An internal error keeps its real cause for the log only: the client always
//! receives the same generic message, so no stack trace, query or host name leaves the server
//! (`docs/ARCHITECTURE.md` section 5).

use std::collections::BTreeMap;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug)]
pub enum ApiError {
    /// 400: malformed JSON or id.
    BadRequest(String),
    /// 401: missing or expired session.
    Unauthorized(String),
    /// 403: not a member, or role too low.
    Forbidden(String),
    /// 404: unknown resource, or one in a workspace the caller is not in.
    NotFound(String),
    /// 409: duplicate email or slug, member already present, owner change attempt.
    Conflict(String),
    /// 422: field rules failed; maps each field to its message.
    Validation(BTreeMap<String, String>),
    /// 429.
    RateLimited,
    /// 413: the request body is over the limit.
    PayloadTooLarge,
    /// 405.
    MethodNotAllowed,
    /// 500: something the client cannot fix. The cause is logged, never returned.
    Internal(BoxError),
}

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'static str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<&'a BTreeMap<String, String>>,
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::Unauthorized(message.into())
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden(message.into())
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    /// One entry per field that failed, for example `[("email", "must be a valid address")]`.
    pub fn validation<K, V>(fields: impl IntoIterator<Item = (K, V)>) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        Self::Validation(
            fields
                .into_iter()
                .map(|(field, message)| (field.into(), message.into()))
                .collect(),
        )
    }

    pub fn internal(cause: impl Into<BoxError>) -> Self {
        Self::Internal(cause.into())
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "BAD_REQUEST",
            Self::Unauthorized(_) => "UNAUTHORIZED",
            Self::Forbidden(_) => "FORBIDDEN",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Conflict(_) => "CONFLICT",
            Self::Validation(_) => "VALIDATION",
            Self::RateLimited => "RATE_LIMITED",
            Self::PayloadTooLarge => "PAYLOAD_TOO_LARGE",
            Self::MethodNotAllowed => "METHOD_NOT_ALLOWED",
            Self::Internal(_) => "INTERNAL",
        }
    }

    /// The text a client may see. For `Internal` it is deliberately generic.
    pub fn message(&self) -> &str {
        match self {
            Self::BadRequest(message)
            | Self::Unauthorized(message)
            | Self::Forbidden(message)
            | Self::NotFound(message)
            | Self::Conflict(message) => message,
            Self::Validation(_) => "validation failed",
            Self::RateLimited => "too many requests",
            Self::PayloadTooLarge => "request body too large",
            Self::MethodNotAllowed => "method not allowed",
            Self::Internal(_) => "internal server error",
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let Self::Internal(cause) = &self {
            // The only place a cause is shown, and it goes to the log, not the client.
            tracing::error!(error = %error_chain(cause.as_ref()), "internal server error");
        }
        let details = match &self {
            Self::Validation(fields) => Some(fields),
            _ => None,
        };
        let body = ErrorEnvelope {
            error: ErrorBody {
                code: self.code(),
                message: self.message(),
                details,
            },
        };
        (self.status(), Json(body)).into_response()
    }
}

/// `outer: inner: innermost`, so the log shows the whole chain.
fn error_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(next) = source {
        text.push_str(": ");
        text.push_str(&next.to_string());
        source = next.source();
    }
    text
}

impl From<mongodb::error::Error> for ApiError {
    fn from(error: mongodb::error::Error) -> Self {
        Self::internal(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn render(error: ApiError) -> (StatusCode, serde_json::Value) {
        let response = error.into_response();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn each_variant_has_the_status_and_code_from_the_api_table() {
        let cases = [
            (ApiError::bad_request("x"), 400, "BAD_REQUEST"),
            (ApiError::unauthorized("x"), 401, "UNAUTHORIZED"),
            (ApiError::forbidden("x"), 403, "FORBIDDEN"),
            (ApiError::not_found("x"), 404, "NOT_FOUND"),
            (ApiError::conflict("x"), 409, "CONFLICT"),
            (ApiError::validation([("f", "m")]), 422, "VALIDATION"),
            (ApiError::RateLimited, 429, "RATE_LIMITED"),
            (ApiError::PayloadTooLarge, 413, "PAYLOAD_TOO_LARGE"),
            (ApiError::MethodNotAllowed, 405, "METHOD_NOT_ALLOWED"),
            (ApiError::internal("boom"), 500, "INTERNAL"),
        ];
        for (error, status, code) in cases {
            let (got_status, body) = render(error).await;
            assert_eq!(got_status.as_u16(), status, "{code}");
            assert_eq!(body["error"]["code"], code);
            assert!(body["error"]["message"].is_string(), "{code}");
        }
    }

    #[tokio::test]
    async fn the_body_has_exactly_the_documented_shape() {
        let (_, body) = render(ApiError::not_found("workspace not found")).await;
        assert_eq!(
            body,
            serde_json::json!({"error": {"code": "NOT_FOUND", "message": "workspace not found"}})
        );
    }

    #[tokio::test]
    async fn validation_errors_carry_details_per_field() {
        let (status, body) = render(ApiError::validation([
            ("email", "invalid"),
            ("name", "too long"),
        ]))
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            body["error"]["details"],
            serde_json::json!({"email": "invalid", "name": "too long"})
        );
    }

    #[tokio::test]
    async fn details_are_left_out_of_other_errors() {
        let (_, body) = render(ApiError::conflict("duplicate")).await;
        assert!(body["error"].get("details").is_none());
    }

    #[tokio::test]
    async fn internal_errors_hide_their_cause() {
        let cause = std::io::Error::other("connect to db.internal:27017 as admin failed");
        let (status, body) = render(ApiError::internal(cause)).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["error"]["message"], "internal server error");
        let shown = body.to_string();
        assert!(!shown.contains("db.internal"), "{shown}");
        assert!(!shown.contains("admin"), "{shown}");
    }

    #[test]
    fn error_chain_walks_every_source() {
        #[derive(Debug)]
        struct Outer(std::io::Error);
        impl std::fmt::Display for Outer {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("outer")
            }
        }
        impl std::error::Error for Outer {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }
        let chain = error_chain(&Outer(std::io::Error::other("inner")));
        assert_eq!(chain, "outer: inner");
    }
}

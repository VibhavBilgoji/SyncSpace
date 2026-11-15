//! Request extractors that fail with [`ApiError`], so a bad request body gets the same JSON
//! error format as every other failure instead of axum's plain-text defaults.

use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use serde::de::DeserializeOwned;

use crate::error::ApiError;

/// Longest serde message echoed back in `details`.
const MAX_DETAIL_CHARS: usize = 200;

/// A JSON body. Failures map to the `docs/API.md` table:
///
/// | Problem | Response |
/// |---|---|
/// | not JSON, truncated JSON, or a body that cannot be read | 400 `BAD_REQUEST` |
/// | missing or wrong `Content-Type` | 400 `BAD_REQUEST` |
/// | valid JSON of the wrong shape (missing field, wrong type) | 422 `VALIDATION` |
/// | body over the size limit | 413 `PAYLOAD_TOO_LARGE` |
#[derive(Debug)]
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(request, state).await {
            Ok(axum::Json(value)) => Ok(Self(value)),
            Err(rejection) => Err(map_rejection(rejection)),
        }
    }
}

fn map_rejection(rejection: JsonRejection) -> ApiError {
    match rejection {
        JsonRejection::JsonSyntaxError(_) => {
            ApiError::bad_request("request body is not valid JSON")
        }
        JsonRejection::MissingJsonContentType(_) => {
            ApiError::bad_request("expected Content-Type: application/json")
        }
        JsonRejection::JsonDataError(error) => {
            let message: String = error.body_text().chars().take(MAX_DETAIL_CHARS).collect();
            ApiError::validation([("body", message)])
        }
        // Reading the body failed: either it was over the limit or the connection broke.
        other if other.status() == StatusCode::PAYLOAD_TOO_LARGE => ApiError::PayloadTooLarge,
        _ => ApiError::bad_request("could not read the request body"),
    }
}

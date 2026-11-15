//! `GET /healthz`: is the server up, and can it reach MongoDB?
//!
//! Public, no session. Render calls it as the health check, and the web app polls it while a
//! sleeping free instance wakes up (`docs/ARCHITECTURE.md` section 6). The answer never says
//! why MongoDB is unreachable: the cause goes to the log.

use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::app::AppState;
use crate::db;

/// A ping slower than this counts as MongoDB being unavailable.
const PING_TIMEOUT: Duration = Duration::from_secs(2);

pub async fn healthz(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    match tokio::time::timeout(PING_TIMEOUT, db::ping(&state.db)).await {
        Ok(Ok(())) => (StatusCode::OK, Json(json!({"status": "ok", "mongo": "ok"}))),
        Ok(Err(error)) => {
            tracing::warn!(%error, "health check: MongoDB ping failed");
            degraded()
        }
        Err(_) => {
            tracing::warn!("health check: MongoDB ping timed out");
            degraded()
        }
    }
}

fn degraded() -> (StatusCode, Json<Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({"status": "degraded", "mongo": "error"})),
    )
}

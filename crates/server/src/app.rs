//! The HTTP application: shared state, routes, middleware and the serve loop.
//!
//! Tests build the same app as production. [`routes`] returns the unfinished router and
//! [`apply_layers`] wraps it, so a test can add its own routes (a route that panics, one that
//! echoes JSON) between the two and still get the real middleware around them.

use std::any::Any;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, Method, Request, Response, header};
use axum::response::IntoResponse;
use axum::routing::get;
use mongodb::Database;
use tokio::net::TcpListener;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::cors::{AllowCredentials, AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::auth::password::HashError;
use crate::auth::{self, AuthService};
use crate::config::Config;
use crate::error::ApiError;
use crate::health;

/// Largest request body on REST routes (`docs/ARCHITECTURE.md` section 5). WebSocket frames
/// have their own, larger limit.
pub const MAX_BODY_BYTES: usize = 256 * 1024;

/// How long a browser may cache a CORS preflight answer.
const CORS_MAX_AGE: Duration = Duration::from_secs(600);

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub config: Arc<Config>,
    pub auth: Arc<AuthService>,
}

impl AppState {
    /// The state with the default password-hashing limits.
    pub fn new(db: Database, config: Arc<Config>) -> Result<Self, HashError> {
        Ok(Self {
            db,
            config,
            auth: Arc::new(AuthService::new()?),
        })
    }
}

/// Routes without middleware.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(health::healthz))
        .nest("/api/v1", api_routes())
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
}

/// Everything under `/api/v1`.
fn api_routes() -> Router<AppState> {
    Router::new().nest("/auth", auth::routes())
}

/// The production app: routes plus middleware.
pub fn router(state: AppState) -> Router {
    let config = state.config.clone();
    apply_layers(routes().with_state(state), &config)
}

/// Wraps a router in the middleware, outermost first: request logging, CORS, the body limit,
/// panic catching.
///
/// CORS sits outside the rest so that 404, 413 and 500 answers carry its headers too, and the
/// panic catcher is innermost so a panicking handler becomes a 500 in the normal error format
/// that the outer layers still see. Panics on client input are bugs (CLAUDE.md rule 3); this is
/// the safety net, not the plan.
pub fn apply_layers(router: Router, config: &Config) -> Router {
    let router = router
        .layer(CatchPanicLayer::custom(panic_response))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(cors_layer(&config.allowed_origins));
    with_request_logging(router)
}

async fn not_found() -> ApiError {
    ApiError::not_found("not found")
}

async fn method_not_allowed() -> ApiError {
    ApiError::MethodNotAllowed
}

fn panic_response(panic: Box<dyn Any + Send + 'static>) -> Response<Body> {
    let what = panic
        .downcast_ref::<&str>()
        .map(|message| (*message).to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "non-string panic payload".to_string());
    ApiError::internal(format!("handler panicked: {what}")).into_response()
}

/// CORS for the configured origins only, with credentials because the session is a cookie.
/// Credentials are granted to those same origins and to nobody else, so a response to a foreign
/// origin carries no CORS header at all. Methods and headers are listed because a wildcard is
/// not allowed together with credentials.
fn cors_layer(origins: &[String]) -> CorsLayer {
    let allowed: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect();
    let credentials_for = allowed.clone();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed))
        .allow_credentials(AllowCredentials::predicate(move |origin, _request| {
            credentials_for.contains(origin)
        }))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE])
        .max_age(CORS_MAX_AGE)
}

/// Adds one log line per response with method, path, status and latency. The query string is
/// deliberately left out (WebSocket tickets travel in it), and so are headers and bodies
/// (cookies, passwords): `docs/ARCHITECTURE.md` section 5.
fn with_request_logging(router: Router) -> Router {
    router.layer(
        TraceLayer::new_for_http()
            .make_span_with(|request: &Request<Body>| {
                tracing::info_span!(
                    "request",
                    method = %request.method(),
                    path = %request.uri().path(),
                )
            })
            .on_request(())
            .on_response(
                |response: &Response<Body>, latency: Duration, _span: &tracing::Span| {
                    tracing::info!(
                        status = response.status().as_u16(),
                        latency_ms = u64::try_from(latency.as_millis()).unwrap_or(u64::MAX),
                        "response"
                    );
                },
            ),
    )
}

/// Serves `router` until `shutdown` completes, then stops accepting connections and lets
/// requests that are already running finish.
pub async fn serve(
    listener: TcpListener,
    router: Router,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await
}

/// Completes on Ctrl-C, or on SIGTERM where that exists (Render stops a service with SIGTERM).
pub async fn shutdown_signal() {
    let ctrl_c = async {
        // If the handler cannot be installed there is nothing to wait for: never complete.
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}

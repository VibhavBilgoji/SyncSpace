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
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderValue, Method, Request, Response, header};
use axum::response::IntoResponse;
use axum::routing::{MethodRouter, delete, get, patch, post};
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
use crate::hub::{Hub, HubConfig};
use crate::rbac::Access;

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
    pub hub: Arc<Hub>,
}

impl AppState {
    /// The state with the default password-hashing limits.
    pub fn new(db: Database, config: Arc<Config>) -> Result<Self, HashError> {
        Ok(Self {
            db,
            config,
            auth: Arc::new(AuthService::new()?),
            hub: Arc::new(Hub::new(HubConfig::default())),
        })
    }
}

/// Routes without middleware.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/ws/v1/docs/{id}", get(crate::hub::socket::doc_socket))
        .route(
            "/ws/v1/workspaces/{id}",
            get(crate::hub::workspace_socket::workspace_socket),
        )
        .route("/metrics", get(metrics_page))
        .nest("/api/v1", api_routes())
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
}

/// One REST route: what it is, who may call it, and the handler.
///
/// The router under `/api/v1` is built from [`api_route_table`], so a route cannot exist without
/// an entry here, and the RBAC matrix test (`tests/rbac_matrix.rs`) walks the same table: it
/// checks every entry against the Role column of `docs/API.md` and calls it as each kind of
/// caller. `access` is what the route promises; the handler enforces it with
/// [`crate::rbac::require_role`] and the test proves the two agree.
pub struct RouteSpec {
    pub method: &'static str,
    /// Relative to `/api/v1`, in axum syntax (`{wid}`).
    pub path: &'static str,
    pub access: Access,
    handler: fn() -> MethodRouter<AppState>,
}

impl RouteSpec {
    const fn new(
        method: &'static str,
        path: &'static str,
        access: Access,
        handler: fn() -> MethodRouter<AppState>,
    ) -> Self {
        Self {
            method,
            path,
            access,
            handler,
        }
    }
}

/// Every REST route under `/api/v1`.
pub fn api_route_table() -> Vec<RouteSpec> {
    use crate::documents::handlers as d;
    use crate::rbac::Role::{Admin, Editor, Viewer};
    use crate::workspaces::handlers as w;
    vec![
        RouteSpec::new("POST", "/auth/register", Access::Public, || {
            post(auth::handlers::register)
        }),
        RouteSpec::new("POST", "/auth/login", Access::Public, || {
            post(auth::handlers::login)
        }),
        RouteSpec::new("POST", "/auth/logout", Access::User, || {
            post(auth::handlers::logout)
        }),
        RouteSpec::new("POST", "/auth/ws-ticket", Access::Member(Viewer), || {
            post(auth::ws_ticket::ws_ticket)
        }),
        RouteSpec::new("GET", "/auth/me", Access::User, || get(auth::handlers::me)),
        RouteSpec::new("GET", "/auth/providers", Access::Public, || {
            get(auth::github::providers)
        }),
        RouteSpec::new("GET", "/auth/github/start", Access::Public, || {
            get(auth::github::start)
        }),
        RouteSpec::new("GET", "/auth/github/callback", Access::Public, || {
            get(auth::github::callback)
        }),
        RouteSpec::new("GET", "/workspaces", Access::User, || {
            get(w::list_workspaces)
        }),
        RouteSpec::new("POST", "/workspaces", Access::User, || {
            post(w::create_workspace)
        }),
        RouteSpec::new("GET", "/workspaces/{wid}", Access::Member(Viewer), || {
            get(w::get_workspace)
        }),
        RouteSpec::new("PATCH", "/workspaces/{wid}", Access::Member(Admin), || {
            patch(w::patch_workspace)
        }),
        RouteSpec::new(
            "GET",
            "/workspaces/{wid}/members",
            Access::Member(Viewer),
            || get(w::list_members),
        ),
        RouteSpec::new(
            "POST",
            "/workspaces/{wid}/members",
            Access::Member(Admin),
            || post(w::add_member),
        ),
        RouteSpec::new(
            "PATCH",
            "/workspaces/{wid}/members/{uid}",
            Access::Member(Admin),
            || patch(w::change_member_role),
        ),
        RouteSpec::new(
            "DELETE",
            "/workspaces/{wid}/members/{uid}",
            Access::Member(Admin),
            || delete(w::remove_member),
        ),
        RouteSpec::new(
            "GET",
            "/workspaces/{wid}/documents",
            Access::Member(Viewer),
            || get(d::list_documents),
        ),
        RouteSpec::new(
            "POST",
            "/workspaces/{wid}/documents",
            Access::Member(Editor),
            || post(d::create_document),
        ),
        RouteSpec::new("GET", "/documents/{id}", Access::Member(Viewer), || {
            get(d::get_document)
        }),
        RouteSpec::new("PATCH", "/documents/{id}", Access::Member(Editor), || {
            patch(d::patch_document)
        }),
        RouteSpec::new("DELETE", "/documents/{id}", Access::Member(Admin), || {
            delete(d::delete_document)
        }),
        RouteSpec::new(
            "GET",
            "/documents/{id}/versions",
            Access::Member(Viewer),
            || get(crate::versions::handlers::list_versions),
        ),
        RouteSpec::new(
            "POST",
            "/documents/{id}/versions",
            Access::Member(Editor),
            || post(crate::versions::handlers::save_version),
        ),
        RouteSpec::new(
            "POST",
            "/documents/{id}/versions/{n}/restore",
            Access::Member(Editor),
            || post(crate::versions::handlers::restore_version),
        ),
        RouteSpec::new(
            "GET",
            "/documents/{id}/versions/{n}",
            Access::Member(Viewer),
            || get(crate::versions::handlers::get_version),
        ),
        RouteSpec::new(
            "GET",
            "/workspaces/{wid}/audit",
            Access::Member(Admin),
            || get(crate::audit_api::list_audit),
        ),
        RouteSpec::new(
            "GET",
            "/workspaces/{wid}/boards",
            Access::Member(Viewer),
            || get(crate::boards::handlers::list_boards),
        ),
        RouteSpec::new(
            "POST",
            "/workspaces/{wid}/boards",
            Access::Member(Editor),
            || post(crate::boards::handlers::create_board),
        ),
        RouteSpec::new("GET", "/boards/{bid}/tasks", Access::Member(Viewer), || {
            get(crate::boards::handlers::list_tasks)
        }),
        RouteSpec::new(
            "POST",
            "/boards/{bid}/tasks",
            Access::Member(Editor),
            || post(crate::boards::handlers::create_task),
        ),
        RouteSpec::new("PATCH", "/tasks/{id}", Access::Member(Editor), || {
            patch(crate::boards::handlers::patch_task)
        }),
        RouteSpec::new("PATCH", "/tasks/{id}/move", Access::Member(Editor), || {
            patch(crate::boards::handlers::move_task)
        }),
        RouteSpec::new("DELETE", "/tasks/{id}", Access::Member(Editor), || {
            delete(crate::boards::handlers::delete_task)
        }),
    ]
}

/// Everything under `/api/v1`, built from [`api_route_table`]. Entries with the same path (one
/// per method) merge into one route.
fn api_routes() -> Router<AppState> {
    api_route_table()
        .into_iter()
        .fold(Router::new(), |router, spec| {
            router.route(spec.path, (spec.handler)())
        })
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

/// The counters of `crate::metrics`, as text. Counts only, no identifiers.
async fn metrics_page(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    // Unset token: the endpoint does not exist. Wrong or missing token: the same 404, so the
    // page cannot be told apart from any other unknown route.
    let allowed = state.config.metrics_token.as_ref().is_some_and(|token| {
        headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .is_some_and(|given| same_secret(given.as_bytes(), token.expose().as_bytes()))
    });
    if !allowed {
        return Err(ApiError::not_found("not found"));
    }
    let body = state
        .hub
        .metrics
        .render(state.hub.open_room_list().len(), state.hub.open_sockets());
    Ok(([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], body))
}

/// Compares in time that does not depend on where the first difference is.
fn same_secret(a: &[u8], b: &[u8]) -> bool {
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= usize::from(a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0));
    }
    diff == 0
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

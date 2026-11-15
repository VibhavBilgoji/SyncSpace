//! Logging setup. Output goes to stdout through `tracing` (Render and Docker collect it
//! from there); `RUST_LOG` picks the level (`info` when unset). What gets logged, and what must not, is decided where the log lines are
//! written (see `app::with_request_logging`).

use tracing_subscriber::EnvFilter;

/// Installs the global subscriber. Calling it twice is harmless: the second call does nothing.
pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .try_init();
}

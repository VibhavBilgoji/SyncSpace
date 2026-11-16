//! SyncSpace API server.
//!
//! Start-up order: read the configuration, start logging, connect to MongoDB, create the
//! indexes, listen. Any failure before listening ends the process with a message and a
//! non-zero exit code, so a bad deploy shows up at once instead of at the first request:
//! exit code 2 for configuration problems, 1 for everything else.

use std::process::ExitCode;

use server::app::{self, AppState};
use server::config::Config;
use server::{db, telemetry};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> ExitCode {
    // A local `.env` is a convenience for development; in production the real environment is
    // used. It is fine for the file to be missing.
    let _ = dotenvy::dotenv();

    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("The server cannot start, the configuration has problems:\n{error}");
            return ExitCode::from(2);
        }
    };

    telemetry::init();

    match run(config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "the server stopped with an error");
            eprintln!("The server stopped with an error: {error}");
            ExitCode::from(1)
        }
    }
}

async fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    // Never log secrets: only these fields are written.
    tracing::info!(
        port = config.port,
        database = %config.mongodb_db,
        origins = ?config.allowed_origins,
        cookie_secure = config.cookie_secure,
        "starting"
    );

    let database = db::connect(&config).await?;
    db::ping(&database).await?;
    db::ensure_indexes(&database).await?;
    tracing::info!("connected to MongoDB, indexes are in place");

    let port = config.port;
    let state = AppState::new(database, std::sync::Arc::new(config))?;
    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(address = %listener.local_addr()?, "listening");

    // Write the open documents every few seconds, and again on the way out: the shutdown signal
    // first writes every changed document and sends the sockets away, because the graceful
    // shutdown of the HTTP server waits for open connections.
    let hub = state.hub.clone();
    let snapshot_db = state.db.clone();
    let worker = server::hub::snapshot::spawn(
        hub.clone(),
        snapshot_db.clone(),
        std::time::Duration::from_secs(state.config.snapshot_interval_secs),
    );
    let stopping = async move {
        app::shutdown_signal().await;
        worker.abort();
        hub.shutdown(&snapshot_db).await;
    };
    app::serve(listener, app::router(state), stopping).await?;
    tracing::info!("shut down cleanly");
    Ok(())
}

//! Helpers shared by the integration tests.
//!
//! A test that needs MongoDB gets its own database, `syncspace_test_<uuid>`, on the server
//! named by `MONGODB_URI` (default `mongodb://localhost:27017`: a local `mongod`, the compose
//! service, or the CI service container). The database is dropped when the test ends, even
//! when it fails, so tests can run in parallel and leave nothing behind
//! (`docs/TESTING.md` section 1). A test that does not need MongoDB uses
//! [`TestApp::without_mongo`], which points the app at a port nobody listens on.

#![allow(dead_code)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use axum::Router;
use mongodb::Database;
use server::app::{self, AppState};
use server::auth::AuthService;
use server::config::Config;
use server::db;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing_subscriber::fmt::MakeWriter;

/// The MongoDB major version production runs. Atlas free clusters run 8.0 (Atlas docs, "Atlas
/// Free Cluster Limits"), and local, CI and docker-compose use the same major so a test cannot
/// pass on a version production does not have. When Atlas moves, change this, the CI service
/// image and `infrastructure/docker-compose.yml` together (`docs/DEPLOYMENT.md`).
pub const EXPECTED_MONGODB_MAJOR: i32 = 8;

/// 32 bytes of 0x01 and of 0x02, base64 encoded: valid, different signing keys.
pub const KEY_A: &str = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=";
pub const KEY_B: &str = "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=";

pub const ALLOWED_ORIGIN: &str = "https://app.example";

/// A server nobody listens on, with short timeouts so a test fails fast instead of waiting.
pub const UNREACHABLE_URI: &str =
    "mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=300&connectTimeoutMS=300";

pub fn mongodb_uri() -> String {
    std::env::var("MONGODB_URI").unwrap_or_else(|_| "mongodb://localhost:27017".to_string())
}

pub fn env_map(uri: &str, db_name: &str, origins: &str) -> HashMap<String, String> {
    [
        ("MONGODB_URI", uri),
        ("MONGODB_DB", db_name),
        ("JWT_SECRET", KEY_A),
        ("TICKET_SECRET", KEY_B),
        ("ALLOWED_ORIGINS", origins),
        ("COOKIE_SECURE", "false"),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_string(), value.to_string()))
    .collect()
}

pub fn test_config(uri: &str, db_name: &str, origins: &str) -> Config {
    Config::from_map(&env_map(uri, db_name, origins)).unwrap()
}

fn unique_db_name() -> String {
    format!("syncspace_test_{}", uuid::Uuid::new_v4().simple())
}

/// A database that exists only for one test.
pub struct TestDb {
    pub db: Database,
    pub name: String,
}

impl TestDb {
    /// Connects to the real MongoDB, or fails with instructions.
    pub async fn connect() -> Self {
        let name = unique_db_name();
        let config = test_config(&mongodb_uri(), &name, ALLOWED_ORIGIN);
        let db = db::connect(&config).await.unwrap();
        if let Err(error) = db::ping(&db).await {
            panic!(
                "These tests need a running MongoDB, but it is not reachable: {error}\n\
                 Start MongoDB (docs/DEPLOYMENT.md section 10) or set MONGODB_URI."
            );
        }
        Self { db, name }
    }
}

impl TestDb {
    /// Takes ownership of a database that something else (the real binary) creates, so it is
    /// dropped when the test ends.
    pub async fn named(name: &str) -> Self {
        let config = test_config(&mongodb_uri(), name, ALLOWED_ORIGIN);
        let db = db::connect(&config).await.unwrap();
        Self {
            db,
            name: name.to_string(),
        }
    }

    pub fn unique_name() -> String {
        unique_db_name()
    }
}

impl Drop for TestDb {
    /// Drops the database on a helper thread with its own runtime and client: the test's own
    /// runtime may already be shutting down, and a failed test must not leave data behind.
    fn drop(&mut self) {
        let name = self.name.clone();
        let uri = mongodb_uri();
        let cleanup = std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            runtime.block_on(async {
                let config = test_config(&uri, &name, ALLOWED_ORIGIN);
                if let Ok(db) = db::connect(&config).await {
                    let _ = db.drop().await;
                }
            });
        });
        let _ = cleanup.join();
    }
}

pub enum MongoMode {
    /// A real database of its own.
    Real,
    /// A port nobody listens on.
    Unreachable,
}

pub struct AppOptions {
    pub mongo: MongoMode,
    /// The `ALLOWED_ORIGINS` value.
    pub origins: &'static str,
    /// The `COOKIE_SECURE` value.
    pub cookie_secure: bool,
    /// Password-hashing capacity and how long a request waits for it (the defaults when `None`).
    pub hash_limits: Option<(usize, Duration)>,
}

impl Default for AppOptions {
    fn default() -> Self {
        Self {
            mongo: MongoMode::Real,
            origins: ALLOWED_ORIGIN,
            cookie_secure: false,
            hash_limits: None,
        }
    }
}

/// The real application on an ephemeral port.
pub struct TestApp {
    pub addr: SocketAddr,
    pub client: reqwest::Client,
    pub db: Database,
    /// The password-hashing service the app uses, so a test can exhaust its capacity.
    pub auth: Arc<AuthService>,
    _test_db: Option<TestDb>,
    shutdown: Option<oneshot::Sender<()>>,
    server: Option<JoinHandle<std::io::Result<()>>>,
}

impl TestApp {
    /// With a real MongoDB and only the production routes.
    pub async fn real() -> Self {
        Self::spawn(AppOptions::default(), |routes| routes).await
    }

    /// Without MongoDB, and only the production routes.
    pub async fn without_mongo() -> Self {
        Self::spawn(
            AppOptions {
                mongo: MongoMode::Unreachable,
                ..AppOptions::default()
            },
            |routes| routes,
        )
        .await
    }

    /// `extra` may add test-only routes. They get the same middleware as the real ones.
    pub async fn spawn(
        options: AppOptions,
        extra: impl FnOnce(Router<AppState>) -> Router<AppState>,
    ) -> Self {
        let secure = options.cookie_secure.to_string();
        let config_for = |uri: &str, db_name: &str| {
            let mut vars = env_map(uri, db_name, options.origins);
            vars.insert("COOKIE_SECURE".to_string(), secure.clone());
            Config::from_map(&vars).unwrap()
        };
        let (test_db, config) = match options.mongo {
            MongoMode::Real => {
                let test_db = TestDb::connect().await;
                let config = config_for(&mongodb_uri(), &test_db.name);
                (Some(test_db), config)
            }
            MongoMode::Unreachable => (
                None,
                config_for(UNREACHABLE_URI, "syncspace_test_unreachable"),
            ),
        };
        let db = db::connect(&config).await.unwrap();
        if test_db.is_some() {
            db::ensure_indexes(&db).await.unwrap();
        }

        let mut state = AppState::new(db.clone(), Arc::new(config.clone())).unwrap();
        if let Some((permits, wait)) = options.hash_limits {
            state.auth = Arc::new(AuthService::with_limits(permits, wait).unwrap());
        }
        let auth = state.auth.clone();
        let router = app::apply_layers(extra(app::routes()).with_state(state), &config);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (shutdown, signal) = oneshot::channel::<()>();
        let server = tokio::spawn(app::serve(listener, router, async move {
            let _ = signal.await;
        }));

        Self {
            addr,
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            db,
            auth,
            _test_db: test_db,
            shutdown: Some(shutdown),
            server: Some(server),
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    /// Stops the server and waits for it, to prove shutdown works and not leave a task behind.
    pub async fn stop(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(server) = self.server.take() {
            let _ = server.await;
        }
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

/// What `tracing` wrote in this test process, shared by every test in it.
static LOG: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new();

/// The shared log buffer. The first call installs a process-wide subscriber that writes to it.
///
/// A subscriber per test thread would be simpler, but `tracing` remembers per call site whether
/// anyone listens, and a test thread that reaches a call site first with no subscriber can
/// make it deaf for the others. One subscriber for the whole process cannot be raced.
fn shared_log() -> Arc<Mutex<Vec<u8>>> {
    LOG.get_or_init(|| {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_writer(LogSink(buffer.clone()))
            .with_ansi(false)
            .with_max_level(tracing::Level::DEBUG)
            .finish();
        // Another subscriber may already be set in this process; then there is nothing to do.
        let _ = tracing::subscriber::set_global_default(subscriber);
        buffer
    })
    .clone()
}

#[derive(Clone)]
struct LogSink(Arc<Mutex<Vec<u8>>>);

pub struct LogWriter(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for LogSink {
    type Writer = LogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LogWriter(self.0.clone())
    }
}

/// A view of the log from the moment it was started.
///
/// Tests run in parallel and share the log, so lines of other tests can appear in it. Assert
/// that strings only this test produces are present, and that strings that must never be
/// logged are absent; both stay true whatever else is logged meanwhile.
pub struct LogCapture {
    buffer: Arc<Mutex<Vec<u8>>>,
    from: usize,
}

impl LogCapture {
    pub fn start() -> Self {
        let buffer = shared_log();
        let from = buffer.lock().unwrap().len();
        Self { buffer, from }
    }

    pub fn text(&self) -> String {
        let bytes = self.buffer.lock().unwrap();
        String::from_utf8_lossy(&bytes[self.from..]).into_owned()
    }
}

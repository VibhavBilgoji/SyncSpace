//! Process-level tests: they run the real `server` binary, so they cover `main.rs` itself, the
//! exit codes and what the process prints. Only `starts_and_serves_healthz` needs a MongoDB.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use common::{KEY_A, KEY_B, TestDb, mongodb_uri};
use mongodb::bson::Document;
use serde_json::{Value, json};

const SERVER: &str = env!("CARGO_BIN_EXE_server");

/// A server process with an empty environment, started away from any `.env` file.
fn server_command() -> Command {
    let mut command = Command::new(SERVER);
    command
        .env_clear()
        .current_dir(std::env::temp_dir())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Windows programs expect this one variable.
    if let Some(root) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", root);
    }
    command
}

fn valid_env(command: &mut Command, uri: &str) {
    command
        .env("MONGODB_URI", uri)
        .env("JWT_SECRET", KEY_A)
        .env("TICKET_SECRET", KEY_B)
        .env("ALLOWED_ORIGINS", "https://app.example")
        .env("COOKIE_SECURE", "false");
}

/// Runs a process that is expected to exit by itself, and kills it if it does not.
fn run_to_end(mut command: Command, limit: Duration) -> Output {
    let mut child = command.spawn().unwrap();
    let started = Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if started.elapsed() > limit {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!(
                "the server did not exit within {limit:?}\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn exits_2_and_lists_every_missing_variable() {
    let output = run_to_end(server_command(), Duration::from_secs(15));
    assert_eq!(output.status.code(), Some(2));
    let stderr = text(&output.stderr);
    assert!(stderr.contains("configuration has problems"), "{stderr}");
    for name in [
        "MONGODB_URI",
        "JWT_SECRET",
        "TICKET_SECRET",
        "ALLOWED_ORIGINS",
    ] {
        assert!(stderr.contains(name), "{name} is not reported: {stderr}");
    }
}

#[test]
fn a_bad_secret_is_reported_without_echoing_it() {
    let mut command = server_command();
    valid_env(&mut command, "mongodb://127.0.0.1:27017");
    command.env("JWT_SECRET", "hunter2-is-not-base64!");
    let output = run_to_end(command, Duration::from_secs(15));
    assert_eq!(output.status.code(), Some(2));
    let everything = format!("{}{}", text(&output.stdout), text(&output.stderr));
    assert!(everything.contains("JWT_SECRET"), "{everything}");
    assert!(!everything.contains("hunter2"), "{everything}");
}

#[test]
fn exits_1_when_mongodb_is_unreachable_and_never_prints_the_password() {
    let mut command = server_command();
    valid_env(
        &mut command,
        "mongodb://appuser:hunter2@127.0.0.1:1/?serverSelectionTimeoutMS=500&connectTimeoutMS=500",
    );
    let output = run_to_end(command, Duration::from_secs(30));
    // 1 is a start-up failure, 2 would mean the configuration was rejected.
    assert_eq!(output.status.code(), Some(1));
    let stderr = text(&output.stderr);
    assert!(stderr.contains("stopped with an error"), "{stderr}");
    let everything = format!("{}{stderr}", text(&output.stdout));
    assert!(
        !everything.contains("hunter2"),
        "the password leaked:\n{everything}"
    );
}

/// Kills the server when the test ends, also when it fails.
struct Running(Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// (mongo)
#[tokio::test]
async fn starts_and_serves_healthz() {
    let db_name = TestDb::unique_name();
    // Dropped at the end of the test: the server below creates this database.
    let test_db = TestDb::named(&db_name).await;

    let port = {
        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        probe.local_addr().unwrap().port()
    };
    let mut command = server_command();
    valid_env(&mut command, &mongodb_uri());
    command
        .env("MONGODB_DB", &db_name)
        .env("PORT", port.to_string())
        .env("RUST_LOG", "info");
    let server = Running(command.spawn().unwrap());

    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/healthz");
    let started = Instant::now();
    let response = loop {
        match client.get(&url).send().await {
            Ok(response) => break response,
            Err(error) if started.elapsed() > Duration::from_secs(30) => {
                panic!("the server never answered: {error}")
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    };
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"status": "ok", "mongo": "ok"})
    );

    // Start-up created the indexes in the configured database.
    let users = test_db.db.collection::<Document>("users");
    let indexes: Vec<_> =
        futures_util::TryStreamExt::try_collect(users.list_indexes().await.unwrap())
            .await
            .unwrap();
    assert_eq!(indexes.len(), 2, "_id_ and the unique email index");

    drop(server);
}

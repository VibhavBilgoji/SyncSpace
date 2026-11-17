//! Creates the demo workspace: `cargo run -p server --bin seed -- --workspace "Engineering Team"`.
//!
//! Reads `MONGODB_URI` (required) and `MONGODB_DB` (default `syncspace`) from the environment or a
//! `.env` file; the other server settings are not needed. The three demo accounts get the
//! password of `--password` or `SEED_PASSWORD`, or a random one that is printed once. Running it
//! again for the same workspace changes nothing and says so.

use std::process::ExitCode;

use mongodb::options::ClientOptions;
use server::seed::{SeedError, SeedOptions, SeedReport, seed};

const USAGE: &str = "usage: seed [--workspace <name>] [--password <password>]";

fn parse_args() -> Result<(String, Option<String>), String> {
    let mut workspace = "Engineering Team".to_string();
    let mut password = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--workspace" => workspace = args.next().ok_or("--workspace needs a value")?,
            "--password" => password = Some(args.next().ok_or("--password needs a value")?),
            "--help" | "-h" => return Err(USAGE.to_string()),
            other => return Err(format!("unknown argument {other}\n{USAGE}")),
        }
    }
    Ok((workspace, password))
}

/// 16 letters and digits from the operating system's random source (two UUIDs).
fn random_password() -> String {
    let a = uuid::Uuid::new_v4().simple().to_string();
    a.chars().take(16).collect()
}

/// Connects, makes sure the indexes exist and seeds.
async fn run(
    uri: &str,
    database_name: &str,
    workspace_name: String,
    password: String,
) -> Result<SeedReport, SeedError> {
    let mut options = ClientOptions::parse(uri).await?;
    options.app_name = Some("syncspace-seed".to_string());
    let client = mongodb::Client::with_options(options)?;
    let db = client.database(database_name);
    server::db::ping(&db).await?;
    server::db::ensure_indexes(&db).await?;
    seed(
        &db,
        &SeedOptions {
            workspace_name,
            password,
        },
    )
    .await
}

#[tokio::main]
async fn main() -> ExitCode {
    let _ = dotenvy::dotenv();
    let (workspace_name, password) = match parse_args() {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let Ok(uri) = std::env::var("MONGODB_URI") else {
        eprintln!("MONGODB_URI is not set (put it in the environment or in .env)");
        return ExitCode::from(2);
    };
    let database_name = std::env::var("MONGODB_DB").unwrap_or_else(|_| "syncspace".to_string());
    let password = password
        .or_else(|| std::env::var("SEED_PASSWORD").ok())
        .filter(|p| !p.is_empty());
    let generated = password.is_none();
    let password = password.unwrap_or_else(random_password);

    let result = run(&uri, &database_name, workspace_name, password.clone()).await;

    match result {
        Ok(report) => {
            println!(
                "Created workspace \"{}\" ({}) in database \"{database_name}\".",
                report.slug, report.workspace_id
            );
            println!(
                "{} documents with {} saved versions, {} tasks on one board, {} audit rows.\n",
                report.documents, report.versions, report.tasks, report.audit_rows
            );
            println!("Sign in at /login with:");
            for user in &report.users {
                let note = if user.created {
                    ""
                } else {
                    "  (account existed: its password was not changed)"
                };
                println!(
                    "  {:<16} {:<26} {}{note}",
                    user.name,
                    user.email,
                    user.role.as_str()
                );
            }
            if report.users.iter().any(|u| u.created) {
                let shown = if generated {
                    &password
                } else {
                    "the password you gave"
                };
                println!("\nPassword of the new accounts: {shown}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("The seed did not run: {error}");
            ExitCode::FAILURE
        }
    }
}

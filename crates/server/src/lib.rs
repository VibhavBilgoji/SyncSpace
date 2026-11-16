//! SyncSpace API server library. `main.rs` wires these modules into the real process; the
//! integration tests build the same app from them.

pub mod api_types;
pub mod app;
pub mod audit;
pub mod auth;
pub mod config;
pub mod db;
pub mod documents;
pub mod error;
pub mod extract;
pub mod health;
pub mod rbac;
pub mod spike;
pub mod telemetry;
pub mod typegen;
pub mod workspaces;

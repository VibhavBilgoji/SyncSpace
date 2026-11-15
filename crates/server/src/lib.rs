//! SyncSpace API server library. `main.rs` wires these modules into the real process; the
//! integration tests build the same app from them.

pub mod app;
pub mod config;
pub mod db;
pub mod error;
pub mod extract;
pub mod health;
pub mod spike;
pub mod telemetry;

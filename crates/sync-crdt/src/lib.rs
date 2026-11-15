//! Shared CRDT logic for SyncSpace.
//!
//! Built natively for the server and to `wasm32-unknown-unknown` for the browser,
//! so both sides run the same Yrs code. Task 0.3 is a spike with one shared text;
//! the block schema from `docs/DATA_MODEL.md` §6 arrives in Phase 2.

mod doc;
pub mod parity;
#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use doc::{CrdtError, MAX_UPDATE_BYTES, SpikeDoc};

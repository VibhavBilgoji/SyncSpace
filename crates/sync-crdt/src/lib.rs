//! Shared CRDT logic for SyncSpace.
//!
//! Built natively for the server and to `wasm32-unknown-unknown` for the browser,
//! so both sides run the same Yrs code. Task 0.3 is a spike with one shared text;
//! the block schema from `docs/DATA_MODEL.md` §6 arrives in Phase 2.

// `SpikeDoc::apply_update` relies on `catch_unwind` to turn a Yrs panic into an error, which
// only works when panics unwind. Building natively with `panic = "abort"` (for example from a
// release profile) would turn one hostile update into a crash, so refuse to compile. Wasm
// always aborts and has no `catch_unwind`; there the server validates updates before they
// reach a browser.
#[cfg(all(panic = "abort", not(target_arch = "wasm32")))]
compile_error!("sync-crdt needs panic = \"unwind\" on native targets (see SpikeDoc::apply_update)");

mod doc;
pub mod parity;
#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use doc::{CrdtError, MAX_UPDATE_BYTES, SpikeDoc};

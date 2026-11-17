//! Shared CRDT logic for SyncSpace.
//!
//! Built natively for the server and to `wasm32-unknown-unknown` for the browser,
//! so both sides run the same Yrs code.
//!
//! [`Document`] is the block document of `docs/DATA_MODEL.md` section 6 (task 2.1): local edits
//! in `ops`, reading in `projection`, y-sync messages in `sync`, and the defences against hostile
//! updates in `validate`. [`SpikeDoc`] is the one-text spike of task 0.3, kept until the hub and
//! the editor replace it.

// `SpikeDoc::apply_update` relies on `catch_unwind` to turn a Yrs panic into an error, which
// only works when panics unwind. Building natively with `panic = "abort"` (for example from a
// release profile) would turn one hostile update into a crash, so refuse to compile. Wasm
// always aborts and has no `catch_unwind`; there the server validates updates before they
// reach a browser.
#[cfg(all(panic = "abort", not(target_arch = "wasm32")))]
compile_error!("sync-crdt needs panic = \"unwind\" on native targets (see SpikeDoc::apply_update)");

mod doc;
mod document;
mod error;
mod ops;
pub mod parity;
mod projection;
pub mod schema;
mod sync;
mod validate;
#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use doc::SpikeDoc;
pub use document::Document;
pub use error::CrdtError;
pub use projection::Block;
pub use schema::{BlockType, order_between};
pub use sync::{
    AwarenessEntry, FrameKind, Handled, MAX_AWARENESS_BYTES, awareness_entries, awareness_frame,
    check_state_vector, frame_kind, is_empty_update, update_frame,
};
pub use validate::MAX_UPDATE_BYTES;

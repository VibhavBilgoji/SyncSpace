//! Defences against hostile input, shared by every document type of the crate.
//!
//! Updates reach the server from the network, so decoding and applying them must never panic
//! and never leave a document half changed (CLAUDE.md rule 3). Yrs 0.28.1 has several ways to
//! panic or to keep a poisoned block on hostile input, found by the property tests of task 0.3:
//!
//! - A `debug_assert!` that a decoded client id fits in 53 bits makes garbage panic in debug
//!   builds. Decoding runs before the document is touched, so catching the panic is safe.
//! - `block_store.rs:320` subtracts two `u32` clocks without a check. Release builds only panic
//!   there because the workspace `Cargo.toml` turns overflow checks on for `yrs`.
//! - A mutated update can decode and apply but leave a block in the pending queue that cannot be
//!   re-encoded, and the next `encode_state_as_update` then panics (`merge_pending_v1`).
//!
//! The answer is to try an update on a throwaway copy first ([`rehearse`]): apply the document's
//! own state and then the update, re-encode (what every joining client triggers) and, for block
//! documents, check the resulting structure ([`check_structure`]). Only when all of that works is
//! the update applied to the real document. Replaying the same update on the same state is
//! deterministic, so a clean rehearsal means the real apply is safe.
//!
//! A block document also **refuses updates that leave anything pending**
//! ([`CrdtError::MissingDependencies`]): an update that builds on changes the document does not
//! have. Yrs keeps such updates in a pending queue, and that queue is both where the earlier
//! panics lived and where re-encoding loses data: found while testing task 2.1, a document with a
//! pending `set_type` re-encodes into a state from which the scratch copy ends up without the
//! block's `type` key, although applying the same updates directly to the real document is fine
//! (checked in every one of 24 delivery orders). Without a pending queue the document is always
//! plain integrated state, so the scratch copy is faithful and the structure check cannot reject
//! a legitimate update. The cost is that a sender must deliver updates in causal order, which
//! the hub does (it applies and broadcasts in arrival order) and a reconnecting client does by
//! sending the diff against the server's state vector (SyncStep1/SyncStep2).
//!
//! `catch_unwind` cannot work on Wasm, where panics abort; there updates were already validated
//! by the server, so the rehearsal is native only.

use std::sync::Arc;

use yrs::block::ClientID;

use yrs::updates::decoder::Decode;
#[cfg(not(target_arch = "wasm32"))]
use yrs::{Any, Map, Out, ReadTxn};
use yrs::{Doc, OffsetKind, Options, Update};
#[cfg(not(target_arch = "wasm32"))]
use yrs::{StateVector, Transact};

use crate::error::CrdtError;
#[cfg(not(target_arch = "wasm32"))]
use crate::projection::parse_block;
#[cfg(not(target_arch = "wasm32"))]
use crate::schema::{
    BLOCKS_ROOT, KEY_SCHEMA, MAX_BLOCKS, META_ROOT, SCHEMA_VERSION, check_block_id,
};

/// Largest update a document accepts. Matches the 1 MiB frame limit in `docs/API.md`.
pub const MAX_UPDATE_BYTES: usize = 1024 * 1024;

/// Yjs client ids are limited to 53 bits so they survive a JavaScript number.
pub(crate) const MAX_CLIENT_ID: u64 = 1 << 53;

/// Client id of the scratch replica used by [`rehearse`]. It never writes.
#[cfg(not(target_arch = "wasm32"))]
const SCRATCH_CLIENT_ID: u64 = 2;

/// A Yrs document with UTF-16 offsets (like JavaScript strings). The guid only names the
/// document and is not part of an update; a fixed one also avoids Yrs' random UUID, which needs
/// OS entropy that `wasm32-unknown-unknown` does not have.
pub(crate) fn new_yrs_doc(guid: &str, client_id: u64) -> Result<Doc, CrdtError> {
    if client_id >= MAX_CLIENT_ID {
        return Err(CrdtError::InvalidClientId);
    }
    let mut options = Options::with_guid_and_client_id(Arc::from(guid), ClientID::new(client_id));
    options.offset_kind = OffsetKind::Utf16;
    Ok(Doc::with_options(options))
}

/// Decodes a v1 update without ever panicking on hostile bytes, and only returns updates that
/// survive an encode and decode round trip (see the module comment).
pub(crate) fn decode_update(bytes: &[u8]) -> Result<Update, CrdtError> {
    fn decode_and_round_trip(bytes: &[u8]) -> Result<Update, yrs::encoding::read::Error> {
        use yrs::updates::encoder::Encode;
        let update = Update::decode_v1(bytes)?;
        Update::decode_v1(&update.encode_v1())?;
        Ok(update)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        std::panic::catch_unwind(|| decode_and_round_trip(bytes))
            .map_err(|_| CrdtError::Decode)?
            .map_err(|_| CrdtError::Decode)
    }
    #[cfg(target_arch = "wasm32")]
    {
        decode_and_round_trip(bytes).map_err(|_| CrdtError::Decode)
    }
}

/// Applies `real`'s state and then `bytes` to a scratch document, re-encodes it, and runs
/// `check` on the result. `init` registers the root types on the scratch document, exactly as
/// the real document did when it was created.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn rehearse(
    real: &Doc,
    guid: &str,
    init: fn(&Doc),
    check: fn(&Doc) -> Result<(), CrdtError>,
    reject_pending: bool,
    bytes: &[u8],
) -> Result<(), CrdtError> {
    let state = {
        let txn = real.transact();
        txn.encode_state_as_update_v1(&StateVector::default())
    };
    std::panic::catch_unwind(|| -> Result<(), CrdtError> {
        let scratch = new_yrs_doc(guid, SCRATCH_CLIENT_ID)?;
        init(&scratch);
        for chunk in [state.as_slice(), bytes] {
            let update = Update::decode_v1(chunk).map_err(|_| CrdtError::Decode)?;
            let mut txn = scratch.transact_mut();
            txn.apply_update(update).map_err(|_| CrdtError::Apply)?;
        }
        // Joining clients get the encoded state, so it must also survive encoding.
        {
            let txn = scratch.transact();
            if reject_pending && txn.has_missing_updates() {
                return Err(CrdtError::MissingDependencies);
            }
            let _ = txn.encode_state_as_update_v1(&StateVector::default());
        }
        check(&scratch)
    })
    .map_err(|_| CrdtError::Apply)?
}

/// A name from untrusted input, shortened for an error message.
#[cfg(not(target_arch = "wasm32"))]
fn clip(text: &str) -> String {
    text.chars().take(40).collect()
}

/// Checks that a block document has the structure of `docs/DATA_MODEL.md` section 6 and stays
/// within its limits: only the roots `blocks` and `meta`, at most [`MAX_BLOCKS`] blocks, and every
/// block with a valid id, type, finite order, text within the cap and properties that belong to
/// its type. Anything else is a hostile or broken update.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn check_structure(doc: &Doc) -> Result<(), CrdtError> {
    use yrs::Transact;
    let invalid = |why: String| CrdtError::Invalid(why);
    let txn = doc.transact();

    for (name, _) in txn.root_refs() {
        if name != BLOCKS_ROOT && name != META_ROOT {
            return Err(invalid(format!("unknown root type `{}`", clip(name))));
        }
    }

    if let Some(meta) = txn.get_map(META_ROOT) {
        for (key, value) in meta.iter(&txn) {
            let ok = key == KEY_SCHEMA
                && matches!(&value, Out::Any(Any::Number(n)) if n.as_f64() == Some(SCHEMA_VERSION));
            if !ok {
                return Err(invalid(format!("unexpected `meta.{}`", clip(key))));
            }
        }
    }

    let Some(blocks) = txn.get_map(BLOCKS_ROOT) else {
        return Ok(());
    };
    let mut count = 0usize;
    for (id, value) in blocks.iter(&txn) {
        count += 1;
        if count > MAX_BLOCKS {
            return Err(invalid(format!("more than {MAX_BLOCKS} blocks")));
        }
        check_block_id(id).map_err(|_| invalid(format!("bad block id `{}`", clip(id))))?;
        let Out::YMap(map) = value else {
            return Err(invalid(format!("block `{id}` is not a map")));
        };
        parse_block(&txn, id, &map).map_err(|why| invalid(format!("block `{id}`: {why}")))?;
    }
    Ok(())
}

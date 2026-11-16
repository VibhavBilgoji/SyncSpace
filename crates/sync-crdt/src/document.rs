//! A document of blocks: the replica the server keeps per open document and the browser edits.
//!
//! The local edits are in [`crate::ops`], the read side in [`crate::projection`] and the y-sync
//! messages in [`crate::sync`]. This file holds the type, its state encoding and the guarded way
//! to apply an update from a peer.

use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{Doc, Map, MapRef, ReadTxn, StateVector, Transact};

use crate::error::CrdtError;
use crate::schema::{BLOCKS_ROOT, KEY_SCHEMA, META_ROOT, SCHEMA_VERSION};
use crate::validate::{MAX_UPDATE_BYTES, decode_update, new_yrs_doc};

/// Every replica uses the same guid.
const DOC_GUID: &str = "syncspace-document";

/// One replica of a block document. Offsets are UTF-16 code units, like JavaScript strings.
pub struct Document {
    pub(crate) doc: Doc,
    pub(crate) blocks: MapRef,
    pub(crate) meta: MapRef,
}

/// Registers the two root types. The scratch replica of the validation does the same.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn init_roots(doc: &Doc) {
    doc.get_or_insert_map(BLOCKS_ROOT);
    doc.get_or_insert_map(META_ROOT);
}

impl Document {
    /// Creates a replica. `client_id` must be unique per live replica: two replicas that share
    /// an id corrupt the document. The caller supplies it because the Wasm build has no source of
    /// randomness of its own.
    pub fn new(client_id: u64) -> Result<Self, CrdtError> {
        let doc = new_yrs_doc(DOC_GUID, client_id)?;
        let blocks = doc.get_or_insert_map(BLOCKS_ROOT);
        let meta = doc.get_or_insert_map(META_ROOT);
        Ok(Self { doc, blocks, meta })
    }

    /// Writes `meta.schema = 1` if it is not there yet and returns the update. The server calls
    /// this once, when a document is created; replicas that load a stored document do not.
    pub fn initialise(&self) -> Vec<u8> {
        let already = {
            let txn = self.doc.transact();
            self.meta.contains_key(&txn, KEY_SCHEMA)
        };
        let mut txn = self.doc.transact_mut();
        if !already {
            self.meta.insert(&mut txn, KEY_SCHEMA, SCHEMA_VERSION);
        }
        txn.encode_update_v1()
    }

    /// What this replica has seen, as an encoded Yrs v1 state vector. It does not change when an
    /// update is rejected.
    pub fn state_vector(&self) -> Vec<u8> {
        let txn = self.doc.transact();
        txn.state_vector().encode_v1()
    }

    /// Whether `other` holds exactly the same changes and deletions (a Yjs snapshot comparison).
    /// This is the right test for "converged": two replicas that merged the same updates can
    /// still encode their state to different bytes, because the ranges of deleted structs may be
    /// split differently depending on the order the updates arrived in.
    pub fn same_state_as(&self, other: &Document) -> bool {
        let mine = self.doc.transact();
        let theirs = other.doc.transact();
        mine.snapshot() == theirs.snapshot()
    }

    /// The whole document as one update. A new peer applies it to catch up.
    pub fn encode_state_as_update(&self) -> Vec<u8> {
        let txn = self.doc.transact();
        txn.encode_state_as_update_v1(&StateVector::default())
    }

    /// What a peer with the given state vector is missing.
    pub fn encode_state_as_update_from(&self, state_vector: &[u8]) -> Result<Vec<u8>, CrdtError> {
        let state_vector = decode_state_vector(state_vector)?;
        let txn = self.doc.transact();
        Ok(txn.encode_state_as_update_v1(&state_vector))
    }

    /// Applies an update from a peer. Never panics on bad input, and a rejected update leaves the
    /// document exactly as it was (see [`crate::validate`]). On native targets the update is
    /// first tried on a scratch copy and the resulting structure is checked against the schema.
    pub fn apply_update(&self, bytes: &[u8]) -> Result<(), CrdtError> {
        if bytes.len() > MAX_UPDATE_BYTES {
            return Err(CrdtError::TooLarge(bytes.len()));
        }
        let update = decode_update(bytes)?;
        #[cfg(not(target_arch = "wasm32"))]
        crate::validate::rehearse(
            &self.doc,
            DOC_GUID,
            init_roots,
            crate::validate::check_structure,
            true,
            bytes,
        )?;
        let mut txn = self.doc.transact_mut();
        txn.apply_update(update).map_err(|_| CrdtError::Apply)
    }
}

/// Decodes a state vector from the network without panicking.
pub(crate) fn decode_state_vector(bytes: &[u8]) -> Result<StateVector, CrdtError> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::panic::catch_unwind(|| StateVector::decode_v1(bytes))
            .map_err(|_| CrdtError::Decode)?
            .map_err(|_| CrdtError::Decode)
    }
    #[cfg(target_arch = "wasm32")]
    {
        StateVector::decode_v1(bytes).map_err(|_| CrdtError::Decode)
    }
}

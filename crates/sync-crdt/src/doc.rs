//! A document holding one shared `Y.Text` named `"text"`.

use yrs::updates::encoder::Encode;
use yrs::{Doc, GetString, ReadTxn, StateVector, Text, TextRef, Transact};

pub use crate::error::CrdtError;
use crate::validate::{MAX_UPDATE_BYTES, decode_update, new_yrs_doc};

/// Name of the shared text inside the Y.Doc.
const TEXT_NAME: &str = "text";

/// Every replica uses the same guid. It only names the document; it is not part of an update.
const DOC_GUID: &str = "syncspace-spike";

#[cfg(not(target_arch = "wasm32"))]
fn init_text(doc: &Doc) {
    doc.get_or_insert_text(TEXT_NAME);
}

/// One replica of the shared text. Offsets are UTF-16 code units, like JavaScript strings.
pub struct SpikeDoc {
    doc: Doc,
    text: TextRef,
}

impl SpikeDoc {
    /// Creates a replica. `client_id` must be unique per live replica: two replicas that
    /// share an id corrupt the document. The caller supplies it because the Wasm build has
    /// no source of randomness of its own.
    pub fn new(client_id: u64) -> Result<Self, CrdtError> {
        let doc = new_yrs_doc(DOC_GUID, client_id)?;
        let text = doc.get_or_insert_text(TEXT_NAME);
        Ok(Self { doc, text })
    }

    /// Inserts `chunk` at `index` (UTF-16 units) and returns the update to send to peers.
    pub fn insert(&self, index: u32, chunk: &str) -> Result<Vec<u8>, CrdtError> {
        let mut txn = self.doc.transact_mut();
        let text_len = self.text.len(&txn);
        if index > text_len {
            return Err(CrdtError::OutOfRange {
                index,
                len: 0,
                text_len,
            });
        }
        self.text.insert(&mut txn, index, chunk);
        Ok(txn.encode_update_v1())
    }

    /// Deletes `len` UTF-16 units starting at `index` and returns the update for peers.
    pub fn delete(&self, index: u32, len: u32) -> Result<Vec<u8>, CrdtError> {
        let mut txn = self.doc.transact_mut();
        let text_len = self.text.len(&txn);
        if u64::from(index) + u64::from(len) > u64::from(text_len) {
            return Err(CrdtError::OutOfRange {
                index,
                len,
                text_len,
            });
        }
        self.text.remove_range(&mut txn, index, len);
        Ok(txn.encode_update_v1())
    }

    pub fn text(&self) -> String {
        let txn = self.doc.transact();
        self.text.get_string(&txn)
    }

    /// Length of the text in UTF-16 units.
    pub fn len(&self) -> u32 {
        let txn = self.doc.transact();
        self.text.len(&txn)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// What this replica has seen, as an encoded Yrs v1 state vector. It does not change when an
    /// update is rejected, which the tests rely on.
    pub fn state_vector(&self) -> Vec<u8> {
        let txn = self.doc.transact();
        txn.state_vector().encode_v1()
    }

    /// The whole document as one update. A new peer applies it to catch up.
    pub fn encode_state_as_update(&self) -> Vec<u8> {
        let txn = self.doc.transact();
        txn.encode_state_as_update_v1(&StateVector::default())
    }

    /// Applies an update from a peer. Never panics on bad input, and a rejected update leaves the
    /// document as it was (see [`crate::validate`] for how).
    pub fn apply_update(&self, bytes: &[u8]) -> Result<(), CrdtError> {
        if bytes.len() > MAX_UPDATE_BYTES {
            return Err(CrdtError::TooLarge(bytes.len()));
        }
        let update = decode_update(bytes)?;
        #[cfg(not(target_arch = "wasm32"))]
        crate::validate::rehearse(&self.doc, DOC_GUID, init_text, |_| Ok(()), false, bytes)?;
        let mut txn = self.doc.transact_mut();
        txn.apply_update(update).map_err(|_| CrdtError::Apply)
    }
}

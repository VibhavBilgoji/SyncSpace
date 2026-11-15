//! A document holding one shared `Y.Text` named `"text"`.

use std::sync::Arc;

use yrs::block::ClientID;
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{
    Doc, GetString, OffsetKind, Options, ReadTxn, StateVector, Text, TextRef, Transact, Update,
};

/// Largest update the document accepts. Matches the 1 MiB frame limit in `docs/API.md`.
pub const MAX_UPDATE_BYTES: usize = 1024 * 1024;

/// Name of the shared text inside the Y.Doc.
const TEXT_NAME: &str = "text";

/// Every replica uses the same guid. It only names the document; it is not part of an
/// update. A fixed value also avoids Yrs' random UUID, which needs OS entropy that
/// `wasm32-unknown-unknown` does not have.
const DOC_GUID: &str = "syncspace-spike";

/// Yjs client ids are limited to 53 bits so they survive a JavaScript number.
const MAX_CLIENT_ID: u64 = 1 << 53;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CrdtError {
    #[error("update is {0} bytes, over the {MAX_UPDATE_BYTES} byte limit")]
    TooLarge(usize),
    #[error("update could not be decoded")]
    Decode,
    #[error("update could not be applied")]
    Apply,
    #[error("range {index}+{len} is outside the text of length {text_len}")]
    OutOfRange { index: u32, len: u32, text_len: u32 },
    #[error("client id must be an integer below 2^53")]
    InvalidClientId,
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
        if client_id >= MAX_CLIENT_ID {
            return Err(CrdtError::InvalidClientId);
        }
        let mut options =
            Options::with_guid_and_client_id(Arc::from(DOC_GUID), ClientID::new(client_id));
        options.offset_kind = OffsetKind::Utf16;
        let doc = Doc::with_options(options);
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

    /// The whole document as one update. A new peer applies it to catch up.
    pub fn encode_state_as_update(&self) -> Vec<u8> {
        let txn = self.doc.transact();
        txn.encode_state_as_update_v1(&StateVector::default())
    }

    /// Applies an update from a peer. Never panics on bad input.
    pub fn apply_update(&self, bytes: &[u8]) -> Result<(), CrdtError> {
        if bytes.len() > MAX_UPDATE_BYTES {
            return Err(CrdtError::TooLarge(bytes.len()));
        }
        let update = decode_update(bytes)?;
        let mut txn = self.doc.transact_mut();
        txn.apply_update(update).map_err(|_| CrdtError::Apply)
    }
}

/// Decodes a v1 update without ever panicking on hostile bytes, and only returns updates
/// that the document can safely keep.
///
/// Two Yrs 0.28.1 behaviours make plain decoding unsafe on untrusted input:
///
/// - A `debug_assert!` that a decoded client id fits in 53 bits makes garbage panic in debug
///   builds. Decoding runs before the document is touched, so catching the panic leaves the
///   document unchanged.
/// - A mutated update can decode and apply fine but leave a block in the document's pending
///   queue that cannot be re-encoded, and the next `encode_state_as_update` then panics
///   inside Yrs (`merge_pending_v1` unwraps). A property test found this with a single
///   flipped byte. Requiring the update to survive an encode and decode round trip first
///   rejects those updates.
///
/// `catch_unwind` cannot work on Wasm, where panics abort; there updates were already
/// validated by the server.
fn decode_update(bytes: &[u8]) -> Result<Update, CrdtError> {
    fn decode_and_round_trip(bytes: &[u8]) -> Result<Update, yrs::encoding::read::Error> {
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

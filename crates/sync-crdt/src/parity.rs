//! Native and Wasm parity fixture support.
//!
//! A fixture is a list of Yrs v1 updates, each prefixed with its length as a
//! little-endian `u32`. Both builds replay the same bytes through [`doc_from_fixture`]
//! and must end with byte-identical state and identical text.

use crate::{CrdtError, SpikeDoc};

/// The replica that reads a fixture never writes, so its client id is never used for edits.
const READER_CLIENT_ID: u64 = 1;

/// Packs updates into the fixture format. Fails if one update does not fit a `u32` length.
pub fn encode_fixture(updates: &[Vec<u8>]) -> Result<Vec<u8>, CrdtError> {
    let mut out = Vec::new();
    for update in updates {
        let len = u32::try_from(update.len()).map_err(|_| CrdtError::TooLarge(update.len()))?;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(update);
    }
    Ok(out)
}

/// Replays a fixture into a fresh document.
pub fn doc_from_fixture(bytes: &[u8]) -> Result<SpikeDoc, CrdtError> {
    let doc = SpikeDoc::new(READER_CLIENT_ID)?;
    let mut rest = bytes;
    while !rest.is_empty() {
        let (len_bytes, tail) = rest.split_at_checked(4).ok_or(CrdtError::Decode)?;
        let len = u32::from_le_bytes(len_bytes.try_into().map_err(|_| CrdtError::Decode)?) as usize;
        let (update, tail) = tail.split_at_checked(len).ok_or(CrdtError::Decode)?;
        doc.apply_update(update)?;
        rest = tail;
    }
    Ok(doc)
}

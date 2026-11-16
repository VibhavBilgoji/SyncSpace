//! The one error type of the crate. Messages never contain document text.

use crate::validate::MAX_UPDATE_BYTES;

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
    #[error("a block id is 12 letters or digits")]
    InvalidBlockId,
    #[error("block {0} does not exist")]
    BlockNotFound(String),
    #[error("block {0} already exists")]
    BlockExists(String),
    #[error("order must be a finite number")]
    InvalidOrder,
    #[error("there is no room between the two orders; renumber the blocks")]
    NoRoom,
    #[error("invalid block properties: {0}")]
    InvalidProps(String),
    #[error("the document would pass a limit: {0}")]
    Limit(String),
    #[error("the update breaks the document schema: {0}")]
    Invalid(String),
    /// The update builds on changes this replica has not received. The hub answers with a
    /// re-sync (its own SyncStep1), not with a protocol error: the sender is not misbehaving.
    #[error("the update depends on changes this replica does not have")]
    MissingDependencies,
}

//! The y-sync protocol on a block document (`docs/API.md` section 3).
//!
//! Binary frames are `yrs::sync::Message`: first varint 0 is a sync message (SyncStep1, SyncStep2
//! or Update), 1 is awareness. This is the standard framing, so it also works with `y-websocket`
//! style clients. [`Document::handle_message`] is what the hub calls for every binary frame of a
//! connection. It never panics on hostile bytes: garbage, truncated and unknown frames are errors
//! (the hub answers them with close code 1008).

use yrs::sync::{Message, SyncMessage};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{ReadTxn, Transact};

use crate::document::{Document, decode_state_vector};
use crate::error::CrdtError;
use crate::validate::MAX_UPDATE_BYTES;

/// A frame is an update plus a few bytes of framing.
const MAX_FRAME_BYTES: usize = MAX_UPDATE_BYTES + 64;

/// What to do after a frame was handled.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Handled {
    /// A frame to send back to the sender (the answer to a SyncStep1).
    pub reply: Option<Vec<u8>>,
    /// The update that was applied, to broadcast to the other connections.
    pub update: Option<Vec<u8>>,
    /// An awareness update (encoded v1) for the hub to apply and relay; task 3.1.
    pub awareness: Option<Vec<u8>>,
}

/// What a binary frame is, without applying it. The hub reads this first so that it can refuse a
/// write from a viewer before anything touches the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    /// "This is what I have": a request for the changes the sender lacks. Read-only.
    SyncStep1,
    /// The changes the receiver lacks. A write.
    SyncStep2,
    /// A change. A write.
    Update,
    /// Presence and cursors; never touches the document.
    Awareness,
}

impl FrameKind {
    /// Whether applying the frame changes the document.
    pub fn is_write(self) -> bool {
        matches!(self, Self::SyncStep2 | Self::Update)
    }
}

/// Classifies a frame. Garbage, truncated frames and message types the document socket does not
/// use (auth, awareness query, custom) are errors, never panics.
pub fn frame_kind(frame: &[u8]) -> Result<FrameKind, CrdtError> {
    if frame.len() > MAX_FRAME_BYTES {
        return Err(CrdtError::TooLarge(frame.len()));
    }
    match decode_message(frame)? {
        Message::Sync(SyncMessage::SyncStep1(_)) => Ok(FrameKind::SyncStep1),
        Message::Sync(SyncMessage::SyncStep2(_)) => Ok(FrameKind::SyncStep2),
        Message::Sync(SyncMessage::Update(_)) => Ok(FrameKind::Update),
        Message::Awareness(_) => Ok(FrameKind::Awareness),
        _ => Err(CrdtError::Decode),
    }
}

/// Whether an update carries no change at all (what a client answers with when it has nothing the
/// server lacks). The hub does not broadcast or count these.
pub fn is_empty_update(update: &[u8]) -> bool {
    use yrs::Update;
    update == Update::default().encode_v1().as_slice()
}

/// Wraps an update in an Update frame.
pub fn update_frame(update: &[u8]) -> Vec<u8> {
    Message::Sync(SyncMessage::Update(update.to_vec())).encode_v1()
}

fn decode_message(frame: &[u8]) -> Result<Message, CrdtError> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::panic::catch_unwind(|| Message::decode_v1(frame))
            .map_err(|_| CrdtError::Decode)?
            .map_err(|_| CrdtError::Decode)
    }
    #[cfg(target_arch = "wasm32")]
    {
        Message::decode_v1(frame).map_err(|_| CrdtError::Decode)
    }
}

impl Document {
    /// The first frame a connection sends: "this is what I have, send me the rest".
    pub fn sync_step1_frame(&self) -> Vec<u8> {
        let txn = self.doc.transact();
        Message::Sync(SyncMessage::SyncStep1(txn.state_vector())).encode_v1()
    }

    /// Handles one binary frame from a peer.
    ///
    /// - SyncStep1: answers with a SyncStep2 holding what the peer lacks.
    /// - SyncStep2 and Update: applied through [`Document::apply_update`] (so with every guard),
    ///   and returned in [`Handled::update`] for broadcast.
    /// - Awareness: returned in [`Handled::awareness`].
    /// - Anything else (auth, awareness query, custom messages) is an error.
    pub fn handle_message(&self, frame: &[u8]) -> Result<Handled, CrdtError> {
        if frame.len() > MAX_FRAME_BYTES {
            return Err(CrdtError::TooLarge(frame.len()));
        }
        match decode_message(frame)? {
            Message::Sync(SyncMessage::SyncStep1(state_vector)) => {
                let missing = self.encode_state_as_update_from(&state_vector.encode_v1())?;
                Ok(Handled {
                    reply: Some(Message::Sync(SyncMessage::SyncStep2(missing)).encode_v1()),
                    ..Handled::default()
                })
            }
            Message::Sync(SyncMessage::SyncStep2(update) | SyncMessage::Update(update)) => {
                self.apply_update(&update)?;
                Ok(Handled {
                    update: Some(update),
                    ..Handled::default()
                })
            }
            Message::Awareness(update) => Ok(Handled {
                awareness: Some(update.encode_v1()),
                ..Handled::default()
            }),
            _ => Err(CrdtError::Decode),
        }
    }
}

/// Decodes a state vector sent by a peer; exposed for the hub's own bookkeeping.
pub fn check_state_vector(bytes: &[u8]) -> Result<(), CrdtError> {
    decode_state_vector(bytes).map(|_| ())
}

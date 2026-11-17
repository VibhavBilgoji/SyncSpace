//! One open document: its CRDT replica and the connections that edit it.
//!
//! The replica sits behind a `std::sync::Mutex` that is only ever held for the length of one
//! synchronous call and **never across an `.await`** (CLAUDE.md rule 4): handlers lock, call,
//! and release before they send anything on a socket.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::body::Bytes;
use mongodb::bson::oid::ObjectId;
use sync_crdt::{CrdtError, Document as CrdtDocument, FrameKind, Handled, is_empty_update};
use tokio::sync::broadcast;

use super::frames::ServerFrame;
use crate::documents::SERVER_CLIENT_ID;
use crate::metrics::Metrics;

/// Something to send to the connections of a room.
#[derive(Debug, Clone)]
pub enum Outgoing {
    /// A binary y-sync frame from connection `from`, for everyone else.
    Frame { from: u64, bytes: Bytes },
    /// A JSON text frame for everyone, the sender included.
    Json(String),
    /// Close every connection with this code (the document was deleted).
    Close { code: u16, reason: &'static str },
    /// Close the connections of one user (their role changed or they were removed).
    CloseUser {
        user: ObjectId,
        code: u16,
        reason: &'static str,
    },
}

/// Why a frame was not applied.
#[derive(Debug, PartialEq, Eq)]
pub enum RoomError {
    /// Applying it could push the document over its size cap.
    TooBig,
    /// The document refused it (see `CrdtError`).
    Crdt(CrdtError),
}

pub struct DocRoom {
    pub id: ObjectId,
    pub workspace_id: ObjectId,
    doc: Mutex<CrdtDocument>,
    tx: broadcast::Sender<Outgoing>,
    active: AtomicU32,
    dirty: AtomicBool,
    /// Serialises snapshots of this room (not the document lock): held across the encode and the
    /// MongoDB write, so two flushes can neither lose a failed write nor let an older state
    /// overwrite a newer one.
    flush_gate: tokio::sync::Mutex<()>,
    max_state_bytes: usize,
    metrics: Arc<Metrics>,
}

impl DocRoom {
    /// A room holding the stored `snapshot`, which must pass the same checks as any update.
    pub fn load(
        id: ObjectId,
        workspace_id: ObjectId,
        snapshot: &[u8],
        max_state_bytes: usize,
        broadcast_capacity: usize,
        metrics: Arc<Metrics>,
    ) -> Result<Self, CrdtError> {
        let doc = CrdtDocument::new(SERVER_CLIENT_ID)?;
        doc.apply_update(snapshot)?;
        let (tx, _) = broadcast::channel(broadcast_capacity.max(1));
        Ok(Self {
            id,
            workspace_id,
            doc: Mutex::new(doc),
            tx,
            active: AtomicU32::new(0),
            dirty: AtomicBool::new(false),
            flush_gate: tokio::sync::Mutex::new(()),
            max_state_bytes,
            metrics,
        })
    }

    /// Locks the replica. A poisoned lock still holds a usable document.
    fn doc(&self) -> MutexGuard<'_, CrdtDocument> {
        self.doc
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Outgoing> {
        self.tx.subscribe()
    }

    /// Sends to every subscribed connection. Having none is fine.
    pub fn broadcast(&self, outgoing: Outgoing) {
        let _ = self.tx.send(outgoing);
    }

    /// A connection joined: returns how many are open now.
    pub fn join(&self) -> u32 {
        self.active.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// A connection left: returns how many are open now (never below zero).
    pub fn leave(&self) -> u32 {
        let mut current = self.active.load(Ordering::SeqCst);
        loop {
            let next = current.saturating_sub(1);
            match self
                .active
                .compare_exchange(current, next, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => return next,
                Err(actual) => current = actual,
            }
        }
    }

    pub fn active(&self) -> u32 {
        self.active.load(Ordering::SeqCst)
    }

    /// Tells everyone how many sockets are open.
    pub fn announce_presence(&self) {
        self.broadcast(Outgoing::Json(
            ServerFrame::PresenceCount {
                active_sockets: self.active(),
            }
            .to_text(),
        ));
    }

    /// The server's own SyncStep1: "this is what I have, send me the rest".
    pub fn sync_step1_frame(&self) -> Vec<u8> {
        self.doc().sync_step1_frame()
    }

    /// Handles one frame. The caller has already read its `kind` and checked the sender's role.
    /// Changes are refused when they could push the document past `max_state_bytes`: the state
    /// after an update is at most the state before plus the update, so checking the sum first
    /// keeps the cap without having to undo anything, and a late joiner's full state always fits
    /// the frame limit the client enforces (`docs/API.md`, limits).
    pub fn apply_frame(&self, frame: &[u8], kind: FrameKind) -> Result<Handled, RoomError> {
        let doc = self.doc();
        if kind.is_write() {
            let current = doc.encode_state_as_update().len();
            if current.saturating_add(frame.len()) > self.max_state_bytes {
                return Err(RoomError::TooBig);
            }
        }
        let mut handled = doc.handle_message(frame).map_err(RoomError::Crdt)?;
        // A client that has nothing the server lacks answers with an empty update: not a change,
        // so nothing is broadcast and the document is not marked dirty.
        if handled.update.as_deref().is_some_and(is_empty_update) {
            handled.update = None;
        }
        if kind.is_write() && handled.update.is_some() {
            self.dirty.store(true, Ordering::SeqCst);
            self.metrics.edit();
        }
        Ok(handled)
    }

    /// Replaces the content with the one of `target_state` (restore, task 4.4), as one update that
    /// goes to every connection. Returns the state as it was **before**, for the RESTORE version.
    /// Under the same cap as any edit; nothing changes when it fails.
    pub fn restore(
        &self,
        target_state: &[u8],
        scratch_client_id: u64,
    ) -> Result<Vec<u8>, RoomError> {
        let target = CrdtDocument::new(2).map_err(RoomError::Crdt)?;
        target.apply_update(target_state).map_err(RoomError::Crdt)?;
        let doc = self.doc();
        let before = doc.encode_state_as_update();
        let update = doc
            .restore_update(&target, scratch_client_id)
            .map_err(RoomError::Crdt)?;
        if is_empty_update(&update) {
            return Ok(before);
        }
        if before.len().saturating_add(update.len()) > self.max_state_bytes {
            return Err(RoomError::TooBig);
        }
        doc.apply_update(&update).map_err(RoomError::Crdt)?;
        self.dirty.store(true, Ordering::SeqCst);
        self.metrics.edit();
        // From nobody: every connection, the restorer's included, takes the new content.
        self.broadcast(Outgoing::Frame {
            from: u64::MAX,
            bytes: Bytes::from(sync_crdt::update_frame(&update)),
        });
        Ok(before)
    }

    /// Whether anything changed since the last snapshot.
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::SeqCst)
    }

    /// Claims the changes made so far for a snapshot: returns whether there were any and clears the
    /// flag **before** the state is encoded. An edit that lands while the snapshot is being
    /// encoded or written sets the flag again, so the next snapshot writes it; setting the flag
    /// after the write instead could erase that edit's flag and lose it.
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::SeqCst)
    }

    /// Puts the flag back (a snapshot write failed).
    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::SeqCst);
    }

    pub fn flush_gate(&self) -> &tokio::sync::Mutex<()> {
        &self.flush_gate
    }

    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    /// The whole state and its blocks, for tests and the snapshot worker.
    pub fn state(&self) -> (Vec<u8>, Vec<sync_crdt::Block>) {
        let doc = self.doc();
        (doc.encode_state_as_update(), doc.blocks())
    }
}

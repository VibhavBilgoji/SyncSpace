//! The WebSocket hub (`docs/ARCHITECTURE.md` section 2, `docs/API.md` section 3): rooms that hold
//! one open document each, and the connections that edit it.
//!
//! - [`socket`] is the connection handler: Origin and ticket checks, the role of the user,
//!   per-frame role checks, rate limits, size limits and the close codes.
//! - [`room`] is one open document: its CRDT replica and the broadcast to its connections.
//! - [`limits`] holds the token buckets and [`frames`] the JSON text frames.
//!
//! A room is created the first time a connection opens a document, from the snapshot in MongoDB.

pub mod frames;
pub mod limits;
pub mod room;
pub mod snapshot;
pub mod socket;
pub mod workspace;
pub mod workspace_socket;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use mongodb::Database;
use mongodb::bson::oid::ObjectId;
use tokio::sync::OnceCell;

pub use self::limits::BucketConfig;
pub use self::room::{DocRoom, Outgoing, RoomError};
use crate::auth::ticket::TicketStore;
use crate::db::documents::{self, BlockRecord};
use crate::error::ApiError;
use crate::metrics::Metrics;
use crate::workspaces::handlers::iso;

use self::frames::ServerFrame;

/// Close codes of `docs/API.md` section 3.
pub const CLOSE_NORMAL: u16 = 1000;
pub const CLOSE_GOING_AWAY: u16 = 1001;
pub const CLOSE_POLICY: u16 = 1008;
pub const CLOSE_TOO_BIG: u16 = 1009;
pub const CLOSE_INTERNAL: u16 = 1011;
pub const CLOSE_UNAUTHORIZED: u16 = 4001;
pub const CLOSE_FORBIDDEN: u16 = 4003;
pub const CLOSE_DOCUMENT_GONE: u16 = 4004;

/// Limits and timeouts. The defaults are the documented ones; tests shrink them.
#[derive(Clone, Debug)]
pub struct HubConfig {
    /// A send that takes longer drops the connection without waiting for the peer.
    pub send_timeout: Duration,
    /// How often an idle connection is pinged (`docs/API.md`: every 20 s).
    pub ping_interval: Duration,
    /// A connection that sends nothing, not even a pong, for this long is closed.
    pub idle_timeout: Duration,
    /// Largest accepted frame (1 MiB). Bigger frames close with 1009.
    pub max_frame_bytes: usize,
    /// What the WebSocket library accepts before it drops the TCP connection without a close
    /// frame; above `max_frame_bytes`, so frames in between still get a clean 1009.
    pub hard_limit_bytes: usize,
    /// Largest document state accepted, kept below `max_frame_bytes` so that the full state a
    /// late joiner receives fits the frame limit the client enforces.
    pub max_state_bytes: usize,
    /// How many messages a room's channel holds per connection before a connection that has not
    /// read them is closed as "too slow". Every update a connection sends is also queued as an
    /// echo for itself, so this must stay well above the update burst: at least 4 times it.
    pub broadcast_capacity: usize,
    /// Document updates: 60 per second, burst 120.
    pub updates: BucketConfig,
    /// Awareness: 20 per second, burst 40.
    pub awareness: BucketConfig,
}

impl Default for HubConfig {
    fn default() -> Self {
        Self {
            send_timeout: Duration::from_secs(10),
            ping_interval: Duration::from_secs(20),
            idle_timeout: Duration::from_secs(60),
            max_frame_bytes: 1024 * 1024,
            hard_limit_bytes: 4 * 1024 * 1024,
            max_state_bytes: 768 * 1024,
            broadcast_capacity: DEFAULT_BROADCAST_CAPACITY,
            updates: BucketConfig {
                burst: 120,
                per_second: 60,
            },
            awareness: BucketConfig {
                burst: 40,
                per_second: 20,
            },
        }
    }
}

/// 512 messages: more than 4 times the update burst (120), see [`HubConfig::broadcast_capacity`].
pub const DEFAULT_BROADCAST_CAPACITY: usize = 512;

/// Why a room could not be opened.
#[derive(Debug)]
pub enum RoomLookup {
    /// There is no such document.
    Missing,
    /// MongoDB or the stored snapshot failed.
    Failed(ApiError),
}

pub struct Hub {
    pub config: HubConfig,
    pub tickets: TicketStore,
    pub metrics: Arc<Metrics>,
    rooms: Mutex<HashMap<ObjectId, Arc<OnceCell<Arc<DocRoom>>>>>,
    workspaces: Mutex<HashMap<ObjectId, Arc<workspace::WorkspaceRoom>>>,
    next_connection: AtomicU64,
}

impl Hub {
    pub fn new(config: HubConfig) -> Self {
        Self {
            config,
            tickets: TicketStore::default(),
            metrics: Arc::new(Metrics::default()),
            rooms: Mutex::new(HashMap::new()),
            workspaces: Mutex::new(HashMap::new()),
            next_connection: AtomicU64::new(1),
        }
    }

    fn rooms(&self) -> MutexGuard<'_, HashMap<ObjectId, Arc<OnceCell<Arc<DocRoom>>>>> {
        self.rooms.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn next_connection_id(&self) -> u64 {
        self.next_connection.fetch_add(1, Ordering::Relaxed)
    }

    /// The room of document `id`, loading it from its snapshot the first time. Two connections
    /// that arrive together share one load.
    pub async fn room(&self, db: &Database, id: ObjectId) -> Result<Arc<DocRoom>, RoomLookup> {
        let cell = self.rooms().entry(id).or_default().clone();
        let max_state_bytes = self.config.max_state_bytes;
        let capacity = self.config.broadcast_capacity;
        let metrics = self.metrics.clone();
        let result = cell
            .get_or_try_init(|| async move {
                let record = documents::find_with_snapshot(db, id)
                    .await
                    .map_err(|error| RoomLookup::Failed(ApiError::from(error)))?
                    .ok_or(RoomLookup::Missing)?;
                let snapshot = record.crdt_snapshot.ok_or_else(|| {
                    RoomLookup::Failed(ApiError::internal("document has no snapshot"))
                })?;
                DocRoom::load(
                    id,
                    record.workspace_id,
                    &snapshot.bytes,
                    max_state_bytes,
                    capacity,
                    metrics,
                )
                .map(Arc::new)
                .map_err(|error| RoomLookup::Failed(ApiError::internal(error)))
            })
            .await;
        match result {
            Ok(room) => Ok(room.clone()),
            Err(error) => {
                // Do not keep a cell that never got a room.
                self.rooms().remove(&id);
                Err(error)
            }
        }
    }

    /// Registers a connection with `room`, unless the room was forgotten in the meantime (its last
    /// socket left and it was flushed and evicted). Checking and joining happen under the same lock
    /// that eviction takes, so a connection can never end up on a room that is no longer in the map:
    /// two connections on "the same document" in two different rooms would diverge.
    pub fn try_join(&self, room: &Arc<DocRoom>) -> bool {
        let rooms = self.rooms();
        let present = rooms
            .get(&room.id)
            .and_then(|cell| cell.get())
            .is_some_and(|current| Arc::ptr_eq(current, room));
        if present {
            room.join();
        }
        present
    }

    /// The open room of a document, if there is one; never loads or creates a room.
    pub fn peek_room(&self, id: ObjectId) -> Option<Arc<DocRoom>> {
        self.rooms().get(&id).and_then(|cell| cell.get().cloned())
    }

    /// Every open room.
    pub fn open_room_list(&self) -> Vec<Arc<DocRoom>> {
        self.rooms()
            .values()
            .filter_map(|cell| cell.get().cloned())
            .collect()
    }

    /// How many sockets are open on all documents.
    pub fn open_sockets(&self) -> u64 {
        self.open_room_list()
            .iter()
            .map(|room| u64::from(room.active()))
            .sum()
    }

    /// Writes the room to MongoDB if anything changed since the last snapshot. `Ok(true)` when it
    /// wrote. On a failure the changes are marked again so the next snapshot tries once more.
    /// The document lock is held only while encoding, never across the write.
    pub async fn flush_room(
        &self,
        db: &Database,
        room: &DocRoom,
    ) -> Result<bool, mongodb::error::Error> {
        // One flush of a room at a time: see `DocRoom::flush_gate`.
        let _gate = room.flush_gate().lock().await;
        if !room.take_dirty() {
            return Ok(false);
        }
        // If this future is dropped before the write is confirmed (task aborted, server going
        // down), the changes are marked again instead of being forgotten.
        let mut guard = DirtyGuard { room, armed: true };
        let (snapshot, blocks) = room.state();
        let records: Vec<BlockRecord> = blocks.iter().map(BlockRecord::from).collect();
        let result = documents::write_snapshot(db, room.id, snapshot, &records).await?;
        guard.armed = false;
        match result {
            Some((revision, saved_at)) => {
                self.metrics.snapshot_write();
                room.broadcast(Outgoing::Json(
                    ServerFrame::SnapshotSaved {
                        revision,
                        saved_at: iso(saved_at),
                    }
                    .to_text(),
                ));
                Ok(true)
            }
            // The document was deleted while it was open: nothing to save.
            None => Ok(false),
        }
    }

    /// Flushes every open room; returns how many were written. A failing room is logged and does
    /// not stop the others.
    pub async fn flush_all(&self, db: &Database) -> usize {
        let mut written = 0;
        for room in self.open_room_list() {
            match self.flush_room(db, &room).await {
                Ok(true) => written += 1,
                Ok(false) => {}
                Err(error) => {
                    tracing::error!(error = %error, "could not write a snapshot");
                }
            }
        }
        written
    }

    /// The last socket of `room` left: write what is left and, when nobody has come back, forget
    /// the room so that memory does not grow with every document ever opened.
    pub async fn release(&self, db: &Database, room: &Arc<DocRoom>) {
        if room.active() > 0 {
            return;
        }
        if let Err(error) = self.flush_room(db, room).await {
            // Keep the room: it is dirty again, and the snapshot worker retries.
            tracing::error!(error = %error, "the final snapshot failed");
            return;
        }
        let mut rooms = self.rooms();
        let same = rooms
            .get(&room.id)
            .and_then(|cell| cell.get())
            .is_some_and(|current| Arc::ptr_eq(current, room));
        if same && room.active() == 0 && !room.is_dirty() {
            rooms.remove(&room.id);
        }
    }

    /// The server is stopping: write every changed document, then send every socket away with
    /// 1001 so that the graceful shutdown of the HTTP server can finish (it waits for open
    /// connections).
    pub async fn shutdown(&self, db: &Database) {
        // Twice: the HTTP server still accepts connections while the first flush runs, and a
        // room opened in that window must be written and sent away too.
        for _ in 0..2 {
            let written = self.flush_all(db).await;
            tracing::info!(written, "final snapshots written");
            for room in self.open_room_list() {
                room.broadcast(Outgoing::Close {
                    code: CLOSE_GOING_AWAY,
                    reason: "server shutting down",
                });
            }
            for room in self.workspace_room_list() {
                room.broadcast(Outgoing::Close {
                    code: CLOSE_GOING_AWAY,
                    reason: "server shutting down",
                });
            }
        }
    }

    /// Closes every open document and workspace socket of `user` in `workspace`. The browser reconnects with a
    /// fresh ticket, which is checked against the membership as it is now (ADR-013: the role is
    /// read at connect, so this is how a change reaches an open page).
    pub fn close_user(&self, workspace: ObjectId, user: ObjectId, code: u16, reason: &'static str) {
        for room in self.open_room_list() {
            if room.workspace_id == workspace {
                room.broadcast(Outgoing::CloseUser { user, code, reason });
            }
        }
        // The workspace sockets too: a removed member must stop hearing about the workspace.
        self.close_user_in_workspace(workspace, user, code, reason);
    }

    /// The document was deleted: close every socket on it with 4004 and forget the room.
    pub fn close_document(&self, id: ObjectId) {
        let removed = self.rooms().remove(&id);
        if let Some(room) = removed.and_then(|cell| cell.get().cloned()) {
            room.broadcast(Outgoing::Close {
                code: CLOSE_DOCUMENT_GONE,
                reason: "document deleted",
            });
        }
    }

    /// How many rooms are open (for tests).
    pub fn open_rooms(&self) -> usize {
        self.rooms().len()
    }
}

/// Puts the dirty flag back unless the write was confirmed.
struct DirtyGuard<'a> {
    room: &'a DocRoom,
    armed: bool,
}

impl Drop for DirtyGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.room.mark_dirty();
        }
    }
}

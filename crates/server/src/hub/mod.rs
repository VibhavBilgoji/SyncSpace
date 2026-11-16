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
pub mod socket;

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
use crate::db::documents;
use crate::error::ApiError;

/// Close codes of `docs/API.md` section 3.
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
    rooms: Mutex<HashMap<ObjectId, Arc<OnceCell<Arc<DocRoom>>>>>,
    next_connection: AtomicU64,
}

impl Hub {
    pub fn new(config: HubConfig) -> Self {
        Self {
            config,
            tickets: TicketStore::default(),
            rooms: Mutex::new(HashMap::new()),
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
        let result = cell
            .get_or_try_init(|| async move {
                let record = documents::find_with_snapshot(db, id)
                    .await
                    .map_err(|error| RoomLookup::Failed(ApiError::from(error)))?
                    .ok_or(RoomLookup::Missing)?;
                let snapshot = record.crdt_snapshot.ok_or_else(|| {
                    RoomLookup::Failed(ApiError::internal("document has no snapshot"))
                })?;
                DocRoom::load(id, record.workspace_id, &snapshot.bytes, max_state_bytes)
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

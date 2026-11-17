//! The workspace socket (`docs/API.md` section 3, "Workspace socket"): one room per workspace
//! that has an open socket, a channel of JSON frames to every one of them. Server to client only;
//! the one thing a client may send is `PING`.
//!
//! A room exists only while somebody is connected. Joining and leaving happen under the map's lock,
//! so a connection can never land on a room that is being forgotten.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, MutexGuard};

use mongodb::bson::oid::ObjectId;
use tokio::sync::broadcast;

use super::Hub;
use super::frames::ServerFrame;
use super::room::Outgoing;

/// The sockets of one workspace.
pub struct WorkspaceRoom {
    pub id: ObjectId,
    tx: broadcast::Sender<Outgoing>,
    active: AtomicU32,
}

impl WorkspaceRoom {
    fn new(id: ObjectId, capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity.max(1));
        Self {
            id,
            tx,
            active: AtomicU32::new(0),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Outgoing> {
        self.tx.subscribe()
    }

    /// Sends to every open socket of the workspace; having none is fine.
    pub fn broadcast(&self, outgoing: Outgoing) {
        let _ = self.tx.send(outgoing);
    }

    /// How many sockets are open.
    pub fn active(&self) -> u32 {
        self.active.load(Ordering::SeqCst)
    }
}

impl Hub {
    fn workspace_rooms(
        &self,
    ) -> MutexGuard<'_, std::collections::HashMap<ObjectId, Arc<WorkspaceRoom>>> {
        self.workspaces
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The room of `workspace`, created if need be, with this connection counted in it.
    pub fn join_workspace(&self, workspace: ObjectId) -> Arc<WorkspaceRoom> {
        let mut rooms = self.workspace_rooms();
        let room = rooms
            .entry(workspace)
            .or_insert_with(|| {
                Arc::new(WorkspaceRoom::new(
                    workspace,
                    self.config.broadcast_capacity,
                ))
            })
            .clone();
        room.active.fetch_add(1, Ordering::SeqCst);
        room
    }

    /// A connection left; the room is forgotten when it was the last one.
    pub fn leave_workspace(&self, room: &Arc<WorkspaceRoom>) {
        let mut rooms = self.workspace_rooms();
        let left = room.active.fetch_sub(1, Ordering::SeqCst).saturating_sub(1);
        if left == 0
            && rooms
                .get(&room.id)
                .is_some_and(|current| Arc::ptr_eq(current, room))
        {
            rooms.remove(&room.id);
        }
    }

    /// Sends a frame to every open workspace socket of `workspace`. Nothing happens, and nothing is
    /// kept, when nobody is connected: a page that opens later loads the current state over REST.
    pub fn publish(&self, workspace: ObjectId, frame: &ServerFrame) {
        let room = self.workspace_rooms().get(&workspace).cloned();
        if let Some(room) = room {
            room.broadcast(Outgoing::Json(frame.to_text()));
        }
    }

    /// Closes the open workspace sockets of `user` (their role changed or they were removed).
    pub(super) fn close_user_in_workspace(
        &self,
        workspace: ObjectId,
        user: ObjectId,
        code: u16,
        reason: &'static str,
    ) {
        let room = self.workspace_rooms().get(&workspace).cloned();
        if let Some(room) = room {
            room.broadcast(Outgoing::CloseUser { user, code, reason });
        }
    }

    /// Every workspace room that has a socket open (the server is stopping).
    pub(super) fn workspace_room_list(&self) -> Vec<Arc<WorkspaceRoom>> {
        self.workspace_rooms().values().cloned().collect()
    }

    /// How many workspace sockets are open (for tests and metrics).
    pub fn open_workspace_sockets(&self) -> u64 {
        self.workspace_room_list()
            .iter()
            .map(|room| u64::from(room.active()))
            .sum()
    }
}

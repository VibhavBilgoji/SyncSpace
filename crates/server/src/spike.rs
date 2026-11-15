//! THROWAWAY (roadmap task 0.3): a WebSocket relay for the Yrs spike.
//!
//! One shared document for everyone who connects to `/spike`. A new connection first receives
//! the whole document state; after that every binary frame is a Yrs v1 update that is applied
//! to the server copy and forwarded to the other connections. There is no auth, no roles and no
//! y-sync handshake: the real hub (Phase 2, `docs/ARCHITECTURE.md`) replaces this file, which
//! must then be deleted. Bind it to localhost only.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use sync_crdt::SpikeDoc;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;

/// Largest accepted frame, from `docs/API.md` (limits). Bigger frames close with 1009.
const MAX_FRAME_BYTES: usize = 1024 * 1024;
/// Hard cap enforced by the WebSocket library. Above it the library drops the TCP connection
/// without a close frame, so it sits above `MAX_FRAME_BYTES`: frames between the two get a clean
/// 1009 from our own check, and only abusive frames above the cap are cut off.
const HARD_LIMIT_BYTES: usize = 4 * 1024 * 1024;
/// Client id of the server's own replica. It never writes, so it only has to differ from
/// every browser (those pick random ids, which are astronomically unlikely to be 0).
const SERVER_CLIENT_ID: u64 = 0;

const CLOSE_POLICY: u16 = 1008;
const CLOSE_TOO_BIG: u16 = 1009;

#[derive(Clone)]
struct SpikeState {
    inner: Arc<Inner>,
}

struct Inner {
    doc: Mutex<SpikeDoc>,
    updates: broadcast::Sender<(u64, Bytes)>,
    next_conn: AtomicU64,
}

impl SpikeState {
    fn new() -> Option<Self> {
        let doc = SpikeDoc::new(SERVER_CLIENT_ID).ok()?;
        let (updates, _) = broadcast::channel(256);
        Some(Self {
            inner: Arc::new(Inner {
                doc: Mutex::new(doc),
                updates,
                next_conn: AtomicU64::new(1),
            }),
        })
    }

    /// Locks the document. A poisoned lock still holds a usable document.
    /// Callers must not hold the guard across `.await` (CLAUDE.md rule 4).
    fn doc(&self) -> MutexGuard<'_, SpikeDoc> {
        self.inner
            .doc
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The relay's routes: `GET /spike` (WebSocket) and `GET /healthz`.
pub fn router() -> Router {
    let Some(state) = SpikeState::new() else {
        // Unreachable: the server client id is a constant below 2^53.
        return Router::new().route("/healthz", get(|| async { "ok" }));
    };
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/spike", get(ws_handler))
        .with_state(state)
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<SpikeState>) -> impl IntoResponse {
    ws.max_message_size(HARD_LIMIT_BYTES)
        .max_frame_size(HARD_LIMIT_BYTES)
        .on_upgrade(move |socket| handle_socket(socket, state))
}

async fn close(socket: &mut WebSocket, code: u16, reason: &'static str) {
    let frame = CloseFrame {
        code,
        reason: reason.into(),
    };
    let _ = socket.send(Message::Close(Some(frame))).await;
}

async fn handle_socket(mut socket: WebSocket, state: SpikeState) {
    let conn_id = state.inner.next_conn.fetch_add(1, Ordering::Relaxed);

    // Subscribe before reading the state so no update falls in the gap. A duplicate is
    // harmless: applying an update twice changes nothing.
    let mut updates = state.inner.updates.subscribe();
    let snapshot = state.doc().encode_state_as_update();
    if socket
        .send(Message::Binary(Bytes::from(snapshot)))
        .await
        .is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            incoming = socket.recv() => match incoming {
                None | Some(Ok(Message::Close(_))) => break,
                // A read error, including a frame over the hard cap: the connection is already gone.
                Some(Err(_)) => break,
                Some(Ok(Message::Binary(bytes))) if bytes.len() > MAX_FRAME_BYTES => {
                    close(&mut socket, CLOSE_TOO_BIG, "frame too large").await;
                    break;
                }
                Some(Ok(Message::Binary(bytes))) => {
                    // Apply under the lock, then release it before any `.await`.
                    let applied = state.doc().apply_update(&bytes);
                    match applied {
                        Ok(()) => {
                            let _ = state.inner.updates.send((conn_id, bytes));
                        }
                        Err(_) => {
                            close(&mut socket, CLOSE_POLICY, "invalid update").await;
                            break;
                        }
                    }
                }
                Some(Ok(Message::Text(_))) => {
                    close(&mut socket, CLOSE_POLICY, "binary frames only").await;
                    break;
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
            },
            forwarded = updates.recv() => match forwarded {
                Ok((from, bytes)) => {
                    if from != conn_id
                        && socket.send(Message::Binary(bytes)).await.is_err()
                    {
                        break;
                    }
                }
                // A slow consumer is disconnected and reconnects (docs/ARCHITECTURE.md §4).
                Err(RecvError::Lagged(_)) => {
                    close(&mut socket, CLOSE_POLICY, "too slow").await;
                    break;
                }
                Err(RecvError::Closed) => break,
            },
        }
    }
}

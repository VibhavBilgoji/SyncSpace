//! THROWAWAY (roadmap task 0.3): a WebSocket relay for the Yrs spike.
//!
//! One shared document for everyone who connects to `/spike`. A new connection first receives
//! the whole document state; after that every binary frame is a Yrs v1 update that is applied
//! to the server copy and forwarded to the other connections. There is no auth, no roles and no
//! y-sync handshake: the real hub (Phase 2, `docs/ARCHITECTURE.md`) replaces this file, which
//! must then be deleted. Bind it to localhost only.
//!
//! The protections below are the ones the hub must keep (`docs/ARCHITECTURE.md` section 4):
//! every send has a timeout, idle peers are pinged and then dropped, the document has a size
//! cap so the state a late joiner receives stays under the frame limit, the broadcast backlog
//! is small, and large updates are applied off the async threads.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use futures_util::{Sink, SinkExt};
use sync_crdt::SpikeDoc;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tokio::time::{Instant, MissedTickBehavior, interval, timeout};

/// Largest accepted frame, from `docs/API.md` (limits). Bigger frames close with 1009.
const MAX_FRAME_BYTES: usize = 1024 * 1024;
/// Hard cap enforced by the WebSocket library. Above it the library drops the TCP connection
/// without a close frame, so it sits above `MAX_FRAME_BYTES`: frames between the two get a clean
/// 1009 from our own check, and only abusive frames above the cap are cut off.
const HARD_LIMIT_BYTES: usize = 4 * 1024 * 1024;
/// Client id of the server's own replica. It never writes, so it only has to differ from
/// every browser (those pick random ids, which are astronomically unlikely to be 0).
const SERVER_CLIENT_ID: u64 = 0;
/// Messages kept for the slowest receiver. Together with `MAX_FRAME_BYTES` this bounds the
/// backlog a stuck peer can pin in memory.
const BROADCAST_CAPACITY: usize = 64;
/// Updates bigger than this are applied on a blocking thread instead of a Tokio worker.
const BLOCKING_THRESHOLD_BYTES: usize = 16 * 1024;

const CLOSE_POLICY: u16 = 1008;
const CLOSE_TOO_BIG: u16 = 1009;

/// Time limits and the document size cap. The defaults suit real use; tests shrink them.
#[derive(Clone, Debug)]
pub struct SpikeConfig {
    /// A send that takes longer drops the connection without waiting for the peer.
    pub send_timeout: Duration,
    /// How often an idle connection is pinged (`docs/API.md`: every 20 s).
    pub ping_interval: Duration,
    /// A connection that sends nothing, not even a pong, for this long is closed.
    pub idle_timeout: Duration,
    /// Largest document state accepted. Kept below `MAX_FRAME_BYTES` so that the full state a
    /// late joiner receives never exceeds the frame limit the client enforces.
    pub max_state_bytes: usize,
}

impl Default for SpikeConfig {
    fn default() -> Self {
        Self {
            send_timeout: Duration::from_secs(10),
            ping_interval: Duration::from_secs(20),
            idle_timeout: Duration::from_secs(60),
            max_state_bytes: 768 * 1024,
        }
    }
}

#[derive(Clone)]
struct SpikeState {
    inner: Arc<Inner>,
}

struct Inner {
    doc: Mutex<SpikeDoc>,
    updates: broadcast::Sender<(u64, Bytes)>,
    next_conn: AtomicU64,
    config: SpikeConfig,
}

/// Why an update was not applied.
#[derive(Debug, PartialEq, Eq)]
enum Rejected {
    /// Applying it could push the document over `max_state_bytes`.
    TooBig,
    /// It is not a valid update.
    Invalid,
}

impl SpikeState {
    fn new(config: SpikeConfig) -> Option<Self> {
        let doc = SpikeDoc::new(SERVER_CLIENT_ID).ok()?;
        let (updates, _) = broadcast::channel(BROADCAST_CAPACITY);
        Some(Self {
            inner: Arc::new(Inner {
                doc: Mutex::new(doc),
                updates,
                next_conn: AtomicU64::new(1),
                config,
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

    /// Applies an update unless it could grow the document past the size cap. The state after
    /// an update is at most the state before plus the update, so checking the sum first keeps
    /// the cap without having to undo anything.
    fn apply(&self, bytes: &[u8]) -> Result<(), Rejected> {
        let doc = self.doc();
        let current = doc.encode_state_as_update().len();
        if current.saturating_add(bytes.len()) > self.inner.config.max_state_bytes {
            return Err(Rejected::TooBig);
        }
        doc.apply_update(bytes).map_err(|_| Rejected::Invalid)
    }
}

/// Applies a frame, moving big ones off the async threads so a 1 MiB decode does not stall
/// every other connection that shares the worker.
async fn apply_frame(state: &SpikeState, bytes: &Bytes) -> Result<(), Rejected> {
    if bytes.len() <= BLOCKING_THRESHOLD_BYTES {
        return state.apply(bytes);
    }
    let state = state.clone();
    let data = bytes.clone();
    tokio::task::spawn_blocking(move || state.apply(&data))
        .await
        .unwrap_or(Err(Rejected::Invalid))
}

/// The relay's routes with default limits: `GET /spike` (WebSocket) and `GET /healthz`.
pub fn router() -> Router {
    router_with_config(SpikeConfig::default())
}

/// Like [`router`], with explicit limits. Tests use short timeouts and a small size cap.
pub fn router_with_config(config: SpikeConfig) -> Router {
    let Some(state) = SpikeState::new(config) else {
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

/// Sends a message and reports whether it went out in time. A peer that stops reading would
/// otherwise block this task inside `send` for as long as the connection lives: the task
/// could not reach any other branch, including the lag check. On `false`, drop the
/// connection without waiting for the peer.
async fn send_within<S>(sink: &mut S, message: Message, limit: Duration) -> bool
where
    S: Sink<Message> + Unpin,
{
    matches!(timeout(limit, sink.send(message)).await, Ok(Ok(())))
}

async fn close<S>(sink: &mut S, code: u16, reason: &'static str, limit: Duration)
where
    S: Sink<Message> + Unpin,
{
    let frame = CloseFrame {
        code,
        reason: reason.into(),
    };
    let _ = send_within(sink, Message::Close(Some(frame)), limit).await;
}

async fn handle_socket(mut socket: WebSocket, state: SpikeState) {
    let conn_id = state.inner.next_conn.fetch_add(1, Ordering::Relaxed);
    let config = state.inner.config.clone();

    // Subscribe before reading the state so no update falls in the gap. A duplicate is
    // harmless: applying an update twice changes nothing.
    let mut updates = state.inner.updates.subscribe();
    let snapshot = state.doc().encode_state_as_update();
    if !send_within(
        &mut socket,
        Message::Binary(Bytes::from(snapshot)),
        config.send_timeout,
    )
    .await
    {
        return;
    }

    let mut ping = interval(config.ping_interval);
    ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
    ping.tick().await; // the first tick fires at once
    let mut last_seen = Instant::now();

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                // Any frame, including a pong, shows the peer is alive.
                if matches!(incoming, Some(Ok(_))) {
                    last_seen = Instant::now();
                }
                match incoming {
                    None | Some(Ok(Message::Close(_))) => break,
                    // A read error, including a frame over the hard cap: the connection is
                    // already gone.
                    Some(Err(_)) => break,
                    Some(Ok(Message::Binary(bytes))) if bytes.len() > MAX_FRAME_BYTES => {
                        close(&mut socket, CLOSE_TOO_BIG, "frame too large", config.send_timeout).await;
                        break;
                    }
                    Some(Ok(Message::Binary(bytes))) => match apply_frame(&state, &bytes).await {
                        Ok(()) => {
                            let _ = state.inner.updates.send((conn_id, bytes));
                        }
                        Err(Rejected::TooBig) => {
                            close(&mut socket, CLOSE_TOO_BIG, "document too large", config.send_timeout).await;
                            break;
                        }
                        Err(Rejected::Invalid) => {
                            close(&mut socket, CLOSE_POLICY, "invalid update", config.send_timeout).await;
                            break;
                        }
                    },
                    Some(Ok(Message::Text(_))) => {
                        close(&mut socket, CLOSE_POLICY, "binary frames only", config.send_timeout).await;
                        break;
                    }
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                }
            },
            forwarded = updates.recv() => match forwarded {
                Ok((from, bytes)) => {
                    if from != conn_id
                        && !send_within(&mut socket, Message::Binary(bytes), config.send_timeout).await
                    {
                        break;
                    }
                }
                // A slow consumer is disconnected and reconnects (docs/ARCHITECTURE.md §4).
                Err(RecvError::Lagged(_)) => {
                    close(&mut socket, CLOSE_POLICY, "too slow", config.send_timeout).await;
                    break;
                }
                Err(RecvError::Closed) => break,
            },
            _ = ping.tick() => {
                if last_seen.elapsed() > config.idle_timeout {
                    close(&mut socket, CLOSE_POLICY, "idle", config.send_timeout).await;
                    break;
                }
                if !send_within(&mut socket, Message::Ping(Bytes::new()), config.send_timeout).await {
                    break;
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant as StdInstant;

    /// A sink whose send never completes, like a socket whose peer stopped reading and whose
    /// buffers are full.
    fn stuck_sink() -> impl Sink<Message, Error = std::convert::Infallible> + Unpin {
        Box::pin(futures_util::sink::unfold((), |(), _message: Message| {
            std::future::pending::<Result<(), std::convert::Infallible>>()
        }))
    }

    #[tokio::test]
    async fn send_within_gives_up_on_a_peer_that_never_reads() {
        let mut sink = stuck_sink();
        let started = StdInstant::now();
        // The outer deadline turns a missing timeout into a failure instead of a hang.
        let sent = timeout(
            Duration::from_secs(2),
            send_within(
                &mut sink,
                Message::Binary(Bytes::from_static(b"x")),
                Duration::from_millis(100),
            ),
        )
        .await;
        assert_eq!(
            sent.ok(),
            Some(false),
            "send_within did not give up by itself"
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn send_within_reports_success_for_a_peer_that_reads() {
        let mut sink = Box::pin(futures_util::sink::drain());
        assert!(
            send_within(
                &mut sink,
                Message::Binary(Bytes::new()),
                Duration::from_secs(1)
            )
            .await
        );
    }

    #[tokio::test]
    async fn close_does_not_hang_on_a_stuck_peer() {
        let mut sink = stuck_sink();
        let started = StdInstant::now();
        let finished = timeout(
            Duration::from_secs(2),
            close(
                &mut sink,
                CLOSE_POLICY,
                "too slow",
                Duration::from_millis(100),
            ),
        )
        .await;
        assert!(finished.is_ok(), "close hung on a stuck peer");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn apply_refuses_an_update_that_could_pass_the_size_cap() {
        let config = SpikeConfig {
            max_state_bytes: 200,
            ..SpikeConfig::default()
        };
        let state = SpikeState::new(config).unwrap();
        let author = SpikeDoc::new(5).unwrap();

        let small = author.insert(0, "hello").unwrap();
        assert_eq!(state.apply(&small), Ok(()));

        let big = author.insert(5, &"x".repeat(300)).unwrap();
        assert_eq!(state.apply(&big), Err(Rejected::TooBig));
        // The refused update changed nothing.
        assert_eq!(state.doc().text(), "hello");
    }

    #[test]
    fn apply_reports_garbage_as_invalid() {
        let state = SpikeState::new(SpikeConfig::default()).unwrap();
        assert_eq!(state.apply(&[0xFF; 16]), Err(Rejected::Invalid));
    }
}

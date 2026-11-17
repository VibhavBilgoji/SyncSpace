//! The document socket: `GET /ws/v1/docs/{id}?ticket=...` (`docs/API.md` section 3).
//!
//! What happens, in order:
//! 1. **Origin**: a browser always sends it; it must be one of `ALLOWED_ORIGINS`, otherwise the
//!    handshake is refused with 403 before anything else. A missing Origin is refused too.
//! 2. **Upgrade**, then the **ticket**: genuine, not expired, for this document, and not used
//!    before (it is burnt at once). Anything wrong closes with 4001.
//! 3. **The document** is loaded (4004 when it does not exist) and the user must still be a
//!    member of its workspace (4003 otherwise). Their role is kept for the life of the socket.
//! 4. **The loop**: every binary frame is classified first; a viewer's writes are refused with a
//!    JSON `ERROR READ_ONLY` and never reach the document; writes go through the guarded apply of
//!    `sync-crdt` and are broadcast to the others; awareness is relayed; text frames are `PING`.
//!    Every send has a timeout, idle peers are pinged and then dropped, a consumer that falls
//!    too far behind is dropped, and large updates are applied off the async threads.
//!
//! A role that changes while the socket is open is not pushed to it (future work, `ROADMAP.md`
//! section 6): the new role applies when the socket reconnects.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::http::header::ORIGIN;
use axum::response::Response;
use futures_util::{Sink, SinkExt};
use mongodb::bson::oid::ObjectId;
use serde::Deserialize;
use sync_crdt::{FrameKind, frame_kind, update_frame};
use tokio::sync::broadcast::error::RecvError;
use tokio::time::{Instant, MissedTickBehavior, interval, timeout};

use super::frames::{ClientFrame, ServerFrame, SocketErrorCode};
use super::limits::TokenBucket;
use super::room::{DocRoom, Outgoing, RoomError};
use super::{
    CLOSE_DOCUMENT_GONE, CLOSE_FORBIDDEN, CLOSE_INTERNAL, CLOSE_POLICY, CLOSE_TOO_BIG,
    CLOSE_UNAUTHORIZED, HubConfig, RoomLookup,
};
use crate::app::AppState;
use crate::auth::ticket::{self, Scope};
use crate::error::ApiError;
use crate::rbac::{Role, parse_id, require_role};

/// Updates bigger than this are applied on a blocking thread instead of a Tokio worker.
const BLOCKING_THRESHOLD_BYTES: usize = 16 * 1024;

#[derive(Debug, Deserialize)]
pub struct TicketQuery {
    pub(super) ticket: Option<String>,
}

/// The `Origin` header must be present and one of the allowed origins, as written in the config.
pub(super) fn check_origin(headers: &HeaderMap, allowed: &[String]) -> Result<(), ApiError> {
    let origin = headers
        .get(ORIGIN)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::forbidden("origin not allowed"))?;
    if allowed.iter().any(|candidate| candidate == origin) {
        Ok(())
    } else {
        Err(ApiError::forbidden("origin not allowed"))
    }
}

pub async fn doc_socket(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<TicketQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    check_origin(&headers, &state.config.allowed_origins)?;
    let doc_id = parse_id(&id)?;
    let hard_limit = state.hub.config.hard_limit_bytes;
    Ok(ws
        .max_message_size(hard_limit)
        .max_frame_size(hard_limit)
        .on_upgrade(move |socket| run(socket, state, doc_id, query.ticket)))
}

/// Who is on the socket and what they may do.
struct Identity {
    room: Arc<DocRoom>,
    role: Role,
    user: ObjectId,
}

type Refusal = (u16, &'static str);

/// Checks the ticket, loads the room and checks that the user is still a member.
async fn authorise(
    state: &AppState,
    doc_id: ObjectId,
    token: Option<String>,
) -> Result<Identity, Refusal> {
    let unauthorized = (CLOSE_UNAUTHORIZED, "unauthorized");
    let token = token.ok_or(unauthorized)?;
    let ticket =
        ticket::verify(state.config.ticket_secret.expose(), &token).map_err(|_| unauthorized)?;
    if ticket.scope != Scope::Doc || ticket.id != doc_id {
        return Err(unauthorized);
    }
    // Burn the ticket before anything else can fail, so a refused attempt cannot be retried.
    if !state.hub.tickets.consume(&ticket) {
        return Err(unauthorized);
    }

    let room = match state.hub.room(&state.db, doc_id).await {
        Ok(room) => room,
        Err(RoomLookup::Missing) => return Err((CLOSE_DOCUMENT_GONE, "document not found")),
        Err(RoomLookup::Failed(error)) => {
            tracing::error!(error = ?error, "could not open a document room");
            return Err((CLOSE_INTERNAL, "internal error"));
        }
    };
    let granted = require_role(&state.db, room.workspace_id, ticket.user, Role::Viewer).await;
    let granted = match granted {
        Ok(granted) => granted,
        Err(error) => {
            // The room was loaded for this attempt: let go of it so a refused user cannot pin it.
            state.hub.release(&state.db, &room).await;
            return Err(match error {
                ApiError::NotFound(_) | ApiError::Forbidden(_) => (CLOSE_FORBIDDEN, "forbidden"),
                _ => (CLOSE_INTERNAL, "internal error"),
            });
        }
    };
    Ok(Identity {
        room,
        role: granted.role,
        user: ticket.user,
    })
}

/// Sends a message and reports whether it went out in time. A peer that stops reading would
/// otherwise block this task inside `send` for as long as the connection lives, and the task
/// could not reach any other branch, including the lag check.
pub(super) async fn send_within<S>(sink: &mut S, message: Message, limit: Duration) -> bool
where
    S: Sink<Message> + Unpin,
{
    matches!(timeout(limit, sink.send(message)).await, Ok(Ok(())))
}

pub(super) async fn close<S>(sink: &mut S, code: u16, reason: &'static str, limit: Duration)
where
    S: Sink<Message> + Unpin,
{
    let frame = CloseFrame {
        code,
        reason: reason.into(),
    };
    let _ = send_within(sink, Message::Close(Some(frame)), limit).await;
}

/// What to do after a frame was handled.
enum Action {
    Send(Message),
    Close(u16, &'static str),
}

fn text(frame: &ServerFrame) -> Action {
    Action::Send(Message::Text(frame.to_text().into()))
}

fn binary(bytes: Vec<u8>) -> Action {
    Action::Send(Message::Binary(Bytes::from(bytes)))
}

/// Per-connection state.
/// The longest text frame accepted (`PING` is under 40 bytes).
pub(super) const MAX_TEXT_BYTES: usize = 1024;

struct Connection {
    id: u64,
    user: ObjectId,
    role: Role,
    room: Arc<DocRoom>,
    updates: TokenBucket,
    awareness: TokenBucket,
    sent_step1: bool,
    config: HubConfig,
}

impl Connection {
    /// The server's own SyncStep1, once per connection.
    fn step1_once(&mut self, actions: &mut Vec<Action>) {
        if !self.sent_step1 {
            self.sent_step1 = true;
            actions.push(binary(self.room.sync_step1_frame()));
        }
    }

    /// Applies a frame to the room, moving big ones off the async threads so that decoding 1 MiB
    /// does not stall every other connection that shares the worker.
    async fn apply(&self, bytes: &Bytes, kind: FrameKind) -> Result<sync_crdt::Handled, RoomError> {
        if bytes.len() <= BLOCKING_THRESHOLD_BYTES {
            return self.room.apply_frame(bytes, kind);
        }
        let room = self.room.clone();
        let data = bytes.clone();
        tokio::task::spawn_blocking(move || room.apply_frame(&data, kind))
            .await
            .unwrap_or(Err(RoomError::Crdt(sync_crdt::CrdtError::Apply)))
    }

    async fn on_binary(&mut self, bytes: Bytes) -> Vec<Action> {
        if bytes.len() > self.config.max_frame_bytes {
            return vec![Action::Close(CLOSE_TOO_BIG, "frame too large")];
        }
        let Ok(kind) = frame_kind(&bytes) else {
            return vec![Action::Close(CLOSE_POLICY, "invalid frame")];
        };

        match kind {
            FrameKind::Awareness => {
                if !self.awareness.try_take() {
                    return vec![text(&ServerFrame::error(
                        SocketErrorCode::RateLimited,
                        "too many awareness frames",
                    ))];
                }
                self.room.broadcast(Outgoing::Frame {
                    from: self.id,
                    bytes,
                });
                Vec::new()
            }
            FrameKind::SyncStep1 => {
                if !self.updates.try_take() {
                    return vec![text(&ServerFrame::error(
                        SocketErrorCode::RateLimited,
                        "too many frames",
                    ))];
                }
                match self.apply(&bytes, kind).await {
                    Ok(handled) => {
                        let mut actions = Vec::new();
                        if let Some(reply) = handled.reply {
                            actions.push(binary(reply));
                        }
                        self.step1_once(&mut actions);
                        actions
                    }
                    Err(_) => vec![Action::Close(CLOSE_POLICY, "invalid frame")],
                }
            }
            FrameKind::SyncStep2 | FrameKind::Update => {
                // The role is checked before the frame touches the document.
                if self.role == Role::Viewer {
                    // Refused frames cost a token too, so a viewer cannot make the server answer
                    // at line speed; past the limit the connection is closed.
                    if !self.updates.try_take() {
                        return vec![Action::Close(CLOSE_POLICY, "too many refused frames")];
                    }
                    return vec![text(&ServerFrame::error(
                        SocketErrorCode::ReadOnly,
                        "viewers cannot edit this document",
                    ))];
                }
                if !self.updates.try_take() {
                    return vec![text(&ServerFrame::error(
                        SocketErrorCode::RateLimited,
                        "too many updates",
                    ))];
                }
                let started = Instant::now();
                match self.apply(&bytes, kind).await {
                    Ok(handled) => {
                        if let Some(update) = handled.update {
                            self.room.broadcast(Outgoing::Frame {
                                from: self.id,
                                bytes: Bytes::from(update_frame(&update)),
                            });
                            self.room.metrics().observe_broadcast(started.elapsed());
                        }
                        Vec::new()
                    }
                    Err(RoomError::TooBig) => {
                        vec![Action::Close(CLOSE_TOO_BIG, "document too large")]
                    }
                    // It builds on changes the server lacks: not a violation. Ask for them.
                    Err(RoomError::Crdt(sync_crdt::CrdtError::MissingDependencies)) => {
                        vec![binary(self.room.sync_step1_frame())]
                    }
                    Err(RoomError::Crdt(_)) => {
                        vec![Action::Close(CLOSE_POLICY, "invalid update")]
                    }
                }
            }
        }
    }

    fn on_text(&mut self, body: &str) -> Vec<Action> {
        // The only text frame is a PING of a few bytes: anything long is refused before parsing.
        if body.len() > MAX_TEXT_BYTES {
            return vec![Action::Close(CLOSE_TOO_BIG, "frame too large")];
        }
        match serde_json::from_str::<ClientFrame>(body) {
            Ok(ClientFrame::Ping { t }) => vec![text(&ServerFrame::Pong { t })],
            Err(_) => vec![Action::Close(CLOSE_POLICY, "invalid frame")],
        }
    }
}

async fn run(mut socket: WebSocket, state: AppState, doc_id: ObjectId, token: Option<String>) {
    let config = state.hub.config.clone();
    let identity = match authorise(&state, doc_id, token).await {
        Ok(identity) => identity,
        Err((code, reason)) => {
            close(&mut socket, code, reason, config.send_timeout).await;
            return;
        }
    };

    let mut connection = Connection {
        id: state.hub.next_connection_id(),
        user: identity.user,
        role: identity.role,
        room: identity.room,
        updates: TokenBucket::new(config.updates),
        awareness: TokenBucket::new(config.awareness),
        sent_step1: false,
        config: config.clone(),
    };
    let room = connection.room.clone();

    // Subscribe before joining, so this connection hears the count that includes itself. The
    // join fails when the room was evicted a moment ago (its last socket had just left); the
    // document is then simply loaded again.
    let mut outgoing = room.subscribe();
    let mut room = room;
    let mut joined = state.hub.try_join(&room);
    let mut gone = false;
    if !joined {
        match state.hub.room(&state.db, doc_id).await {
            Ok(fresh) => {
                room = fresh;
                outgoing = room.subscribe();
                joined = state.hub.try_join(&room);
            }
            Err(RoomLookup::Missing) => gone = true,
            Err(RoomLookup::Failed(_)) => {}
        }
    }
    if !joined {
        let (code, reason) = if gone {
            (CLOSE_DOCUMENT_GONE, "document not found")
        } else {
            (CLOSE_INTERNAL, "internal error")
        };
        close(&mut socket, code, reason, config.send_timeout).await;
        return;
    }
    connection.room = room.clone();
    room.announce_presence();

    let mut ping = interval(config.ping_interval);
    ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
    ping.tick().await; // the first tick fires at once
    let mut last_seen = Instant::now();

    'connection: loop {
        let actions = tokio::select! {
            incoming = socket.recv() => {
                // Any frame, including a pong, shows the peer is alive.
                if matches!(incoming, Some(Ok(_))) {
                    last_seen = Instant::now();
                }
                match incoming {
                    // A read error, including a frame over the hard cap: the connection is gone.
                    None | Some(Ok(Message::Close(_)) | Err(_)) => break 'connection,
                    Some(Ok(Message::Binary(bytes))) => connection.on_binary(bytes).await,
                    Some(Ok(Message::Text(body))) => connection.on_text(&body),
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => Vec::new(),
                }
            }
            forwarded = outgoing.recv() => match forwarded {
                Ok(Outgoing::Frame { from, bytes }) => {
                    if from == connection.id { Vec::new() } else { vec![Action::Send(Message::Binary(bytes))] }
                }
                Ok(Outgoing::Json(body)) => vec![Action::Send(Message::Text(body.into()))],
                Ok(Outgoing::Close { code, reason }) => vec![Action::Close(code, reason)],
                Ok(Outgoing::CloseUser { user, code, reason }) => {
                    if user == connection.user { vec![Action::Close(code, reason)] } else { Vec::new() }
                }
                // A slow consumer is disconnected and reconnects (`docs/ARCHITECTURE.md` section 4).
                Err(RecvError::Lagged(_)) => vec![Action::Close(CLOSE_POLICY, "too slow")],
                Err(RecvError::Closed) => break 'connection,
            },
            _ = ping.tick() => {
                if last_seen.elapsed() > config.idle_timeout {
                    vec![Action::Close(CLOSE_POLICY, "idle")]
                } else {
                    vec![Action::Send(Message::Ping(Bytes::new()))]
                }
            }
        };

        for action in actions {
            match action {
                Action::Send(message) => {
                    if !send_within(&mut socket, message, config.send_timeout).await {
                        break 'connection;
                    }
                }
                Action::Close(code, reason) => {
                    close(&mut socket, code, reason, config.send_timeout).await;
                    break 'connection;
                }
            }
        }
    }

    let remaining = room.leave();
    room.announce_presence();
    if remaining == 0 {
        state.hub.release(&state.db, &room).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use std::time::Instant as StdInstant;

    fn headers(origin: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(origin) = origin {
            headers.insert(ORIGIN, HeaderValue::from_str(origin).unwrap());
        }
        headers
    }

    #[test]
    fn the_origin_must_be_present_and_listed() {
        let allowed = vec![
            "https://app.example".to_string(),
            "http://localhost:3000".to_string(),
        ];
        assert!(check_origin(&headers(Some("https://app.example")), &allowed).is_ok());
        assert!(check_origin(&headers(Some("http://localhost:3000")), &allowed).is_ok());
        for bad in [
            Some("https://evil.example"),
            Some("https://app.example.evil.example"),
            Some("http://app.example"),
            Some("https://app.example/"),
            Some("null"),
            None,
        ] {
            assert!(check_origin(&headers(bad), &allowed).is_err(), "{bad:?}");
        }
    }

    /// A sink whose send never completes, like a socket whose peer stopped reading.
    fn stuck_sink() -> impl Sink<Message, Error = std::convert::Infallible> + Unpin {
        Box::pin(futures_util::sink::unfold((), |(), _message: Message| {
            std::future::pending::<Result<(), std::convert::Infallible>>()
        }))
    }

    #[tokio::test]
    async fn send_within_gives_up_on_a_peer_that_never_reads() {
        let mut sink = stuck_sink();
        let started = StdInstant::now();
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
    async fn close_does_not_hang_on_a_stuck_peer() {
        let mut sink = stuck_sink();
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
    }
}

//! `GET /ws/v1/workspaces/:id` (`docs/API.md` section 3, "Workspace socket"): live events of a
//! workspace, such as `BOARD_EVENT`. The handshake is the document socket's: the `Origin` is
//! checked before the upgrade, then a single-use ticket of scope `workspace` and the caller's
//! membership. The server only sends; a client may send `PING`.
//!
//! The role is not needed here (everyone who may read the workspace may hear its events), but the
//! membership is checked once at connect, and `Hub::close_user` closes the socket when the user is
//! removed or their role changes (ADR-014).

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use mongodb::bson::oid::ObjectId;
use tokio::sync::broadcast::error::RecvError;
use tokio::time::{Instant, MissedTickBehavior, interval};

use super::frames::{ClientFrame, ServerFrame};
use super::limits::TokenBucket;
use super::room::Outgoing;
use super::socket::{MAX_TEXT_BYTES, TicketQuery, check_origin, close, send_within};
use super::workspace::WorkspaceRoom;
use super::{CLOSE_FORBIDDEN, CLOSE_INTERNAL, CLOSE_POLICY, CLOSE_TOO_BIG, CLOSE_UNAUTHORIZED};
use crate::app::AppState;
use crate::auth::ticket::{self, Scope};
use crate::error::ApiError;
use crate::rbac::{Role, parse_id, require_role};

pub async fn workspace_socket(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<TicketQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    check_origin(&headers, &state.config.allowed_origins)?;
    let workspace_id = parse_id(&id)?;
    let hard_limit = state.hub.config.hard_limit_bytes;
    Ok(ws
        .max_message_size(hard_limit)
        .max_frame_size(hard_limit)
        .on_upgrade(move |socket| run(socket, state, workspace_id, query.ticket)))
}

type Refusal = (u16, &'static str);

/// Checks the ticket and that the user is still a member; returns the user.
async fn authorise(
    state: &AppState,
    workspace_id: ObjectId,
    token: Option<String>,
) -> Result<ObjectId, Refusal> {
    let unauthorized = (CLOSE_UNAUTHORIZED, "unauthorized");
    let token = token.ok_or(unauthorized)?;
    let ticket =
        ticket::verify(state.config.ticket_secret.expose(), &token).map_err(|_| unauthorized)?;
    if ticket.scope != Scope::Workspace || ticket.id != workspace_id {
        return Err(unauthorized);
    }
    // Burn the ticket before anything else can fail, so a refused attempt cannot be retried.
    if !state.hub.tickets.consume(&ticket) {
        return Err(unauthorized);
    }
    match require_role(&state.db, workspace_id, ticket.user, Role::Viewer).await {
        Ok(_) => Ok(ticket.user),
        // An unknown workspace and a missing membership look the same.
        Err(ApiError::NotFound(_) | ApiError::Forbidden(_)) => Err((CLOSE_FORBIDDEN, "forbidden")),
        Err(_) => Err((CLOSE_INTERNAL, "internal error")),
    }
}

async fn run(
    mut socket: WebSocket,
    state: AppState,
    workspace_id: ObjectId,
    token: Option<String>,
) {
    let config = state.hub.config.clone();
    let user = match authorise(&state, workspace_id, token).await {
        Ok(user) => user,
        Err((code, reason)) => {
            close(&mut socket, code, reason, config.send_timeout).await;
            return;
        }
    };

    // Counted in and subscribed together: an event sent from now on reaches this socket.
    let room: Arc<WorkspaceRoom> = state.hub.join_workspace(workspace_id);
    let mut outgoing = room.subscribe();
    let mut pings = TokenBucket::new(config.awareness);

    let mut tick = interval(config.ping_interval);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    tick.tick().await; // the first tick fires at once
    let mut last_seen = Instant::now();

    loop {
        // The action to take: send a message, or close with a code.
        let step: Step = tokio::select! {
            incoming = socket.recv() => {
                if matches!(incoming, Some(Ok(_))) {
                    last_seen = Instant::now();
                }
                match incoming {
                    None | Some(Ok(Message::Close(_)) | Err(_)) => break,
                    Some(Ok(Message::Text(body))) => {
                        if body.len() > MAX_TEXT_BYTES {
                            Step::Close(CLOSE_TOO_BIG, "frame too large")
                        } else if !pings.try_take() {
                            Step::Close(CLOSE_POLICY, "too many frames")
                        } else {
                            match serde_json::from_str::<ClientFrame>(&body) {
                                Ok(ClientFrame::Ping { t }) => {
                                    Step::Send(Message::Text(ServerFrame::Pong { t }.to_text().into()))
                                }
                                Err(_) => Step::Close(CLOSE_POLICY, "invalid frame"),
                            }
                        }
                    }
                    // This socket carries no binary frames.
                    Some(Ok(Message::Binary(_))) => Step::Close(CLOSE_POLICY, "invalid frame"),
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => Step::Nothing,
                }
            }
            forwarded = outgoing.recv() => match forwarded {
                Ok(Outgoing::Json(body)) => Step::Send(Message::Text(body.into())),
                Ok(Outgoing::Close { code, reason }) => Step::Close(code, reason),
                Ok(Outgoing::CloseUser { user: target, code, reason }) => {
                    if target == user { Step::Close(code, reason) } else { Step::Nothing }
                }
                Ok(Outgoing::Frame { .. }) => Step::Nothing,
                Err(RecvError::Lagged(_)) => Step::Close(CLOSE_POLICY, "too slow"),
                Err(RecvError::Closed) => break,
            },
            _ = tick.tick() => {
                if last_seen.elapsed() > config.idle_timeout {
                    Step::Close(CLOSE_POLICY, "idle")
                } else {
                    Step::Send(Message::Ping(axum::body::Bytes::new()))
                }
            }
        };
        match step {
            Step::Nothing => {}
            Step::Send(message) => {
                if !send_within(&mut socket, message, config.send_timeout).await {
                    break;
                }
            }
            Step::Close(code, reason) => {
                close(&mut socket, code, reason, config.send_timeout).await;
                break;
            }
        }
    }
    state.hub.leave_workspace(&room);
}

enum Step {
    Nothing,
    Send(Message),
    Close(u16, &'static str),
}

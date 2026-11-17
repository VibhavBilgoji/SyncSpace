//! A WebSocket client for the hub tests: it speaks the y-sync frames with a CRDT replica of its
//! own, so a test can edit like a browser would.

#![allow(dead_code)]
#![allow(clippy::result_large_err)] // `tungstenite::Error` is large; test code
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use sync_crdt::{Document, update_frame};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use super::{ALLOWED_ORIGIN, Account, TestApp};

/// Longest a test waits for the server before it calls that a hang.
pub const WAIT: Duration = Duration::from_secs(10);

pub type Stream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// What arrived, with pings and pongs left out.
#[derive(Debug, PartialEq)]
pub enum Frame {
    Binary(Vec<u8>),
    Json(Value),
    /// A close frame with its code, or `None` when the connection just ended.
    Closed(Option<u16>),
}

pub struct WsClient {
    pub stream: Stream,
    /// This client's own replica, like the browser's.
    pub doc: Document,
}

/// Asks for a ticket over REST, as a browser would.
pub async fn ticket_for(app: &TestApp, who: &Account, scope: &str, id: &str) -> String {
    let (status, body) = app
        .call(
            "POST",
            "/api/v1/auth/ws-ticket",
            Some(who),
            Some(json!({"scope": scope, "id": id})),
        )
        .await;
    assert_eq!(status, 200, "ticket for {who:?}: {body}");
    body["ticket"].as_str().unwrap().to_string()
}

pub fn doc_url(app: &TestApp, doc_id: &str, ticket: Option<&str>) -> String {
    match ticket {
        Some(ticket) => format!("ws://{}/ws/v1/docs/{doc_id}?ticket={ticket}", app.addr),
        None => format!("ws://{}/ws/v1/docs/{doc_id}", app.addr),
    }
}

/// Opens the handshake with an explicit `Origin` (or none) and ticket.
pub async fn connect_raw(
    app: &TestApp,
    doc_id: &str,
    ticket: Option<&str>,
    origin: Option<&str>,
) -> Result<Stream, WsError> {
    let mut request = doc_url(app, doc_id, ticket).into_client_request()?;
    if let Some(origin) = origin {
        request
            .headers_mut()
            .insert("Origin", HeaderValue::from_str(origin).unwrap());
    }
    let (stream, _) = connect_async(request).await?;
    Ok(stream)
}

pub fn workspace_url(app: &TestApp, workspace_id: &str, ticket: Option<&str>) -> String {
    match ticket {
        Some(ticket) => format!(
            "ws://{}/ws/v1/workspaces/{workspace_id}?ticket={ticket}",
            app.addr
        ),
        None => format!("ws://{}/ws/v1/workspaces/{workspace_id}", app.addr),
    }
}

/// Opens the workspace-socket handshake with an explicit `Origin` (or none) and ticket.
pub async fn connect_workspace_raw(
    app: &TestApp,
    workspace_id: &str,
    ticket: Option<&str>,
    origin: Option<&str>,
) -> Result<Stream, WsError> {
    let mut request = workspace_url(app, workspace_id, ticket).into_client_request()?;
    if let Some(origin) = origin {
        request
            .headers_mut()
            .insert("Origin", HeaderValue::from_str(origin).unwrap());
    }
    let (stream, _) = connect_async(request).await?;
    Ok(stream)
}

impl WsClient {
    /// A workspace socket with a fresh ticket of `who` (the replica is unused: the workspace
    /// socket carries only JSON).
    pub async fn open_workspace(app: &TestApp, who: &Account, workspace_id: &str) -> Self {
        let ticket = ticket_for(app, who, "workspace", workspace_id).await;
        let stream = connect_workspace_raw(app, workspace_id, Some(&ticket), Some(ALLOWED_ORIGIN))
            .await
            .expect("the handshake should succeed");
        Self {
            stream,
            doc: Document::new(1).unwrap(),
        }
    }

    /// The next JSON frame of type `kind`, skipping others; a close is a failure.
    pub async fn next_event(&mut self, kind: &str) -> Value {
        self.recv_json(kind).await
    }

    /// Whether any JSON frame arrives within `limit` (for "nothing was sent").
    pub async fn quiet_for(&mut self, limit: Duration) -> bool {
        self.recv_within(limit).await.is_none()
    }

    /// Connects with a fresh ticket of `who`, the allowed origin and a replica with `client_id`.
    pub async fn open(app: &TestApp, who: &Account, doc_id: &str, client_id: u64) -> Self {
        let ticket = ticket_for(app, who, "doc", doc_id).await;
        let stream = connect_raw(app, doc_id, Some(&ticket), Some(ALLOWED_ORIGIN))
            .await
            .expect("the handshake should succeed");
        Self {
            stream,
            doc: Document::new(client_id).unwrap(),
        }
    }

    /// The next frame, or a failure if the server says nothing for [`WAIT`].
    pub async fn recv(&mut self) -> Frame {
        self.recv_within(WAIT).await.expect("the server went quiet")
    }

    /// The next frame, or `None` if nothing arrives within `limit`.
    pub async fn recv_within(&mut self, limit: Duration) -> Option<Frame> {
        loop {
            let next = timeout(limit, self.stream.next()).await.ok()?;
            match next {
                None | Some(Err(_)) => return Some(Frame::Closed(None)),
                Some(Ok(Message::Binary(bytes))) => return Some(Frame::Binary(bytes.to_vec())),
                Some(Ok(Message::Text(body))) => {
                    return Some(Frame::Json(serde_json::from_str(&body).unwrap()));
                }
                Some(Ok(Message::Close(frame))) => {
                    return Some(Frame::Closed(frame.map(|f| u16::from(f.code))));
                }
                Some(Ok(_)) => {}
            }
        }
    }

    /// The next JSON frame of this `type`, skipping other JSON frames.
    pub async fn recv_json(&mut self, kind: &str) -> Value {
        loop {
            match self.recv().await {
                Frame::Json(value) if value["type"] == kind => return value,
                Frame::Json(_) => {}
                other => panic!("expected a {kind} frame, got {other:?}"),
            }
        }
    }

    /// The next binary frame, skipping JSON frames (presence counts).
    pub async fn recv_binary(&mut self) -> Vec<u8> {
        loop {
            match self.recv().await {
                Frame::Binary(bytes) => return bytes,
                Frame::Json(_) => {}
                other => panic!("expected a binary frame, got {other:?}"),
            }
        }
    }

    /// Reads until the connection closes and returns the close code (`None` without a code).
    pub async fn recv_close(&mut self) -> Option<u16> {
        loop {
            match self.recv().await {
                Frame::Closed(code) => return code,
                Frame::Binary(_) | Frame::Json(_) => {}
            }
        }
    }

    pub async fn send_binary(&mut self, bytes: Vec<u8>) {
        self.stream
            .send(Message::Binary(bytes.into()))
            .await
            .unwrap();
    }

    pub async fn send_text(&mut self, body: &str) {
        self.stream.send(Message::Text(body.into())).await.unwrap();
    }

    /// Sends an update the local replica made.
    pub async fn send_update(&mut self, update: &[u8]) {
        self.send_binary(update_frame(update)).await;
    }

    /// The join handshake: send SyncStep1, then take the SyncStep2 and the server's SyncStep1
    /// into the local replica (and answer it).
    pub async fn handshake(&mut self) {
        let step1 = self.doc.sync_step1_frame();
        self.send_binary(step1).await;
        let step2 = self.recv_binary().await; // the answer to our step 1
        let server_step1 = self.recv_binary().await; // the server's own step 1
        self.doc.handle_message(&step2).unwrap();
        let reply = self
            .doc
            .handle_message(&server_step1)
            .unwrap()
            .reply
            .unwrap();
        self.send_binary(reply).await;
    }

    /// The join handshake of a viewer: like [`WsClient::handshake`] but without the final
    /// SyncStep2, which a viewer must not send.
    pub async fn handshake_read_only(&mut self) {
        let step1 = self.doc.sync_step1_frame();
        self.send_binary(step1).await;
        let step2 = self.recv_binary().await;
        let _server_step1 = self.recv_binary().await;
        self.doc.handle_message(&step2).unwrap();
    }

    /// Takes a binary frame from the server into the local replica.
    pub fn apply(&self, frame: &[u8]) {
        self.doc.handle_message(frame).unwrap();
    }

    pub async fn close(mut self) {
        let _ = self.stream.close(None).await;
    }
}

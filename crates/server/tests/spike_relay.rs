//! Integration tests for the throwaway spike relay, over real WebSocket connections.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test code may unwrap (CLAUDE.md rule 3 covers non-test code)

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use sync_crdt::SpikeDoc;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

const WAIT: Duration = Duration::from_secs(5);
const QUIET: Duration = Duration::from_millis(300);

async fn start_relay() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, server::spike::router())
            .await
            .unwrap();
    });
    addr
}

async fn connect(addr: std::net::SocketAddr) -> Client {
    let (ws, _) = connect_async(format!("ws://{addr}/spike")).await.unwrap();
    ws
}

/// Next binary frame, ignoring pings.
async fn next_binary(ws: &mut Client) -> Vec<u8> {
    loop {
        let msg = timeout(WAIT, ws.next())
            .await
            .expect("timed out waiting for a frame")
            .expect("socket closed")
            .expect("socket error");
        match msg {
            Message::Binary(bytes) => return bytes.to_vec(),
            Message::Ping(_) | Message::Pong(_) => {}
            other => panic!("expected a binary frame, got {other:?}"),
        }
    }
}

/// The close code the relay ends the connection with.
async fn close_code(ws: &mut Client) -> u16 {
    loop {
        let msg = timeout(WAIT, ws.next())
            .await
            .expect("timed out waiting for close");
        match msg {
            Some(Ok(Message::Close(Some(frame)))) => return u16::from(frame.code),
            Some(Ok(_)) => {}
            other => panic!("expected a close frame, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn relay_broadcasts_update_to_other_client() {
    let addr = start_relay().await;
    let mut a = connect(addr).await;
    let mut b = connect(addr).await;
    next_binary(&mut a).await; // initial state
    next_binary(&mut b).await;

    let author = SpikeDoc::new(5).unwrap();
    let update = author.insert(0, "hello").unwrap();
    a.send(Message::Binary(update.clone().into()))
        .await
        .unwrap();

    assert_eq!(next_binary(&mut b).await, update);

    // The sender does not get its own update back.
    assert!(timeout(QUIET, a.next()).await.is_err());
}

#[tokio::test]
async fn relay_sends_full_state_to_late_joiner() {
    let addr = start_relay().await;
    let mut a = connect(addr).await;
    let mut b = connect(addr).await;
    next_binary(&mut a).await;
    next_binary(&mut b).await;

    let author = SpikeDoc::new(5).unwrap();
    a.send(Message::Binary(author.insert(0, "hi 🙂").unwrap().into()))
        .await
        .unwrap();
    // B receiving it proves the relay applied it before the late joiner connects.
    next_binary(&mut b).await;

    let mut late = connect(addr).await;
    let state = next_binary(&mut late).await;
    let replica = SpikeDoc::new(9).unwrap();
    replica.apply_update(&state).unwrap();
    assert_eq!(replica.text(), "hi 🙂");
}

#[tokio::test]
async fn relay_closes_with_1008_on_garbage_and_keeps_serving_others() {
    let addr = start_relay().await;
    let mut bad = connect(addr).await;
    let mut a = connect(addr).await;
    let mut b = connect(addr).await;
    next_binary(&mut bad).await;
    next_binary(&mut a).await;
    next_binary(&mut b).await;

    bad.send(Message::Binary(vec![0xFF; 16].into()))
        .await
        .unwrap();
    assert_eq!(close_code(&mut bad).await, 1008);

    let author = SpikeDoc::new(5).unwrap();
    let update = author.insert(0, "still works").unwrap();
    a.send(Message::Binary(update.clone().into()))
        .await
        .unwrap();
    assert_eq!(next_binary(&mut b).await, update);
}

#[tokio::test]
async fn relay_closes_with_1008_on_text_frames() {
    let addr = start_relay().await;
    let mut ws = connect(addr).await;
    next_binary(&mut ws).await;
    ws.send(Message::Text("hello".into())).await.unwrap();
    assert_eq!(close_code(&mut ws).await, 1008);
}

#[tokio::test]
async fn relay_closes_with_1009_on_oversized_frame() {
    let addr = start_relay().await;
    let mut ws = connect(addr).await;
    next_binary(&mut ws).await;
    let too_big = vec![0u8; 1024 * 1024 + 1];
    ws.send(Message::Binary(too_big.into())).await.unwrap();
    assert_eq!(close_code(&mut ws).await, 1009);
}

#[tokio::test]
async fn healthz_answers_ok() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let addr = start_relay().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(b"GET /healthz HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.ends_with("ok"), "{response}");
}

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

#[tokio::test]
async fn relay_survives_a_client_that_disconnects_abruptly() {
    let addr = start_relay().await;
    let mut gone = connect(addr).await;
    let mut a = connect(addr).await;
    let mut b = connect(addr).await;
    next_binary(&mut gone).await;
    next_binary(&mut a).await;
    next_binary(&mut b).await;

    // Drop without a close handshake, then keep editing.
    drop(gone);

    let author = SpikeDoc::new(5).unwrap();
    let u1 = author.insert(0, "one").unwrap();
    a.send(Message::Binary(u1.clone().into())).await.unwrap();
    assert_eq!(next_binary(&mut b).await, u1);
    let u2 = author.insert(3, "two").unwrap();
    a.send(Message::Binary(u2.clone().into())).await.unwrap();
    assert_eq!(next_binary(&mut b).await, u2);
}

#[tokio::test]
async fn relay_does_not_apply_an_invalid_update_to_the_shared_document() {
    let addr = start_relay().await;
    let mut a = connect(addr).await;
    let mut bad = connect(addr).await;
    next_binary(&mut a).await;
    next_binary(&mut bad).await;

    let author = SpikeDoc::new(5).unwrap();
    a.send(Message::Binary(author.insert(0, "good").unwrap().into()))
        .await
        .unwrap();
    // Wait until the relay has applied it (the sender gets no echo, so poll via a joiner).
    tokio::time::sleep(Duration::from_millis(100)).await;

    bad.send(Message::Binary(vec![0xFF; 32].into()))
        .await
        .unwrap();
    assert_eq!(close_code(&mut bad).await, 1008);

    let mut late = connect(addr).await;
    let replica = SpikeDoc::new(9).unwrap();
    replica.apply_update(&next_binary(&mut late).await).unwrap();
    assert_eq!(replica.text(), "good");
}

#[tokio::test]
async fn relay_accepts_a_frame_of_exactly_the_limit() {
    // 1 MiB is allowed (only larger gets 1009). All zeros decodes as an empty update, so the
    // relay applies it and the connection stays usable.
    let addr = start_relay().await;
    let mut ws = connect(addr).await;
    let mut peer = connect(addr).await;
    next_binary(&mut ws).await;
    next_binary(&mut peer).await;
    ws.send(Message::Binary(vec![0u8; 1024 * 1024].into()))
        .await
        .unwrap();
    assert_eq!(next_binary(&mut peer).await.len(), 1024 * 1024);

    let author = SpikeDoc::new(5).unwrap();
    let update = author.insert(0, "still open").unwrap();
    ws.send(Message::Binary(update.clone().into()))
        .await
        .unwrap();
    assert_eq!(next_binary(&mut peer).await, update);
}

#[tokio::test]
async fn relay_keeps_the_connection_open_across_ping_frames() {
    let addr = start_relay().await;
    let mut a = connect(addr).await;
    let mut b = connect(addr).await;
    next_binary(&mut a).await;
    next_binary(&mut b).await;
    a.send(Message::Ping(vec![1, 2].into())).await.unwrap();
    let author = SpikeDoc::new(5).unwrap();
    let update = author.insert(0, "after ping").unwrap();
    a.send(Message::Binary(update.clone().into()))
        .await
        .unwrap();
    assert_eq!(next_binary(&mut b).await, update);
}

#[tokio::test]
async fn relay_disconnects_a_consumer_that_falls_too_far_behind() {
    let addr = start_relay().await;
    let mut slow = connect(addr).await; // never reads after the first frame
    let mut sender = connect(addr).await;
    next_binary(&mut slow).await;
    next_binary(&mut sender).await;

    // Broadcast capacity is 256 and the slow socket's buffers fill, so a few hundred large
    // updates make the receiver lag.
    let author = SpikeDoc::new(5).unwrap();
    let chunk = "x".repeat(60_000);
    for i in 0..600u32 {
        let update = author.insert(i * 60_000, &chunk).unwrap();
        if sender.send(Message::Binary(update.into())).await.is_err() {
            break;
        }
    }

    // Draining the slow socket must end in a close frame with 1008 , never hang.
    let mut closed_with = None;
    loop {
        match timeout(WAIT, slow.next()).await.expect("slow client hung") {
            Some(Ok(Message::Close(Some(frame)))) => {
                closed_with = Some(u16::from(frame.code));
                break;
            }
            Some(Ok(Message::Close(None))) | Some(Err(_)) | None => break,
            Some(Ok(_)) => {}
        }
    }
    assert_eq!(closed_with, Some(1008));
}

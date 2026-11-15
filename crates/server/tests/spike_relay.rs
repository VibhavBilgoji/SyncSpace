//! Integration tests for the throwaway spike relay, over real WebSocket connections.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test code may unwrap (CLAUDE.md rule 3 covers non-test code)

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use server::spike::{SpikeConfig, router_with_config};
use sync_crdt::SpikeDoc;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

const WAIT: Duration = Duration::from_secs(5);
const QUIET: Duration = Duration::from_millis(300);

async fn start_relay_with(config: SpikeConfig) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router_with_config(config))
            .await
            .unwrap();
    });
    addr
}

async fn start_relay() -> std::net::SocketAddr {
    start_relay_with(SpikeConfig::default()).await
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
    // relay applies it and the connection stays usable. The document size cap is raised so
    // that this test checks the frame limit alone (the cap has its own test).
    let addr = start_relay_with(SpikeConfig {
        max_state_bytes: 8 * 1024 * 1024,
        ..SpikeConfig::default()
    })
    .await;
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
    // A long send timeout keeps this on the lag path; the timeout path has its own tests.
    let addr = start_relay_with(SpikeConfig {
        send_timeout: Duration::from_secs(60),
        ..SpikeConfig::default()
    })
    .await;
    let mut slow = connect(addr).await; // never reads after the first frame
    let mut sender = connect(addr).await;
    next_binary(&mut slow).await;
    next_binary(&mut sender).await;

    // Broadcast capacity is 64 and the slow socket's buffers fill, so a few hundred large
    // frames make the receiver lag. All-zero frames are valid empty updates: they are relayed
    // but do not grow the document, which the size cap would otherwise refuse.
    for _ in 0..600u32 {
        let update = vec![0u8; 60_000];
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

/// The state a late joiner receives must fit the frame limit the client enforces, so the relay
/// refuses updates that would push the document past its cap, and what it already holds stays
/// loadable. Nothing here depends on timing: the writer sends everything at once, the relay
/// handles frames in order and closes the connection at the first refusal, and what was
/// accepted is read back from a late joiner.
#[tokio::test]
async fn relay_refuses_updates_past_the_size_cap_and_late_joiners_still_load() {
    const CAP: usize = 4096;
    let addr = start_relay_with(SpikeConfig {
        max_state_bytes: CAP,
        ..SpikeConfig::default()
    })
    .await;
    let mut writer = connect(addr).await;
    next_binary(&mut writer).await;

    let author = SpikeDoc::new(5).unwrap();
    let chunk = "x".repeat(1000);
    for _ in 0..10 {
        let update = author.insert(author.len(), &chunk).unwrap();
        // The relay may already have closed the connection; that is the outcome under test.
        if writer.send(Message::Binary(update.into())).await.is_err() {
            break;
        }
    }
    assert_eq!(close_code(&mut writer).await, 1009);

    // Everything the relay accepted was applied before it closed the writer.
    let mut late = connect(addr).await;
    let state = next_binary(&mut late).await;
    assert!(
        state.len() <= CAP,
        "state of {} bytes passed the cap",
        state.len()
    );
    let replica = SpikeDoc::new(9).unwrap();
    replica.apply_update(&state).unwrap();
    let held = replica.text();
    assert!(
        held.chars().all(|c| c == 'x') && held.len() % 1000 == 0,
        "the relay must hold whole accepted updates only"
    );
    let accepted = held.len() / 1000;
    assert!(
        (1..10).contains(&accepted),
        "the cap should stop the writer part way, accepted {accepted}"
    );
}

/// Reads and discards frames for `duration`, which answers the relay's pings.
async fn stay_alive(ws: &mut Client, duration: Duration) {
    let end = tokio::time::Instant::now() + duration;
    while let Ok(Some(Ok(_))) = tokio::time::timeout_at(end, ws.next()).await {}
}

/// Whether the relay ends this connection within `limit`: a close frame, an error or EOF.
async fn connection_ends(ws: &mut Client, limit: Duration) -> bool {
    let end = tokio::time::Instant::now() + limit;
    loop {
        match tokio::time::timeout_at(end, ws.next()).await {
            Err(_) => return false,
            Ok(None | Some(Err(_) | Ok(Message::Close(_)))) => return true,
            Ok(Some(Ok(_))) => {}
        }
    }
}

/// A peer that never answers pings is dropped, while one that does keeps its connection.
#[tokio::test]
async fn relay_drops_a_peer_that_stops_answering_pings_and_keeps_a_live_one() {
    let addr = start_relay_with(SpikeConfig {
        ping_interval: Duration::from_millis(200),
        idle_timeout: Duration::from_millis(1500),
        ..SpikeConfig::default()
    })
    .await;
    let mut silent = connect(addr).await; // not polled, so tungstenite never answers pings
    let mut live = connect(addr).await;
    let mut author = connect(addr).await;
    next_binary(&mut silent).await;
    next_binary(&mut live).await;
    next_binary(&mut author).await;

    // Reading is what answers the relay's pings, so poll the two live connections past the
    // idle timeout. Pings are consumed here; the silent peer is left alone.
    tokio::join!(
        stay_alive(&mut live, Duration::from_millis(2500)),
        stay_alive(&mut author, Duration::from_millis(2500)),
    );

    let update = SpikeDoc::new(5).unwrap().insert(0, "still here").unwrap();
    author
        .send(Message::Binary(update.clone().into()))
        .await
        .unwrap();
    assert_eq!(next_binary(&mut live).await, update);

    // The silent peer was dropped for being idle. How the drop looks to it depends on the
    // platform (a close frame, or a reset that discards unread data), so only require that the
    // connection ends, and that it ends instead of idling on as before.
    assert!(
        connection_ends(&mut silent, Duration::from_secs(10)).await,
        "the idle peer was never dropped"
    );
}

/// The 28-byte update that makes Yrs 0.28.1 panic while integrating it. It is the shared
/// fixture of the `sync-crdt` tests and was built against a document where client 1 has
/// typed "existing".
const YRS_PANIC_UPDATE: &[u8] =
    include_bytes!("../../sync-crdt/tests/fixtures/yrs_integration_panic.bin");

/// An update that panics inside Yrs must not take the room down or change it: the sender is
/// closed with 1008, nothing is broadcast, and a client that joins afterwards receives
/// exactly the state the room had before.
#[tokio::test]
async fn relay_closes_with_1008_on_an_update_that_panics_yrs_and_leaves_the_room_untouched() {
    let addr = start_relay().await;
    let mut attacker = connect(addr).await;
    let mut peer = connect(addr).await;
    next_binary(&mut attacker).await;
    next_binary(&mut peer).await;

    // Give the room the content the hostile update was built against.
    let existing = SpikeDoc::new(1).unwrap().insert(0, "existing").unwrap();
    attacker
        .send(Message::Binary(existing.clone().into()))
        .await
        .unwrap();
    assert_eq!(next_binary(&mut peer).await, existing);

    // What a joining client receives before the attack.
    let mut probe = connect(addr).await;
    let before = next_binary(&mut probe).await;

    attacker
        .send(Message::Binary(YRS_PANIC_UPDATE.to_vec().into()))
        .await
        .unwrap();
    assert_eq!(close_code(&mut attacker).await, 1008);

    // Nothing was broadcast.
    assert!(timeout(QUIET, peer.next()).await.is_err());

    // The room still answers, with byte-identical state.
    let mut joiner = connect(addr).await;
    assert_eq!(next_binary(&mut joiner).await, before);

    // And it still accepts good updates from everyone else.
    let update = SpikeDoc::new(7).unwrap().insert(0, "fine").unwrap();
    peer.send(Message::Binary(update.clone().into()))
        .await
        .unwrap();
    assert_eq!(next_binary(&mut joiner).await, update);
}

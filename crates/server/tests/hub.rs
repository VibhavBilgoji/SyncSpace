//! Integration tests for the WebSocket hub (`docs/TESTING.md`: "Tickets" and "WebSocket frames"),
//! against a real MongoDB and the real server on a real socket. Each test client holds a CRDT
//! replica of its own and speaks the y-sync frames, like a browser.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use common::ws::{Frame, WsClient, connect_raw, ticket_for};
use common::{ALLOWED_ORIGIN, Account, AppOptions, KEY_B, TestApp, TestWorkspace};
use mongodb::bson::oid::ObjectId;
use serde_json::{Value, json};
use server::auth::ticket::{self, Scope, TICKET_SECONDS};
use server::db::workspaces;
use server::hub::HubConfig;
use server::hub::limits::BucketConfig;
use server::rbac::Role;
use sync_crdt::{BlockType, Document};
use tokio_tungstenite::tungstenite::Error as WsError;
use tokio_tungstenite::tungstenite::Message;

const CLOSE_POLICY: u16 = 1008;
const CLOSE_TOO_BIG: u16 = 1009;
const CLOSE_UNAUTHORIZED: u16 = 4001;
const CLOSE_FORBIDDEN: u16 = 4003;
const CLOSE_DOCUMENT_GONE: u16 = 4004;

/// A workspace with an owner (ADMIN), an editor and a viewer, and one document in it.
struct Room<'a> {
    workspace: TestWorkspace<'a>,
    doc_id: String,
}

impl<'a> Room<'a> {
    async fn new(app: &'a TestApp) -> Self {
        let workspace = TestWorkspace::new(app)
            .await
            .with_member(Role::Editor)
            .await
            .with_member(Role::Viewer)
            .await;
        let record = server::documents::new_record(
            workspace.id,
            workspace.owner.id,
            "Shared".to_string(),
            "📄".to_string(),
        )
        .unwrap();
        server::db::documents::insert(&app.db, &record)
            .await
            .unwrap();
        Self {
            doc_id: record.id.to_hex(),
            workspace,
        }
    }

    fn admin(&self) -> &Account {
        &self.workspace.owner
    }

    fn editor(&self) -> &Account {
        self.workspace.member(Role::Editor)
    }

    fn viewer(&self) -> &Account {
        self.workspace.member(Role::Viewer)
    }
}

fn ticket_key() -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(KEY_B)
        .unwrap()
}

fn block_id(n: u32) -> String {
    format!("blk{n:09}")
}

/// Connects as `who` and runs the join handshake (a viewer does not send the final SyncStep2).
async fn join(app: &TestApp, room: &Room<'_>, who: &Account, client_id: u64) -> WsClient {
    let mut client = WsClient::open(app, who, &room.doc_id, client_id).await;
    if who.id == room.viewer().id {
        client.handshake_read_only().await;
    } else {
        client.handshake().await;
    }
    client
}

async fn expect_close_code(mut client: WsClient, code: u16) {
    assert_eq!(client.recv_close().await, Some(code));
}

// ----------------------------------------------------------------------------------- tickets

#[tokio::test]
async fn a_member_gets_a_ticket_that_lives_thirty_seconds_and_names_the_document() {
    let app = TestApp::real().await;
    let room = Room::new(&app).await;
    for who in [room.admin(), room.editor(), room.viewer()] {
        let (status, body) = app
            .call(
                "POST",
                "/api/v1/auth/ws-ticket",
                Some(who),
                Some(json!({"scope": "doc", "id": room.doc_id})),
            )
            .await;
        assert_eq!(status, 200, "{body}");
        assert!(body["expires_at"].as_str().unwrap().ends_with('Z'));
        let ticket = ticket::verify(&ticket_key(), body["ticket"].as_str().unwrap()).unwrap();
        assert_eq!(ticket.user, who.id);
        assert_eq!(ticket.scope, Scope::Doc);
        assert_eq!(ticket.id.to_hex(), room.doc_id);
        assert_eq!(ticket.expires_at - TICKET_SECONDS, ticket.expires_at - 30);
    }
    // A workspace ticket for a member too.
    let (status, body) = app
        .call(
            "POST",
            "/api/v1/auth/ws-ticket",
            Some(room.viewer()),
            Some(json!({"scope": "workspace", "id": room.workspace.hex()})),
        )
        .await;
    assert_eq!(status, 200, "{body}");
}

#[tokio::test]
async fn a_ticket_is_refused_to_outsiders_and_for_unknown_ids_with_the_same_404() {
    let app = TestApp::real().await;
    let room = Room::new(&app).await;
    let outsider = app.account("Outsider").await;

    let ask = |who: Option<&Account>, body: Value| {
        let app = &app;
        let who = who.cloned();
        async move {
            app.call("POST", "/api/v1/auth/ws-ticket", who.as_ref(), Some(body))
                .await
        }
    };
    let real = ask(Some(&outsider), json!({"scope": "doc", "id": room.doc_id})).await;
    let unknown = ask(
        Some(&outsider),
        json!({"scope": "doc", "id": ObjectId::new().to_hex()}),
    )
    .await;
    assert_eq!(real.0, 404, "{}", real.1);
    assert_eq!(real, unknown, "existence of a document must not leak");
    assert_eq!(
        ask(
            Some(&outsider),
            json!({"scope": "workspace", "id": room.workspace.hex()})
        )
        .await
        .0,
        404
    );

    assert_eq!(
        ask(None, json!({"scope": "doc", "id": room.doc_id}))
            .await
            .0,
        401
    );
    for bad in [
        json!({"scope": "galaxy", "id": room.doc_id}),
        json!({"scope": "doc"}),
        json!({"id": room.doc_id}),
    ] {
        assert_eq!(ask(Some(room.admin()), bad).await.0, 422);
    }
    assert_eq!(
        ask(Some(room.admin()), json!({"scope": "doc", "id": "nope"}))
            .await
            .0,
        400
    );
}

// ------------------------------------------------------------------------------- the handshake

#[tokio::test]
async fn the_origin_must_be_allowed_or_the_handshake_is_refused_with_403() {
    let app = TestApp::real().await;
    let room = Room::new(&app).await;
    for origin in [None, Some("https://evil.example"), Some("null")] {
        let ticket = ticket_for(&app, room.admin(), "doc", &room.doc_id).await;
        match connect_raw(&app, &room.doc_id, Some(&ticket), origin).await {
            Err(WsError::Http(response)) => assert_eq!(response.status(), 403, "{origin:?}"),
            other => panic!("origin {origin:?} should be refused with 403, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn a_bad_ticket_closes_with_4001() {
    let app = TestApp::real().await;
    let room = Room::new(&app).await;
    let doc = ObjectId::parse_str(&room.doc_id).unwrap();
    let key = ticket_key();
    let other_doc = ObjectId::new();
    let now = server::auth::session::now();
    let make = |secret: &[u8], scope: Scope, id: ObjectId, issued_at: u64| {
        ticket::issue(
            secret,
            room.admin().id,
            scope,
            id,
            issued_at,
            TICKET_SECONDS,
        )
        .unwrap()
        .0
    };

    let cases: Vec<(&str, Option<String>)> = vec![
        ("no ticket", None),
        ("garbage", Some("not-a-ticket".into())),
        (
            "an expired ticket",
            Some(make(&key, Scope::Doc, doc, now - 60)),
        ),
        ("another key", Some(make(&[3u8; 32], Scope::Doc, doc, now))),
        (
            "a ticket for another document",
            Some(make(&key, Scope::Doc, other_doc, now)),
        ),
        (
            "a workspace ticket",
            Some(make(&key, Scope::Workspace, doc, now)),
        ),
        (
            "a session token",
            Some(server::auth::session::issue(&key, room.admin().id, now).unwrap()),
        ),
    ];
    for (name, ticket) in cases {
        let stream = connect_raw(&app, &room.doc_id, ticket.as_deref(), Some(ALLOWED_ORIGIN))
            .await
            .unwrap_or_else(|error| panic!("{name}: handshake failed: {error}"));
        let mut client = WsClient {
            stream,
            doc: Document::new(1).unwrap(),
        };
        assert_eq!(
            client.recv_close().await,
            Some(CLOSE_UNAUTHORIZED),
            "{name}"
        );
    }
}

#[tokio::test]
async fn a_ticket_works_once() {
    let app = TestApp::real().await;
    let room = Room::new(&app).await;
    let ticket = ticket_for(&app, room.editor(), "doc", &room.doc_id).await;

    let first = connect_raw(&app, &room.doc_id, Some(&ticket), Some(ALLOWED_ORIGIN))
        .await
        .unwrap();
    let mut first = WsClient {
        stream: first,
        doc: Document::new(1).unwrap(),
    };
    first.handshake().await; // it really is connected
    let second = connect_raw(&app, &room.doc_id, Some(&ticket), Some(ALLOWED_ORIGIN))
        .await
        .unwrap();
    let second = WsClient {
        stream: second,
        doc: Document::new(2).unwrap(),
    };
    expect_close_code(second, CLOSE_UNAUTHORIZED).await;
}

#[tokio::test]
async fn a_user_who_was_removed_after_asking_for_a_ticket_is_closed_with_4003() {
    let app = TestApp::real().await;
    let room = Room::new(&app).await;
    let ticket = ticket_for(&app, room.editor(), "doc", &room.doc_id).await;
    workspaces::remove_member(&app.db, room.workspace.id, room.editor().id)
        .await
        .unwrap()
        .unwrap();

    let stream = connect_raw(&app, &room.doc_id, Some(&ticket), Some(ALLOWED_ORIGIN))
        .await
        .unwrap();
    let client = WsClient {
        stream,
        doc: Document::new(1).unwrap(),
    };
    expect_close_code(client, CLOSE_FORBIDDEN).await;
}

#[tokio::test]
async fn a_refused_connection_does_not_keep_the_room_in_memory() {
    let app = TestApp::real().await;
    let room = Room::new(&app).await;
    let ticket = ticket_for(&app, room.editor(), "doc", &room.doc_id).await;
    workspaces::remove_member(&app.db, room.workspace.id, room.editor().id)
        .await
        .unwrap()
        .unwrap();
    let stream = connect_raw(&app, &room.doc_id, Some(&ticket), Some(ALLOWED_ORIGIN))
        .await
        .unwrap();
    let client = WsClient {
        stream,
        doc: Document::new(1).unwrap(),
    };
    expect_close_code(client, CLOSE_FORBIDDEN).await;
    assert_eq!(
        app.hub.open_rooms(),
        0,
        "the room loaded for the refused user is released"
    );
}

#[tokio::test]
async fn a_document_that_does_not_exist_closes_with_4004() {
    let app = TestApp::real().await;
    let room = Room::new(&app).await;
    let missing = ObjectId::new();
    let (ticket, _) = ticket::issue(
        &ticket_key(),
        room.admin().id,
        Scope::Doc,
        missing,
        server::auth::session::now(),
        TICKET_SECONDS,
    )
    .unwrap();
    let stream = connect_raw(&app, &missing.to_hex(), Some(&ticket), Some(ALLOWED_ORIGIN))
        .await
        .unwrap();
    let client = WsClient {
        stream,
        doc: Document::new(1).unwrap(),
    };
    expect_close_code(client, CLOSE_DOCUMENT_GONE).await;
}

#[tokio::test]
async fn deleting_a_document_sends_everyone_on_it_away_with_4004() {
    let app = TestApp::real().await;
    let room = Room::new(&app).await;
    let mut editor = join(&app, &room, room.editor(), 11).await;
    let mut viewer = join(&app, &room, room.viewer(), 12).await;

    let (status, _) = app
        .call(
            "DELETE",
            &format!("/api/v1/documents/{}", room.doc_id),
            Some(room.admin()),
            None,
        )
        .await;
    assert_eq!(status, 204);
    assert_eq!(editor.recv_close().await, Some(CLOSE_DOCUMENT_GONE));
    assert_eq!(viewer.recv_close().await, Some(CLOSE_DOCUMENT_GONE));
    assert_eq!(app.hub.open_rooms(), 0, "the room is forgotten");

    // A new ticket for it is refused, and so is a socket with an old one.
    let (status, _) = app
        .call(
            "POST",
            "/api/v1/auth/ws-ticket",
            Some(room.admin()),
            Some(json!({"scope": "doc", "id": room.doc_id})),
        )
        .await;
    assert_eq!(status, 404);
}

// ------------------------------------------------------------------------------------- frames

#[tokio::test]
async fn two_editors_converge_and_a_late_joiner_gets_the_full_state() {
    let app = TestApp::real().await;
    let room = Room::new(&app).await;
    let mut a = join(&app, &room, room.admin(), 101).await;
    let mut b = join(&app, &room, room.editor(), 102).await;
    // Both start from the one paragraph the document was created with.
    assert_eq!(a.doc.blocks().len(), 1);
    assert_eq!(a.doc.blocks(), b.doc.blocks());
    let first = a.doc.blocks()[0].block_id.clone();

    // A types in the first block; B receives it. B creates a new block; A receives it.
    let update = a.doc.insert_text(&first, 0, "Hello").unwrap();
    a.send_update(&update).await;
    let frame = b.recv_binary().await;
    b.apply(&frame);
    assert_eq!(b.doc.block_content(&first).unwrap(), "Hello");

    let update = b
        .doc
        .create_block(&block_id(7), BlockType::Heading1, 2.0, "Title")
        .unwrap();
    b.send_update(&update).await;
    let frame = a.recv_binary().await;
    a.apply(&frame);
    assert_eq!(a.doc.blocks(), b.doc.blocks());
    assert_eq!(a.doc.blocks().len(), 2);

    // Neither client gets its own update echoed back: the next thing A hears is not an update.
    assert!(
        a.recv_within(Duration::from_millis(300))
            .await
            .is_none_or(|f| !matches!(f, Frame::Binary(_)))
    );

    // A late joiner asks for the state and gets all of it.
    let late = join(&app, &room, room.admin(), 103).await;
    assert_eq!(late.doc.blocks(), a.doc.blocks());
    assert_eq!(late.doc.block_content(&first).unwrap(), "Hello");
}

#[tokio::test]
async fn simultaneous_edits_of_one_block_from_two_clients_merge() {
    let app = TestApp::real().await;
    let room = Room::new(&app).await;
    let mut a = join(&app, &room, room.admin(), 201).await;
    let mut b = join(&app, &room, room.editor(), 202).await;
    let first = a.doc.blocks()[0].block_id.clone();

    // Both type at the same index before hearing from each other.
    let ua = a.doc.insert_text(&first, 0, "AAA ").unwrap();
    let ub = b.doc.insert_text(&first, 0, "BBB ").unwrap();
    a.send_update(&ua).await;
    b.send_update(&ub).await;
    let for_b = b.recv_binary().await;
    let for_a = a.recv_binary().await;
    a.apply(&for_a);
    b.apply(&for_b);

    assert_eq!(a.doc.blocks(), b.doc.blocks());
    let text = a.doc.block_content(&first).unwrap();
    assert!(text.contains("AAA ") && text.contains("BBB "), "{text}");
}

#[tokio::test]
async fn a_viewers_changes_are_refused_not_applied_and_not_broadcast() {
    let app = TestApp::real().await;
    let room = Room::new(&app).await;
    let mut editor = join(&app, &room, room.editor(), 301).await;
    let mut viewer = join(&app, &room, room.viewer(), 302).await;
    let first = viewer.doc.blocks()[0].block_id.clone();

    // An update, and the same change as a SyncStep2: both are writes.
    let update = viewer.doc.insert_text(&first, 0, "forged").unwrap();
    viewer.send_update(&update).await;
    let error = viewer.recv_json("ERROR").await;
    assert_eq!(error["code"], "READ_ONLY");
    let step2 = yrs_step2(&update);
    viewer.send_binary(step2).await;
    assert_eq!(viewer.recv_json("ERROR").await["code"], "READ_ONLY");

    // The connection stays open, the editor heard nothing, and the server did not apply it.
    assert!(
        editor
            .recv_within(Duration::from_millis(400))
            .await
            .is_none_or(|f| !matches!(f, Frame::Binary(_))),
        "the editor must not receive a viewer's update"
    );
    let room_state = app
        .hub
        .room(&app.db, ObjectId::parse_str(&room.doc_id).unwrap())
        .await
        .unwrap()
        .state()
        .1;
    assert_eq!(room_state[0].content, "", "nothing was applied");
    let fresh = join(&app, &room, room.admin(), 303).await;
    assert_eq!(fresh.doc.block_content(&first).unwrap(), "");

    // A viewer can still read: SyncStep1 is allowed, and awareness is relayed.
    let mut again = join(&app, &room, room.viewer(), 304).await;
    assert_eq!(again.doc.blocks().len(), 1);
    again.send_binary(awareness_frame()).await;
    let relayed = editor.recv_binary().await;
    assert_eq!(
        sync_crdt::frame_kind(&relayed),
        Ok(sync_crdt::FrameKind::Awareness)
    );
}

/// A SyncStep2 frame carrying `update`.
fn yrs_step2(update: &[u8]) -> Vec<u8> {
    use yrs::sync::{Message, SyncMessage};
    use yrs::updates::encoder::Encode;
    Message::Sync(SyncMessage::SyncStep2(update.to_vec())).encode_v1()
}

fn awareness_frame() -> Vec<u8> {
    use yrs::sync::Message;
    use yrs::sync::awareness::AwarenessUpdate;
    use yrs::updates::encoder::Encode;
    Message::Awareness(AwarenessUpdate {
        clients: Default::default(),
    })
    .encode_v1()
}

#[tokio::test]
async fn presence_counts_follow_joins_and_leaves_and_ping_gets_pong() {
    let app = TestApp::real().await;
    let room = Room::new(&app).await;
    let mut a = WsClient::open(&app, room.admin(), &room.doc_id, 401).await;
    assert_eq!(a.recv_json("PRESENCE_COUNT").await["active_sockets"], 1);

    let b = WsClient::open(&app, room.editor(), &room.doc_id, 402).await;
    assert_eq!(a.recv_json("PRESENCE_COUNT").await["active_sockets"], 2);

    b.close().await;
    assert_eq!(a.recv_json("PRESENCE_COUNT").await["active_sockets"], 1);

    a.send_text(r#"{"type":"PING","t":1234}"#).await;
    let pong = a.recv_json("PONG").await;
    assert_eq!(pong["t"], 1234);
}

#[tokio::test]
async fn hostile_frames_close_with_1008_and_leave_the_document_alone() {
    let app = TestApp::real().await;
    let room = Room::new(&app).await;
    let doc = ObjectId::parse_str(&room.doc_id).unwrap();
    let before = {
        let first = join(&app, &room, room.admin(), 500).await;
        first.doc.encode_state_as_update()
    };

    let hostile: Vec<(&str, Message)> = vec![
        ("garbage bytes", Message::Binary(vec![0xFF; 64].into())),
        ("an empty frame", Message::Binary(Vec::new().into())),
        (
            "an unknown message type",
            Message::Binary(vec![99, 1, 2].into()),
        ),
        (
            "an update that is not an update",
            Message::Binary(update_frame_of(&[1, 2, 3]).into()),
        ),
        ("text that is not JSON", Message::Text("nope".into())),
        (
            "an unknown text frame",
            Message::Text(r#"{"type":"HELLO"}"#.into()),
        ),
    ];
    for (name, message) in hostile {
        let mut client = join(&app, &room, room.editor(), 501).await;
        use futures_util::SinkExt;
        client.stream.send(message).await.unwrap();
        assert_eq!(client.recv_close().await, Some(CLOSE_POLICY), "{name}");
    }

    // An update that breaks the schema (an unknown block type) is refused just the same.
    let hostile_update = hostile_block_update();
    let mut client = join(&app, &room, room.editor(), 502).await;
    client.send_update(&hostile_update).await;
    assert_eq!(
        client.recv_close().await,
        Some(CLOSE_POLICY),
        "a schema-breaking update"
    );

    let state = app.hub.room(&app.db, doc).await.unwrap().state().0;
    let check = Document::new(9).unwrap();
    check.apply_update(&state).unwrap();
    let again = Document::new(8).unwrap();
    again.apply_update(&before).unwrap();
    assert_eq!(
        check.blocks(),
        again.blocks(),
        "the document did not change"
    );
}

fn update_frame_of(update: &[u8]) -> Vec<u8> {
    sync_crdt::update_frame(update)
}

/// An update from a raw Yrs replica that adds a block of an unknown type.
fn hostile_block_update() -> Vec<u8> {
    use yrs::{Any, Doc, In, Map, MapPrelim, TextPrelim, Transact};
    let doc = Doc::new();
    let blocks = doc.get_or_insert_map("blocks");
    let mut txn = doc.transact_mut();
    blocks.insert(
        &mut txn,
        "blk000000666",
        MapPrelim::from_iter([
            ("type", In::Any(Any::from("HEADING_9"))),
            ("order", In::Any(Any::from(1.0))),
            ("text", In::from(TextPrelim::new(""))),
            ("props", In::from(MapPrelim::default())),
        ]),
    );
    txn.encode_update_v1()
}

#[tokio::test]
async fn an_update_that_builds_on_missing_changes_gets_a_resync_not_a_close() {
    let app = TestApp::real().await;
    let room = Room::new(&app).await;
    let mut editor = join(&app, &room, room.editor(), 601).await;

    // An author makes two changes, the second building on the first; only the second is sent.
    let author = Document::new(602).unwrap();
    author
        .apply_update(&editor.doc.encode_state_as_update())
        .unwrap();
    let _first = author
        .create_block(&block_id(1), BlockType::Paragraph, 5.0, "one")
        .unwrap();
    let second = author.insert_text(&block_id(1), 3, "!").unwrap();
    editor.send_update(&second).await;

    // The server does not close: it answers with its own SyncStep1.
    let frame = editor.recv_binary().await;
    assert_eq!(
        sync_crdt::frame_kind(&frame),
        Ok(sync_crdt::FrameKind::SyncStep1),
        "a re-sync request"
    );
    // The connection works afterwards: send the whole thing.
    let diff = author
        .encode_state_as_update_from(&editor.doc.state_vector())
        .unwrap();
    editor.send_update(&diff).await;
    editor.send_text(r#"{"type":"PING","t":7}"#).await;
    assert_eq!(editor.recv_json("PONG").await["t"], 7);
    let room_blocks = app
        .hub
        .room(&app.db, ObjectId::parse_str(&room.doc_id).unwrap())
        .await
        .unwrap()
        .state()
        .1;
    assert!(room_blocks.iter().any(|b| b.content == "one!"));
}

#[tokio::test]
async fn a_frame_over_one_mebibyte_closes_with_1009() {
    let app = TestApp::real().await;
    let room = Room::new(&app).await;
    let mut client = join(&app, &room, room.editor(), 701).await;
    client.send_binary(vec![0u8; 1024 * 1024 + 10]).await;
    assert_eq!(client.recv_close().await, Some(CLOSE_TOO_BIG));
}

#[tokio::test]
async fn a_document_cannot_grow_past_its_size_cap() {
    let app = TestApp::spawn(
        AppOptions {
            hub: Some(HubConfig {
                max_state_bytes: 4_000,
                ..HubConfig::default()
            }),
            ..AppOptions::default()
        },
        |routes| routes,
    )
    .await;
    let room = Room::new(&app).await;
    let mut client = join(&app, &room, room.editor(), 801).await;
    let first = client.doc.blocks()[0].block_id.clone();

    let small = client.doc.insert_text(&first, 0, "ok").unwrap();
    client.send_update(&small).await;
    let big = client
        .doc
        .insert_text(&first, 2, &"x".repeat(6_000))
        .unwrap();
    client.send_update(&big).await;
    assert_eq!(client.recv_close().await, Some(CLOSE_TOO_BIG));

    // What was accepted before is still there.
    let fresh = join(&app, &room, room.admin(), 802).await;
    assert_eq!(fresh.doc.block_content(&first).unwrap(), "ok");
}

#[tokio::test]
async fn too_many_updates_get_a_rate_limited_error_and_the_connection_stays_open() {
    let app = TestApp::spawn(
        AppOptions {
            hub: Some(HubConfig {
                updates: BucketConfig {
                    burst: 5,
                    per_second: 1,
                },
                awareness: BucketConfig {
                    burst: 3,
                    per_second: 1,
                },
                ..HubConfig::default()
            }),
            ..AppOptions::default()
        },
        |routes| routes,
    )
    .await;
    let room = Room::new(&app).await;
    let mut client = WsClient::open(&app, room.editor(), &room.doc_id, 901).await;
    // The first frame (SyncStep1) takes one token of the five.
    client.handshake().await;
    let first = client.doc.blocks()[0].block_id.clone();
    for n in 0..8 {
        let update = client.doc.insert_text(&first, 0, &n.to_string()).unwrap();
        client.send_update(&update).await;
    }
    let error = client.recv_json("ERROR").await;
    assert_eq!(error["code"], "RATE_LIMITED");

    // Awareness has a bucket of its own: three go through, then the error.
    for _ in 0..5 {
        client.send_binary(awareness_frame()).await;
    }
    let mut errors = 0;
    while let Some(frame) = client.recv_within(Duration::from_millis(500)).await {
        if matches!(&frame, Frame::Json(v) if v["type"] == "ERROR" && v["code"] == "RATE_LIMITED") {
            errors += 1;
        }
    }
    assert!(errors >= 2, "expected awareness errors, saw {errors}");

    // Still connected.
    client.send_text(r#"{"type":"PING","t":9}"#).await;
    assert_eq!(client.recv_json("PONG").await["t"], 9);
}

#[tokio::test]
async fn a_consumer_that_falls_too_far_behind_is_dropped_with_1008() {
    let app = TestApp::spawn(
        AppOptions {
            hub: Some(HubConfig {
                // A long send timeout keeps this on the lag path; the timeout has its own tests.
                send_timeout: Duration::from_secs(60),
                updates: BucketConfig {
                    burst: 100_000,
                    per_second: 100_000,
                },
                ..HubConfig::default()
            }),
            ..AppOptions::default()
        },
        |routes| routes,
    )
    .await;
    let room = Room::new(&app).await;
    let mut slow = join(&app, &room, room.viewer(), 1001).await; // never reads after the join
    let mut sender = join(&app, &room, room.editor(), 1002).await;

    // Broadcast capacity is 64 and the slow socket's buffers fill, so a few hundred large frames
    // make the receiver lag. All-zero payloads are valid empty updates: relayed, not growing the
    // document.
    // This test failed on CI runners only ("the server went quiet" after the full wait), never
    // locally, so it records what happened: how many frames the sender got out and how many the
    // slow socket then read before the silence.
    let mut sent = 0u32;
    let mut send_error = None;
    for _ in 0..600u32 {
        let frame = update_frame_of(&vec![0u8; 60_000]);
        match sender.stream.send_frame_ok(frame).await {
            Ok(()) => sent += 1,
            Err(error) => {
                send_error = Some(error.to_string());
                break;
            }
        }
    }
    // When the server cut the sender off, say how: everything the sender can still read, up to
    // and including the close frame (the first thing in its buffer is only the join-time PRESENCE_COUNT).
    let mut sender_saw = Vec::new();
    if send_error.is_some() {
        // The raw close frame, so the reason text is kept (the test client reports only the code).
        use futures_util::StreamExt;
        while sender_saw.len() < 20 {
            match tokio::time::timeout(Duration::from_secs(3), sender.stream.next()).await {
                Ok(Some(Ok(Message::Close(frame)))) => {
                    sender_saw.push(format!("closed {frame:?}"));
                    break;
                }
                Ok(Some(Ok(Message::Text(body)))) => sender_saw.push(body.to_string()),
                Ok(Some(Ok(Message::Binary(bytes)))) => {
                    sender_saw.push(format!("binary {} bytes", bytes.len()))
                }
                Ok(Some(Ok(_))) => {}
                Ok(Some(Err(error))) => {
                    sender_saw.push(format!("read error {error}"));
                    break;
                }
                Ok(None) => {
                    sender_saw.push("stream ended".to_string());
                    break;
                }
                Err(_) => {
                    sender_saw.push("silence".to_string());
                    break;
                }
            }
        }
    }
    let mut received = 0u32;
    let outcome = loop {
        match slow.recv_within(common::ws::WAIT).await {
            Some(Frame::Closed(code)) => break Some(code),
            Some(_) => received += 1,
            None => break None,
        }
    };
    assert_eq!(
        outcome,
        Some(Some(CLOSE_POLICY)),
        "sender sent {sent} of 600 frames (send error: {send_error:?}; read from the sender \
         afterwards: {sender_saw:?}); the slow socket read {received} frames and then heard \
         nothing for {:?}",
        common::ws::WAIT
    );
}

trait SendFrame {
    async fn send_frame_ok(&mut self, frame: Vec<u8>) -> Result<(), WsError>;
}

impl SendFrame for common::ws::Stream {
    async fn send_frame_ok(&mut self, frame: Vec<u8>) -> Result<(), WsError> {
        use futures_util::SinkExt;
        self.send(Message::Binary(frame.into())).await
    }
}

#[tokio::test]
async fn an_idle_connection_is_pinged_and_then_closed() {
    let app = TestApp::spawn(
        AppOptions {
            hub: Some(HubConfig {
                ping_interval: Duration::from_millis(100),
                idle_timeout: Duration::from_millis(400),
                ..HubConfig::default()
            }),
            ..AppOptions::default()
        },
        |routes| routes,
    )
    .await;
    let room = Room::new(&app).await;
    let mut client = join(&app, &room, room.editor(), 1101).await;
    let room_handle = app
        .hub
        .room(&app.db, ObjectId::parse_str(&room.doc_id).unwrap())
        .await
        .unwrap();
    assert_eq!(room_handle.active(), 1);
    // Do not read for longer than the idle timeout: no pong goes out, because the client library
    // only answers pings while it is being read.
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert_eq!(
        room_handle.active(),
        0,
        "the idle connection was dropped by the server"
    );
    // The close frame can be lost when the client answers a ping after the server hung up (the
    // reset discards what was still unread), so only the end of the connection is checked.
    assert!(matches!(
        client.recv_close().await,
        Some(CLOSE_POLICY) | None
    ));
}

// ------------------------------------------------------------------------- live role changes

#[tokio::test]
async fn demoting_a_member_closes_their_socket_and_the_reconnect_is_read_only() {
    let app = TestApp::real().await;
    let room = Room::new(&app).await;
    let mut editor = join(&app, &room, room.editor(), 1201).await;
    let mut other = join(&app, &room, room.admin(), 1202).await;

    let (status, body) = app
        .call(
            "PATCH",
            &format!(
                "/api/v1/workspaces/{}/members/{}",
                room.workspace.hex(),
                room.editor().id.to_hex()
            ),
            Some(room.admin()),
            Some(json!({"role": "VIEWER"})),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(editor.recv_close().await, Some(1000));
    assert!(
        other
            .recv_within(Duration::from_millis(300))
            .await
            .is_none_or(|f| !matches!(f, Frame::Closed(_))),
        "other users keep their sockets"
    );

    // The reconnect has the new role: writes are refused.
    let mut again = WsClient::open(&app, room.editor(), &room.doc_id, 1203).await;
    again.handshake_read_only().await;
    let first = again.doc.blocks()[0].block_id.clone();
    let update = again.doc.insert_text(&first, 0, "x").unwrap();
    again.send_update(&update).await;
    assert_eq!(again.recv_json("ERROR").await["code"], "READ_ONLY");
}

#[tokio::test]
async fn removing_a_member_closes_their_socket_with_4003_and_they_cannot_return() {
    let app = TestApp::real().await;
    let room = Room::new(&app).await;
    let mut editor = join(&app, &room, room.editor(), 1301).await;
    let (status, _) = app
        .call(
            "DELETE",
            &format!(
                "/api/v1/workspaces/{}/members/{}",
                room.workspace.hex(),
                room.editor().id.to_hex()
            ),
            Some(room.admin()),
            None,
        )
        .await;
    assert_eq!(status, 204);
    assert_eq!(editor.recv_close().await, Some(CLOSE_FORBIDDEN));
    let (status, _) = app
        .call(
            "POST",
            "/api/v1/auth/ws-ticket",
            Some(room.editor()),
            Some(json!({"scope": "doc", "id": room.doc_id})),
        )
        .await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn a_viewer_who_keeps_sending_writes_is_disconnected() {
    let app = TestApp::spawn(
        AppOptions {
            hub: Some(HubConfig {
                updates: BucketConfig {
                    burst: 5,
                    per_second: 1,
                },
                ..HubConfig::default()
            }),
            ..AppOptions::default()
        },
        |routes| routes,
    )
    .await;
    let room = Room::new(&app).await;
    let mut viewer = join(&app, &room, room.viewer(), 1401).await;
    let first = viewer.doc.blocks()[0].block_id.clone();
    for _ in 0..20 {
        let update = viewer.doc.insert_text(&first, 0, "x").unwrap();
        viewer.send_update(&update).await;
    }
    let mut closed = None;
    while let Some(frame) = viewer.recv_within(Duration::from_secs(2)).await {
        if let Frame::Closed(code) = frame {
            closed = code;
            break;
        }
    }
    assert_eq!(closed, Some(CLOSE_POLICY));
}

#[tokio::test]
async fn a_long_text_frame_is_refused_before_it_is_parsed() {
    let app = TestApp::real().await;
    let room = Room::new(&app).await;
    let mut client = join(&app, &room, room.editor(), 1402).await;
    let padding = " ".repeat(2000);
    client
        .send_text(&format!(r#"{{"type":"PING","t":1,"pad":"{padding}"}}"#))
        .await;
    assert_eq!(client.recv_close().await, Some(CLOSE_TOO_BIG));
}

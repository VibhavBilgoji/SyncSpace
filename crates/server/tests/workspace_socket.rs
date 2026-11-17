//! The workspace socket (`docs/API.md` section 3, "Workspace socket"): `BOARD_EVENT` for boards and
//! tasks, the handshake rules it shares with the document socket, and the closes of ADR-014.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use common::ws::{Frame, WsClient, connect_workspace_raw, ticket_for};
use common::{ALLOWED_ORIGIN, Account, TestApp, TestWorkspace};
use futures_util::SinkExt;
use mongodb::bson::DateTime;
use mongodb::bson::oid::ObjectId;
use serde_json::{Value, json};
use server::db::tasks::{self, TaskRecord};
use server::rbac::Role;
use tokio_tungstenite::tungstenite::Message;

const QUIET: Duration = Duration::from_millis(400);

async fn workspace_with_people(app: &TestApp) -> TestWorkspace<'_> {
    TestWorkspace::new(app)
        .await
        .with_member(Role::Editor)
        .await
        .with_member(Role::Viewer)
        .await
}

async fn make_board(app: &TestApp, workspace: &TestWorkspace<'_>, who: &Account) -> Value {
    let (status, body) = app
        .call(
            "POST",
            &format!("/api/v1/workspaces/{}/boards", workspace.hex()),
            Some(who),
            Some(json!({"name": "Sprint"})),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    body
}

async fn make_task(app: &TestApp, board: &str, who: &Account, title: &str, status: &str) -> Value {
    let (code, body) = app
        .call(
            "POST",
            &format!("/api/v1/boards/{board}/tasks"),
            Some(who),
            Some(json!({"title": title, "status": status})),
        )
        .await;
    assert_eq!(code, 201, "{body}");
    body
}

async fn open(app: &TestApp, who: &Account, workspace: &TestWorkspace<'_>) -> WsClient {
    WsClient::open_workspace(app, who, &workspace.hex()).await
}

// ----- events -----

#[tokio::test]
async fn every_board_change_reaches_every_open_socket_of_the_workspace() {
    let app = TestApp::real().await;
    let workspace = workspace_with_people(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let viewer = workspace.member(Role::Viewer).clone();
    let mut watching_viewer = open(&app, &viewer, &workspace).await;
    let mut acting_editor = open(&app, &editor, &workspace).await;

    let board = make_board(&app, &workspace, &editor).await;
    for client in [&mut watching_viewer, &mut acting_editor] {
        let event = client.next_event("BOARD_EVENT").await;
        assert_eq!(event["kind"], "BOARD_CREATED");
        assert_eq!(event["board_id"], board["id"]);
        assert_eq!(event["board"]["name"], "Sprint");
        assert!(event.get("task").is_none() && event.get("status").is_none());
    }

    let board_id = board["id"].as_str().unwrap();
    let task = make_task(&app, board_id, &editor, "Write it", "TODO").await;
    let task_id = task["id"].as_str().unwrap();
    for client in [&mut watching_viewer, &mut acting_editor] {
        let event = client.next_event("BOARD_EVENT").await;
        assert_eq!(event["kind"], "TASK_CREATED");
        assert_eq!(event["board_id"], board["id"]);
        assert_eq!(event["task"]["id"], task["id"]);
        assert_eq!(event["task"]["title"], "Write it");
        assert_eq!(event["task"]["status"], "TODO");
    }

    let (status, _) = app
        .call(
            "PATCH",
            &format!("/api/v1/tasks/{task_id}"),
            Some(&editor),
            Some(json!({"title": "Written", "priority": "HIGH"})),
        )
        .await;
    assert_eq!(status, 200);
    let (status, moved) = app
        .call(
            "PATCH",
            &format!("/api/v1/tasks/{task_id}/move"),
            Some(&editor),
            Some(json!({"to_status": "DONE"})),
        )
        .await;
    assert_eq!(status, 200);
    let (status, _) = app
        .call(
            "DELETE",
            &format!("/api/v1/tasks/{task_id}"),
            Some(&editor),
            None,
        )
        .await;
    assert_eq!(status, 204);

    for client in [&mut watching_viewer, &mut acting_editor] {
        let updated = client.next_event("BOARD_EVENT").await;
        assert_eq!(updated["kind"], "TASK_UPDATED");
        assert_eq!(updated["task"]["title"], "Written");
        assert_eq!(updated["task"]["priority"], "HIGH");

        let moved_event = client.next_event("BOARD_EVENT").await;
        assert_eq!(moved_event["kind"], "TASK_MOVED");
        assert_eq!(moved_event["task"]["status"], "DONE");
        assert_eq!(moved_event["task"]["order"], moved["order"]);

        let deleted = client.next_event("BOARD_EVENT").await;
        assert_eq!(deleted["kind"], "TASK_DELETED");
        assert_eq!(
            deleted["task"]["id"], task["id"],
            "the deleted task, so a page can drop it"
        );
        assert_eq!(deleted["task"]["title"], "Written");
    }
}

#[tokio::test]
async fn a_renumbered_column_is_announced_before_the_move() {
    let app = TestApp::real().await;
    let workspace = workspace_with_people(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let board = make_board(&app, &workspace, &editor).await;
    let board_id = board["id"].as_str().unwrap();
    let board_oid = ObjectId::parse_str(board_id).unwrap();
    // Two tasks whose positions are closer than the move rule allows.
    let mut ids = Vec::new();
    for (title, order) in [("A", 1.0), ("B", 1.0 + 5e-10)] {
        let record = TaskRecord {
            id: ObjectId::new(),
            workspace_id: workspace.id,
            board_id: board_oid,
            title: title.to_string(),
            description: String::new(),
            status: "TODO".to_string(),
            priority: "MEDIUM".to_string(),
            assignee_ids: vec![],
            tags: vec![],
            order,
            created_by: editor.id,
            created_at: DateTime::now(),
            updated_at: DateTime::now(),
        };
        tasks::insert(&app.db, &record).await.unwrap();
        ids.push(record.id.to_hex());
    }
    let mover = make_task(&app, board_id, &editor, "Mover", "REVIEW").await;
    let mut socket = open(&app, &editor, &workspace).await;

    let (status, _) = app
        .call(
            "PATCH",
            &format!("/api/v1/tasks/{}/move", mover["id"].as_str().unwrap()),
            Some(&editor),
            Some(json!({"to_status": "TODO", "before_id": ids[0], "after_id": ids[1]})),
        )
        .await;
    assert_eq!(status, 200);
    let first = socket.next_event("BOARD_EVENT").await;
    assert_eq!(first["kind"], "COLUMN_RENUMBERED");
    assert_eq!(first["board_id"], board["id"]);
    assert_eq!(first["status"], "TODO");
    assert!(first.get("task").is_none());
    let second = socket.next_event("BOARD_EVENT").await;
    assert_eq!(second["kind"], "TASK_MOVED");
    assert_eq!(second["task"]["order"], 1.5);
}

#[tokio::test]
async fn another_workspaces_sockets_hear_nothing() {
    let app = TestApp::real().await;
    let mine = workspace_with_people(&app).await;
    let theirs = TestWorkspace::new(&app).await;
    let their_owner = theirs.owner.clone();
    let mut outsider_socket = open(&app, &their_owner, &theirs).await;
    let editor = mine.member(Role::Editor).clone();
    let mut own_socket = open(&app, &editor, &mine).await;

    make_board(&app, &mine, &editor).await;
    assert_eq!(
        own_socket.next_event("BOARD_EVENT").await["kind"],
        "BOARD_CREATED"
    );
    assert!(
        outsider_socket.quiet_for(QUIET).await,
        "an event leaked to another workspace"
    );
}

#[tokio::test]
async fn refused_and_failed_actions_send_no_event() {
    let app = TestApp::real().await;
    let workspace = workspace_with_people(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let viewer = workspace.member(Role::Viewer).clone();
    let board = make_board(&app, &workspace, &editor).await;
    let board_id = board["id"].as_str().unwrap();
    let task = make_task(&app, board_id, &editor, "Keep", "TODO").await;
    let mut socket = open(&app, &editor, &workspace).await;

    // A viewer is refused, a blank title and a bad neighbour are 422, an unknown task is 404.
    let attempts: Vec<(&str, String, Option<Value>, &Account, u16)> = vec![
        (
            "POST",
            format!("/api/v1/boards/{board_id}/tasks"),
            Some(json!({"title": "x"})),
            &viewer,
            403,
        ),
        (
            "POST",
            format!("/api/v1/boards/{board_id}/tasks"),
            Some(json!({"title": ""})),
            &editor,
            422,
        ),
        (
            "PATCH",
            format!("/api/v1/tasks/{}/move", task["id"].as_str().unwrap()),
            Some(json!({"to_status": "DONE", "before_id": ObjectId::new().to_hex()})),
            &editor,
            422,
        ),
        (
            "PATCH",
            format!("/api/v1/tasks/{}", ObjectId::new().to_hex()),
            Some(json!({"title": "x"})),
            &editor,
            404,
        ),
        (
            "DELETE",
            format!("/api/v1/tasks/{}", task["id"].as_str().unwrap()),
            None,
            &viewer,
            403,
        ),
    ];
    for (method, path, body, who, want) in attempts {
        let (status, _) = app.call(method, &path, Some(who), body).await;
        assert_eq!(status, want, "{method} {path}");
    }
    assert!(
        socket.quiet_for(QUIET).await,
        "a refused action was announced"
    );
}

// ----- the handshake -----

#[tokio::test]
async fn the_handshake_has_the_rules_of_the_document_socket() {
    let app = TestApp::real().await;
    let workspace = workspace_with_people(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let wid = workspace.hex();

    // A bad or missing Origin is refused before the upgrade.
    let ticket = ticket_for(&app, &editor, "workspace", &wid).await;
    assert!(
        connect_workspace_raw(&app, &wid, Some(&ticket), Some("https://evil.example"))
            .await
            .is_err()
    );
    assert!(
        connect_workspace_raw(&app, &wid, Some(&ticket), None)
            .await
            .is_err()
    );

    // No ticket, garbage, and a ticket for a document each close with 4001.
    let (status, document) = app
        .call(
            "POST",
            &format!("/api/v1/workspaces/{wid}/documents"),
            Some(&editor),
            Some(json!({"title": "Real document"})),
        )
        .await;
    assert_eq!(status, 201, "{document}");
    let doc_ticket = ticket_for(&app, &editor, "doc", document["id"].as_str().unwrap()).await;
    for ticket in [None, Some("garbage".to_string()), Some(doc_ticket)] {
        let stream = connect_workspace_raw(&app, &wid, ticket.as_deref(), Some(ALLOWED_ORIGIN))
            .await
            .unwrap();
        let mut client = WsClient {
            stream,
            doc: sync_crdt::Document::new(1).unwrap(),
        };
        assert_eq!(client.recv_close().await, Some(4001), "{ticket:?}");
    }

    // A ticket for another workspace does not open this one.
    let other = TestWorkspace::new(&app).await;
    let their_ticket = ticket_for(&app, &other.owner.clone(), "workspace", &other.hex()).await;
    let stream = connect_workspace_raw(&app, &wid, Some(&their_ticket), Some(ALLOWED_ORIGIN))
        .await
        .unwrap();
    let mut client = WsClient {
        stream,
        doc: sync_crdt::Document::new(1).unwrap(),
    };
    assert_eq!(client.recv_close().await, Some(4001));
}

#[tokio::test]
async fn a_ticket_works_once() {
    let app = TestApp::real().await;
    let workspace = workspace_with_people(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let wid = workspace.hex();
    let ticket = ticket_for(&app, &editor, "workspace", &wid).await;
    let first = connect_workspace_raw(&app, &wid, Some(&ticket), Some(ALLOWED_ORIGIN))
        .await
        .unwrap();
    let mut first = WsClient {
        stream: first,
        doc: sync_crdt::Document::new(1).unwrap(),
    };
    first.send_text(r#"{"type":"PING","t":1}"#).await;
    assert_eq!(first.recv_json("PONG").await["t"], 1);

    let second = connect_workspace_raw(&app, &wid, Some(&ticket), Some(ALLOWED_ORIGIN))
        .await
        .unwrap();
    let mut second = WsClient {
        stream: second,
        doc: sync_crdt::Document::new(1).unwrap(),
    };
    assert_eq!(
        second.recv_close().await,
        Some(4001),
        "a used ticket is refused"
    );
}

#[tokio::test]
async fn an_outsider_cannot_even_get_a_ticket_and_a_removed_member_is_closed_with_4003() {
    let app = TestApp::real().await;
    let workspace = workspace_with_people(&app).await;
    let viewer = workspace.member(Role::Viewer).clone();
    let outsider = app.account("Outsider").await;
    let (status, _) = app
        .call(
            "POST",
            "/api/v1/auth/ws-ticket",
            Some(&outsider),
            Some(json!({"scope": "workspace", "id": workspace.hex()})),
        )
        .await;
    assert_eq!(status, 404);

    // Removed between the ticket and the connection.
    let wid = workspace.hex();
    let ticket = ticket_for(&app, &viewer, "workspace", &wid).await;
    server::db::workspaces::remove_member(&app.db, workspace.id, viewer.id)
        .await
        .unwrap();
    let stream = connect_workspace_raw(&app, &wid, Some(&ticket), Some(ALLOWED_ORIGIN))
        .await
        .unwrap();
    let mut client = WsClient {
        stream,
        doc: sync_crdt::Document::new(1).unwrap(),
    };
    assert_eq!(client.recv_close().await, Some(4003));
}

// ----- ADR-014 -----

#[tokio::test]
async fn removing_a_member_closes_their_workspace_socket_with_4003_and_only_theirs() {
    let app = TestApp::real().await;
    let workspace = workspace_with_people(&app).await;
    let owner = workspace.owner.clone();
    let editor = workspace.member(Role::Editor).clone();
    let viewer = workspace.member(Role::Viewer).clone();
    let mut viewer_socket = open(&app, &viewer, &workspace).await;
    let mut editor_socket = open(&app, &editor, &workspace).await;

    let (status, _) = app
        .call(
            "DELETE",
            &format!(
                "/api/v1/workspaces/{}/members/{}",
                workspace.hex(),
                viewer.id.to_hex()
            ),
            Some(&owner),
            None,
        )
        .await;
    assert_eq!(status, 204);
    assert_eq!(viewer_socket.recv_close().await, Some(4003));
    // Everyone else stays connected and still hears events.
    make_board(&app, &workspace, &editor).await;
    assert_eq!(
        editor_socket.next_event("BOARD_EVENT").await["kind"],
        "BOARD_CREATED"
    );
}

#[tokio::test]
async fn changing_a_role_closes_that_users_workspace_socket_with_1000() {
    let app = TestApp::real().await;
    let workspace = workspace_with_people(&app).await;
    let owner = workspace.owner.clone();
    let editor = workspace.member(Role::Editor).clone();
    let mut socket = open(&app, &editor, &workspace).await;
    let (status, _) = app
        .call(
            "PATCH",
            &format!(
                "/api/v1/workspaces/{}/members/{}",
                workspace.hex(),
                editor.id.to_hex()
            ),
            Some(&owner),
            Some(json!({"role": "VIEWER"})),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(socket.recv_close().await, Some(1000));
}

// ----- the socket itself -----

#[tokio::test]
async fn it_answers_ping_and_closes_on_frames_it_does_not_accept() {
    let app = TestApp::real().await;
    let workspace = workspace_with_people(&app).await;
    let viewer = workspace.member(Role::Viewer).clone();

    let mut socket = open(&app, &viewer, &workspace).await;
    socket.send_text(r#"{"type":"PING","t":42}"#).await;
    assert_eq!(socket.recv_json("PONG").await["t"], 42);

    for (frame, code) in [
        (Message::Text("not json".into()), 1008),
        (Message::Text(r#"{"type":"EDIT"}"#.into()), 1008),
        (Message::Binary(vec![1, 2, 3].into()), 1008),
        (
            Message::Text(
                format!(r#"{{"type":"PING","t":1,"pad":"{}"}}"#, "x".repeat(2000)).into(),
            ),
            1009,
        ),
    ] {
        let mut client = open(&app, &viewer, &workspace).await;
        client.stream.send(frame).await.unwrap();
        assert_eq!(client.recv_close().await, Some(code));
    }
}

#[tokio::test]
async fn a_room_is_forgotten_when_its_last_socket_leaves() {
    let app = TestApp::real().await;
    let workspace = workspace_with_people(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let first = open(&app, &editor, &workspace).await;
    let second = open(&app, &editor, &workspace).await;
    // The server counts a socket in a moment after the client's handshake returns.
    wait_for(|| app.hub.open_workspace_sockets() == 2).await;
    drop(first);
    wait_for(|| app.hub.open_workspace_sockets() == 1).await;
    drop(second);
    wait_for(|| app.hub.open_workspace_sockets() == 0).await;
    // Publishing to a workspace nobody is connected to is harmless.
    make_board(&app, &workspace, &editor).await;
}

async fn wait_for(condition: impl Fn() -> bool) {
    for _ in 0..100 {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the condition never became true");
}

#[tokio::test]
async fn the_server_shutting_down_sends_workspace_sockets_away_with_1001() {
    let app = TestApp::real().await;
    let workspace = workspace_with_people(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let mut socket = open(&app, &editor, &workspace).await;
    wait_for(|| app.hub.open_workspace_sockets() == 1).await;
    app.hub.shutdown(&app.db).await;
    assert_eq!(socket.recv_close().await, Some(1001));
    let _ = Frame::Closed(None);
}

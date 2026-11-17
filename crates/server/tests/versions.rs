//! The version endpoints (tasks 4.3 and 4.4): manual save, list, preview, audit, the broadcast and
//! the roles, against a real MongoDB.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::ws::WsClient;
use common::{Account, TestApp, TestWorkspace};
use mongodb::bson::oid::ObjectId;
use serde_json::{Value, json};
use server::db::versions::{self, AUTOMATIC, NewVersion, RESTORE};
use server::rbac::Role;

struct Fixture<'a> {
    workspace: TestWorkspace<'a>,
    doc_id: String,
    doc: ObjectId,
}

impl<'a> Fixture<'a> {
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
            "Versioned".to_string(),
            "📄".to_string(),
        )
        .unwrap();
        server::db::documents::insert(&app.db, &record)
            .await
            .unwrap();
        Self {
            doc_id: record.id.to_hex(),
            doc: record.id,
            workspace,
        }
    }

    fn editor(&self) -> &Account {
        self.workspace.member(Role::Editor)
    }

    fn viewer(&self) -> &Account {
        self.workspace.member(Role::Viewer)
    }

    fn path(&self, tail: &str) -> String {
        format!("/api/v1/documents/{}/versions{tail}", self.doc_id)
    }

    async fn save(&self, app: &TestApp, who: &Account, label: &str) -> (u16, Value) {
        app.call(
            "POST",
            &self.path(""),
            Some(who),
            Some(json!({"label": label})),
        )
        .await
    }

    /// A stored row of any type, with the current snapshot as its state.
    async fn insert_row(
        &self,
        app: &TestApp,
        change_type: &'static str,
        restored_from: Option<i32>,
    ) {
        let record = server::db::documents::find_with_snapshot(&app.db, self.doc)
            .await
            .unwrap()
            .unwrap();
        versions::insert_next(
            &app.db,
            NewVersion {
                document_id: self.doc,
                workspace_id: self.workspace.id,
                change_type,
                label: None,
                restored_from,
                created_by: (change_type != AUTOMATIC).then_some(self.workspace.owner.id),
                state: &record.crdt_snapshot.unwrap().bytes,
            },
            50,
        )
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn an_editor_saves_a_named_version_and_one_audit_row_is_written() {
    let app = TestApp::real().await;
    let fx = Fixture::new(&app).await;
    let (status, body) = fx.save(&app, fx.editor(), "  Before launch  ").await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["version_number"], 1);
    assert_eq!(body["change_type"], "MANUAL");
    assert_eq!(body["label"], "Before launch");
    assert_eq!(body["created_by"]["id"], fx.editor().id.to_hex());
    assert!(body["created_by"]["name"].as_str().unwrap().len() > 1);
    assert!(body.get("crdt_snapshot").is_none());

    let (_, second) = fx.save(&app, &fx.workspace.owner, "Second").await;
    assert_eq!(second["version_number"], 2);

    let rows = app.audit_rows(fx.workspace.id).await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0].get_str("action").unwrap(), "VERSION_SAVE");
    assert_eq!(rows[0].get_str("entity_type").unwrap(), "DOCUMENT");
    assert_eq!(rows[0].get_str("entity_id").unwrap(), fx.doc_id);
    assert_eq!(rows[0].get_object_id("actor_id").unwrap(), fx.editor().id);
    let metadata = rows[0].get_document("metadata").unwrap();
    assert_eq!(metadata.get_i32("version_number").unwrap(), 1);
    assert_eq!(metadata.get_str("label").unwrap(), "Before launch");
}

#[tokio::test]
async fn a_bad_label_is_a_422_and_writes_nothing() {
    let app = TestApp::real().await;
    let fx = Fixture::new(&app).await;
    for label in ["", "   ", "tab\there", &"x".repeat(81)] {
        let (status, body) = fx.save(&app, fx.editor(), label).await;
        assert_eq!(status, 422, "{label:?}: {body}");
        assert!(body["error"]["details"]["label"].is_string());
    }
    let (status, _) = app
        .call("POST", &fx.path(""), Some(fx.editor()), Some(json!({})))
        .await;
    assert_eq!(status, 422);
    assert!(app.audit_rows(fx.workspace.id).await.is_empty());
    assert!(
        versions::list(&app.db, fx.doc, 10, None)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_viewer_cannot_save_and_an_outsider_sees_a_404() {
    let app = TestApp::real().await;
    let fx = Fixture::new(&app).await;
    let (status, _) = fx.save(&app, fx.viewer(), "Nope").await;
    assert_eq!(status, 403);
    let outsider = app.account("Outsider").await;
    let (status, real) = fx.save(&app, &outsider, "Nope").await;
    let (_, unknown) = app
        .call(
            "POST",
            &format!("/api/v1/documents/{}/versions", ObjectId::new().to_hex()),
            Some(&outsider),
            Some(json!({"label": "Nope"})),
        )
        .await;
    assert_eq!(status, 404);
    assert_eq!(real, unknown, "existence of a document must not leak");
    assert!(app.audit_rows(fx.workspace.id).await.is_empty());
}

#[tokio::test]
async fn a_save_captures_the_live_state_not_only_the_last_snapshot() {
    let app = TestApp::real().await;
    let fx = Fixture::new(&app).await;
    let mut client = WsClient::open(&app, fx.editor(), &fx.doc_id, 11).await;
    client.handshake().await;
    let first = client.doc.blocks()[0].block_id.clone();
    let update = client.doc.insert_text(&first, 0, "typed just now").unwrap();
    client.send_update(&update).await;
    // No snapshot has been written yet (the worker is not running in tests), but the room has it.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let (status, body) = fx.save(&app, fx.editor(), "Live").await;
    assert_eq!(status, 201, "{body}");
    let (status, preview) = app
        .call("GET", &fx.path("/1"), Some(fx.viewer()), None)
        .await;
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["version"]["label"], "Live");
    assert_eq!(preview["blocks"][0]["content"], "typed just now");

    // The open socket was told, with the summary as the payload.
    let frame = client.recv_json("VERSION_CREATED").await;
    assert_eq!(frame["version_number"], 1);
    assert_eq!(frame["label"], "Live");
}

#[tokio::test]
async fn the_list_is_newest_first_with_creators_pages_and_a_kept_flag() {
    let app = TestApp::real().await;
    let fx = Fixture::new(&app).await;
    fx.insert_row(&app, AUTOMATIC, None).await; // 1, will not be pruned (retention 50) ...
    fx.save(&app, fx.editor(), "Named").await; // 2
    fx.insert_row(&app, RESTORE, Some(1)).await; // 3: restored from 1, which exists
    fx.insert_row(&app, RESTORE, Some(7)).await; // 4: restored from 7, which does not

    let (status, page) = app
        .call("GET", &fx.path("?limit=3"), Some(fx.viewer()), None)
        .await;
    assert_eq!(status, 200, "{page}");
    let numbers: Vec<i64> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["version_number"].as_i64().unwrap())
        .collect();
    assert_eq!(numbers, vec![4, 3, 2]);
    assert_eq!(page["next_cursor"], "2");
    assert_eq!(page["items"][0]["restored_from_kept"], false);
    assert_eq!(page["items"][0]["label"], "Restored from Version 7");
    assert_eq!(page["items"][1]["restored_from_kept"], true);
    assert_eq!(
        page["items"][2]["created_by"]["id"],
        fx.editor().id.to_hex()
    );

    let (_, last) = app
        .call(
            "GET",
            &fx.path("?limit=3&cursor=2"),
            Some(fx.viewer()),
            None,
        )
        .await;
    assert_eq!(last["items"].as_array().unwrap().len(), 1);
    assert_eq!(last["items"][0]["change_type"], "AUTOMATIC");
    assert_eq!(last["items"][0]["created_by"], Value::Null);
    assert_eq!(last["items"][0]["label"], "Periodic Snapshot");
    assert_eq!(last["next_cursor"], Value::Null);
}

#[tokio::test]
async fn bad_paging_and_unknown_versions_are_refused() {
    let app = TestApp::real().await;
    let fx = Fixture::new(&app).await;
    for query in [
        "?limit=0",
        "?limit=101",
        "?cursor=abc",
        "?cursor=0",
        "?limit=x",
    ] {
        let (status, _) = app
            .call("GET", &fx.path(query), Some(fx.viewer()), None)
            .await;
        assert!(status == 422 || status == 400, "{query} -> {status}");
    }
    let (status, _) = app
        .call("GET", &fx.path("/9"), Some(fx.viewer()), None)
        .await;
    assert_eq!(status, 404);
    let (status, _) = app
        .call("GET", &fx.path("/not-a-number"), Some(fx.viewer()), None)
        .await;
    assert!(status == 400 || status == 404, "{status}");
}

#[tokio::test]
async fn versions_of_another_workspaces_document_are_a_404() {
    let app = TestApp::real().await;
    let fx = Fixture::new(&app).await;
    fx.save(&app, fx.editor(), "Private").await;
    let other = TestWorkspace::new(&app).await;
    for tail in ["", "/1"] {
        let (status, _) = app
            .call("GET", &fx.path(tail), Some(&other.owner), None)
            .await;
        assert_eq!(status, 404, "{tail}");
    }
}

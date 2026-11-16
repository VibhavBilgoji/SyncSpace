//! Integration tests for the documents REST API (`docs/API.md` section 2, "Documents";
//! `docs/DATA_MODEL.md` section 2 and invariant 8), against a real MongoDB.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use common::{Account, TestApp, TestWorkspace};
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Document as BsonDocument, doc};
use serde_json::{Value, json};
use server::db::documents as store;
use server::rbac::Role;
use sync_crdt::Document as CrdtDocument;

fn list_path(workspace: &TestWorkspace) -> String {
    format!("/api/v1/workspaces/{}/documents", workspace.hex())
}

fn doc_path(id: &str) -> String {
    format!("/api/v1/documents/{id}")
}

fn assert_error(answer: &(u16, Value), status: u16, code: &str) {
    assert_eq!(answer.0, status, "{}", answer.1);
    assert_eq!(answer.1["error"]["code"], code, "{}", answer.1);
    assert!(answer.1["error"]["message"].is_string(), "{}", answer.1);
}

async fn create(
    app: &TestApp,
    workspace: &TestWorkspace<'_>,
    who: &Account,
    body: Value,
) -> (u16, Value) {
    app.call("POST", &list_path(workspace), Some(who), Some(body))
        .await
}

/// Creates a document through the API and returns its JSON.
async fn create_ok(
    app: &TestApp,
    workspace: &TestWorkspace<'_>,
    who: &Account,
    title: &str,
) -> Value {
    let (status, body) = create(app, workspace, who, json!({"title": title})).await;
    assert_eq!(status, 201, "{body}");
    body
}

fn object_id(hex: &Value) -> ObjectId {
    ObjectId::parse_str(hex.as_str().unwrap()).unwrap()
}

async fn audit_actions(app: &TestApp, workspace: &TestWorkspace<'_>) -> Vec<String> {
    app.audit_rows(workspace.id)
        .await
        .iter()
        .map(|row| row.get_str("action").unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn creating_a_document_returns_it_with_one_empty_paragraph_and_writes_one_audit_row() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app)
        .await
        .with_member(Role::Editor)
        .await;
    let editor = workspace.member(Role::Editor).clone();

    let (status, body) = create(
        &app,
        &workspace,
        &editor,
        json!({"title": "  Quarterly plan ", "icon": "🚀"}),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["title"], "Quarterly plan");
    assert_eq!(body["icon"], "🚀");
    assert_eq!(body["revision"], 0);
    assert_eq!(body["workspace_id"], workspace.hex());
    assert_eq!(body["created_by"], editor.id.to_hex());
    assert!(body["created_at"].as_str().unwrap().ends_with('Z'));
    let blocks = body["blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0]["type"], "PARAGRAPH");
    assert_eq!(blocks[0]["content"], "");
    assert!(body.get("crdt_snapshot").is_none());

    // The stored snapshot is what the hub will load: a replica must accept it.
    let stored = store::find_with_snapshot(&app.db, object_id(&body["id"]))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (stored.next_version, stored.changed_since_version),
        (1, false)
    );
    let replica = CrdtDocument::new(9).unwrap();
    replica
        .apply_update(&stored.crdt_snapshot.unwrap().bytes)
        .unwrap();
    assert_eq!(replica.blocks().len(), 1);
    assert_eq!(replica.blocks()[0].block_id, blocks[0]["block_id"]);

    let rows = app.audit_rows(workspace.id).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].get_str("action").unwrap(), "DOC_CREATE");
    assert_eq!(rows[0].get_str("entity_type").unwrap(), "DOCUMENT");
    assert_eq!(
        rows[0].get_str("entity_id").unwrap(),
        body["id"].as_str().unwrap()
    );
    assert_eq!(rows[0].get_object_id("actor_id").unwrap(), editor.id);
    assert_eq!(
        rows[0].get_document("metadata").unwrap(),
        &doc! {"title": "Quarterly plan"}
    );
}

#[tokio::test]
async fn a_document_without_an_icon_gets_the_default_one() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app).await;
    let body = create_ok(&app, &workspace, &workspace.owner, "No icon").await;
    assert_eq!(body["icon"], "📄");
}

#[tokio::test]
async fn creation_validation_reports_every_problem_and_checks_the_role_first() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app)
        .await
        .with_member(Role::Editor)
        .await
        .with_member(Role::Viewer)
        .await;
    let editor = workspace.member(Role::Editor).clone();
    let viewer = workspace.member(Role::Viewer).clone();

    let bad = create(
        &app,
        &workspace,
        &editor,
        json!({"title": "  ", "icon": "x".repeat(17)}),
    )
    .await;
    assert_error(&bad, 422, "VALIDATION");
    assert!(bad.1["error"]["details"]["title"].is_string());
    assert!(bad.1["error"]["details"]["icon"].is_string());

    // Edges: 120 characters pass, 121 fail; tabs are control characters.
    assert_eq!(
        create(&app, &workspace, &editor, json!({"title": "é".repeat(120)}))
            .await
            .0,
        201
    );
    assert_error(
        &create(&app, &workspace, &editor, json!({"title": "é".repeat(121)})).await,
        422,
        "VALIDATION",
    );
    assert_error(
        &create(&app, &workspace, &editor, json!({"title": "tab\there"})).await,
        422,
        "VALIDATION",
    );
    // A missing title is a wrong shape (422); broken JSON is 400.
    let missing = app
        .call(
            "POST",
            &list_path(&workspace),
            Some(&editor),
            Some(json!({"icon": "🚀"})),
        )
        .await;
    assert_error(&missing, 422, "VALIDATION");
    let raw = app
        .client
        .post(app.url(&list_path(&workspace)))
        .header("Cookie", format!("ss_session={}", editor.token))
        .header("Content-Type", "application/json")
        .body("{nope")
        .send()
        .await
        .unwrap();
    assert_eq!(raw.status(), 400);

    // A viewer is refused before the body is read, even with a bad body.
    assert_error(
        &create(&app, &workspace, &viewer, json!({"title": ""})).await,
        403,
        "FORBIDDEN",
    );
    assert_error(
        &create(&app, &workspace, &viewer, json!({"title": "ok"})).await,
        403,
        "FORBIDDEN",
    );

    // Only the two valid documents were written, and only they were audited.
    assert_eq!(audit_actions(&app, &workspace).await, ["DOC_CREATE"; 1]);
}

#[tokio::test]
async fn the_list_has_the_summaries_of_this_workspace_newest_change_first() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app)
        .await
        .with_member(Role::Viewer)
        .await;
    let elsewhere = TestWorkspace::new(&app).await;
    let owner = workspace.owner.clone();

    let first = create_ok(&app, &workspace, &owner, "First").await;
    tokio::time::sleep(Duration::from_millis(5)).await;
    let second = create_ok(&app, &workspace, &owner, "Second").await;
    create_ok(&app, &elsewhere, &elsewhere.owner, "Not ours").await;

    let viewer = workspace.member(Role::Viewer);
    let (status, body) = app
        .call("GET", &list_path(&workspace), Some(viewer), None)
        .await;
    assert_eq!(status, 200, "{body}");
    let titles: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["Second", "First"]);
    let keys: Vec<&str> = {
        let mut keys: Vec<&str> = body[0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        keys
    };
    assert_eq!(keys, ["icon", "id", "revision", "title", "updated_at"]);

    // Renaming the older one moves it to the top.
    tokio::time::sleep(Duration::from_millis(5)).await;
    let renamed = app
        .call(
            "PATCH",
            &doc_path(first["id"].as_str().unwrap()),
            Some(&owner),
            Some(json!({"title": "First, renamed"})),
        )
        .await;
    assert_eq!(renamed.0, 200);
    let (_, body) = app
        .call("GET", &list_path(&workspace), Some(viewer), None)
        .await;
    assert_eq!(body[0]["id"], first["id"]);
    assert_eq!(body[1]["id"], second["id"]);
}

#[tokio::test]
async fn an_empty_workspace_has_an_empty_list_and_an_outsider_gets_404() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app).await;
    let (status, body) = app
        .call("GET", &list_path(&workspace), Some(&workspace.owner), None)
        .await;
    assert_eq!((status, body), (200, json!([])));

    let outsider = app.account("Outsider").await;
    let real = app
        .call("GET", &list_path(&workspace), Some(&outsider), None)
        .await;
    let unknown = app
        .call(
            "GET",
            &format!("/api/v1/workspaces/{}/documents", ObjectId::new().to_hex()),
            Some(&outsider),
            None,
        )
        .await;
    assert_error(&real, 404, "NOT_FOUND");
    assert_eq!(real, unknown, "existence of a workspace must not leak");
}

#[tokio::test]
async fn a_viewer_reads_a_document_with_its_blocks() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app)
        .await
        .with_member(Role::Viewer)
        .await;
    let created = create_ok(&app, &workspace, &workspace.owner, "Readable").await;
    let viewer = workspace.member(Role::Viewer);

    let (status, body) = app
        .call(
            "GET",
            &doc_path(created["id"].as_str().unwrap()),
            Some(viewer),
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, created, "a fresh read equals what creation returned");
}

#[tokio::test]
async fn a_document_of_another_workspace_is_404_with_the_same_answer_as_an_unknown_id() {
    let app = TestApp::real().await;
    let mine = TestWorkspace::new(&app).await;
    let theirs = TestWorkspace::new(&app).await;
    let secret = create_ok(&app, &theirs, &theirs.owner, "Secret").await;
    let id = secret["id"].as_str().unwrap();

    let existing = app
        .call("GET", &doc_path(id), Some(&mine.owner), None)
        .await;
    let unknown = app
        .call(
            "GET",
            &doc_path(&ObjectId::new().to_hex()),
            Some(&mine.owner),
            None,
        )
        .await;
    assert_error(&existing, 404, "NOT_FOUND");
    assert_eq!(
        existing, unknown,
        "the answer must not reveal that the document exists"
    );

    for (method, body) in [
        ("PATCH", Some(json!({"title": "Hijacked"}))),
        ("DELETE", None),
    ] {
        let answer = app
            .call(method, &doc_path(id), Some(&mine.owner), body)
            .await;
        assert_eq!(answer, unknown, "{method}");
    }
    // Nothing changed.
    let (_, still) = app
        .call("GET", &doc_path(id), Some(&theirs.owner), None)
        .await;
    assert_eq!(still, secret);
    assert_eq!(audit_actions(&app, &theirs).await, ["DOC_CREATE"]);
}

#[tokio::test]
async fn malformed_ids_are_400() {
    let app = TestApp::real().await;
    let ada = app.account("Ada").await;
    for bad in [
        "nope",
        "0123456789abcdef0123456",
        "zzzzzzzzzzzzzzzzzzzzzzzz",
    ] {
        for (method, body) in [
            ("GET", None),
            ("PATCH", Some(json!({"title": "x"}))),
            ("DELETE", None),
        ] {
            let answer = app.call(method, &doc_path(bad), Some(&ada), body).await;
            assert_error(&answer, 400, "BAD_REQUEST");
        }
    }
}

#[tokio::test]
async fn an_editor_renames_a_document_and_changes_its_icon_without_touching_the_content() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app)
        .await
        .with_member(Role::Editor)
        .await;
    let editor = workspace.member(Role::Editor).clone();
    let created = create_ok(&app, &workspace, &editor, "Draft").await;
    let id = object_id(&created["id"]);
    let before = store::find_with_snapshot(&app.db, id)
        .await
        .unwrap()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;

    let (status, body) = app
        .call(
            "PATCH",
            &doc_path(created["id"].as_str().unwrap()),
            Some(&editor),
            Some(json!({"title": " Final ", "icon": "✅"})),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["title"], "Final");
    assert_eq!(body["icon"], "✅");
    assert_eq!(body["blocks"], created["blocks"]);
    assert_eq!(body["created_at"], created["created_at"]);
    assert_ne!(body["updated_at"], created["updated_at"]);

    // One field at a time works, and the CRDT snapshot is untouched.
    let (status, body) = app
        .call(
            "PATCH",
            &doc_path(created["id"].as_str().unwrap()),
            Some(&editor),
            Some(json!({"icon": "📘"})),
        )
        .await;
    assert_eq!(
        (status, body["title"].as_str(), body["icon"].as_str()),
        (200, Some("Final"), Some("📘"))
    );
    let after = store::find_with_snapshot(&app.db, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.crdt_snapshot, before.crdt_snapshot);
    assert_eq!(after.revision, before.revision);
    // Renaming is not an audited action.
    assert_eq!(audit_actions(&app, &workspace).await, ["DOC_CREATE"]);
}

#[tokio::test]
async fn patch_validation_and_permissions() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app)
        .await
        .with_member(Role::Editor)
        .await
        .with_member(Role::Viewer)
        .await;
    let editor = workspace.member(Role::Editor).clone();
    let viewer = workspace.member(Role::Viewer).clone();
    let created = create_ok(&app, &workspace, &editor, "Draft").await;
    let path = doc_path(created["id"].as_str().unwrap());

    let empty = app
        .call("PATCH", &path, Some(&editor), Some(json!({})))
        .await;
    assert_error(&empty, 422, "VALIDATION");
    let bad = app
        .call(
            "PATCH",
            &path,
            Some(&editor),
            Some(json!({"title": "", "icon": ""})),
        )
        .await;
    assert_error(&bad, 422, "VALIDATION");
    assert!(bad.1["error"]["details"]["title"].is_string());
    assert!(bad.1["error"]["details"]["icon"].is_string());

    // A viewer is refused before the body is read.
    for body in [json!({"title": "Hijack"}), json!({})] {
        let answer = app.call("PATCH", &path, Some(&viewer), Some(body)).await;
        assert_error(&answer, 403, "FORBIDDEN");
    }
    let (_, still) = app.call("GET", &path, Some(&editor), None).await;
    assert_eq!(still["title"], "Draft");
}

#[tokio::test]
async fn only_an_admin_deletes_a_document_and_its_versions_go_with_it() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app)
        .await
        .with_member(Role::Editor)
        .await
        .with_member(Role::Viewer)
        .await;
    let created = create_ok(&app, &workspace, &workspace.owner, "Doomed").await;
    let other = create_ok(&app, &workspace, &workspace.owner, "Survivor").await;
    let id = object_id(&created["id"]);
    let path = doc_path(created["id"].as_str().unwrap());

    // Versions of both documents (the endpoints for them come in task 4.3).
    let versions = app.db.collection::<BsonDocument>("document_versions");
    for (document, number) in [(id, 1), (id, 2), (object_id(&other["id"]), 1)] {
        versions
            .insert_one(doc! {"document_id": document, "version_number": number})
            .await
            .unwrap();
    }

    // Below ADMIN is refused and nothing happens.
    for who in [
        workspace.member(Role::Editor),
        workspace.member(Role::Viewer),
    ] {
        let answer = app.call("DELETE", &path, Some(who), None).await;
        assert_error(&answer, 403, "FORBIDDEN");
    }
    assert_eq!(
        audit_actions(&app, &workspace).await,
        ["DOC_CREATE", "DOC_CREATE"]
    );

    let (status, body) = app
        .call("DELETE", &path, Some(&workspace.owner), None)
        .await;
    assert_eq!((status, body), (204, Value::Null));

    let gone = app.call("GET", &path, Some(&workspace.owner), None).await;
    assert_error(&gone, 404, "NOT_FOUND");
    assert!(store::find(&app.db, id).await.unwrap().is_none());
    assert_eq!(
        versions
            .count_documents(doc! {"document_id": id})
            .await
            .unwrap(),
        0,
        "invariant 8: the versions are deleted with the document"
    );
    assert_eq!(
        versions.count_documents(doc! {}).await.unwrap(),
        1,
        "another document's version stays"
    );

    let rows = app.audit_rows(workspace.id).await;
    let last = rows.last().unwrap();
    assert_eq!(last.get_str("action").unwrap(), "DOC_DELETE");
    assert_eq!(last.get_str("entity_type").unwrap(), "DOCUMENT");
    assert_eq!(
        last.get_str("entity_id").unwrap(),
        created["id"].as_str().unwrap()
    );
    assert_eq!(
        last.get_document("metadata").unwrap(),
        &doc! {"title": "Doomed"}
    );
    assert_eq!(rows.len(), 3);

    // Deleting again is a 404 and writes nothing.
    let again = app
        .call("DELETE", &path, Some(&workspace.owner), None)
        .await;
    assert_error(&again, 404, "NOT_FOUND");
    assert_eq!(app.audit_rows(workspace.id).await.len(), 3);

    // The survivor is untouched.
    let (status, _) = app
        .call(
            "GET",
            &doc_path(other["id"].as_str().unwrap()),
            Some(&workspace.owner),
            None,
        )
        .await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn simultaneous_creations_all_succeed_with_distinct_ids_and_block_ids() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app).await;
    let owner = workspace.owner.clone();
    let results = futures_util::future::join_all((0..6).map(|n| {
        create(
            &app,
            &workspace,
            &owner,
            json!({"title": format!("Doc {n}")}),
        )
    }))
    .await;
    let mut ids = std::collections::BTreeSet::new();
    let mut blocks = std::collections::BTreeSet::new();
    for (status, body) in &results {
        assert_eq!(*status, 201, "{body}");
        ids.insert(body["id"].as_str().unwrap().to_string());
        blocks.insert(body["blocks"][0]["block_id"].as_str().unwrap().to_string());
    }
    assert_eq!((ids.len(), blocks.len()), (6, 6));
    assert_eq!(audit_actions(&app, &workspace).await.len(), 6);
}

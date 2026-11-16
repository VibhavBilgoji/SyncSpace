//! Integration tests for workspaces (`docs/API.md` section 2, `docs/PRD.md` FR2), against a real
//! MongoDB. Users and workspaces are written straight to the database where the test is not
//! about creating them, so no test pays for password hashing it does not need.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{TestApp, TestWorkspace};
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime, Document, doc};
use serde_json::{Value, json};
use server::rbac::Role;

const WORKSPACES: &str = "/api/v1/workspaces";

fn assert_error(answer: &(u16, Value), status: u16, code: &str) {
    assert_eq!(answer.0, status, "{}", answer.1);
    assert_eq!(answer.1["error"]["code"], code, "{}", answer.1);
    assert!(answer.1["error"]["message"].is_string(), "{}", answer.1);
}

async fn create(app: &TestApp, who: &common::Account, name: &str, slug: &str) -> (u16, Value) {
    app.call(
        "POST",
        WORKSPACES,
        Some(who),
        Some(json!({"name": name, "slug": slug})),
    )
    .await
}

async fn workspace_count(app: &TestApp) -> u64 {
    app.db
        .collection::<Document>("workspaces")
        .count_documents(doc! {})
        .await
        .unwrap()
}

#[tokio::test]
async fn creating_a_workspace_makes_the_caller_its_owner_and_only_admin() {
    let app = TestApp::real().await;
    let ada = app.account("Ada").await;

    let (status, body) = create(&app, &ada, "  Engineering Team ", "engineering").await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["name"], "Engineering Team");
    assert_eq!(body["slug"], "engineering");
    assert_eq!(body["owner_id"], ada.id.to_hex());
    assert_eq!(body["role"], "ADMIN");
    assert!(body["id"].as_str().unwrap().len() == 24);
    assert_eq!(body["created_at"], body["updated_at"]);
    assert!(body["created_at"].as_str().unwrap().ends_with('Z'));

    // Invariant 1: one owner, who is an ADMIN member.
    let stored = app
        .db
        .collection::<Document>("workspaces")
        .find_one(doc! {})
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.get_object_id("owner_id").unwrap(), ada.id);
    let members = stored.get_array("members").unwrap();
    assert_eq!(members.len(), 1);
    let member = members[0].as_document().unwrap();
    assert_eq!(member.get_object_id("user_id").unwrap(), ada.id);
    assert_eq!(member.get_str("role").unwrap(), "ADMIN");
    assert!(member.get_datetime("joined_at").is_ok());

    // Creating a workspace is not an audited action.
    assert_eq!(app.audit_row_count().await, 0);
}

#[tokio::test]
async fn a_taken_slug_is_409_and_no_second_workspace_appears() {
    let app = TestApp::real().await;
    let ada = app.account("Ada").await;
    let grace = app.account("Grace").await;

    assert_eq!(create(&app, &ada, "One", "shared-slug").await.0, 201);
    let answer = create(&app, &grace, "Two", "shared-slug").await;
    assert_error(&answer, 409, "CONFLICT");
    assert_eq!(workspace_count(&app).await, 1);
}

#[tokio::test]
async fn concurrent_creations_of_one_slug_give_one_201_and_one_409() {
    let app = TestApp::real().await;
    let ada = app.account("Ada").await;
    let grace = app.account("Grace").await;

    let (a, b) = tokio::join!(
        create(&app, &ada, "A", "race-slug"),
        create(&app, &grace, "B", "race-slug")
    );
    let mut statuses = [a.0, b.0];
    statuses.sort_unstable();
    assert_eq!(statuses, [201, 409], "{a:?} {b:?}");
    assert_eq!(workspace_count(&app).await, 1);
}

#[tokio::test]
async fn creation_validation_reports_every_problem_at_once() {
    let app = TestApp::real().await;
    let ada = app.account("Ada").await;

    let answer = create(&app, &ada, "   ", "Not A Slug").await;
    assert_error(&answer, 422, "VALIDATION");
    assert!(answer.1["error"]["details"]["name"].is_string());
    assert!(answer.1["error"]["details"]["slug"].is_string());

    // The slug is never changed for the caller: upper case is rejected, not lower-cased.
    assert_error(&create(&app, &ada, "Ok", "UPPER").await, 422, "VALIDATION");
    // Edges: 2 and 41 characters fail, 3 and 40 pass.
    assert_error(&create(&app, &ada, "Ok", "ab").await, 422, "VALIDATION");
    assert_error(
        &create(&app, &ada, "Ok", &"a".repeat(41)).await,
        422,
        "VALIDATION",
    );
    assert_eq!(create(&app, &ada, "Ok", "abc").await.0, 201);
    assert_eq!(create(&app, &ada, "Ok", &"a".repeat(40)).await.0, 201);
    // Edges of the name: 60 characters pass, 61 fail.
    assert_eq!(
        create(&app, &ada, &"é".repeat(60), "sixty-chars").await.0,
        201
    );
    assert_error(
        &create(&app, &ada, &"é".repeat(61), "sixty-one").await,
        422,
        "VALIDATION",
    );

    // Wrong shapes are 422, broken JSON is 400.
    let missing = app
        .call("POST", WORKSPACES, Some(&ada), Some(json!({"name": "x"})))
        .await;
    assert_error(&missing, 422, "VALIDATION");
    let raw = app
        .client
        .post(app.url(WORKSPACES))
        .header("Cookie", format!("ss_session={}", ada.token))
        .header("Content-Type", "application/json")
        .body("{not json")
        .send()
        .await
        .unwrap();
    assert_eq!(raw.status(), 400);
}

#[tokio::test]
async fn the_list_holds_only_the_callers_workspaces_with_role_and_counts() {
    let app = TestApp::real().await;
    let mine = TestWorkspace::new(&app)
        .await
        .with_member(Role::Viewer)
        .await
        .with_member(Role::Editor)
        .await;
    let other = TestWorkspace::new(&app).await;
    let viewer = mine.member(Role::Viewer).clone();

    // Two documents and one board in `mine`, one document in somebody else's workspace.
    let documents = app.db.collection::<Document>("documents");
    for workspace in [mine.id, mine.id, other.id] {
        documents
            .insert_one(doc! {"workspace_id": workspace, "created_at": DateTime::now()})
            .await
            .unwrap();
    }
    app.db
        .collection::<Document>("boards")
        .insert_one(doc! {"workspace_id": mine.id})
        .await
        .unwrap();

    let (status, body) = app.call("GET", WORKSPACES, Some(&viewer), None).await;
    assert_eq!(status, 200, "{body}");
    let list = body.as_array().unwrap();
    assert_eq!(list.len(), 1, "{body}");
    let card = &list[0];
    assert_eq!(card["id"], mine.hex());
    assert_eq!(card["role"], "VIEWER");
    assert_eq!(card["is_owner"], false);
    assert_eq!(card["member_count"], 3);
    assert_eq!(card["document_count"], 2);
    assert_eq!(card["board_count"], 1);

    // The owner sees the same workspace as ADMIN and as owner; the other workspace has no
    // documents or boards of its own to count.
    let (_, body) = app.call("GET", WORKSPACES, Some(&other.owner), None).await;
    assert_eq!(body[0]["id"], other.hex());
    assert_eq!(body[0]["role"], "ADMIN");
    assert_eq!(body[0]["is_owner"], true);
    assert_eq!(body[0]["document_count"], 1);
    assert_eq!(body[0]["board_count"], 0);
}

#[tokio::test]
async fn the_list_is_empty_for_a_user_without_workspaces_and_sorted_by_name() {
    let app = TestApp::real().await;
    let ada = app.account("Ada").await;
    let (status, body) = app.call("GET", WORKSPACES, Some(&ada), None).await;
    assert_eq!((status, body), (200, json!([])));

    for (name, slug) in [("zebra", "zeb"), ("Apple", "app"), ("mango", "man")] {
        assert_eq!(create(&app, &ada, name, slug).await.0, 201);
    }
    let (_, body) = app.call("GET", WORKSPACES, Some(&ada), None).await;
    let names: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Apple", "mango", "zebra"]);
}

#[tokio::test]
async fn a_member_reads_the_workspace_with_their_own_role() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app)
        .await
        .with_member(Role::Viewer)
        .await
        .with_member(Role::Editor)
        .await;
    let path = format!("{WORKSPACES}/{}", workspace.hex());

    for (who, role) in [
        (&workspace.owner, "ADMIN"),
        (workspace.member(Role::Editor), "EDITOR"),
        (workspace.member(Role::Viewer), "VIEWER"),
    ] {
        let (status, body) = app.call("GET", &path, Some(who), None).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["id"], workspace.hex());
        assert_eq!(body["role"], role);
        assert_eq!(body["owner_id"], workspace.owner.id.to_hex());
    }
}

#[tokio::test]
async fn a_non_member_gets_the_same_404_as_for_a_workspace_that_does_not_exist() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app).await;
    let outsider = app.account("Outsider").await;

    let real = app
        .call(
            "GET",
            &format!("{WORKSPACES}/{}", workspace.hex()),
            Some(&outsider),
            None,
        )
        .await;
    let missing = app
        .call(
            "GET",
            &format!("{WORKSPACES}/{}", ObjectId::new().to_hex()),
            Some(&outsider),
            None,
        )
        .await;
    assert_error(&real, 404, "NOT_FOUND");
    assert_eq!(real, missing, "existence must not leak");
}

#[tokio::test]
async fn a_malformed_workspace_id_is_400_in_the_error_format() {
    let app = TestApp::real().await;
    let ada = app.account("Ada").await;
    for bad in [
        "nope",
        "0123456789abcdef0123456",
        "zzzzzzzzzzzzzzzzzzzzzzzz",
    ] {
        let answer = app
            .call("GET", &format!("{WORKSPACES}/{bad}"), Some(&ada), None)
            .await;
        assert_error(&answer, 400, "BAD_REQUEST");
    }
}

#[tokio::test]
async fn an_admin_renames_the_workspace_and_changes_its_slug() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app).await;
    let before = workspace.stored().await;
    let path = format!("{WORKSPACES}/{}", workspace.hex());

    let (status, body) = app
        .call(
            "PATCH",
            &path,
            Some(&workspace.owner),
            Some(json!({"name": "  Renamed ", "slug": "renamed"})),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["name"], "Renamed");
    assert_eq!(body["slug"], "renamed");
    assert_eq!(body["role"], "ADMIN");

    let after = workspace.stored().await;
    assert_eq!(after.name, "Renamed");
    assert_eq!(after.slug, "renamed");
    assert!(after.updated_at >= before.updated_at);
    assert_eq!(after.created_at, before.created_at);
    assert_eq!(after.owner_id, before.owner_id);
    assert_eq!(
        after.members, before.members,
        "a patch must not touch members"
    );

    // One field at a time works too.
    let (status, body) = app
        .call(
            "PATCH",
            &path,
            Some(&workspace.owner),
            Some(json!({"name": "Only name"})),
        )
        .await;
    assert_eq!((status, body["slug"].as_str()), (200, Some("renamed")));
    assert_eq!(
        app.audit_row_count().await,
        0,
        "settings changes are not audited"
    );
}

#[tokio::test]
async fn patching_to_a_taken_slug_is_409_and_changes_nothing() {
    let app = TestApp::real().await;
    let first = TestWorkspace::new(&app).await;
    let second = TestWorkspace::new(&app).await;
    let taken = first.stored().await.slug;
    let original = second.stored().await;

    let answer = app
        .call(
            "PATCH",
            &format!("{WORKSPACES}/{}", second.hex()),
            Some(&second.owner),
            Some(json!({"name": "Would rename", "slug": taken})),
        )
        .await;
    assert_error(&answer, 409, "CONFLICT");
    assert_eq!(second.stored().await, original);

    // Keeping your own slug is not a conflict.
    let same = app
        .call(
            "PATCH",
            &format!("{WORKSPACES}/{}", second.hex()),
            Some(&second.owner),
            Some(json!({"slug": original.slug})),
        )
        .await;
    assert_eq!(same.0, 200, "{}", same.1);
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
    let path = format!("{WORKSPACES}/{}", workspace.hex());
    let original = workspace.stored().await;

    let empty = app
        .call("PATCH", &path, Some(&workspace.owner), Some(json!({})))
        .await;
    assert_error(&empty, 422, "VALIDATION");
    let bad = app
        .call(
            "PATCH",
            &path,
            Some(&workspace.owner),
            Some(json!({"name": "", "slug": "X"})),
        )
        .await;
    assert_error(&bad, 422, "VALIDATION");
    assert!(bad.1["error"]["details"]["name"].is_string());
    assert!(bad.1["error"]["details"]["slug"].is_string());

    // Below ADMIN is 403, and the role is checked before the body: a bad body from an editor
    // is still 403.
    for who in [
        workspace.member(Role::Editor),
        workspace.member(Role::Viewer),
    ] {
        let answer = app
            .call("PATCH", &path, Some(who), Some(json!({"name": "Hijack"})))
            .await;
        assert_error(&answer, 403, "FORBIDDEN");
        let answer = app.call("PATCH", &path, Some(who), Some(json!({}))).await;
        assert_error(&answer, 403, "FORBIDDEN");
    }
    assert_eq!(workspace.stored().await, original);
}

//! Integration tests for members and their audit rows (`docs/TESTING.md`, "Members" and
//! "Audit"; `docs/API.md` section 2; `docs/DATA_MODEL.md` section 3 and invariants 1, 6, 7).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{Account, TestApp, TestWorkspace};
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Document, doc};
use serde_json::{Value, json};
use server::rbac::Role;

fn members_path(workspace: &TestWorkspace) -> String {
    format!("/api/v1/workspaces/{}/members", workspace.hex())
}

fn member_path(workspace: &TestWorkspace, user: &Account) -> String {
    format!("{}/{}", members_path(workspace), user.id.to_hex())
}

fn assert_error(answer: &(u16, Value), status: u16, code: &str) {
    assert_eq!(answer.0, status, "{}", answer.1);
    assert_eq!(answer.1["error"]["code"], code, "{}", answer.1);
    assert!(answer.1["error"]["message"].is_string(), "{}", answer.1);
}

async fn add(
    app: &TestApp,
    workspace: &TestWorkspace<'_>,
    by: &Account,
    email: &str,
    role: &str,
) -> (u16, Value) {
    app.call(
        "POST",
        &members_path(workspace),
        Some(by),
        Some(json!({"email": email, "role": role})),
    )
    .await
}

async fn member_roles(workspace: &TestWorkspace<'_>) -> Vec<(ObjectId, Role)> {
    workspace
        .stored()
        .await
        .members
        .iter()
        .map(|m| (m.user_id, m.role))
        .collect()
}

#[tokio::test]
async fn adding_a_member_stores_them_and_writes_exactly_one_audit_row() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app).await;
    let grace = app.account("Grace Hopper").await;

    let (status, body) = add(&app, &workspace, &workspace.owner, &grace.email, "EDITOR").await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["user_id"], grace.id.to_hex());
    assert_eq!(body["name"], "Grace Hopper");
    assert_eq!(body["email"], grace.email);
    assert_eq!(body["role"], "EDITOR");
    assert_eq!(body["is_owner"], false);
    assert!(body["joined_at"].as_str().unwrap().ends_with('Z'));
    assert!(body.get("password_hash").is_none());

    assert!(
        member_roles(&workspace)
            .await
            .contains(&(grace.id, Role::Editor))
    );

    let rows = app.audit_rows(workspace.id).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    let row = &rows[0];
    assert_eq!(row.get_str("action").unwrap(), "MEMBER_ADD");
    assert_eq!(row.get_str("entity_type").unwrap(), "WORKSPACE");
    assert_eq!(row.get_str("entity_id").unwrap(), workspace.hex());
    assert_eq!(row.get_object_id("actor_id").unwrap(), workspace.owner.id);
    assert_eq!(row.get_object_id("workspace_id").unwrap(), workspace.id);
    assert_eq!(
        row.get_document("metadata").unwrap(),
        &doc! {"user_id": grace.id.to_hex(), "assigned": "EDITOR"}
    );
    assert!(row.get_datetime("timestamp").is_ok());
}

#[tokio::test]
async fn the_email_of_a_new_member_is_matched_ignoring_case_and_spaces() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app).await;
    let grace = app.account("Grace").await;
    let shouted = format!("  {}  ", grace.email.to_uppercase());

    let (status, body) = add(&app, &workspace, &workspace.owner, &shouted, "VIEWER").await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["email"], grace.email);
}

#[tokio::test]
async fn an_unknown_email_is_404_a_bad_one_is_422_and_neither_writes_a_row() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app).await;

    let unknown = add(
        &app,
        &workspace,
        &workspace.owner,
        "nobody@example.com",
        "VIEWER",
    )
    .await;
    assert_error(&unknown, 404, "NOT_FOUND");
    assert_eq!(
        unknown.1["error"]["message"],
        "No SyncSpace account uses this email"
    );

    let invalid = add(&app, &workspace, &workspace.owner, "not-an-email", "VIEWER").await;
    assert_error(&invalid, 422, "VALIDATION");
    assert!(invalid.1["error"]["details"]["email"].is_string());

    for bad_role in ["OWNER", "admin", ""] {
        let answer = add(
            &app,
            &workspace,
            &workspace.owner,
            "a@example.com",
            bad_role,
        )
        .await;
        assert_error(&answer, 422, "VALIDATION");
    }
    let no_role = app
        .call(
            "POST",
            &members_path(&workspace),
            Some(&workspace.owner),
            Some(json!({"email": "a@example.com"})),
        )
        .await;
    assert_error(&no_role, 422, "VALIDATION");

    assert_eq!(app.audit_row_count().await, 0);
    assert_eq!(workspace.stored().await.members.len(), 1);
}

#[tokio::test]
async fn adding_someone_who_is_already_a_member_is_409_and_changes_nothing() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app)
        .await
        .with_member(Role::Viewer)
        .await;
    let viewer = workspace.member(Role::Viewer).clone();

    let again = add(&app, &workspace, &workspace.owner, &viewer.email, "ADMIN").await;
    assert_error(&again, 409, "CONFLICT");
    let owner = add(
        &app,
        &workspace,
        &workspace.owner,
        &workspace.owner.email,
        "VIEWER",
    )
    .await;
    assert_error(&owner, 409, "CONFLICT");

    let roles = member_roles(&workspace).await;
    assert_eq!(roles.len(), 2);
    assert!(
        roles.contains(&(viewer.id, Role::Viewer)),
        "role must not change"
    );
    assert!(roles.contains(&(workspace.owner.id, Role::Admin)));
    assert_eq!(app.audit_row_count().await, 0);
}

#[tokio::test]
async fn two_simultaneous_adds_of_one_user_give_one_201_one_409_and_one_row() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app).await;
    let grace = app.account("Grace").await;

    let (a, b) = tokio::join!(
        add(&app, &workspace, &workspace.owner, &grace.email, "EDITOR"),
        add(&app, &workspace, &workspace.owner, &grace.email, "VIEWER"),
    );
    let mut statuses = [a.0, b.0];
    statuses.sort_unstable();
    assert_eq!(statuses, [201, 409], "{a:?} {b:?}");
    assert_eq!(
        workspace.stored().await.members.len(),
        2,
        "one entry per user"
    );
    assert_eq!(app.audit_rows(workspace.id).await.len(), 1);
}

#[tokio::test]
async fn the_member_list_has_the_owner_first_and_every_field() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app)
        .await
        .with_member(Role::Editor)
        .await
        .with_member(Role::Viewer)
        .await;

    // A viewer may read the list.
    let viewer = workspace.member(Role::Viewer);
    let (status, body) = app
        .call("GET", &members_path(&workspace), Some(viewer), None)
        .await;
    assert_eq!(status, 200, "{body}");
    let list = body.as_array().unwrap();
    assert_eq!(list.len(), 3);
    assert_eq!(list[0]["user_id"], workspace.owner.id.to_hex());
    assert_eq!(list[0]["is_owner"], true);
    assert_eq!(list[0]["role"], "ADMIN");
    assert_eq!(list[1]["role"], "EDITOR");
    assert_eq!(list[2]["role"], "VIEWER");
    for entry in list {
        for field in ["user_id", "name", "email", "role", "joined_at", "is_owner"] {
            assert!(!entry[field].is_null(), "{field} missing in {entry}");
        }
        assert!(entry.get("password_hash").is_none());
    }
    assert_eq!(list.iter().filter(|m| m["is_owner"] == true).count(), 1);
}

#[tokio::test]
async fn changing_a_role_stores_it_and_writes_one_role_change_row() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app)
        .await
        .with_member(Role::Viewer)
        .await;
    let viewer = workspace.member(Role::Viewer).clone();

    let (status, body) = app
        .call(
            "PATCH",
            &member_path(&workspace, &viewer),
            Some(&workspace.owner),
            Some(json!({"role": "EDITOR"})),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["user_id"], viewer.id.to_hex());
    assert_eq!(body["role"], "EDITOR");
    assert_eq!(body["is_owner"], false);
    assert_eq!(body["name"], viewer.name);

    assert!(
        member_roles(&workspace)
            .await
            .contains(&(viewer.id, Role::Editor))
    );

    let rows = app.audit_rows(workspace.id).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].get_str("action").unwrap(), "ROLE_CHANGE");
    assert_eq!(rows[0].get_str("entity_type").unwrap(), "MEMBER");
    assert_eq!(rows[0].get_str("entity_id").unwrap(), viewer.id.to_hex());
    assert_eq!(
        rows[0].get_document("metadata").unwrap(),
        &doc! {"from": "VIEWER", "to": "EDITOR"}
    );
    assert_eq!(
        rows[0].get_object_id("actor_id").unwrap(),
        workspace.owner.id
    );
}

#[tokio::test]
async fn setting_the_role_a_member_already_has_is_200_and_writes_no_row() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app)
        .await
        .with_member(Role::Editor)
        .await;
    let editor = workspace.member(Role::Editor).clone();

    let (status, body) = app
        .call(
            "PATCH",
            &member_path(&workspace, &editor),
            Some(&workspace.owner),
            Some(json!({"role": "EDITOR"})),
        )
        .await;
    assert_eq!((status, &body["role"]), (200, &json!("EDITOR")));
    assert_eq!(
        app.audit_row_count().await,
        0,
        "nothing changed, nothing to audit"
    );
}

#[tokio::test]
async fn the_owner_cannot_be_demoted_or_removed_even_by_another_admin() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app)
        .await
        .with_member(Role::Admin)
        .await;
    let admin = workspace.member(Role::Admin).clone();
    let owner = workspace.owner.clone();

    // By themselves and by a second admin.
    for by in [&owner, &admin] {
        let demote = app
            .call(
                "PATCH",
                &member_path(&workspace, &owner),
                Some(by),
                Some(json!({"role": "VIEWER"})),
            )
            .await;
        assert_error(&demote, 409, "CONFLICT");
        // Even "changing" to ADMIN is refused: the owner's membership is locked.
        let same = app
            .call(
                "PATCH",
                &member_path(&workspace, &owner),
                Some(by),
                Some(json!({"role": "ADMIN"})),
            )
            .await;
        assert_error(&same, 409, "CONFLICT");
        let remove = app
            .call("DELETE", &member_path(&workspace, &owner), Some(by), None)
            .await;
        assert_error(&remove, 409, "CONFLICT");
    }

    let stored = workspace.stored().await;
    assert_eq!(stored.owner_id, owner.id);
    assert_eq!(stored.role_of(owner.id), Some(Role::Admin));
    assert_eq!(app.audit_row_count().await, 0);
}

#[tokio::test]
async fn changing_or_removing_someone_who_is_not_a_member_is_404() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app).await;
    let stranger = app.account("Stranger").await;

    let change = app
        .call(
            "PATCH",
            &member_path(&workspace, &stranger),
            Some(&workspace.owner),
            Some(json!({"role": "VIEWER"})),
        )
        .await;
    assert_error(&change, 404, "NOT_FOUND");
    let remove = app
        .call(
            "DELETE",
            &member_path(&workspace, &stranger),
            Some(&workspace.owner),
            None,
        )
        .await;
    assert_error(&remove, 404, "NOT_FOUND");
    assert_eq!(app.audit_row_count().await, 0);
}

#[tokio::test]
async fn removing_a_member_deletes_them_audits_it_and_cuts_their_access() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app)
        .await
        .with_member(Role::Editor)
        .await
        .with_member(Role::Viewer)
        .await;
    let editor = workspace.member(Role::Editor).clone();
    let viewer = workspace.member(Role::Viewer).clone();

    let (status, body) = app
        .call(
            "DELETE",
            &member_path(&workspace, &editor),
            Some(&workspace.owner),
            None,
        )
        .await;
    assert_eq!((status, body), (204, Value::Null));

    let roles = member_roles(&workspace).await;
    assert_eq!(roles.len(), 2);
    assert!(!roles.iter().any(|(id, _)| *id == editor.id));
    assert!(roles.contains(&(viewer.id, Role::Viewer)));

    let rows = app.audit_rows(workspace.id).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].get_str("action").unwrap(), "MEMBER_REMOVE");
    assert_eq!(rows[0].get_str("entity_type").unwrap(), "WORKSPACE");
    assert_eq!(rows[0].get_str("entity_id").unwrap(), workspace.hex());
    assert_eq!(
        rows[0].get_document("metadata").unwrap(),
        &doc! {"user_id": editor.id.to_hex(), "role": "EDITOR"}
    );

    // Their session still verifies, but the workspace is gone for them: 404, same as unknown.
    let path = format!("/api/v1/workspaces/{}", workspace.hex());
    let (status, _) = app.call("GET", &path, Some(&editor), None).await;
    assert_eq!(status, 404);

    // Removing twice: the second time they are not a member.
    let again = app
        .call(
            "DELETE",
            &member_path(&workspace, &editor),
            Some(&workspace.owner),
            None,
        )
        .await;
    assert_error(&again, 404, "NOT_FOUND");
    assert_eq!(app.audit_rows(workspace.id).await.len(), 1);
}

#[tokio::test]
async fn removing_a_member_takes_them_off_this_workspaces_tasks_only() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app)
        .await
        .with_member(Role::Editor)
        .await;
    let elsewhere = TestWorkspace::new(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let colleague = ObjectId::new();

    let tasks = app.db.collection::<Document>("tasks");
    let mine = tasks
        .insert_one(doc! {"workspace_id": workspace.id, "assignee_ids": [editor.id, colleague]})
        .await
        .unwrap()
        .inserted_id;
    let theirs = tasks
        .insert_one(doc! {"workspace_id": elsewhere.id, "assignee_ids": [editor.id]})
        .await
        .unwrap()
        .inserted_id;

    let (status, _) = app
        .call(
            "DELETE",
            &member_path(&workspace, &editor),
            Some(&workspace.owner),
            None,
        )
        .await;
    assert_eq!(status, 204);

    let mine = tasks.find_one(doc! {"_id": mine}).await.unwrap().unwrap();
    assert_eq!(
        mine.get_array("assignee_ids").unwrap(),
        &vec![mongodb::bson::Bson::ObjectId(colleague)],
        "only the removed member leaves the assignees"
    );
    let theirs = tasks.find_one(doc! {"_id": theirs}).await.unwrap().unwrap();
    assert_eq!(theirs.get_array("assignee_ids").unwrap().len(), 1);
}

#[tokio::test]
async fn a_non_owner_admin_can_demote_themselves_and_then_loses_admin_rights() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app)
        .await
        .with_member(Role::Admin)
        .await;
    let admin = workspace.member(Role::Admin).clone();

    // A second admin can manage members.
    let (status, _) = app
        .call(
            "PATCH",
            &member_path(&workspace, &admin),
            Some(&admin),
            Some(json!({"role": "VIEWER"})),
        )
        .await;
    assert_eq!(status, 200);

    // The very next request is checked against the new role.
    let (status, body) = app
        .call("GET", &members_path(&workspace), Some(&admin), None)
        .await;
    assert_eq!(status, 200, "a viewer may still read: {body}");
    let denied = add(&app, &workspace, &admin, "x@example.com", "VIEWER").await;
    assert_error(&denied, 403, "FORBIDDEN");

    // The workspace always keeps its owner as an admin.
    assert_eq!(
        workspace.stored().await.role_of(workspace.owner.id),
        Some(Role::Admin)
    );
}

#[tokio::test]
async fn editors_and_viewers_cannot_manage_members() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app)
        .await
        .with_member(Role::Editor)
        .await
        .with_member(Role::Viewer)
        .await;
    let target = workspace.member(Role::Viewer).clone();
    let grace = app.account("Grace").await;
    let before = workspace.stored().await;

    for who in [
        workspace.member(Role::Editor),
        workspace.member(Role::Viewer),
    ] {
        assert_error(
            &add(&app, &workspace, who, &grace.email, "VIEWER").await,
            403,
            "FORBIDDEN",
        );
        let change = app
            .call(
                "PATCH",
                &member_path(&workspace, &target),
                Some(who),
                Some(json!({"role": "ADMIN"})),
            )
            .await;
        assert_error(&change, 403, "FORBIDDEN");
        let remove = app
            .call("DELETE", &member_path(&workspace, &target), Some(who), None)
            .await;
        assert_error(&remove, 403, "FORBIDDEN");
    }
    assert_eq!(workspace.stored().await, before);
    assert_eq!(
        app.audit_row_count().await,
        0,
        "refused requests are not audited"
    );
}

#[tokio::test]
async fn malformed_user_ids_are_400() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app).await;
    for bad in ["nope", "0123456789abcdef0123456"] {
        for (method, body) in [("PATCH", Some(json!({"role": "VIEWER"}))), ("DELETE", None)] {
            let answer = app
                .call(
                    method,
                    &format!("{}/{bad}", members_path(&workspace)),
                    Some(&workspace.owner),
                    body,
                )
                .await;
            assert_error(&answer, 400, "BAD_REQUEST");
        }
    }
}

#[tokio::test]
async fn nothing_but_the_three_member_actions_writes_an_audit_row() {
    let app = TestApp::real().await;
    let owner = app.account("Olivia").await;
    let grace = app.account("Grace").await;

    // Create, read, list and patch a workspace: no rows.
    let (status, created) = app
        .call(
            "POST",
            "/api/v1/workspaces",
            Some(&owner),
            Some(json!({"name": "W", "slug": "audit-check"})),
        )
        .await;
    assert_eq!(status, 201);
    let id = created["id"].as_str().unwrap().to_string();
    let path = format!("/api/v1/workspaces/{id}");
    app.call("GET", &path, Some(&owner), None).await;
    app.call("GET", "/api/v1/workspaces", Some(&owner), None)
        .await;
    app.call("GET", &format!("{path}/members"), Some(&owner), None)
        .await;
    app.call("PATCH", &path, Some(&owner), Some(json!({"name": "W2"})))
        .await;
    assert_eq!(app.audit_row_count().await, 0);

    // Add, change, remove: exactly three rows, in that order.
    let (status, _) = app
        .call(
            "POST",
            &format!("{path}/members"),
            Some(&owner),
            Some(json!({"email": grace.email, "role": "VIEWER"})),
        )
        .await;
    assert_eq!(status, 201);
    let member = format!("{path}/members/{}", grace.id.to_hex());
    app.call(
        "PATCH",
        &member,
        Some(&owner),
        Some(json!({"role": "ADMIN"})),
    )
    .await;
    app.call("DELETE", &member, Some(&owner), None).await;

    let rows = app.audit_rows(ObjectId::parse_str(&id).unwrap()).await;
    let actions: Vec<&str> = rows.iter().map(|r| r.get_str("action").unwrap()).collect();
    assert_eq!(actions, ["MEMBER_ADD", "ROLE_CHANGE", "MEMBER_REMOVE"]);
}

/// The handlers refuse to touch the owner before they write, and the database update repeats
/// the rule in its filter. This calls the update directly, so the second guard is tested on its
/// own and a handler that forgot its check could still not demote or remove the owner.
#[tokio::test]
async fn the_database_updates_themselves_refuse_to_touch_the_owner() {
    use server::db::workspaces::{remove_member, set_member_role};

    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app)
        .await
        .with_member(Role::Viewer)
        .await;
    let owner = workspace.owner.id;
    let before = workspace.stored().await;

    assert!(
        set_member_role(&app.db, workspace.id, owner, Role::Viewer)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        remove_member(&app.db, workspace.id, owner)
            .await
            .unwrap()
            .is_none()
    );
    // Nor can they touch someone who is not a member.
    let stranger = ObjectId::new();
    assert!(
        set_member_role(&app.db, workspace.id, stranger, Role::Admin)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        remove_member(&app.db, workspace.id, stranger)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(workspace.stored().await, before);
}

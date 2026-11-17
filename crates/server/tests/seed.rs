//! The demo seed (`server::seed`, task 7.5) against a fresh MongoDB database, checked through the
//! real API: what it creates must be usable like data made by people.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{Account, TestApp};
use mongodb::bson::oid::ObjectId;
use serde_json::{Value, json};
use server::audit::ACTIONS;
use server::db::users;
use server::rbac::Role;
use server::seed::{PEOPLE, SeedError, SeedOptions, SeedReport, seed, slug_of};

const PASSWORD: &str = "demo passphrase 123";

fn options(name: &str) -> SeedOptions {
    SeedOptions {
        workspace_name: name.to_string(),
        password: PASSWORD.to_string(),
    }
}

/// The seeded accounts as signed-in sessions (the test harness signs a session for a user id).
async fn sessions(app: &TestApp, report: &SeedReport) -> Vec<Account> {
    let mut out = Vec::new();
    for person in &report.users {
        let user = users::find_by_email(&app.db, &person.email)
            .await
            .unwrap()
            .unwrap();
        out.push(app.session_for(user.id, &user.name, &user.email));
    }
    out
}

async fn get(app: &TestApp, who: &Account, path: &str) -> Value {
    let (status, body) = app.call("GET", path, Some(who), None).await;
    assert_eq!(status, 200, "{path}: {body}");
    body
}

#[tokio::test]
async fn a_fresh_database_gets_a_workspace_with_three_members_and_their_roles() {
    let app = TestApp::real().await;
    let report = seed(&app.db, &options("Engineering Team")).await.unwrap();
    assert_eq!(report.slug, "engineering-team");
    assert_eq!(report.users.len(), 3);
    assert!(report.users.iter().all(|u| u.created));
    let accounts = sessions(&app, &report).await;

    let list = get(&app, &accounts[0], "/api/v1/workspaces").await;
    let ours = &list[0];
    assert_eq!(ours["name"], "Engineering Team");
    assert_eq!(ours["slug"], "engineering-team");
    assert_eq!(ours["role"], "ADMIN");
    assert_eq!(ours["is_owner"], true);
    assert_eq!(ours["member_count"], 3);
    assert_eq!(ours["document_count"], 3);
    assert_eq!(ours["board_count"], 1);

    let wid = report.workspace_id.to_hex();
    let members = get(
        &app,
        &accounts[0],
        &format!("/api/v1/workspaces/{wid}/members"),
    )
    .await;
    let seen: Vec<(String, String)> = members
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["email"].as_str().unwrap().into(),
                m["role"].as_str().unwrap().into(),
            )
        })
        .collect();
    let want: Vec<(String, String)> = PEOPLE
        .iter()
        .map(|(_, email, role)| ((*email).to_string(), role.as_str().to_string()))
        .collect();
    assert_eq!(seen, want, "the owner first, then in join order");

    // The editor and the viewer see the workspace with their own roles.
    for (account, role) in accounts.iter().zip(["ADMIN", "EDITOR", "VIEWER"]) {
        let list = get(&app, account, "/api/v1/workspaces").await;
        assert_eq!(list[0]["role"], role);
    }
}

#[tokio::test]
async fn the_demo_accounts_can_log_in_with_the_given_password() {
    let app = TestApp::real().await;
    seed(&app.db, &options("Engineering Team")).await.unwrap();
    for (_, email, _) in PEOPLE {
        let (status, body) = app
            .call(
                "POST",
                "/api/v1/auth/login",
                None::<&Account>,
                Some(json!({"email": email, "password": PASSWORD})),
            )
            .await;
        assert_eq!(status, 200, "{email}: {body}");
    }
    let (status, _) = app
        .call(
            "POST",
            "/api/v1/auth/login",
            None::<&Account>,
            Some(json!({"email": PEOPLE[0].1, "password": "not the password"})),
        )
        .await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn it_creates_documents_with_content_and_two_saved_versions_each() {
    let app = TestApp::real().await;
    let report = seed(&app.db, &options("Engineering Team")).await.unwrap();
    let accounts = sessions(&app, &report).await;
    let wid = report.workspace_id.to_hex();
    assert_eq!((report.documents, report.versions), (3, 6));

    let docs = get(
        &app,
        &accounts[2],
        &format!("/api/v1/workspaces/{wid}/documents"),
    )
    .await;
    let titles: Vec<&str> = docs
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["title"].as_str().unwrap())
        .collect();
    for title in [
        "Product Requirements",
        "Sprint 12 Planning",
        "Architecture Overview",
    ] {
        assert!(titles.contains(&title), "{titles:?}");
    }
    for doc in docs.as_array().unwrap() {
        let id = doc["id"].as_str().unwrap();
        let detail = get(&app, &accounts[2], &format!("/api/v1/documents/{id}")).await;
        let blocks = detail["blocks"].as_array().unwrap();
        assert!(
            blocks.len() >= 6,
            "{}: only {} blocks",
            detail["title"],
            blocks.len()
        );
        assert_eq!(blocks[0]["type"], "HEADING_1");
        let orders: Vec<f64> = blocks
            .iter()
            .map(|b| b["order"].as_f64().unwrap())
            .collect();
        assert!(orders.windows(2).all(|w| w[0] < w[1]));

        // Two manual versions, newest first, written by the editor.
        let versions = get(
            &app,
            &accounts[2],
            &format!("/api/v1/documents/{id}/versions"),
        )
        .await;
        let items = versions["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["label"], "Ready for review");
        assert_eq!(items[1]["label"], "Outline");
        assert!(items.iter().all(|v| v["change_type"] == "MANUAL"));
        assert_eq!(items[0]["created_by"]["name"], "Sparsh Pai Bir");

        // The versions are three different stages of the document: outline, review, current.
        let blocks_of = |version: i64| {
            let app = &app;
            let who = &accounts[2];
            let path = format!("/api/v1/documents/{id}/versions/{version}");
            async move {
                get(app, who, &path).await["blocks"]
                    .as_array()
                    .unwrap()
                    .len()
            }
        };
        let (outline, review) = (blocks_of(1).await, blocks_of(2).await);
        assert!(
            outline < review && review < blocks.len(),
            "{}: {outline} < {review} < {} blocks expected",
            detail["title"],
            blocks.len()
        );
    }
}

#[tokio::test]
async fn a_seeded_document_can_be_previewed_and_restored_like_any_other() {
    let app = TestApp::real().await;
    let report = seed(&app.db, &options("Engineering Team")).await.unwrap();
    let accounts = sessions(&app, &report).await;
    let wid = report.workspace_id.to_hex();
    let docs = get(
        &app,
        &accounts[1],
        &format!("/api/v1/workspaces/{wid}/documents"),
    )
    .await;
    let id = docs[0]["id"].as_str().unwrap().to_string();

    // The outline is version 1: a heading and an intro.
    let preview = get(
        &app,
        &accounts[2],
        &format!("/api/v1/documents/{id}/versions/1"),
    )
    .await;
    assert_eq!(preview["blocks"].as_array().unwrap().len(), 2);
    assert_eq!(preview["blocks"][0]["type"], "HEADING_1");

    let (status, restored) = app
        .call(
            "POST",
            &format!("/api/v1/documents/{id}/versions/1/restore"),
            Some(&accounts[1]),
            None,
        )
        .await;
    assert_eq!(status, 200, "{restored}");
    assert_eq!(restored["restore_version"]["change_type"], "RESTORE");
    let after = get(&app, &accounts[1], &format!("/api/v1/documents/{id}")).await;
    assert_eq!(after["blocks"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn the_board_has_tasks_in_every_column_assigned_to_members() {
    let app = TestApp::real().await;
    let report = seed(&app.db, &options("Engineering Team")).await.unwrap();
    let accounts = sessions(&app, &report).await;
    let wid = report.workspace_id.to_hex();
    assert_eq!(report.tasks, 9);

    let boards = get(
        &app,
        &accounts[2],
        &format!("/api/v1/workspaces/{wid}/boards"),
    )
    .await;
    assert_eq!(boards.as_array().unwrap().len(), 1);
    assert_eq!(boards[0]["name"], "Sprint 12");
    let counts = &boards[0]["task_counts"];
    for column in ["backlog", "todo", "in_progress", "review", "done"] {
        assert!(counts[column].as_u64().unwrap() >= 1, "{column}: {counts}");
    }
    assert_eq!(
        ["backlog", "todo", "in_progress", "review", "done"]
            .iter()
            .map(|c| counts[*c].as_u64().unwrap())
            .sum::<u64>(),
        9
    );

    let bid = boards[0]["id"].as_str().unwrap();
    let tasks = get(&app, &accounts[2], &format!("/api/v1/boards/{bid}/tasks")).await;
    let member_ids: Vec<String> = accounts.iter().map(|a| a.id.to_hex()).collect();
    for task in tasks.as_array().unwrap() {
        for assignee in task["assignee_ids"].as_array().unwrap() {
            assert!(member_ids.contains(&assignee.as_str().unwrap().to_string()));
        }
        assert!(task["order"].as_f64().unwrap() >= 1.0);
    }
    // Orders inside one column are strictly increasing, so the board renders in a fixed order.
    let todo: Vec<f64> = tasks
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["status"] == "TODO")
        .map(|t| t["order"].as_f64().unwrap())
        .collect();
    assert!(todo.len() >= 2 && todo.windows(2).all(|w| w[0] < w[1]));
}

#[tokio::test]
async fn the_audit_trail_has_a_row_for_every_seeded_action_and_only_known_actions() {
    let app = TestApp::real().await;
    let report = seed(&app.db, &options("Engineering Team")).await.unwrap();
    let accounts = sessions(&app, &report).await;
    let wid = report.workspace_id.to_hex();

    let rows = app.audit_rows(report.workspace_id).await;
    assert_eq!(rows.len(), report.audit_rows);
    for row in &rows {
        assert!(ACTIONS.contains(&row.get_str("action").unwrap()));
    }
    let count = |action: &str| {
        rows.iter()
            .filter(|r| r.get_str("action").unwrap() == action)
            .count()
    };
    assert_eq!(count("MEMBER_ADD"), 2);
    assert_eq!(count("DOC_CREATE"), 3);
    assert_eq!(count("VERSION_SAVE"), 6);
    assert_eq!(count("TASK_CREATE"), 9);
    assert_eq!(
        count("TASK_MOVE"),
        7,
        "every task that is not in the first column"
    );

    // The admin reads them through the API, newest first.
    let (status, page) = app
        .call(
            "GET",
            &format!("/api/v1/workspaces/{wid}/audit?limit=200"),
            Some(&accounts[0]),
            None,
        )
        .await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["items"].as_array().unwrap().len(), report.audit_rows);
    assert!(
        page["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["actor"]["name"].is_string())
    );
}

#[tokio::test]
async fn running_it_twice_for_the_same_workspace_changes_nothing_and_says_so() {
    let app = TestApp::real().await;
    let first = seed(&app.db, &options("Engineering Team")).await.unwrap();
    let rows_before = app.audit_rows(first.workspace_id).await.len();

    let error = seed(&app.db, &options("Engineering Team"))
        .await
        .unwrap_err();
    assert!(
        matches!(&error, SeedError::WorkspaceExists(slug) if slug == "engineering-team"),
        "{error}"
    );
    assert!(error.to_string().contains("already exists"));
    // The same slug from a different spelling is the same workspace.
    assert!(matches!(
        seed(&app.db, &options("  engineering   TEAM ")).await,
        Err(SeedError::WorkspaceExists(_))
    ));

    assert_eq!(app.audit_rows(first.workspace_id).await.len(), rows_before);
    let workspaces = app
        .db
        .collection::<mongodb::bson::Document>("workspaces")
        .count_documents(mongodb::bson::doc! {})
        .await
        .unwrap();
    assert_eq!(workspaces, 1);
    let accounts = app
        .db
        .collection::<mongodb::bson::Document>("users")
        .count_documents(mongodb::bson::doc! {})
        .await
        .unwrap();
    assert_eq!(accounts, 3);
}

#[tokio::test]
async fn a_second_workspace_reuses_the_existing_accounts_and_keeps_their_passwords() {
    let app = TestApp::real().await;
    seed(&app.db, &options("Engineering Team")).await.unwrap();
    let second = seed(
        &app.db,
        &SeedOptions {
            workspace_name: "Design Team".to_string(),
            password: "a different password".to_string(),
        },
    )
    .await
    .unwrap();
    assert!(
        second.users.iter().all(|u| !u.created),
        "no account was created again"
    );
    assert_ne!(second.workspace_id, ObjectId::new());

    // The first password still works, the new one does not.
    let login = |password: &'static str| {
        let app = &app;
        async move {
            app.call(
                "POST",
                "/api/v1/auth/login",
                None::<&Account>,
                Some(json!({"email": PEOPLE[1].1, "password": password})),
            )
            .await
            .0
        }
    };
    assert_eq!(login(PASSWORD).await, 200);
    assert_eq!(login("a different password").await, 401);
    let accounts = sessions(&app, &second).await;
    let list = get(&app, &accounts[1], "/api/v1/workspaces").await;
    assert_eq!(list.as_array().unwrap().len(), 2, "both workspaces");
}

#[tokio::test]
async fn bad_names_are_refused_before_anything_is_written() {
    let app = TestApp::real().await;
    for (name, expect_invalid_name) in [("", true), ("   ", true), ("???", false), ("é", false)] {
        let error = seed(&app.db, &options(name)).await.unwrap_err();
        if expect_invalid_name {
            assert!(matches!(error, SeedError::InvalidName), "{name:?}: {error}");
        } else {
            assert!(matches!(error, SeedError::InvalidSlug), "{name:?}: {error}");
        }
    }
    let accounts = app
        .db
        .collection::<mongodb::bson::Document>("users")
        .count_documents(mongodb::bson::doc! {})
        .await
        .unwrap();
    assert_eq!(accounts, 0, "no account was made for a refused name");
}

#[test]
fn slugs_follow_the_workspace_rules() {
    for name in ["Engineering Team", "Design Team 2", "a-b-c", "Ünïcode Team"] {
        let slug = slug_of(name);
        assert!(
            server::workspaces::validation::is_valid_slug(&slug) || slug.len() < 3,
            "{name} gave {slug}"
        );
    }
    assert_eq!(Role::Admin.as_str(), "ADMIN");
}

//! Integration tests for the boards and tasks REST API (`docs/API.md` section 2, "Boards and
//! tasks"; `docs/DATA_MODEL.md` section 7), against a real MongoDB.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{Account, TestApp, TestWorkspace};
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime, doc};
use serde_json::{Value, json};
use server::db::tasks::{self as store, TaskRecord};
use server::rbac::Role;

fn boards_path(workspace: &TestWorkspace) -> String {
    format!("/api/v1/workspaces/{}/boards", workspace.hex())
}

fn board_tasks_path(board: &str) -> String {
    format!("/api/v1/boards/{board}/tasks")
}

fn task_path(id: &str) -> String {
    format!("/api/v1/tasks/{id}")
}

fn move_path(id: &str) -> String {
    format!("/api/v1/tasks/{id}/move")
}

fn assert_error(answer: &(u16, Value), status: u16, code: &str) {
    assert_eq!(answer.0, status, "{}", answer.1);
    assert_eq!(answer.1["error"]["code"], code, "{}", answer.1);
}

fn oid(hex: &Value) -> ObjectId {
    ObjectId::parse_str(hex.as_str().unwrap()).unwrap()
}

async fn make_board(app: &TestApp, workspace: &TestWorkspace<'_>, who: &Account) -> String {
    let (status, body) = app
        .call(
            "POST",
            &boards_path(workspace),
            Some(who),
            Some(json!({"name": "Sprint"})),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    body["id"].as_str().unwrap().to_string()
}

async fn make_task(app: &TestApp, board: &str, who: &Account, title: &str, status: &str) -> Value {
    let (code, body) = app
        .call(
            "POST",
            &board_tasks_path(board),
            Some(who),
            Some(json!({"title": title, "status": status})),
        )
        .await;
    assert_eq!(code, 201, "{body}");
    body
}

async fn tasks_of(app: &TestApp, board: &str, who: &Account) -> Vec<Value> {
    let (status, body) = app
        .call("GET", &board_tasks_path(board), Some(who), None)
        .await;
    assert_eq!(status, 200, "{body}");
    body.as_array().unwrap().clone()
}

fn titles(tasks: &[Value]) -> Vec<&str> {
    tasks.iter().map(|t| t["title"].as_str().unwrap()).collect()
}

async fn actions(app: &TestApp, workspace: &TestWorkspace<'_>) -> Vec<String> {
    app.audit_rows(workspace.id)
        .await
        .iter()
        .map(|row| row.get_str("action").unwrap().to_string())
        .collect()
}

async fn editor_workspace(app: &TestApp) -> TestWorkspace<'_> {
    TestWorkspace::new(app)
        .await
        .with_member(Role::Editor)
        .await
        .with_member(Role::Viewer)
        .await
}

// ----- boards -----

#[tokio::test]
async fn an_editor_creates_a_board_and_it_is_listed_with_zero_counts() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();

    let (status, body) = app
        .call(
            "POST",
            &boards_path(&workspace),
            Some(&editor),
            Some(json!({"name": "  Launch plan  "})),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["name"], "Launch plan");
    assert_eq!(body["workspace_id"], workspace.hex());
    assert_eq!(
        body["task_counts"],
        json!({"backlog": 0, "todo": 0, "in_progress": 0, "review": 0, "done": 0})
    );
    assert!(body["created_at"].is_string());

    let (status, list) = app
        .call("GET", &boards_path(&workspace), Some(&editor), None)
        .await;
    assert_eq!(status, 200);
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], body["id"]);
    // Boards are not in the audit table.
    assert!(actions(&app, &workspace).await.is_empty());
}

#[tokio::test]
async fn a_viewer_cannot_create_a_board_and_an_outsider_cannot_see_the_workspace() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let viewer = workspace.member(Role::Viewer).clone();
    let outsider = app.account("Outsider").await;

    let refused = app
        .call(
            "POST",
            &boards_path(&workspace),
            Some(&viewer),
            Some(json!({"name": "Nope"})),
        )
        .await;
    assert_error(&refused, 403, "FORBIDDEN");
    let (_, list) = app
        .call("GET", &boards_path(&workspace), Some(&viewer), None)
        .await;
    assert!(list.as_array().unwrap().is_empty(), "nothing was created");

    let hidden = app
        .call("GET", &boards_path(&workspace), Some(&outsider), None)
        .await;
    assert_error(&hidden, 404, "NOT_FOUND");
}

#[tokio::test]
async fn board_names_must_be_one_to_sixty_characters() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    for name in ["", "   ", &"x".repeat(61), "line\nbreak"] {
        let answer = app
            .call(
                "POST",
                &boards_path(&workspace),
                Some(&editor),
                Some(json!({"name": name})),
            )
            .await;
        assert_error(&answer, 422, "VALIDATION");
        assert!(answer.1["error"]["details"]["name"].is_string());
    }
    let ok = app
        .call(
            "POST",
            &boards_path(&workspace),
            Some(&editor),
            Some(json!({"name": "x".repeat(60)})),
        )
        .await;
    assert_eq!(ok.0, 201);
}

#[tokio::test]
async fn the_board_list_counts_tasks_per_column_and_only_for_its_own_boards() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let first = make_board(&app, &workspace, &editor).await;
    let second = make_board(&app, &workspace, &editor).await;
    for (title, status) in [
        ("a", "TODO"),
        ("b", "TODO"),
        ("c", "DONE"),
        ("d", "IN_PROGRESS"),
    ] {
        make_task(&app, &first, &editor, title, status).await;
    }
    make_task(&app, &second, &editor, "other", "REVIEW").await;

    let (_, list) = app
        .call("GET", &boards_path(&workspace), Some(&editor), None)
        .await;
    let counts = |id: &str| {
        list.as_array()
            .unwrap()
            .iter()
            .find(|b| b["id"] == id)
            .unwrap()["task_counts"]
            .clone()
    };
    assert_eq!(
        counts(&first),
        json!({"backlog": 0, "todo": 2, "in_progress": 1, "review": 0, "done": 1})
    );
    assert_eq!(
        counts(&second),
        json!({"backlog": 0, "todo": 0, "in_progress": 0, "review": 1, "done": 0})
    );

    // Another workspace's boards never show up here.
    let other = TestWorkspace::new(&app).await;
    make_board(&app, &other, &other.owner.clone()).await;
    let (_, list_again) = app
        .call("GET", &boards_path(&workspace), Some(&editor), None)
        .await;
    assert_eq!(list_again.as_array().unwrap().len(), 2);
}

// ----- creating tasks -----

#[tokio::test]
async fn creating_a_task_applies_defaults_places_it_last_and_writes_one_audit_row() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let board = make_board(&app, &workspace, &editor).await;

    let (status, first) = app
        .call(
            "POST",
            &board_tasks_path(&board),
            Some(&editor),
            Some(json!({"title": "  Write the plan  "})),
        )
        .await;
    assert_eq!(status, 201, "{first}");
    assert_eq!(first["title"], "Write the plan");
    assert_eq!(first["description"], "");
    assert_eq!(first["status"], "BACKLOG");
    assert_eq!(first["priority"], "MEDIUM");
    assert_eq!(first["assignee_ids"], json!([]));
    assert_eq!(first["tags"], json!([]));
    assert_eq!(first["order"], 1.0);
    assert_eq!(first["board_id"], board);

    // The end of the same column, and the start of another.
    let second = make_task(&app, &board, &editor, "Second", "BACKLOG").await;
    assert_eq!(second["order"], 2.0);
    let elsewhere = make_task(&app, &board, &editor, "Elsewhere", "REVIEW").await;
    assert_eq!(elsewhere["order"], 1.0);

    let rows = app.audit_rows(workspace.id).await;
    assert_eq!(rows.len(), 3);
    let row = &rows[0];
    assert_eq!(row.get_str("action").unwrap(), "TASK_CREATE");
    assert_eq!(row.get_str("entity_type").unwrap(), "TASK");
    assert_eq!(
        row.get_str("entity_id").unwrap(),
        first["id"].as_str().unwrap()
    );
    assert_eq!(row.get_object_id("actor_id").unwrap(), editor.id);
    let meta = row.get_document("metadata").unwrap();
    assert_eq!(meta.get_str("title").unwrap(), "Write the plan");
    assert_eq!(meta.get_str("status").unwrap(), "BACKLOG");
    assert_eq!(meta.get_str("priority").unwrap(), "MEDIUM");
}

#[tokio::test]
async fn a_task_can_be_created_with_every_field() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let viewer = workspace.member(Role::Viewer).clone();
    let board = make_board(&app, &workspace, &editor).await;

    let (status, task) = app
        .call(
            "POST",
            &board_tasks_path(&board),
            Some(&editor),
            Some(json!({
                "title": "Everything",
                "description": "Line one\nLine two",
                "status": "IN_PROGRESS",
                "priority": "URGENT",
                "assignee_ids": [editor.id.to_hex(), viewer.id.to_hex(), editor.id.to_hex()],
                "tags": ["Backend", "api", "backend"]
            })),
        )
        .await;
    assert_eq!(status, 201, "{task}");
    assert_eq!(task["description"], "Line one\nLine two");
    assert_eq!(task["status"], "IN_PROGRESS");
    assert_eq!(task["priority"], "URGENT");
    assert_eq!(
        task["assignee_ids"],
        json!([editor.id.to_hex(), viewer.id.to_hex()])
    );
    assert_eq!(task["tags"], json!(["backend", "api"]));
}

#[tokio::test]
async fn a_viewer_cannot_create_a_task_and_nothing_is_stored_or_audited() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let viewer = workspace.member(Role::Viewer).clone();
    let board = make_board(&app, &workspace, &editor).await;

    let refused = app
        .call(
            "POST",
            &board_tasks_path(&board),
            Some(&viewer),
            Some(json!({"title": "Sneaky"})),
        )
        .await;
    assert_error(&refused, 403, "FORBIDDEN");
    assert!(tasks_of(&app, &board, &editor).await.is_empty());
    assert!(actions(&app, &workspace).await.is_empty());
}

#[tokio::test]
async fn bad_task_input_is_reported_field_by_field_and_stores_nothing() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let board = make_board(&app, &workspace, &editor).await;
    let stranger = app.account("Stranger").await;

    let answer = app
        .call(
            "POST",
            &board_tasks_path(&board),
            Some(&editor),
            Some(json!({
                "title": "",
                "description": "x".repeat(5001),
                "assignee_ids": [stranger.id.to_hex()],
                "tags": ["", "ok"]
            })),
        )
        .await;
    assert_error(&answer, 422, "VALIDATION");
    let details = &answer.1["error"]["details"];
    for field in ["title", "description", "assignee_ids", "tags"] {
        assert!(details[field].is_string(), "{field} missing: {details}");
    }

    // Wrong enum values never reach the handler's own checks.
    for body in [
        json!({"title": "x", "status": "SOMEDAY"}),
        json!({"title": "x", "priority": "WHENEVER"}),
        json!({"title": "x", "assignee_ids": ["not an id"]}),
        json!({"title": "x", "tags": (0..11).map(|n| format!("t{n}")).collect::<Vec<_>>()}),
        json!({"title": "y".repeat(201)}),
    ] {
        let answer = app
            .call(
                "POST",
                &board_tasks_path(&board),
                Some(&editor),
                Some(body.clone()),
            )
            .await;
        assert_eq!(answer.0, 422, "{body} gave {}", answer.1);
    }
    assert!(tasks_of(&app, &board, &editor).await.is_empty());
    assert!(actions(&app, &workspace).await.is_empty());
}

// ----- listing -----

#[tokio::test]
async fn tasks_come_back_by_column_then_order_and_a_viewer_may_read_them() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let viewer = workspace.member(Role::Viewer).clone();
    let board = make_board(&app, &workspace, &editor).await;
    for (title, status) in [
        ("done-1", "DONE"),
        ("todo-1", "TODO"),
        ("backlog-1", "BACKLOG"),
        ("todo-2", "TODO"),
        ("review-1", "REVIEW"),
        ("progress-1", "IN_PROGRESS"),
    ] {
        make_task(&app, &board, &editor, title, status).await;
    }
    let tasks = tasks_of(&app, &board, &viewer).await;
    assert_eq!(
        titles(&tasks),
        [
            "backlog-1",
            "todo-1",
            "todo-2",
            "progress-1",
            "review-1",
            "done-1"
        ]
    );
}

#[tokio::test]
async fn tasks_can_be_filtered_by_assignee_and_priority() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let viewer = workspace.member(Role::Viewer).clone();
    let board = make_board(&app, &workspace, &editor).await;
    for (title, priority, assignee) in [
        ("urgent-mine", "URGENT", &editor),
        ("low-mine", "LOW", &editor),
        ("urgent-theirs", "URGENT", &viewer),
    ] {
        let (status, body) = app
            .call(
                "POST",
                &board_tasks_path(&board),
                Some(&editor),
                Some(json!({
                    "title": title,
                    "priority": priority,
                    "assignee_ids": [assignee.id.to_hex()]
                })),
            )
            .await;
        assert_eq!(status, 201, "{body}");
    }
    let get = |query: String| {
        let app = &app;
        let board = board.clone();
        let editor = editor.clone();
        async move {
            app.call(
                "GET",
                &format!("{}?{query}", board_tasks_path(&board)),
                Some(&editor),
                None,
            )
            .await
        }
    };
    let (_, mine) = get(format!("assignee={}", editor.id.to_hex())).await;
    assert_eq!(
        titles(mine.as_array().unwrap()),
        ["urgent-mine", "low-mine"]
    );
    let (_, urgent) = get("priority=URGENT".to_string()).await;
    assert_eq!(
        titles(urgent.as_array().unwrap()),
        ["urgent-mine", "urgent-theirs"]
    );
    let (_, both) = get(format!("assignee={}&priority=URGENT", viewer.id.to_hex())).await;
    assert_eq!(titles(both.as_array().unwrap()), ["urgent-theirs"]);
    let (_, empty_filters) = get("assignee=&priority=".to_string()).await;
    assert_eq!(empty_filters.as_array().unwrap().len(), 3);

    assert_error(&get("priority=SOON".to_string()).await, 422, "VALIDATION");
    assert_error(&get("assignee=nobody".to_string()).await, 422, "VALIDATION");
}

// ----- editing -----

#[tokio::test]
async fn patching_changes_only_the_given_fields_and_is_not_audited() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let board = make_board(&app, &workspace, &editor).await;
    let task = make_task(&app, &board, &editor, "Original", "TODO").await;
    let id = task["id"].as_str().unwrap();
    let audit_before = actions(&app, &workspace).await.len();

    let (status, patched) = app
        .call(
            "PATCH",
            &task_path(id),
            Some(&editor),
            Some(json!({
                "title": " Renamed ",
                "priority": "HIGH",
                "tags": ["Ship"],
                "assignee_ids": [editor.id.to_hex()],
                "description": "Now with words"
            })),
        )
        .await;
    assert_eq!(status, 200, "{patched}");
    assert_eq!(patched["title"], "Renamed");
    assert_eq!(patched["priority"], "HIGH");
    assert_eq!(patched["tags"], json!(["ship"]));
    assert_eq!(patched["assignee_ids"], json!([editor.id.to_hex()]));
    assert_eq!(patched["description"], "Now with words");
    // The column and the position are only changed by a move.
    assert_eq!(patched["status"], "TODO");
    assert_eq!(patched["order"], task["order"]);
    assert_eq!(patched["created_at"], task["created_at"]);

    let (_, only_title) = app
        .call(
            "PATCH",
            &task_path(id),
            Some(&editor),
            Some(json!({"title": "Third name"})),
        )
        .await;
    assert_eq!(only_title["title"], "Third name");
    assert_eq!(only_title["priority"], "HIGH", "unsent fields are kept");
    assert_eq!(actions(&app, &workspace).await.len(), audit_before);
}

#[tokio::test]
async fn patching_refuses_bad_input_a_viewer_and_a_task_that_is_not_there() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let viewer = workspace.member(Role::Viewer).clone();
    let stranger = app.account("Stranger").await;
    let board = make_board(&app, &workspace, &editor).await;
    let task = make_task(&app, &board, &editor, "Keep me", "TODO").await;
    let id = task["id"].as_str().unwrap();

    let empty = app
        .call("PATCH", &task_path(id), Some(&editor), Some(json!({})))
        .await;
    assert_error(&empty, 422, "VALIDATION");
    for body in [
        json!({"title": ""}),
        json!({"assignee_ids": [stranger.id.to_hex()]}),
        json!({"tags": [""]}),
        json!({"priority": "SOON"}),
    ] {
        let answer = app
            .call("PATCH", &task_path(id), Some(&editor), Some(body.clone()))
            .await;
        assert_eq!(answer.0, 422, "{body} gave {}", answer.1);
    }
    let by_viewer = app
        .call(
            "PATCH",
            &task_path(id),
            Some(&viewer),
            Some(json!({"title": "Hijacked"})),
        )
        .await;
    assert_error(&by_viewer, 403, "FORBIDDEN");
    let missing = app
        .call(
            "PATCH",
            &task_path(&ObjectId::new().to_hex()),
            Some(&editor),
            Some(json!({"title": "x"})),
        )
        .await;
    assert_error(&missing, 404, "NOT_FOUND");

    let tasks = tasks_of(&app, &board, &editor).await;
    assert_eq!(titles(&tasks), ["Keep me"]);
}

// ----- moving -----

async fn column_titles(app: &TestApp, board: &str, who: &Account, status: &str) -> Vec<String> {
    tasks_of(app, board, who)
        .await
        .iter()
        .filter(|t| t["status"] == status)
        .map(|t| t["title"].as_str().unwrap().to_string())
        .collect()
}

async fn move_to(app: &TestApp, who: &Account, id: &str, body: Value) -> (u16, Value) {
    app.call("PATCH", &move_path(id), Some(who), Some(body))
        .await
}

#[tokio::test]
async fn moving_to_another_column_without_neighbours_puts_the_task_last() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let board = make_board(&app, &workspace, &editor).await;
    let task = make_task(&app, &board, &editor, "Mover", "BACKLOG").await;
    make_task(&app, &board, &editor, "Stays-1", "TODO").await;
    make_task(&app, &board, &editor, "Stays-2", "TODO").await;
    let id = task["id"].as_str().unwrap();

    let (status, moved) = move_to(&app, &editor, id, json!({"to_status": "TODO"})).await;
    assert_eq!(status, 200, "{moved}");
    assert_eq!(moved["status"], "TODO");
    assert_eq!(moved["order"], 3.0, "the last plus 1.0");
    assert_eq!(
        column_titles(&app, &board, &editor, "TODO").await,
        ["Stays-1", "Stays-2", "Mover"]
    );
    assert!(
        column_titles(&app, &board, &editor, "BACKLOG")
            .await
            .is_empty()
    );

    // Into an empty column: 1.0.
    let (_, to_done) = move_to(&app, &editor, id, json!({"to_status": "DONE"})).await;
    assert_eq!(to_done["order"], 1.0);
}

#[tokio::test]
async fn moving_between_two_tasks_takes_the_midpoint_and_the_head_takes_half() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let board = make_board(&app, &workspace, &editor).await;
    let a = make_task(&app, &board, &editor, "A", "TODO").await;
    let b = make_task(&app, &board, &editor, "B", "TODO").await;
    let c = make_task(&app, &board, &editor, "C", "TODO").await;
    let mover = make_task(&app, &board, &editor, "Mover", "REVIEW").await;
    let mover_id = mover["id"].as_str().unwrap();
    let (a_id, b_id, c_id) = (
        a["id"].as_str().unwrap(),
        b["id"].as_str().unwrap(),
        c["id"].as_str().unwrap(),
    );

    // Between A (1.0) and B (2.0).
    let (_, between) = move_to(
        &app,
        &editor,
        mover_id,
        json!({"to_status": "TODO", "before_id": a_id, "after_id": b_id}),
    )
    .await;
    assert_eq!(between["order"], 1.5);
    assert_eq!(
        column_titles(&app, &board, &editor, "TODO").await,
        ["A", "Mover", "B", "C"]
    );

    // To the head: half of the first.
    let (_, head) = move_to(
        &app,
        &editor,
        mover_id,
        json!({"to_status": "TODO", "after_id": a_id}),
    )
    .await;
    assert_eq!(head["order"], 0.5);
    assert_eq!(
        column_titles(&app, &board, &editor, "TODO").await,
        ["Mover", "A", "B", "C"]
    );

    // Only `before_id`: just after it, before whatever follows.
    let (_, after_b) = move_to(
        &app,
        &editor,
        mover_id,
        json!({"to_status": "TODO", "before_id": b_id}),
    )
    .await;
    assert_eq!(after_b["order"], 2.5);
    // Only `before_id` of the last task: the tail.
    let (_, tail) = move_to(
        &app,
        &editor,
        mover_id,
        json!({"to_status": "TODO", "before_id": c_id}),
    )
    .await;
    assert_eq!(tail["order"], 4.0);
    assert_eq!(
        column_titles(&app, &board, &editor, "TODO").await,
        ["A", "B", "C", "Mover"]
    );
}

#[tokio::test]
async fn a_task_can_be_reordered_inside_its_own_column() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let board = make_board(&app, &workspace, &editor).await;
    let one = make_task(&app, &board, &editor, "One", "TODO").await;
    make_task(&app, &board, &editor, "Two", "TODO").await;
    let three = make_task(&app, &board, &editor, "Three", "TODO").await;

    // "One" goes to the end of its own column; it is not its own neighbour.
    let (status, moved) = move_to(
        &app,
        &editor,
        one["id"].as_str().unwrap(),
        json!({"to_status": "TODO", "before_id": three["id"]}),
    )
    .await;
    assert_eq!(status, 200, "{moved}");
    assert_eq!(
        column_titles(&app, &board, &editor, "TODO").await,
        ["Two", "Three", "One"]
    );

    // The last task "moved" to the end of its own column stays where it is: the end is measured
    // without the task itself, so it does not climb by 1.0 every time.
    let (_, again) = move_to(
        &app,
        &editor,
        one["id"].as_str().unwrap(),
        json!({"to_status": "TODO"}),
    )
    .await;
    assert_eq!(again["order"], moved["order"]);
    let (_, thrice) = move_to(
        &app,
        &editor,
        one["id"].as_str().unwrap(),
        json!({"to_status": "TODO"}),
    )
    .await;
    assert_eq!(thrice["order"], moved["order"]);

    // A task is never its own neighbour, in its own column either.
    for field in ["before_id", "after_id"] {
        let answer = move_to(
            &app,
            &editor,
            one["id"].as_str().unwrap(),
            json!({"to_status": "TODO", field: one["id"]}),
        )
        .await;
        assert_error(&answer, 422, "VALIDATION");
        assert!(answer.1["error"]["details"][field].is_string());
    }
    assert_eq!(
        column_titles(&app, &board, &editor, "TODO").await,
        ["Two", "Three", "One"]
    );
}

#[tokio::test]
async fn moving_writes_one_audit_row_with_both_columns_and_the_computed_order() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let board = make_board(&app, &workspace, &editor).await;
    let task = make_task(&app, &board, &editor, "Audited", "TODO").await;
    let before = app.audit_rows(workspace.id).await.len();

    let (_, moved) = move_to(
        &app,
        &editor,
        task["id"].as_str().unwrap(),
        json!({"to_status": "DONE"}),
    )
    .await;
    let rows = app.audit_rows(workspace.id).await;
    assert_eq!(rows.len(), before + 1);
    let row = rows.last().unwrap();
    assert_eq!(row.get_str("action").unwrap(), "TASK_MOVE");
    assert_eq!(row.get_str("entity_type").unwrap(), "TASK");
    assert_eq!(
        row.get_str("entity_id").unwrap(),
        task["id"].as_str().unwrap()
    );
    let meta = row.get_document("metadata").unwrap();
    assert_eq!(meta.get_str("from_column").unwrap(), "TODO");
    assert_eq!(meta.get_str("to_column").unwrap(), "DONE");
    assert_eq!(
        meta.get_f64("calculated_order").unwrap(),
        moved["order"].as_f64().unwrap()
    );
}

#[tokio::test]
async fn neighbours_must_be_other_tasks_of_the_target_column_in_the_right_order() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let board = make_board(&app, &workspace, &editor).await;
    let a = make_task(&app, &board, &editor, "A", "TODO").await;
    let b = make_task(&app, &board, &editor, "B", "TODO").await;
    let elsewhere = make_task(&app, &board, &editor, "Elsewhere", "DONE").await;
    let mover = make_task(&app, &board, &editor, "Mover", "REVIEW").await;
    let id = mover["id"].as_str().unwrap();
    let other_workspace = TestWorkspace::new(&app).await;
    let other_board = make_board(&app, &other_workspace, &other_workspace.owner.clone()).await;
    let foreign = make_task(
        &app,
        &other_board,
        &other_workspace.owner.clone(),
        "Foreign",
        "TODO",
    )
    .await;

    for (body, field) in [
        // Not in the target column.
        (
            json!({"to_status": "TODO", "before_id": elsewhere["id"]}),
            "before_id",
        ),
        (
            json!({"to_status": "TODO", "after_id": elsewhere["id"]}),
            "after_id",
        ),
        // The task itself.
        (
            json!({"to_status": "TODO", "before_id": mover["id"]}),
            "before_id",
        ),
        // Unknown, malformed and from another workspace.
        (
            json!({"to_status": "TODO", "before_id": ObjectId::new().to_hex()}),
            "before_id",
        ),
        (
            json!({"to_status": "TODO", "after_id": "garbage"}),
            "after_id",
        ),
        (
            json!({"to_status": "TODO", "before_id": foreign["id"]}),
            "before_id",
        ),
        // The wrong way round.
        (
            json!({"to_status": "TODO", "before_id": b["id"], "after_id": a["id"]}),
            "before_id",
        ),
        (
            json!({"to_status": "TODO", "before_id": a["id"], "after_id": a["id"]}),
            "before_id",
        ),
    ] {
        let answer = move_to(&app, &editor, id, body.clone()).await;
        assert_error(&answer, 422, "VALIDATION");
        assert!(
            answer.1["error"]["details"][field].is_string(),
            "{body}: {}",
            answer.1
        );
    }
    // Nothing moved and nothing was audited for the refused attempts.
    let after = tasks_of(&app, &board, &editor).await;
    let mover_now = after.iter().find(|t| t["id"] == mover["id"]).unwrap();
    assert_eq!(mover_now["status"], "REVIEW");
    assert_eq!(
        actions(&app, &workspace)
            .await
            .iter()
            .filter(|a| *a == "TASK_MOVE")
            .count(),
        0
    );

    // A body with a status that does not exist.
    let bad = move_to(&app, &editor, id, json!({"to_status": "SOMEDAY"})).await;
    assert_eq!(bad.0, 422);
}

#[tokio::test]
async fn a_viewer_cannot_move_a_task() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let viewer = workspace.member(Role::Viewer).clone();
    let board = make_board(&app, &workspace, &editor).await;
    let task = make_task(&app, &board, &editor, "Fixed", "TODO").await;
    let refused = move_to(
        &app,
        &viewer,
        task["id"].as_str().unwrap(),
        json!({"to_status": "DONE"}),
    )
    .await;
    assert_error(&refused, 403, "FORBIDDEN");
    let now = tasks_of(&app, &board, &viewer).await;
    assert_eq!(now[0]["status"], "TODO");
    assert!(
        !actions(&app, &workspace)
            .await
            .contains(&"TASK_MOVE".to_string())
    );
}

#[tokio::test]
async fn a_column_with_no_room_left_is_renumbered_before_the_task_goes_in() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let board = make_board(&app, &workspace, &editor).await;
    let board_id = ObjectId::parse_str(&board).unwrap();
    // Three tasks whose first two orders are closer than 1e-9.
    let mut ids = Vec::new();
    for (title, order) in [("A", 1.0), ("B", 1.0 + 5e-10), ("C", 7.0)] {
        let record = TaskRecord {
            id: ObjectId::new(),
            workspace_id: workspace.id,
            board_id,
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
        store::insert(&app.db, &record).await.unwrap();
        ids.push(record.id.to_hex());
    }
    let mover = make_task(&app, &board, &editor, "Mover", "REVIEW").await;

    let (status, moved) = move_to(
        &app,
        &editor,
        mover["id"].as_str().unwrap(),
        json!({"to_status": "TODO", "before_id": ids[0], "after_id": ids[1]}),
    )
    .await;
    assert_eq!(status, 200, "{moved}");
    // The column became 1.0, 2.0, 3.0 and the task went between the first two.
    assert_eq!(moved["order"], 1.5);
    let tasks = tasks_of(&app, &board, &editor).await;
    let orders: Vec<(String, f64)> = tasks
        .iter()
        .filter(|t| t["status"] == "TODO")
        .map(|t| {
            (
                t["title"].as_str().unwrap().to_string(),
                t["order"].as_f64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        orders,
        [
            ("A".to_string(), 1.0),
            ("Mover".to_string(), 1.5),
            ("B".to_string(), 2.0),
            ("C".to_string(), 3.0)
        ]
    );
}

#[tokio::test]
async fn repeated_moves_to_the_head_keep_the_order_correct_across_a_renumber() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let board = make_board(&app, &workspace, &editor).await;
    let first = make_task(&app, &board, &editor, "First", "TODO").await;
    let mut expected = vec!["First".to_string()];
    // Each new task goes to the head: the first order halves every time, past 1e-9.
    for n in 0..34 {
        let title = format!("T{n}");
        let task = make_task(&app, &board, &editor, &title, "BACKLOG").await;
        let head = tasks_of(&app, &board, &editor)
            .await
            .iter()
            .find(|t| t["status"] == "TODO" && t["title"] != title)
            .map(|t| t["id"].clone())
            .unwrap();
        let (status, moved) = move_to(
            &app,
            &editor,
            task["id"].as_str().unwrap(),
            json!({"to_status": "TODO", "after_id": head}),
        )
        .await;
        assert_eq!(status, 200, "{moved}");
        expected.insert(0, title);
    }
    assert_eq!(column_titles(&app, &board, &editor, "TODO").await, expected);
    // The first task is still last.
    assert_eq!(expected.last().unwrap(), first["title"].as_str().unwrap());
}

// ----- deleting -----

#[tokio::test]
async fn deleting_removes_the_task_and_writes_one_audit_row_with_its_title() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let board = make_board(&app, &workspace, &editor).await;
    let task = make_task(&app, &board, &editor, "Short lived", "TODO").await;
    let id = task["id"].as_str().unwrap();

    let (status, _) = app
        .call("DELETE", &task_path(id), Some(&editor), None)
        .await;
    assert_eq!(status, 204);
    assert!(tasks_of(&app, &board, &editor).await.is_empty());
    let rows = app.audit_rows(workspace.id).await;
    let row = rows.last().unwrap();
    assert_eq!(row.get_str("action").unwrap(), "TASK_DELETE");
    assert_eq!(row.get_str("entity_id").unwrap(), id);
    assert_eq!(
        row.get_document("metadata")
            .unwrap()
            .get_str("title")
            .unwrap(),
        "Short lived"
    );
    let again = app
        .call("DELETE", &task_path(id), Some(&editor), None)
        .await;
    assert_error(&again, 404, "NOT_FOUND");
    assert_eq!(
        actions(&app, &workspace)
            .await
            .iter()
            .filter(|a| *a == "TASK_DELETE")
            .count(),
        1,
        "a refused second delete writes no row"
    );
}

#[tokio::test]
async fn a_viewer_cannot_delete_a_task() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let viewer = workspace.member(Role::Viewer).clone();
    let board = make_board(&app, &workspace, &editor).await;
    let task = make_task(&app, &board, &editor, "Safe", "TODO").await;
    let refused = app
        .call(
            "DELETE",
            &task_path(task["id"].as_str().unwrap()),
            Some(&viewer),
            None,
        )
        .await;
    assert_error(&refused, 403, "FORBIDDEN");
    assert_eq!(tasks_of(&app, &board, &editor).await.len(), 1);
}

// ----- isolation -----

#[tokio::test]
async fn another_workspaces_board_and_task_look_exactly_like_missing_ones() {
    let app = TestApp::real().await;
    let mine = editor_workspace(&app).await;
    let editor = mine.member(Role::Editor).clone();
    let theirs = TestWorkspace::new(&app).await;
    let their_owner = theirs.owner.clone();
    let board = make_board(&app, &theirs, &their_owner).await;
    let task = make_task(&app, &board, &their_owner, "Private", "TODO").await;
    let task_id = task["id"].as_str().unwrap();
    let missing_board = ObjectId::new().to_hex();
    let missing_task = ObjectId::new().to_hex();

    let calls = |board: String, task: String| {
        vec![
            ("GET", board_tasks_path(&board), None),
            (
                "POST",
                board_tasks_path(&board),
                Some(json!({"title": "Intruder"})),
            ),
            (
                "PATCH",
                task_path(&task),
                Some(json!({"title": "Intruder"})),
            ),
            (
                "PATCH",
                move_path(&task),
                Some(json!({"to_status": "DONE"})),
            ),
            ("DELETE", task_path(&task), None),
        ]
    };
    let real = calls(board.clone(), task_id.to_string());
    let fake = calls(missing_board, missing_task);
    for ((method, path, body), (_, fake_path, fake_body)) in real.into_iter().zip(fake) {
        let to_foreign = app.call(method, &path, Some(&editor), body).await;
        let to_missing = app.call(method, &fake_path, Some(&editor), fake_body).await;
        assert_eq!(to_foreign.0, 404, "{method} {path}: {}", to_foreign.1);
        assert_eq!(
            to_foreign.1["error"], to_missing.1["error"],
            "{method} {path} reveals that the id exists"
        );
    }
    // And nothing of theirs changed.
    let theirs_now = tasks_of(&app, &board, &their_owner).await;
    assert_eq!(titles(&theirs_now), ["Private"]);
    assert_eq!(theirs_now[0]["status"], "TODO");
}

#[tokio::test]
async fn a_removed_member_leaves_the_tasks_they_were_assigned_to() {
    let app = TestApp::real().await;
    let workspace = editor_workspace(&app).await;
    let editor = workspace.member(Role::Editor).clone();
    let viewer = workspace.member(Role::Viewer).clone();
    let board = make_board(&app, &workspace, &editor).await;
    let (_, task) = app
        .call(
            "POST",
            &board_tasks_path(&board),
            Some(&editor),
            Some(json!({"title": "Shared", "assignee_ids": [editor.id.to_hex(), viewer.id.to_hex()]})),
        )
        .await;
    let removed = app
        .call(
            "DELETE",
            &format!(
                "/api/v1/workspaces/{}/members/{}",
                workspace.hex(),
                viewer.id.to_hex()
            ),
            Some(&workspace.owner),
            None,
        )
        .await;
    assert_eq!(removed.0, 204, "{}", removed.1);
    let stored = store::find(&app.db, oid(&task["id"]))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.assignee_ids, vec![editor.id]);
    let raw = app
        .db
        .collection::<mongodb::bson::Document>("tasks")
        .count_documents(doc! { "assignee_ids": viewer.id })
        .await
        .unwrap();
    assert_eq!(raw, 0);
}

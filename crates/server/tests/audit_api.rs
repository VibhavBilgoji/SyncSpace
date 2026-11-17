//! Integration tests for `GET /workspaces/:wid/audit` (`docs/API.md` section 2, "Audit"),
//! against a real MongoDB.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{Account, TestApp, TestWorkspace};
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime, Document};
use serde_json::{Value, json};
use server::audit::{self, AuditEvent};
use server::rbac::Role;

fn audit_path(workspace: &TestWorkspace, query: &str) -> String {
    let suffix = if query.is_empty() {
        String::new()
    } else {
        format!("?{query}")
    };
    format!("/api/v1/workspaces/{}/audit{suffix}", workspace.hex())
}

fn assert_error(answer: &(u16, Value), status: u16, code: &str) {
    assert_eq!(answer.0, status, "{}", answer.1);
    assert_eq!(answer.1["error"]["code"], code, "{}", answer.1);
}

/// Inserts a row for `event` stamped at `millis`, the way `audit::record` would, but at a chosen
/// time, so ordering and paging can be tested without waiting.
async fn insert_at(
    app: &TestApp,
    workspace: ObjectId,
    actor: ObjectId,
    event: &AuditEvent,
    millis: i64,
) -> String {
    let mut row = event.to_row(workspace, actor);
    row.insert("timestamp", DateTime::from_millis(millis));
    let id = row.get_object_id("_id").unwrap().to_hex();
    app.db
        .collection::<Document>(audit::COLLECTION)
        .insert_one(row)
        .await
        .unwrap();
    id
}

fn doc_event(title: &str) -> AuditEvent {
    AuditEvent::DocCreate {
        document_id: ObjectId::new(),
        title: title.to_string(),
    }
}

fn ids(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["event_id"].as_str().unwrap().to_string())
        .collect()
}

async fn admin_workspace(app: &TestApp) -> TestWorkspace<'_> {
    TestWorkspace::new(app)
        .await
        .with_member(Role::Editor)
        .await
        .with_member(Role::Viewer)
        .await
}

#[tokio::test]
async fn an_admin_reads_the_log_newest_first_with_the_actor_names() {
    let app = TestApp::real().await;
    let workspace = admin_workspace(&app).await;
    let owner = workspace.owner.clone();
    let editor = workspace.member(Role::Editor).clone();
    let first = insert_at(&app, workspace.id, owner.id, &doc_event("Plan"), 1_000).await;
    let second = insert_at(
        &app,
        workspace.id,
        editor.id,
        &AuditEvent::TaskMove {
            task_id: ObjectId::new(),
            from_column: "TODO".into(),
            to_column: "DONE".into(),
            order: 2.5,
        },
        2_000,
    )
    .await;

    let (status, page) = app
        .call("GET", &audit_path(&workspace, ""), Some(&owner), None)
        .await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(ids(&page), [second.clone(), first.clone()]);
    assert_eq!(page["next_cursor"], Value::Null);

    let newest = &page["items"][0];
    assert_eq!(
        newest["actor"],
        json!({"id": editor.id.to_hex(), "name": editor.name})
    );
    assert_eq!(newest["action"], "TASK_MOVE");
    assert_eq!(newest["entity_type"], "TASK");
    assert!(newest["entity_id"].is_string());
    assert_eq!(
        newest["metadata"],
        json!({"from_column": "TODO", "to_column": "DONE", "calculated_order": 2.5})
    );
    assert_eq!(newest["timestamp"], "1970-01-01T00:00:02Z");
    let oldest = &page["items"][1];
    assert_eq!(oldest["actor"]["name"], owner.name);
    assert_eq!(oldest["metadata"], json!({"title": "Plan"}));
}

#[tokio::test]
async fn the_log_shows_what_really_happened_through_the_api() {
    let app = TestApp::real().await;
    let workspace = admin_workspace(&app).await;
    let owner = workspace.owner.clone();
    let (status, created) = app
        .call(
            "POST",
            &format!("/api/v1/workspaces/{}/documents", workspace.hex()),
            Some(&owner),
            Some(json!({"title": "Through the API"})),
        )
        .await;
    assert_eq!(status, 201, "{created}");

    let (_, page) = app
        .call("GET", &audit_path(&workspace, ""), Some(&owner), None)
        .await;
    let items = page["items"].as_array().unwrap();
    // The fixture adds its members straight to the database (no rows); the API wrote one row.
    let actions: Vec<&str> = items
        .iter()
        .map(|e| e["action"].as_str().unwrap())
        .collect();
    assert_eq!(actions.first(), Some(&"DOC_CREATE"));
    assert_eq!(actions, ["DOC_CREATE"]);
    assert_eq!(items[0]["entity_id"], created["id"]);
    assert_eq!(items[0]["actor"]["id"], owner.id.to_hex());
}

#[tokio::test]
async fn only_an_admin_may_read_it() {
    let app = TestApp::real().await;
    let workspace = admin_workspace(&app).await;
    let outsider = app.account("Outsider").await;
    let editor = workspace.member(Role::Editor).clone();
    let viewer = workspace.member(Role::Viewer).clone();
    insert_at(&app, workspace.id, editor.id, &doc_event("Secret"), 1_000).await;

    for who in [&editor, &viewer] {
        let answer = app
            .call("GET", &audit_path(&workspace, ""), Some(who), None)
            .await;
        assert_error(&answer, 403, "FORBIDDEN");
        assert!(!answer.1.to_string().contains("Secret"));
    }
    assert_error(
        &app.call("GET", &audit_path(&workspace, ""), Some(&outsider), None)
            .await,
        404,
        "NOT_FOUND",
    );
    assert_error(
        &app.call("GET", &audit_path(&workspace, ""), None::<&Account>, None)
            .await,
        401,
        "UNAUTHORIZED",
    );
}

#[tokio::test]
async fn it_never_shows_another_workspaces_rows() {
    let app = TestApp::real().await;
    let mine = admin_workspace(&app).await;
    let theirs = TestWorkspace::new(&app).await;
    insert_at(
        &app,
        theirs.id,
        theirs.owner.id,
        &doc_event("Theirs"),
        5_000,
    )
    .await;
    let own = insert_at(&app, mine.id, mine.owner.id, &doc_event("Mine"), 1_000).await;

    let (_, page) = app
        .call("GET", &audit_path(&mine, ""), Some(&mine.owner), None)
        .await;
    assert!(ids(&page).contains(&own));
    assert!(!page.to_string().contains("Theirs"));
}

#[tokio::test]
async fn it_can_be_narrowed_by_action_actor_and_entity_type() {
    let app = TestApp::real().await;
    let workspace = admin_workspace(&app).await;
    let owner = workspace.owner.clone();
    let editor = workspace.member(Role::Editor).clone();
    let doc_by_owner = insert_at(&app, workspace.id, owner.id, &doc_event("A"), 1_000).await;
    let doc_by_editor = insert_at(&app, workspace.id, editor.id, &doc_event("B"), 2_000).await;
    let task_by_editor = insert_at(
        &app,
        workspace.id,
        editor.id,
        &AuditEvent::TaskDelete {
            task_id: ObjectId::new(),
            title: "C".into(),
        },
        3_000,
    )
    .await;
    let get = |query: String| {
        let app = &app;
        let workspace = &workspace;
        let owner = owner.clone();
        async move {
            let (status, page) = app
                .call("GET", &audit_path(workspace, &query), Some(&owner), None)
                .await;
            assert_eq!(status, 200, "{page}");
            ids(&page)
        }
    };
    assert_eq!(
        get("action=DOC_CREATE&entity_type=DOCUMENT".into()).await,
        [doc_by_editor.clone(), doc_by_owner.clone()]
    );
    assert_eq!(
        get("action=TASK_DELETE".into()).await,
        std::slice::from_ref(&task_by_editor)
    );
    assert_eq!(
        get("entity_type=TASK".into()).await,
        std::slice::from_ref(&task_by_editor)
    );
    assert_eq!(
        get(format!("actor_id={}&action=DOC_CREATE", editor.id.to_hex())).await,
        std::slice::from_ref(&doc_by_editor)
    );
    assert_eq!(
        get(format!("actor_id={}", owner.id.to_hex())).await.first(),
        Some(&doc_by_owner)
    );
    assert!(get("action=MEMBER_REMOVE".into()).await.is_empty());
    // Empty filters, as an untouched form sends them, mean "all".
    assert_eq!(get("action=&actor_id=&entity_type=".into()).await.len(), 3);
}

#[tokio::test]
async fn it_pages_without_gaps_or_repeats_even_when_rows_share_a_timestamp() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app).await;
    let owner = workspace.owner.clone();
    // The fixture wrote its own rows (none for a workspace with only an owner); add seven, three
    // of them at the same instant.
    let mut inserted = Vec::new();
    for (n, millis) in [1_000, 2_000, 3_000, 3_000, 3_000, 4_000, 5_000]
        .into_iter()
        .enumerate()
    {
        inserted.push(
            insert_at(
                &app,
                workspace.id,
                owner.id,
                &doc_event(&format!("D{n}")),
                millis,
            )
            .await,
        );
    }
    let mut expected = inserted.clone();
    // Newest first; for equal times the larger (later created) id first.
    expected.sort_by(|a, b| {
        let key = |id: &String| {
            let millis = app_millis(&inserted, id);
            (millis, ObjectId::parse_str(id).unwrap())
        };
        key(b).cmp(&key(a))
    });

    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0;
    loop {
        let query = match &cursor {
            Some(c) => format!("limit=3&cursor={c}"),
            None => "limit=3".to_string(),
        };
        let (status, page) = app
            .call("GET", &audit_path(&workspace, &query), Some(&owner), None)
            .await;
        assert_eq!(status, 200, "{page}");
        seen.extend(ids(&page));
        pages += 1;
        assert!(pages <= 4, "paging does not end");
        match page["next_cursor"].as_str() {
            Some(next) => cursor = Some(next.to_string()),
            None => break,
        }
    }
    assert_eq!(pages, 3, "7 rows in pages of 3");
    assert_eq!(seen, expected, "no gaps, no repeats, newest first");
}

/// The timestamp each of the rows above was given, by position.
fn app_millis(inserted: &[String], id: &String) -> i64 {
    const MILLIS: [i64; 7] = [1_000, 2_000, 3_000, 3_000, 3_000, 4_000, 5_000];
    MILLIS[inserted.iter().position(|i| i == id).unwrap()]
}

#[tokio::test]
async fn the_last_page_has_no_cursor_and_an_exact_fit_does_not_invent_one() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app).await;
    let owner = workspace.owner.clone();
    for n in 0..4 {
        insert_at(&app, workspace.id, owner.id, &doc_event("x"), 1_000 + n).await;
    }
    let (_, exact) = app
        .call(
            "GET",
            &audit_path(&workspace, "limit=4"),
            Some(&owner),
            None,
        )
        .await;
    assert_eq!(exact["items"].as_array().unwrap().len(), 4);
    assert_eq!(exact["next_cursor"], Value::Null);
    let (_, short) = app
        .call(
            "GET",
            &audit_path(&workspace, "limit=3"),
            Some(&owner),
            None,
        )
        .await;
    assert!(short["next_cursor"].is_string());
}

#[tokio::test]
async fn bad_query_values_are_422_with_one_entry_per_field() {
    let app = TestApp::real().await;
    let workspace = TestWorkspace::new(&app).await;
    let owner = workspace.owner.clone();
    for (query, field) in [
        ("limit=0", "limit"),
        ("limit=201", "limit"),
        ("limit=-1", "limit"),
        ("cursor=garbage", "cursor"),
        ("cursor=12_notanid", "cursor"),
        ("action=DOC_EXPLODE", "action"),
        ("entity_type=PLANET", "entity_type"),
        ("actor_id=nobody", "actor_id"),
    ] {
        let answer = app
            .call("GET", &audit_path(&workspace, query), Some(&owner), None)
            .await;
        assert_error(&answer, 422, "VALIDATION");
        assert!(
            answer.1["error"]["details"][field].is_string(),
            "{query}: {}",
            answer.1
        );
    }
    let all = app
        .call(
            "GET",
            &audit_path(&workspace, "limit=0&action=NOPE&actor_id=x"),
            Some(&owner),
            None,
        )
        .await;
    let details = all.1["error"]["details"].as_object().unwrap();
    assert_eq!(details.len(), 3, "every problem at once: {details:?}");
}

#[tokio::test]
async fn reading_the_log_writes_nothing() {
    let app = TestApp::real().await;
    let workspace = admin_workspace(&app).await;
    let owner = workspace.owner.clone();
    let before = app.audit_rows(workspace.id).await.len();
    for _ in 0..3 {
        let (status, _) = app
            .call("GET", &audit_path(&workspace, ""), Some(&owner), None)
            .await;
        assert_eq!(status, 200);
    }
    assert_eq!(app.audit_rows(workspace.id).await.len(), before);
}

#[test]
fn a_cursor_survives_the_round_trip_and_refuses_junk() {
    let cursor = audit::Cursor {
        timestamp: DateTime::from_millis(1_700_000_000_123),
        id: ObjectId::new(),
    };
    assert_eq!(audit::Cursor::decode(&cursor.encode()), Some(cursor));
    for junk in ["", "_", "12", "x_y", "12_", "_abc", "12_zz", "1_2_3"] {
        assert_eq!(audit::Cursor::decode(junk), None, "{junk:?}");
    }
}

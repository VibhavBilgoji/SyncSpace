//! The RBAC matrix (`docs/TESTING.md`, "RBAC matrix"): every REST route is called as an ADMIN,
//! an EDITOR, a VIEWER, a signed-in non-member and an anonymous caller, and the answer must be
//! the one the permission matrix of `docs/API.md` promises.
//!
//! The routes come from `server::app::api_route_table()`, the same table the router is built
//! from, so a route cannot be added without showing up here. Three guards keep the table, the
//! documentation and this test from drifting:
//! - every table entry needs a fixture below, and every fixture a table entry;
//! - every table entry must appear in `docs/API.md` with the same role;
//! - every route in `docs/API.md` must be in the table, unless it belongs to a feature that is
//!   not built yet (listed in `NOT_BUILT_YET`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::{Account, TestApp, TestWorkspace};
use serde_json::{Value, json};
use server::app::{RouteSpec, api_route_table};
use server::rbac::{Access, Role};

/// Documented routes of features that later tasks build. When a task adds them to the table,
/// it also adds a fixture, and the entry here stops mattering.
const NOT_BUILT_YET: &[&str] = &[
    "/auth/ws-ticket",
    "/documents/{id}/versions",
    "/workspaces/{wid}/boards",
    "/boards/",
    "/tasks/",
    "/workspaces/{wid}/audit",
];

/// The people in one test workspace, and one spare account.
struct Ctx {
    workspace_hex: String,
    /// A viewer other than the caller, who the member routes act on.
    target: Account,
    /// A registered account that is in no workspace, for "add this member".
    spare: Account,
    /// A document of the workspace, for the routes under `/documents/{id}`.
    doc_hex: String,
}

/// What a route needs to be called successfully.
struct Fixture {
    /// The status a caller who is allowed gets.
    ok: u16,
    body: fn(&Ctx) -> Option<Value>,
}

fn fixture(key: &str) -> Option<Fixture> {
    let none: fn(&Ctx) -> Option<Value> = |_| None;
    Some(match key {
        "POST /auth/register" => Fixture {
            ok: 201,
            body: |_| {
                Some(json!({
                    "name": "Matrix User",
                    "email": format!("{}@example.com", uuid::Uuid::new_v4().simple()),
                    "password": "a long enough passphrase"
                }))
            },
        },
        // Unknown credentials: the handler is reached and answers 401, whoever asks.
        "POST /auth/login" => Fixture {
            ok: 401,
            body: |_| Some(json!({"email": "nobody@example.com", "password": "whatever it is"})),
        },
        "POST /auth/logout" => Fixture {
            ok: 204,
            body: none,
        },
        "GET /auth/me" => Fixture {
            ok: 200,
            body: none,
        },
        "GET /workspaces" => Fixture {
            ok: 200,
            body: none,
        },
        "POST /workspaces" => Fixture {
            ok: 201,
            body: |_| {
                Some(json!({
                    "name": "Matrix",
                    "slug": format!("m-{}", &uuid::Uuid::new_v4().simple().to_string()[..12])
                }))
            },
        },
        "GET /workspaces/{wid}" => Fixture {
            ok: 200,
            body: none,
        },
        "PATCH /workspaces/{wid}" => Fixture {
            ok: 200,
            body: |_| Some(json!({"name": "Renamed"})),
        },
        "GET /workspaces/{wid}/members" => Fixture {
            ok: 200,
            body: none,
        },
        "POST /workspaces/{wid}/members" => Fixture {
            ok: 201,
            body: |ctx| Some(json!({"email": ctx.spare.email, "role": "VIEWER"})),
        },
        "PATCH /workspaces/{wid}/members/{uid}" => Fixture {
            ok: 200,
            body: |_| Some(json!({"role": "EDITOR"})),
        },
        "DELETE /workspaces/{wid}/members/{uid}" => Fixture {
            ok: 204,
            body: none,
        },
        "GET /workspaces/{wid}/documents" => Fixture {
            ok: 200,
            body: none,
        },
        "POST /workspaces/{wid}/documents" => Fixture {
            ok: 201,
            body: |_| Some(json!({"title": "Matrix document"})),
        },
        "GET /documents/{id}" => Fixture {
            ok: 200,
            body: none,
        },
        "PATCH /documents/{id}" => Fixture {
            ok: 200,
            body: |_| Some(json!({"title": "Renamed document"})),
        },
        "DELETE /documents/{id}" => Fixture {
            ok: 204,
            body: none,
        },
        _ => return None,
    })
}

fn key(spec: &RouteSpec) -> String {
    format!("{} {}", spec.method, spec.path)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Caller {
    Anonymous,
    /// Signed in, member of no workspace.
    Outsider,
    Member(Role),
}

const CALLERS: [Caller; 5] = [
    Caller::Anonymous,
    Caller::Outsider,
    Caller::Member(Role::Viewer),
    Caller::Member(Role::Editor),
    Caller::Member(Role::Admin),
];

/// The status the API promises for this caller on this route.
fn expected(access: Access, caller: Caller, ok: u16) -> u16 {
    match (access, caller) {
        (Access::Public, _) => ok,
        (_, Caller::Anonymous) => 401,
        (Access::User, _) => ok,
        (Access::Member(_), Caller::Outsider) => 404,
        (Access::Member(min), Caller::Member(role)) if role < min => 403,
        (Access::Member(_), Caller::Member(_)) => ok,
    }
}

async fn workspace_for_a_case(app: &TestApp) -> (TestWorkspace<'_>, Ctx) {
    let workspace = TestWorkspace::new(app)
        .await
        .with_member(Role::Viewer)
        .await
        .with_member(Role::Editor)
        .await
        .with_member(Role::Admin)
        .await
        .with_member(Role::Viewer)
        .await;
    let target = workspace.members[3].1.clone();
    let spare = app.account("Spare").await;
    let record = server::documents::new_record(
        workspace.id,
        workspace.owner.id,
        "Matrix document".to_string(),
        "📄".to_string(),
    )
    .unwrap();
    server::db::documents::insert(&app.db, &record)
        .await
        .unwrap();
    let ctx = Ctx {
        workspace_hex: workspace.hex(),
        target,
        spare,
        doc_hex: record.id.to_hex(),
    };
    (workspace, ctx)
}

fn caller_account<'a>(
    caller: Caller,
    workspace: &'a TestWorkspace<'_>,
    outsider: &'a Account,
) -> Option<&'a Account> {
    match caller {
        Caller::Anonymous => None,
        Caller::Outsider => Some(outsider),
        Caller::Member(Role::Admin) => Some(&workspace.owner),
        Caller::Member(role) => Some(workspace.member(role)),
    }
}

#[tokio::test]
async fn every_route_gives_every_kind_of_caller_the_documented_status() {
    let app = TestApp::real().await;
    let table = api_route_table();
    assert!(table.len() >= 12, "the table lost routes");
    let mut checked = 0;

    for spec in &table {
        let fixture = fixture(&key(spec)).unwrap_or_else(|| {
            panic!("{} has no fixture in tests/rbac_matrix.rs", key(spec));
        });
        for caller in CALLERS {
            // A fresh workspace per case: the mutating routes change it.
            let (workspace, ctx) = workspace_for_a_case(&app).await;
            let outsider = app.account("Outsider").await;
            let account = caller_account(caller, &workspace, &outsider);
            let path = format!("/api/v1{}", spec.path)
                .replace("{wid}", &ctx.workspace_hex)
                .replace("{uid}", &ctx.target.id.to_hex())
                .replace("{id}", &ctx.doc_hex);

            let (status, body) = app
                .call(spec.method, &path, account, (fixture.body)(&ctx))
                .await;
            let want = expected(spec.access, caller, fixture.ok);
            assert_eq!(
                status, want,
                "{} {} as {caller:?}: {body}",
                spec.method, spec.path
            );
            if status >= 400 {
                assert!(
                    body["error"]["code"].is_string(),
                    "{} {} as {caller:?} answered {status} outside the error format: {body}",
                    spec.method,
                    spec.path
                );
            }
            checked += 1;
        }
    }
    assert_eq!(checked, table.len() * CALLERS.len());
}

#[tokio::test]
async fn a_refused_call_changes_nothing() {
    // A caller who is refused must not have caused the change: run each mutating member route
    // as a viewer and as an outsider, and compare the workspace and the audit log afterwards.
    let app = TestApp::real().await;
    for spec in api_route_table()
        .iter()
        .filter(|s| s.method != "GET" && matches!(s.access, Access::Member(_)))
    {
        let fixture = fixture(&key(spec)).unwrap();
        for caller in [Caller::Outsider, Caller::Member(Role::Viewer)] {
            let (workspace, ctx) = workspace_for_a_case(&app).await;
            let outsider = app.account("Outsider").await;
            let before = workspace.stored().await;
            let path = format!("/api/v1{}", spec.path)
                .replace("{wid}", &ctx.workspace_hex)
                .replace("{uid}", &ctx.target.id.to_hex())
                .replace("{id}", &ctx.doc_hex);
            let (status, _) = app
                .call(
                    spec.method,
                    &path,
                    caller_account(caller, &workspace, &outsider),
                    (fixture.body)(&ctx),
                )
                .await;
            assert!(status == 403 || status == 404, "{} -> {status}", key(spec));
            assert_eq!(workspace.stored().await, before, "{}", key(spec));
            assert!(
                app.audit_rows(workspace.id).await.is_empty(),
                "{}",
                key(spec)
            );
        }
    }
}

#[test]
fn the_table_has_a_fixture_for_every_route_and_no_stale_fixture() {
    let in_table: BTreeSet<String> = api_route_table().iter().map(key).collect();
    let probes = [
        "POST /auth/register",
        "POST /auth/login",
        "POST /auth/logout",
        "GET /auth/me",
        "GET /workspaces",
        "POST /workspaces",
        "GET /workspaces/{wid}",
        "PATCH /workspaces/{wid}",
        "GET /workspaces/{wid}/members",
        "POST /workspaces/{wid}/members",
        "PATCH /workspaces/{wid}/members/{uid}",
        "DELETE /workspaces/{wid}/members/{uid}",
        "GET /workspaces/{wid}/documents",
        "POST /workspaces/{wid}/documents",
        "GET /documents/{id}",
        "PATCH /documents/{id}",
        "DELETE /documents/{id}",
    ];
    let with_fixture: BTreeSet<String> = probes
        .iter()
        .filter(|k| fixture(k).is_some())
        .map(|k| (*k).to_string())
        .collect();
    assert_eq!(in_table, with_fixture);
    assert_eq!(in_table.len(), api_route_table().len(), "duplicate route");
}

/// `GET /workspaces/:wid` in the docs is `GET /workspaces/{wid}` in the table.
fn to_axum(path: &str) -> String {
    path.split('/')
        .map(|part| match part.strip_prefix(':') {
            Some(name) => format!("{{{name}}}"),
            None => part.to_string(),
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn access_from_doc(role: &str) -> Access {
    match role {
        "public" => Access::Public,
        "user" => Access::User,
        "member" => Access::User, // `ws-ticket`: any signed-in user until the hub exists
        "VIEWER" => Access::Member(Role::Viewer),
        "EDITOR" => Access::Member(Role::Editor),
        "ADMIN" => Access::Member(Role::Admin),
        other => panic!("unknown role `{other}` in docs/API.md"),
    }
}

/// `"METHOD /path" -> role cell`, from the endpoint tables of `docs/API.md` section 2.
fn documented_routes() -> BTreeMap<String, String> {
    let doc = include_str!("../../../docs/API.md");
    let start = doc.find("## 2. REST endpoints").unwrap();
    let end = doc.find("## 3. WebSocket protocol").unwrap();
    let mut routes = BTreeMap::new();
    for line in doc[start..end].lines().filter(|l| l.starts_with("| `")) {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        let Some((method, path)) = cells[1].trim_matches('`').split_once(' ') else {
            continue;
        };
        if path.starts_with('/') {
            routes.insert(format!("{method} {}", to_axum(path)), cells[2].to_string());
        }
    }
    routes
}

#[test]
fn the_route_table_agrees_with_the_permission_matrix_in_api_md() {
    let documented = documented_routes();
    assert!(
        documented.len() >= 30,
        "parsed too few routes: {}",
        documented.len()
    );

    let table = api_route_table();
    for spec in &table {
        let role = documented
            .get(&key(spec))
            .unwrap_or_else(|| panic!("{} is not documented in docs/API.md", key(spec)));
        assert_eq!(
            access_from_doc(role),
            spec.access,
            "{}: the table and the Role column of docs/API.md disagree",
            key(spec)
        );
    }

    let in_table: BTreeSet<String> = table.iter().map(key).collect();
    for documented_key in documented.keys() {
        let path = documented_key.split_once(' ').unwrap().1;
        // `/healthz` is served at the server root, not under `/api/v1` (`tests/api.rs`).
        if path == "/healthz" {
            continue;
        }
        let not_built = NOT_BUILT_YET.iter().any(|prefix| path.starts_with(prefix));
        assert!(
            in_table.contains(documented_key) || not_built,
            "{documented_key} is documented but missing from the route table"
        );
    }
}

#[test]
fn the_not_built_list_does_not_hide_a_route_that_exists() {
    for spec in api_route_table() {
        assert!(
            !NOT_BUILT_YET.iter().any(|p| spec.path.starts_with(p)),
            "{} is built; take it off NOT_BUILT_YET",
            key(&spec)
        );
    }
}

#[test]
fn the_audit_log_has_no_write_route() {
    // Audit rows are append-only (CLAUDE.md rule 5): no route under /audit may change them.
    for spec in api_route_table() {
        if spec.path.contains("audit") {
            assert_eq!(
                spec.method,
                "GET",
                "{} must not write audit rows",
                key(&spec)
            );
        }
    }
}

//! Tests for the generated TypeScript types (task 1.4, `docs/TESTING.md` section 3: "Generated
//! TypeScript types up to date"). They need no database.
//!
//! The first test is the staleness check as a plain `cargo test`: it fails locally before CI
//! does, and says how to fix it.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use server::api_types::{
    ApiErrorBody, ApiErrorResponse, Block, BlockType, Board, ChangeType, Document, DocumentSummary,
    ErrorCode, Page, Priority, Status, StatusCounts, Task, UserRef, VersionSummary,
};
use server::auth::handlers::UserResponse;
use server::error::ApiError;
use server::rbac::Role;
use server::typegen::{default_dir, generate};
use server::workspaces::handlers::{MemberResponse, SummaryResponse, WorkspaceResponse};

/// The request bodies are documented next to their endpoints in `API.md` section 2, not in the
/// "Shared types" block of section 4.
const REQUEST_TYPES: [&str; 8] = [
    "AddMemberRequest",
    "ChangeRoleRequest",
    "CreateDocumentRequest",
    "CreateWorkspaceRequest",
    "LoginRequest",
    "RegisterRequest",
    "UpdateDocumentRequest",
    "UpdateWorkspaceRequest",
];

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("syncspace_typegen_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// File name to contents of every `.ts` file in `dir`.
fn read_all(dir: &Path) -> BTreeMap<String, String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "ts"))
        .map(|path| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (
                name,
                fs::read_to_string(&path).unwrap().replace("\r\n", "\n"),
            )
        })
        .collect()
}

fn generated() -> BTreeMap<String, String> {
    let dir = temp_dir();
    generate(&dir).unwrap();
    let files = read_all(&dir);
    fs::remove_dir_all(&dir).unwrap();
    files
}

/// The field names of `export type X = { a: T, b?: U, ... }`, without doc comments.
fn fields_of(source: &str) -> BTreeSet<String> {
    // Drop `/** ... */` comments.
    let mut text = String::new();
    let mut rest = source;
    while let Some(start) = rest.find("/**") {
        text.push_str(&rest[..start]);
        rest = &rest[start + 3..];
        rest = &rest[rest.find("*/").unwrap() + 2..];
    }
    text.push_str(rest);

    let body_start = text.find("= {").unwrap() + 3;
    let body_end = text.rfind('}').unwrap();
    let body = &text[body_start..body_end];

    // Split on top-level commas.
    let (mut depth, mut field, mut names) = (0usize, String::new(), BTreeSet::new());
    let mut flush = |field: &mut String| {
        if let Some((name, _)) = field.split_once(':') {
            names.insert(name.trim().trim_end_matches('?').to_string());
        }
        field.clear();
    };
    for c in body.chars() {
        match c {
            '{' | '<' | '[' | '(' => depth += 1,
            '}' | '>' | ']' | ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                flush(&mut field);
                continue;
            }
            _ => {}
        }
        field.push(c);
    }
    flush(&mut field);
    names
}

fn keys_of(value: &Value) -> BTreeSet<String> {
    value.as_object().unwrap().keys().cloned().collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn the_committed_types_are_up_to_date() {
    let fresh = generated();
    let committed = read_all(&default_dir());
    let fix = "run `cargo run -p server --bin gen-types` and commit apps/web/src/types/generated";
    assert_eq!(
        fresh.keys().collect::<Vec<_>>(),
        committed.keys().collect::<Vec<_>>(),
        "the set of generated files differs from the committed one: {fix}"
    );
    for (name, text) in &fresh {
        assert_eq!(text, &committed[name], "{name} is stale: {fix}");
    }
}

#[test]
fn generation_removes_files_of_types_that_no_longer_exist_and_keeps_other_files() {
    let dir = temp_dir();
    fs::write(dir.join("Old.ts"), "export type Old = string;\n").unwrap();
    fs::write(dir.join("notes.txt"), "keep me").unwrap();

    let names = generate(&dir).unwrap();

    assert!(!dir.join("Old.ts").exists(), "a stale type file survived");
    assert!(!names.contains(&"Old".to_string()));
    assert_eq!(
        fs::read_to_string(dir.join("notes.txt")).unwrap(),
        "keep me"
    );
    // The same types give the same files in another directory. (`ts-rs` remembers what it wrote
    // in this process, so each run of `generate` uses a directory of its own; the binary runs
    // once per process.)
    let other = temp_dir();
    generate(&other).unwrap();
    let mut first = read_all(&dir);
    first.remove("notes.txt");
    assert_eq!(first, read_all(&other));
    fs::remove_dir_all(&other).unwrap();
    fs::remove_dir_all(&dir).unwrap();
}

/// The type names in the "Shared types" block of `API.md` section 4.
fn documented_type_names() -> BTreeSet<String> {
    let doc = include_str!("../../../docs/API.md");
    let start = doc.find("## 4. Shared types").unwrap();
    let block_start = doc[start..].find("```ts").unwrap() + start;
    let block_end = doc[block_start + 5..].find("```").unwrap() + block_start + 5;
    doc[block_start..block_end]
        .lines()
        .filter(|line| !line.starts_with(char::is_whitespace))
        .filter_map(|line| {
            let rest = line
                .strip_prefix("interface ")
                .or_else(|| line.strip_prefix("type "))?;
            let end = rest.find([' ', '<', '=', '{']).unwrap_or(rest.len());
            Some(rest[..end].to_string())
        })
        .collect()
}

#[test]
fn the_exported_types_are_exactly_the_documented_ones_and_the_request_bodies() {
    let exported: BTreeSet<String> = generated()
        .keys()
        .filter_map(|file| file.strip_suffix(".ts").map(str::to_string))
        .filter(|name| name != "index")
        .collect();
    let mut expected = documented_type_names();
    expected.extend(REQUEST_TYPES.iter().map(|s| (*s).to_string()));
    assert_eq!(
        exported, expected,
        "docs/API.md section 4 (plus the request bodies) and the exports of typegen.rs differ"
    );
    assert!(expected.len() >= 25, "parsed too few documented types");
}

#[test]
fn the_index_re_exports_every_type() {
    let files = generated();
    let index = &files["index.ts"];
    for file in files.keys().filter(|f| *f != "index.ts") {
        let name = file.strip_suffix(".ts").unwrap();
        assert!(
            index.contains(&format!("export type {{ {name} }} from \"./{name}\";")),
            "{name} is missing from index.ts"
        );
    }
    assert_eq!(
        index.lines().count(),
        files.len(),
        "one header line plus one line per type"
    );
}

#[test]
fn every_file_says_it_is_generated() {
    for (name, text) in generated() {
        let first = text.lines().next().unwrap();
        if name == "index.ts" {
            assert!(
                first.starts_with("// Generated by `cargo run -p server --bin gen-types`"),
                "{first}"
            );
        } else {
            assert!(first.contains("generated by [ts-rs]"), "{name}: {first}");
        }
    }
}

#[test]
fn nothing_is_bigint_or_any() {
    for (name, text) in generated() {
        assert!(!text.contains("bigint"), "{name} uses bigint");
        for bad in [": any", "<any", "any[", "any>", "unknown[]"] {
            assert!(!text.contains(bad), "{name} contains `{bad}`");
        }
    }
}

#[test]
fn unions_and_shapes_match_the_wire() {
    let files = generated();
    let text = |name: &str| files[&format!("{name}.ts")].clone();

    assert!(text("Role").contains(r#"export type Role = "VIEWER" | "EDITOR" | "ADMIN";"#));
    assert!(text("Status").contains(r#""BACKLOG" | "TODO" | "IN_PROGRESS" | "REVIEW" | "DONE""#));
    assert!(text("BlockType").contains(r#""HEADING_1" | "HEADING_2""#));
    assert!(text("ChangeType").contains(r#""AUTOMATIC" | "MANUAL" | "RESTORE""#));
    assert!(text("Page").contains("export type Page<T>"));
    assert!(text("Page").contains("next_cursor: string | null"));
    assert!(text("Workspace").contains("role: Role"));
    assert!(
        text("Block").contains("type: BlockType"),
        "serde rename of `type` was lost"
    );
    assert!(text("Block").contains("properties: Record<string, unknown>"));
    assert!(text("VersionSummary").contains("created_by: UserRef | null"));
    assert!(text("Document").contains("revision: number"));

    // Request bodies: optional fields are optional, required ones are not.
    assert_eq!(
        fields_of(&text("UpdateWorkspaceRequest")),
        set(&["name", "slug"])
    );
    assert!(text("UpdateWorkspaceRequest").contains("name?: string"));
    assert!(text("UpdateWorkspaceRequest").contains("slug?: string"));
    assert_eq!(
        fields_of(&text("RegisterRequest")),
        set(&["name", "email", "password"])
    );
    assert_eq!(
        fields_of(&text("LoginRequest")),
        set(&["email", "password"])
    );
    assert_eq!(
        fields_of(&text("CreateWorkspaceRequest")),
        set(&["name", "slug"])
    );
    assert_eq!(
        fields_of(&text("AddMemberRequest")),
        set(&["email", "role"])
    );
    assert_eq!(fields_of(&text("ChangeRoleRequest")), set(&["role"]));
    assert!(!text("RegisterRequest").contains('?'));
}

#[test]
fn serialised_values_have_exactly_the_keys_of_the_generated_types() {
    let files = generated();
    let declared = |name: &str| fields_of(&files[&format!("{name}.ts")]);
    let task = Task {
        id: "t".into(),
        board_id: "b".into(),
        title: "t".into(),
        description: String::new(),
        status: Status::Todo,
        priority: Priority::Medium,
        assignee_ids: vec![],
        tags: vec![],
        order: 1.0,
        created_at: "x".into(),
        updated_at: "x".into(),
    };
    let block = Block {
        block_id: "a".into(),
        block_type: BlockType::Paragraph,
        content: String::new(),
        properties: json!({}),
        order: 1.0,
    };
    let user = UserRef {
        id: "u".into(),
        name: "n".into(),
    };
    let cases: Vec<(&str, Value)> = vec![
        (
            "User",
            serde_json::to_value(UserResponse {
                id: "u".into(),
                name: "n".into(),
                email: "e".into(),
            })
            .unwrap(),
        ),
        (
            "Workspace",
            serde_json::to_value(WorkspaceResponse {
                id: "w".into(),
                name: "n".into(),
                slug: "s".into(),
                owner_id: "o".into(),
                role: Role::Admin,
                created_at: "x".into(),
                updated_at: "x".into(),
            })
            .unwrap(),
        ),
        (
            "WorkspaceSummary",
            serde_json::to_value(SummaryResponse {
                id: "w".into(),
                name: "n".into(),
                slug: "s".into(),
                role: Role::Viewer,
                is_owner: false,
                member_count: 1,
                document_count: 0,
                board_count: 0,
            })
            .unwrap(),
        ),
        (
            "Member",
            serde_json::to_value(MemberResponse {
                user_id: "u".into(),
                name: "n".into(),
                email: "e".into(),
                role: Role::Editor,
                joined_at: "x".into(),
                is_owner: false,
            })
            .unwrap(),
        ),
        ("Block", serde_json::to_value(&block).unwrap()),
        ("Task", serde_json::to_value(&task).unwrap()),
        ("UserRef", serde_json::to_value(&user).unwrap()),
        (
            "DocumentSummary",
            serde_json::to_value(DocumentSummary {
                id: "d".into(),
                title: "t".into(),
                icon: "i".into(),
                revision: 1,
                updated_at: "x".into(),
            })
            .unwrap(),
        ),
        (
            "Document",
            serde_json::to_value(Document {
                id: "d".into(),
                workspace_id: "w".into(),
                title: "t".into(),
                icon: "i".into(),
                revision: 1,
                blocks: vec![block],
                created_by: "u".into(),
                created_at: "x".into(),
                updated_at: "x".into(),
            })
            .unwrap(),
        ),
        (
            "VersionSummary",
            serde_json::to_value(VersionSummary {
                version_number: 1,
                change_type: ChangeType::Manual,
                label: Some("l".into()),
                restored_from: None,
                restored_from_kept: None,
                created_by: Some(user),
                created_at: "x".into(),
            })
            .unwrap(),
        ),
        (
            "StatusCounts",
            serde_json::to_value(StatusCounts {
                backlog: 0,
                todo: 0,
                in_progress: 0,
                review: 0,
                done: 0,
            })
            .unwrap(),
        ),
        (
            "Board",
            serde_json::to_value(Board {
                id: "b".into(),
                workspace_id: "w".into(),
                name: "n".into(),
                task_counts: StatusCounts {
                    backlog: 0,
                    todo: 0,
                    in_progress: 0,
                    review: 0,
                    done: 0,
                },
                created_at: "x".into(),
            })
            .unwrap(),
        ),
        (
            "Page",
            serde_json::to_value(Page::<Task> {
                items: vec![task],
                next_cursor: None,
            })
            .unwrap(),
        ),
        (
            "ApiErrorResponse",
            serde_json::to_value(ApiErrorResponse {
                error: ApiErrorBody {
                    code: ErrorCode::Validation,
                    message: "m".into(),
                    details: Some(BTreeMap::from([("f".to_string(), "m".to_string())])),
                },
            })
            .unwrap(),
        ),
    ];
    for (name, value) in &cases {
        assert_eq!(
            &keys_of(value),
            &declared(name),
            "{name}: serialised keys and generated keys differ"
        );
    }
    // `AuditEntry` and `ApiErrorBody` through their JSON shape.
    let audit = json!({"event_id": "e", "actor": {"id": "u", "name": "n"}, "action": "A",
        "entity_type": "T", "entity_id": "i", "metadata": {}, "timestamp": "x"});
    assert_eq!(keys_of(&audit), declared("AuditEntry"));
    assert_eq!(
        keys_of(&json!({"code": "NOT_FOUND", "message": "m", "details": {}})),
        declared("ApiErrorBody")
    );
}

#[test]
fn error_codes_cover_every_api_error_and_nothing_else() {
    let errors = [
        ApiError::bad_request("x"),
        ApiError::unauthorized("x"),
        ApiError::forbidden("x"),
        ApiError::not_found("x"),
        ApiError::conflict("x"),
        ApiError::validation([("f", "m")]),
        ApiError::RateLimited,
        ApiError::PayloadTooLarge,
        ApiError::MethodNotAllowed,
        ApiError::internal("x"),
    ];
    let from_errors: BTreeSet<String> = errors.iter().map(|e| e.code().to_string()).collect();

    let all_codes = [
        ErrorCode::BadRequest,
        ErrorCode::Unauthorized,
        ErrorCode::Forbidden,
        ErrorCode::NotFound,
        ErrorCode::Conflict,
        ErrorCode::Validation,
        ErrorCode::RateLimited,
        ErrorCode::PayloadTooLarge,
        ErrorCode::MethodNotAllowed,
        ErrorCode::Internal,
    ];
    let from_enum: BTreeSet<String> = all_codes
        .iter()
        .map(|c| {
            serde_json::to_value(c)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(from_errors, from_enum);

    // The generated union lists exactly these.
    let text = &generated()["ErrorCode.ts"];
    for code in &from_enum {
        assert!(
            text.contains(&format!("\"{code}\"")),
            "{code} missing from ErrorCode.ts"
        );
    }
    assert_eq!(text.matches(" | ").count(), from_enum.len() - 1);
}

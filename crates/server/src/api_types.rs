//! Wire types of `docs/API.md` that no handler returns yet.
//!
//! They exist now so that the web track can type its mocks against the contract (task 1.4); the
//! tasks that build the endpoints (2.2 documents, 4.3 versions, 5.1 boards and tasks, 6.1 audit)
//! return these types from their handlers and may change them, after which the TypeScript is
//! regenerated (`cargo run -p server --bin gen-types`). The types that already have handlers
//! (`User`, `Workspace`, `WorkspaceSummary`, `Member`, `Role` and the request bodies) are
//! defined next to their handlers and only derive `TS` there.
//!
//! Ids are 24-character hex strings and timestamps are RFC 3339 strings, as everywhere in the
//! API (`docs/API.md` section 1).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum BlockType {
    #[serde(rename = "PARAGRAPH")]
    Paragraph,
    #[serde(rename = "HEADING_1")]
    Heading1,
    #[serde(rename = "HEADING_2")]
    Heading2,
    #[serde(rename = "CODE")]
    Code,
    #[serde(rename = "CHECKLIST")]
    Checklist,
    #[serde(rename = "CALLOUT")]
    Callout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Status {
    Backlog,
    Todo,
    InProgress,
    Review,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Priority {
    Low,
    Medium,
    High,
    Urgent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ChangeType {
    Automatic,
    Manual,
    Restore,
}

/// `{id, name}`: who did something.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UserRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Block {
    pub block_id: String,
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub block_type: BlockType,
    /// Plain text.
    pub content: String,
    /// `language` for CODE, `checked` for CHECKLIST, `tone` for CALLOUT.
    #[ts(type = "Record<string, unknown>")]
    pub properties: serde_json::Value,
    /// Fractional position; blocks render sorted by `(order, block_id)`.
    pub order: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DocumentSummary {
    pub id: String,
    pub title: String,
    /// One emoji or a short icon key.
    pub icon: String,
    /// Shown as `rev_N`; only increases.
    pub revision: i64,
    pub updated_at: String,
}

/// A document with its blocks (never the CRDT snapshot).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Document {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    pub icon: String,
    pub revision: i64,
    pub blocks: Vec<Block>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct VersionSummary {
    pub version_number: i32,
    pub change_type: ChangeType,
    /// `null` never happens for MANUAL; the others get a generated label, so in practice this is
    /// always a string, but the contract allows `null`.
    pub label: Option<String>,
    /// The version that was restored; only for RESTORE.
    pub restored_from: Option<i32>,
    /// False when the restored version was an automatic one that has since been pruned.
    pub restored_from_kept: Option<bool>,
    /// `null` for AUTOMATIC ("Server Snapshot").
    pub created_by: Option<UserRef>,
    pub created_at: String,
}

/// One version with its blocks (`GET /documents/:id/versions/:n`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct VersionDetail {
    pub version: VersionSummary,
    pub blocks: Vec<Block>,
}

/// How many tasks a board has in each column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StatusCounts {
    pub backlog: u32,
    pub todo: u32,
    pub in_progress: u32,
    pub review: u32,
    pub done: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Board {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub task_counts: StatusCounts,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Task {
    pub id: String,
    pub board_id: String,
    pub title: String,
    pub description: String,
    pub status: Status,
    pub priority: Priority,
    pub assignee_ids: Vec<String>,
    pub tags: Vec<String>,
    /// Position within its (board, status) column, computed by the server.
    pub order: f64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AuditEntry {
    pub event_id: String,
    pub actor: UserRef,
    pub action: String,
    pub entity_type: String,
    pub entity_id: String,
    #[ts(type = "Record<string, unknown>")]
    pub metadata: serde_json::Value,
    pub timestamp: String,
}

/// Cursor pagination (`docs/API.md` section 1): `?limit=50&cursor=<opaque>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Page<T> {
    pub items: Vec<T>,
    /// `null` on the last page.
    pub next_cursor: Option<String>,
}

/// Every `code` the API can answer with (`docs/API.md` section 1, error table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    BadRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    Validation,
    RateLimited,
    PayloadTooLarge,
    MethodNotAllowed,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ApiErrorBody {
    pub code: ErrorCode,
    pub message: String,
    /// Field to message, for `VALIDATION`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub details: Option<BTreeMap<String, String>>,
}

/// The body of every error response: `{"error": {...}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ApiErrorResponse {
    pub error: ApiErrorBody,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enums_use_the_names_of_the_api() {
        let json = |value: &dyn erased::Json| value.json();
        assert_eq!(json(&BlockType::Heading1), "\"HEADING_1\"");
        assert_eq!(json(&BlockType::Heading2), "\"HEADING_2\"");
        assert_eq!(json(&BlockType::Paragraph), "\"PARAGRAPH\"");
        assert_eq!(json(&Status::InProgress), "\"IN_PROGRESS\"");
        assert_eq!(json(&Priority::Urgent), "\"URGENT\"");
        assert_eq!(json(&ChangeType::Restore), "\"RESTORE\"");
        assert_eq!(json(&ErrorCode::PayloadTooLarge), "\"PAYLOAD_TOO_LARGE\"");
        assert_eq!(json(&ErrorCode::MethodNotAllowed), "\"METHOD_NOT_ALLOWED\"");
    }

    #[test]
    fn a_block_serialises_its_type_as_type() {
        let block = Block {
            block_id: "abc".into(),
            block_type: BlockType::Code,
            content: "fn main() {}".into(),
            properties: serde_json::json!({"language": "rust"}),
            order: 1.5,
        };
        let value = serde_json::to_value(&block).unwrap();
        assert_eq!(value["type"], "CODE");
        assert!(value.get("block_type").is_none());
        let back: Block = serde_json::from_value(value).unwrap();
        assert_eq!(back, block);
    }

    #[test]
    fn an_error_body_leaves_details_out_when_there_are_none() {
        let body = ApiErrorResponse {
            error: ApiErrorBody {
                code: ErrorCode::NotFound,
                message: "workspace not found".into(),
                details: None,
            },
        };
        assert_eq!(
            serde_json::to_value(&body).unwrap(),
            serde_json::json!({"error": {"code": "NOT_FOUND", "message": "workspace not found"}})
        );
    }

    /// A tiny helper so the test above can pass different enums to one closure.
    mod erased {
        pub trait Json {
            fn json(&self) -> String;
        }
        impl<T: serde::Serialize> Json for T {
            fn json(&self) -> String {
                serde_json::to_string(self).unwrap()
            }
        }
    }
}

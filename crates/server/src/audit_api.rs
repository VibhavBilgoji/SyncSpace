//! `GET /workspaces/:wid/audit` (`docs/API.md` section 2, "Audit"): the append-only log, read by
//! admins only, newest first. There is no route that writes to it; rows come from the actions
//! themselves ([`crate::audit::record`]).

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Bson, Document};
use serde::Deserialize;

use crate::api_types::{AuditEntry, Page, UserRef};
use crate::app::AppState;
use crate::audit::{self, ACTIONS, Cursor, ENTITY_TYPES, Filter};
use crate::auth::AuthUser;
use crate::db::users;
use crate::error::ApiError;
use crate::rbac::{Role, parse_id, require_role};
use crate::workspaces::handlers::iso;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;
/// Shown when the actor's account no longer exists.
const UNKNOWN_ACTOR: &str = "Unknown user";

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    limit: Option<i64>,
    cursor: Option<String>,
    action: Option<String>,
    actor_id: Option<String>,
    entity_type: Option<String>,
}

/// An empty value (`?action=`) means "no filter", as a form that was left alone sends it.
fn present(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|v| !v.is_empty())
}

fn entry(row: &Document, names: &HashMap<ObjectId, String>) -> Option<AuditEntry> {
    let actor_id = row.get_object_id("actor_id").ok()?;
    Some(AuditEntry {
        event_id: row.get_object_id("_id").ok()?.to_hex(),
        actor: UserRef {
            id: actor_id.to_hex(),
            name: names
                .get(&actor_id)
                .cloned()
                .unwrap_or_else(|| UNKNOWN_ACTOR.to_string()),
        },
        action: row.get_str("action").ok()?.to_string(),
        entity_type: row.get_str("entity_type").ok()?.to_string(),
        entity_id: row.get_str("entity_id").ok()?.to_string(),
        metadata: Bson::Document(row.get_document("metadata").ok()?.clone()).into_relaxed_extjson(),
        timestamp: iso(*row.get_datetime("timestamp").ok()?),
    })
}

pub async fn list_audit(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Page<AuditEntry>>, ApiError> {
    let workspace_id = parse_id(&wid)?;
    require_role(&state.db, workspace_id, user.id, Role::Admin).await?;

    let mut problems: Vec<(&str, &str)> = Vec::new();
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        problems.push(("limit", "must be 1 to 200"));
    }
    let after = match present(&query.cursor) {
        None => None,
        Some(raw) => {
            let cursor = Cursor::decode(raw);
            if cursor.is_none() {
                problems.push(("cursor", "not a valid cursor"));
            }
            cursor
        }
    };
    let action = match present(&query.action) {
        None => None,
        Some(raw) if ACTIONS.contains(&raw) => Some(raw.to_string()),
        Some(_) => {
            problems.push(("action", "not one of the audited actions"));
            None
        }
    };
    let entity_type = match present(&query.entity_type) {
        None => None,
        Some(raw) if ENTITY_TYPES.contains(&raw) => Some(raw.to_string()),
        Some(_) => {
            problems.push(("entity_type", "must be DOCUMENT, TASK, WORKSPACE or MEMBER"));
            None
        }
    };
    let actor_id = match present(&query.actor_id) {
        None => None,
        Some(raw) => {
            let id = ObjectId::parse_str(raw).ok();
            if id.is_none() {
                problems.push(("actor_id", "must be a user id"));
            }
            id
        }
    };
    if !problems.is_empty() {
        return Err(ApiError::validation(problems));
    }

    let filter = Filter {
        action,
        actor_id,
        entity_type,
    };
    // One extra row tells whether there is a next page.
    let mut rows = audit::list(&state.db, workspace_id, &filter, after, limit + 1).await?;
    let has_more = i64::try_from(rows.len()).unwrap_or(i64::MAX) > limit;
    rows.truncate(usize::try_from(limit).unwrap_or(0));

    let actors: Vec<ObjectId> = rows
        .iter()
        .filter_map(|row| row.get_object_id("actor_id").ok())
        .collect();
    let names: HashMap<ObjectId, String> = users::find_many(&state.db, &actors)
        .await?
        .into_iter()
        .map(|user| (user.id, user.name))
        .collect();

    let next_cursor = has_more
        .then(|| {
            let last = rows.last()?;
            Some(
                Cursor {
                    timestamp: *last.get_datetime("timestamp").ok()?,
                    id: last.get_object_id("_id").ok()?,
                }
                .encode(),
            )
        })
        .flatten();
    let items = rows.iter().filter_map(|row| entry(row, &names)).collect();
    Ok(Json(Page { items, next_cursor }))
}

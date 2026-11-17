//! The version endpoints. As elsewhere, the role is checked before the body is read and the audit
//! row is written only after the change it records succeeded.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use mongodb::bson::oid::ObjectId;
use serde::Deserialize;
use sync_crdt::Document as CrdtDocument;
use ts_rs::TS;

use super::{creators, summary, validate_label};
use crate::api_types::{Block, Page, RestoreResponse, UserRef, VersionDetail, VersionSummary};
use crate::app::AppState;
use crate::audit::{self, AuditEvent};
use crate::auth::AuthUser;
use crate::db::documents::{self, BlockRecord, DocumentRecord};
use crate::db::versions::{self, NewVersion};
use crate::documents::api_block;
use crate::error::ApiError;
use crate::extract::ApiJson;
use crate::hub::frames::ServerFrame;
use crate::hub::room::{Outgoing, RoomError};
use crate::rbac::{Role, parse_id, require_role};

const DEFAULT_LIMIT: i64 = 20;
const MAX_LIMIT: i64 = 100;

fn document_not_found() -> ApiError {
    ApiError::not_found("document not found")
}

fn version_not_found() -> ApiError {
    ApiError::not_found("version not found")
}

/// The same lookup as the document endpoints: another workspace's document and a missing one are
/// indistinguishable.
pub async fn require_document(
    state: &AppState,
    id: ObjectId,
    user: ObjectId,
    min: Role,
) -> Result<DocumentRecord, ApiError> {
    let record = documents::find(&state.db, id)
        .await?
        .ok_or_else(document_not_found)?;
    require_role(&state.db, record.workspace_id, user, min)
        .await
        .map_err(|error| match error {
            ApiError::NotFound(_) => document_not_found(),
            other => other,
        })?;
    Ok(record)
}

#[derive(Debug, Deserialize, TS)]
#[ts(rename = "SaveVersionRequest")]
pub struct SaveInput {
    pub label: String,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    limit: Option<i64>,
    cursor: Option<String>,
}

/// The current CRDT state: the open room's when there is one (newer than the last snapshot),
/// otherwise the stored snapshot.
pub async fn current_state(state: &AppState, id: ObjectId) -> Result<Vec<u8>, ApiError> {
    if let Some(room) = state.hub.peek_room(id) {
        return Ok(room.state().0);
    }
    let record = documents::find_with_snapshot(&state.db, id)
        .await?
        .ok_or_else(document_not_found)?;
    record
        .crdt_snapshot
        .map(|b| b.bytes)
        .ok_or_else(|| ApiError::internal("a document has no snapshot"))
}

pub async fn save_version(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ApiJson(input): ApiJson<SaveInput>,
) -> Result<Response, ApiError> {
    let id = parse_id(&id)?;
    let record = require_document(&state, id, user.id, Role::Editor).await?;
    let label = validate_label(&input.label)?;
    let bytes = current_state(&state, id).await?;
    let saved = versions::insert_next(
        &state.db,
        NewVersion {
            document_id: id,
            workspace_id: record.workspace_id,
            change_type: versions::MANUAL,
            label: Some(label.clone()),
            restored_from: None,
            created_by: Some(user.id),
            state: &bytes,
        },
        state.config.auto_version_retention,
    )
    .await
    .map_err(version_error)?;
    audit::record(
        &state.db,
        record.workspace_id,
        user.id,
        &AuditEvent::VersionSave {
            document_id: id,
            version_number: saved.version_number,
            label,
        },
    )
    .await?;
    let who = creators(&state.db, std::slice::from_ref(&saved)).await?;
    let body = summary(
        &saved,
        saved.created_by.and_then(|c| who.get(&c).cloned()),
        None,
    );
    if let Some(room) = state.hub.peek_room(id) {
        room.broadcast(Outgoing::Json(
            ServerFrame::VersionCreated(body.clone()).to_text(),
        ));
    }
    Ok((StatusCode::CREATED, Json(body)).into_response())
}

pub async fn list_versions(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Page<VersionSummary>>, ApiError> {
    let id = parse_id(&id)?;
    require_document(&state, id, user.id, Role::Viewer).await?;
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(ApiError::validation([("limit", "must be 1 to 100")]));
    }
    let before = match query.cursor.as_deref() {
        None => None,
        Some(raw) => Some(
            raw.parse::<i32>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| ApiError::validation([("cursor", "not a valid cursor")]))?,
        ),
    };
    // One extra row tells whether there is a next page.
    let mut rows = versions::list(&state.db, id, limit + 1, before).await?;
    let has_more = i64::try_from(rows.len()).unwrap_or(i64::MAX) > limit;
    rows.truncate(usize::try_from(limit).unwrap_or(0));
    let who = creators(&state.db, &rows).await?;
    let restored: Vec<i32> = rows.iter().filter_map(|r| r.restored_from).collect();
    let kept = versions::existing_numbers(&state.db, id, &restored).await?;
    let items = rows
        .iter()
        .map(|r| {
            summary(
                r,
                r.created_by.and_then(|c| who.get(&c).cloned()),
                r.restored_from.map(|n| kept.contains(&n)),
            )
        })
        .collect();
    let next_cursor = has_more
        .then(|| rows.last().map(|r| r.version_number.to_string()))
        .flatten();
    Ok(Json(Page { items, next_cursor }))
}

pub async fn get_version(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, number)): Path<(String, i32)>,
) -> Result<Json<VersionDetail>, ApiError> {
    let id = parse_id(&id)?;
    require_document(&state, id, user.id, Role::Viewer).await?;
    let record = versions::get(&state.db, id, number)
        .await?
        .ok_or_else(version_not_found)?;
    let blocks = blocks_of(&record)?;
    let who = creators(&state.db, std::slice::from_ref(&record)).await?;
    let kept = match record.restored_from {
        Some(n) => Some(
            versions::existing_numbers(&state.db, id, &[n])
                .await?
                .contains(&n),
        ),
        None => None,
    };
    Ok(Json(VersionDetail {
        version: summary(
            &record,
            record.created_by.and_then(|c| who.get(&c).cloned()),
            kept,
        ),
        blocks,
    }))
}

/// The CRDT state bytes of a stored version.
pub fn state_of(record: &versions::VersionRecord) -> Result<Vec<u8>, ApiError> {
    let compressed = record
        .crdt_snapshot
        .as_ref()
        .ok_or_else(|| ApiError::internal("a version has no snapshot"))?;
    versions::decompress(&compressed.bytes)
        .map_err(|_| ApiError::internal("a stored version could not be read"))
}

/// The blocks of a stored version, rebuilt through the same checks as any incoming update.
pub fn blocks_of(record: &versions::VersionRecord) -> Result<Vec<Block>, ApiError> {
    let state = state_of(record)?;
    let replica =
        CrdtDocument::new(2).map_err(|_| ApiError::internal("could not create a replica"))?;
    replica
        .apply_update(&state)
        .map_err(|_| ApiError::internal("a stored version could not be read"))?;
    Ok(replica
        .blocks()
        .iter()
        .filter_map(|b| api_block(&BlockRecord::from(b)))
        .collect())
}

pub fn version_error(error: versions::VersionError) -> ApiError {
    match error {
        versions::VersionError::NoDocument => document_not_found(),
        versions::VersionError::Mongo(e) => e.into(),
        versions::VersionError::Corrupt => ApiError::internal("a version could not be stored"),
    }
}

/// A random replica id for the scratch copy a restore works on: 53 bits, never 0 or 1.
fn scratch_client_id() -> u64 {
    let bits = uuid::Uuid::new_v4().as_u128() as u64 & ((1u64 << 53) - 1);
    bits.max(2)
}

pub async fn restore_version(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, number)): Path<(String, i32)>,
) -> Result<Json<RestoreResponse>, ApiError> {
    let id = parse_id(&id)?;
    let record = require_document(&state, id, user.id, Role::Editor).await?;
    let archived = versions::get(&state.db, id, number)
        .await?
        .ok_or_else(version_not_found)?;
    let target = state_of(&archived)?;

    let room = state
        .hub
        .room(&state.db, id)
        .await
        .map_err(|_| document_not_found())?;
    let before = match room.restore(&target, scratch_client_id()) {
        Ok(before) => before,
        Err(RoomError::TooBig) => return Err(ApiError::PayloadTooLarge),
        Err(RoomError::Crdt(_)) => {
            return Err(ApiError::internal("the version could not be restored"));
        }
    };
    // The RESTORE row holds the content as it was before, so a restore can be undone.
    let saved = versions::insert_next(
        &state.db,
        NewVersion {
            document_id: id,
            workspace_id: record.workspace_id,
            change_type: versions::RESTORE,
            label: None,
            restored_from: Some(number),
            created_by: Some(user.id),
            state: &before,
        },
        state.config.auto_version_retention,
    )
    .await
    .map_err(version_error)?;
    audit::record(
        &state.db,
        record.workspace_id,
        user.id,
        &AuditEvent::VersionRestore {
            document_id: id,
            restored_from: number,
            version_number: saved.version_number,
        },
    )
    .await?;
    // Written at once, so the revision in the answer includes the restore.
    state
        .hub
        .flush_room(&state.db, &room)
        .await
        .map_err(ApiError::from)?;
    let revision = documents::find(&state.db, id)
        .await?
        .map(|d| d.revision)
        .unwrap_or_default();

    let me = UserRef {
        id: user.id.to_hex(),
        name: crate::db::users::find_by_id(&state.db, user.id)
            .await?
            .map(|u| u.name)
            .unwrap_or_default(),
    };
    let body = summary(&saved, Some(me.clone()), Some(true));
    room.broadcast(Outgoing::Json(
        ServerFrame::DocRestored {
            restored_from: number,
            by: me,
            restore_version: body.clone(),
        }
        .to_text(),
    ));
    // Nobody else has it open: write what is left and let the room go.
    state.hub.release(&state.db, &room).await;
    Ok(Json(RestoreResponse {
        restore_version: body,
        revision,
    }))
}

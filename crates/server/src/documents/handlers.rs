//! The five document endpoints (`docs/API.md` section 2, "Documents"). Each handler takes the
//! signed-in user first, checks the role before it reads the body, and writes the audit row only
//! after the change it records has succeeded.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use mongodb::bson::oid::ObjectId;

use super::validation::{CreateInput, PatchInput, validate_create, validate_patch};
use super::{detail, new_record, summary};
use crate::api_types::{Document, DocumentSummary};
use crate::app::AppState;
use crate::audit::{self, AuditEvent};
use crate::auth::AuthUser;
use crate::db::documents::{self, DocumentRecord};
use crate::error::ApiError;
use crate::extract::ApiJson;
use crate::rbac::{Granted, Role, parse_id, require_role};

fn document_not_found() -> ApiError {
    ApiError::not_found("document not found")
}

/// Loads a document and checks the caller's role in its workspace. A document in a workspace the
/// caller is not a member of, and a document that does not exist, give the same 404 with the same
/// message, so the API never reveals which ids exist.
async fn require_document(
    state: &AppState,
    id: ObjectId,
    user: ObjectId,
    min: Role,
) -> Result<(DocumentRecord, Granted), ApiError> {
    let record = documents::find(&state.db, id)
        .await?
        .ok_or_else(document_not_found)?;
    let granted = require_role(&state.db, record.workspace_id, user, min)
        .await
        .map_err(|error| match error {
            ApiError::NotFound(_) => document_not_found(),
            other => other,
        })?;
    Ok((record, granted))
}

pub async fn list_documents(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> Result<Json<Vec<DocumentSummary>>, ApiError> {
    let workspace_id = parse_id(&wid)?;
    require_role(&state.db, workspace_id, user.id, Role::Viewer).await?;
    let records = documents::list_for_workspace(&state.db, workspace_id).await?;
    Ok(Json(records.iter().map(summary).collect()))
}

pub async fn create_document(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    ApiJson(input): ApiJson<CreateInput>,
) -> Result<Response, ApiError> {
    let workspace_id = parse_id(&wid)?;
    require_role(&state.db, workspace_id, user.id, Role::Editor).await?;
    let (title, icon) = validate_create(input)?;
    let record = new_record(workspace_id, user.id, title, icon)?;
    documents::insert(&state.db, &record).await?;
    audit::record(
        &state.db,
        workspace_id,
        user.id,
        &AuditEvent::DocCreate {
            document_id: record.id,
            title: record.title.clone(),
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(detail(&record))).into_response())
}

pub async fn get_document(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Document>, ApiError> {
    let (record, _) = require_document(&state, parse_id(&id)?, user.id, Role::Viewer).await?;
    Ok(Json(detail(&record)))
}

pub async fn patch_document(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ApiJson(input): ApiJson<PatchInput>,
) -> Result<Json<Document>, ApiError> {
    let id = parse_id(&id)?;
    require_document(&state, id, user.id, Role::Editor).await?;
    let (title, icon) = validate_patch(input)?;
    let updated = documents::update_details(&state.db, id, title.as_deref(), icon.as_deref())
        .await?
        .ok_or_else(document_not_found)?;
    Ok(Json(detail(&updated)))
}

pub async fn delete_document(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id = parse_id(&id)?;
    let (record, _) = require_document(&state, id, user.id, Role::Admin).await?;
    if !documents::delete(&state.db, id).await? {
        return Err(document_not_found());
    }
    audit::record(
        &state.db,
        record.workspace_id,
        user.id,
        &AuditEvent::DocDelete {
            document_id: id,
            title: record.title,
        },
    )
    .await?;
    // Everyone connected to the document is sent away with 4004.
    state.hub.close_document(id);
    Ok(StatusCode::NO_CONTENT)
}

//! The workspace and member endpoints (`docs/API.md` section 2, "Workspaces and members").
//!
//! Each handler under `/workspaces/{wid}` takes the signed-in user first (so anonymous callers
//! get 401 whatever else is wrong), then checks the role with [`require_role`], and only then
//! looks at the body. The audit row is written after the change it records succeeded.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use mongodb::bson::DateTime;
use mongodb::bson::oid::ObjectId;
use serde::Serialize;
use ts_rs::TS;

use super::validation::{
    AddMemberInput, CreateInput, PatchInput, RoleInput, validate_create, validate_member_email,
    validate_patch,
};
use crate::app::AppState;
use crate::audit::{self, AuditEvent};
use crate::auth::AuthUser;
use crate::db::users::{self, User};
use crate::db::workspaces::{self, MemberDoc, Workspace, WriteError};
use crate::error::ApiError;
use crate::extract::ApiJson;
use crate::hub::{CLOSE_FORBIDDEN, CLOSE_NORMAL};
use crate::rbac::{Role, parse_id, require_role};

pub(crate) fn iso(date: DateTime) -> String {
    date.try_to_rfc3339_string().unwrap_or_default()
}

/// The `Workspace` of `docs/API.md` section 4, with the caller's role in it.
#[derive(Debug, Serialize, TS)]
#[ts(rename = "Workspace")]
pub struct WorkspaceResponse {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub owner_id: String,
    pub role: Role,
    pub created_at: String,
    pub updated_at: String,
}

impl WorkspaceResponse {
    fn new(workspace: &Workspace, role: Role) -> Self {
        Self {
            id: workspace.id.to_hex(),
            name: workspace.name.clone(),
            slug: workspace.slug.clone(),
            owner_id: workspace.owner_id.to_hex(),
            role,
            created_at: iso(workspace.created_at),
            updated_at: iso(workspace.updated_at),
        }
    }
}

/// The `WorkspaceSummary` of `docs/API.md` section 4: what a card in the workspace picker shows.
#[derive(Debug, Serialize, TS)]
#[ts(rename = "WorkspaceSummary")]
pub struct SummaryResponse {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub role: Role,
    pub is_owner: bool,
    pub member_count: usize,
    pub document_count: u64,
    pub board_count: u64,
}

/// The `Member` of `docs/API.md` section 4.
#[derive(Debug, Serialize, TS)]
#[ts(rename = "Member")]
pub struct MemberResponse {
    pub user_id: String,
    pub name: String,
    pub email: String,
    pub role: Role,
    pub joined_at: String,
    pub is_owner: bool,
}

impl MemberResponse {
    fn new(user: &User, member: &MemberDoc, owner_id: ObjectId) -> Self {
        Self {
            user_id: user.id.to_hex(),
            name: user.name.clone(),
            email: user.email.clone(),
            role: member.role,
            joined_at: iso(member.joined_at),
            is_owner: user.id == owner_id,
        }
    }
}

fn slug_taken() -> ApiError {
    ApiError::conflict("this slug is already taken")
}

fn member_not_found() -> ApiError {
    ApiError::not_found("member not found")
}

impl From<WriteError> for ApiError {
    fn from(error: WriteError) -> Self {
        match error {
            WriteError::DuplicateSlug => slug_taken(),
            WriteError::Database(error) => Self::internal(error),
        }
    }
}

pub async fn list_workspaces(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<SummaryResponse>>, ApiError> {
    let found = workspaces::list_for_user(&state.db, user.id).await?;
    let ids: Vec<ObjectId> = found.iter().map(|workspace| workspace.id).collect();
    let documents = workspaces::count_by_workspace(&state.db, "documents", &ids).await?;
    let boards = workspaces::count_by_workspace(&state.db, "boards", &ids).await?;
    let summaries = found
        .iter()
        .filter_map(|workspace| {
            let role = workspace.role_of(user.id)?;
            Some(SummaryResponse {
                id: workspace.id.to_hex(),
                name: workspace.name.clone(),
                slug: workspace.slug.clone(),
                role,
                is_owner: workspace.owner_id == user.id,
                member_count: workspace.members.len(),
                document_count: documents.get(&workspace.id).copied().unwrap_or(0),
                board_count: boards.get(&workspace.id).copied().unwrap_or(0),
            })
        })
        .collect();
    Ok(Json(summaries))
}

pub async fn create_workspace(
    State(state): State<AppState>,
    user: AuthUser,
    ApiJson(input): ApiJson<CreateInput>,
) -> Result<Response, ApiError> {
    let (name, slug) = validate_create(input)?;
    let workspace = Workspace::new(name, slug, user.id);
    workspaces::insert(&state.db, &workspace).await?;
    Ok((
        StatusCode::CREATED,
        Json(WorkspaceResponse::new(&workspace, Role::Admin)),
    )
        .into_response())
}

pub async fn get_workspace(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> Result<Json<WorkspaceResponse>, ApiError> {
    let granted = require_role(&state.db, parse_id(&wid)?, user.id, Role::Viewer).await?;
    Ok(Json(WorkspaceResponse::new(
        &granted.workspace,
        granted.role,
    )))
}

pub async fn patch_workspace(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    ApiJson(input): ApiJson<PatchInput>,
) -> Result<Json<WorkspaceResponse>, ApiError> {
    let id = parse_id(&wid)?;
    let granted = require_role(&state.db, id, user.id, Role::Admin).await?;
    let (name, slug) = validate_patch(input)?;
    let updated = workspaces::update_details(&state.db, id, name.as_deref(), slug.as_deref())
        .await?
        .ok_or_else(|| ApiError::not_found("workspace not found"))?;
    Ok(Json(WorkspaceResponse::new(&updated, granted.role)))
}

pub async fn list_members(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> Result<Json<Vec<MemberResponse>>, ApiError> {
    let granted = require_role(&state.db, parse_id(&wid)?, user.id, Role::Viewer).await?;
    let workspace = granted.workspace;
    let ids: Vec<ObjectId> = workspace.members.iter().map(|m| m.user_id).collect();
    let accounts = users::find_many(&state.db, &ids).await?;

    let mut members: Vec<MemberResponse> = workspace
        .members
        .iter()
        .filter_map(|member| {
            let account = accounts.iter().find(|u| u.id == member.user_id)?;
            Some(MemberResponse::new(account, member, workspace.owner_id))
        })
        .collect();
    // The owner first, then in the order people joined. The array is appended to, so its own
    // order is the join order even when two people join in the same millisecond; the sort is
    // stable and only lifts the owner.
    members.sort_by_key(|member| !member.is_owner);
    Ok(Json(members))
}

pub async fn add_member(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    ApiJson(input): ApiJson<AddMemberInput>,
) -> Result<Response, ApiError> {
    let id = parse_id(&wid)?;
    let granted = require_role(&state.db, id, user.id, Role::Admin).await?;
    let email = validate_member_email(&input.email)?;
    let account = users::find_by_email(&state.db, &email)
        .await?
        .ok_or_else(|| ApiError::not_found("No SyncSpace account uses this email"))?;

    let member = MemberDoc {
        user_id: account.id,
        role: input.role,
        joined_at: DateTime::now(),
    };
    if !workspaces::add_member(&state.db, id, &member).await? {
        return Err(ApiError::conflict("already a member of this workspace"));
    }
    audit::record(
        &state.db,
        id,
        user.id,
        &AuditEvent::MemberAdd {
            user_id: account.id,
            role: input.role,
        },
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(MemberResponse::new(
            &account,
            &member,
            granted.workspace.owner_id,
        )),
    )
        .into_response())
}

pub async fn change_member_role(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, uid)): Path<(String, String)>,
    ApiJson(input): ApiJson<RoleInput>,
) -> Result<Json<MemberResponse>, ApiError> {
    let id = parse_id(&wid)?;
    let target = parse_id(&uid)?;
    let granted = require_role(&state.db, id, user.id, Role::Admin).await?;
    let workspace = &granted.workspace;
    if target == workspace.owner_id {
        return Err(ApiError::conflict("the owner's role cannot be changed"));
    }
    let current = workspace.member(target).ok_or_else(member_not_found)?;

    let mut member = current.clone();
    if current.role != input.role {
        // One update that also checks the owner and membership, so a request that raced with a
        // removal finds nothing to change.
        let before = workspaces::set_member_role(&state.db, id, target, input.role)
            .await?
            .ok_or_else(member_not_found)?;
        let from = before.role_of(target).ok_or_else(member_not_found)?;
        member.role = input.role;
        if from != input.role {
            audit::record(
                &state.db,
                id,
                user.id,
                &AuditEvent::RoleChange {
                    user_id: target,
                    from,
                    to: input.role,
                },
            )
            .await?;
        }
        // A plain close: the page reconnects and gets the new role (no ROLE_CHANGED frame).
        state
            .hub
            .close_user(id, target, CLOSE_NORMAL, "role changed");
    }

    let account = users::find_by_id(&state.db, target)
        .await?
        .ok_or_else(|| ApiError::internal("a member has no user account"))?;
    Ok(Json(MemberResponse::new(
        &account,
        &member,
        workspace.owner_id,
    )))
}

pub async fn remove_member(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, uid)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let id = parse_id(&wid)?;
    let target = parse_id(&uid)?;
    let granted = require_role(&state.db, id, user.id, Role::Admin).await?;
    if target == granted.workspace.owner_id {
        return Err(ApiError::conflict("the owner cannot be removed"));
    }
    if granted.workspace.member(target).is_none() {
        return Err(member_not_found());
    }

    let before = workspaces::remove_member(&state.db, id, target)
        .await?
        .ok_or_else(member_not_found)?;
    let role = before.role_of(target).ok_or_else(member_not_found)?;
    workspaces::clear_assignments(&state.db, id, target).await?;
    audit::record(
        &state.db,
        id,
        user.id,
        &AuditEvent::MemberRemove {
            user_id: target,
            role,
        },
    )
    .await?;
    state
        .hub
        .close_user(id, target, CLOSE_FORBIDDEN, "removed from the workspace");
    Ok(StatusCode::NO_CONTENT)
}

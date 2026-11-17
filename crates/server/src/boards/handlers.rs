//! The board and task endpoints. As elsewhere, the role is checked before the body is used, and
//! the audit row is written only after the change it records has succeeded.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Bson, DateTime, Document};
use serde::Deserialize;

use super::ordering::position_between;
use super::validation::{
    CreateBoardInput, CreateTaskInput, MoveTaskInput, PatchTaskInput, validate_create_board,
    validate_create_task, validate_patch_task,
};
use super::{board_response, task_response};
use crate::api_types::{Board, Priority, Task};
use crate::app::AppState;
use crate::audit::{self, AuditEvent};
use crate::auth::AuthUser;
use crate::db::boards::{self, BoardRecord};
use crate::db::tasks::{self, TaskRecord};
use crate::error::ApiError;
use crate::extract::ApiJson;
use crate::rbac::{Granted, Role, parse_id, require_role};

fn board_not_found() -> ApiError {
    ApiError::not_found("board not found")
}

fn task_not_found() -> ApiError {
    ApiError::not_found("task not found")
}

/// Loads a board and checks the caller's role in its workspace. A board in a workspace the caller
/// is not a member of and a board that does not exist give the same 404 with the same message.
async fn require_board(
    state: &AppState,
    id: ObjectId,
    user: ObjectId,
    min: Role,
) -> Result<(BoardRecord, Granted), ApiError> {
    let record = boards::find(&state.db, id)
        .await?
        .ok_or_else(board_not_found)?;
    let granted = require_role(&state.db, record.workspace_id, user, min)
        .await
        .map_err(|error| match error {
            ApiError::NotFound(_) => board_not_found(),
            other => other,
        })?;
    Ok((record, granted))
}

/// The same for a task, through its workspace.
async fn require_task(
    state: &AppState,
    id: ObjectId,
    user: ObjectId,
    min: Role,
) -> Result<(TaskRecord, Granted), ApiError> {
    let record = tasks::find(&state.db, id)
        .await?
        .ok_or_else(task_not_found)?;
    let granted = require_role(&state.db, record.workspace_id, user, min)
        .await
        .map_err(|error| match error {
            ApiError::NotFound(_) => task_not_found(),
            other => other,
        })?;
    Ok((record, granted))
}

pub async fn list_boards(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> Result<Json<Vec<Board>>, ApiError> {
    let workspace_id = parse_id(&wid)?;
    require_role(&state.db, workspace_id, user.id, Role::Viewer).await?;
    let records = boards::list_for_workspace(&state.db, workspace_id).await?;
    let ids: Vec<ObjectId> = records.iter().map(|b| b.id).collect();
    let mut counts = boards::task_counts(&state.db, &ids).await?;
    Ok(Json(
        records
            .iter()
            .map(|record| board_response(record, counts.remove(&record.id)))
            .collect(),
    ))
}

pub async fn create_board(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    ApiJson(input): ApiJson<CreateBoardInput>,
) -> Result<Response, ApiError> {
    let workspace_id = parse_id(&wid)?;
    require_role(&state.db, workspace_id, user.id, Role::Editor).await?;
    let name = validate_create_board(input)?;
    let record = BoardRecord {
        id: ObjectId::new(),
        workspace_id,
        name,
        created_by: user.id,
        created_at: DateTime::now(),
    };
    boards::insert(&state.db, &record).await?;
    Ok((StatusCode::CREATED, Json(board_response(&record, None))).into_response())
}

#[derive(Debug, Deserialize)]
pub struct ListTasksQuery {
    assignee: Option<String>,
    priority: Option<String>,
}

pub async fn list_tasks(
    State(state): State<AppState>,
    user: AuthUser,
    Path(bid): Path<String>,
    Query(query): Query<ListTasksQuery>,
) -> Result<Json<Vec<Task>>, ApiError> {
    let (board, _) = require_board(&state, parse_id(&bid)?, user.id, Role::Viewer).await?;
    let mut problems: Vec<(&str, &str)> = Vec::new();
    let assignee = match query.assignee.as_deref() {
        None | Some("") => None,
        Some(raw) => match ObjectId::parse_str(raw) {
            Ok(id) => Some(id),
            Err(_) => {
                problems.push(("assignee", "must be a user id"));
                None
            }
        },
    };
    let priority = match query.priority.as_deref() {
        None | Some("") => None,
        Some(raw) => match Priority::parse(raw) {
            Some(priority) => Some(priority),
            None => {
                problems.push(("priority", "must be LOW, MEDIUM, HIGH or URGENT"));
                None
            }
        },
    };
    if !problems.is_empty() {
        return Err(ApiError::validation(problems));
    }
    let records = tasks::list_for_board(
        &state.db,
        board.id,
        assignee,
        priority.map(Priority::as_str),
    )
    .await?;
    Ok(Json(records.iter().map(task_response).collect()))
}

pub async fn create_task(
    State(state): State<AppState>,
    user: AuthUser,
    Path(bid): Path<String>,
    ApiJson(input): ApiJson<CreateTaskInput>,
) -> Result<Response, ApiError> {
    let (board, granted) = require_board(&state, parse_id(&bid)?, user.id, Role::Editor).await?;
    let new = validate_create_task(input, &granted.workspace)?;
    // At the end of its column.
    let column = tasks::column(&state.db, board.id, new.status.as_str(), None).await?;
    let order = position_between(column.last().map(|slot| slot.order), None).unwrap_or(1.0);
    let now = DateTime::now();
    let record = TaskRecord {
        id: ObjectId::new(),
        workspace_id: board.workspace_id,
        board_id: board.id,
        title: new.title,
        description: new.description,
        status: new.status.as_str().to_string(),
        priority: new.priority.as_str().to_string(),
        assignee_ids: new.assignee_ids,
        tags: new.tags,
        order,
        created_by: user.id,
        created_at: now,
        updated_at: now,
    };
    tasks::insert(&state.db, &record).await?;
    audit::record(
        &state.db,
        record.workspace_id,
        user.id,
        &AuditEvent::TaskCreate {
            task_id: record.id,
            title: record.title.clone(),
            status: record.status.clone(),
            priority: record.priority.clone(),
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(task_response(&record))).into_response())
}

pub async fn patch_task(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ApiJson(input): ApiJson<PatchTaskInput>,
) -> Result<Json<Task>, ApiError> {
    let id = parse_id(&id)?;
    let (_, granted) = require_task(&state, id, user.id, Role::Editor).await?;
    let changes = validate_patch_task(input, &granted.workspace)?;
    let mut set = Document::new();
    if let Some(title) = changes.title {
        set.insert("title", title);
    }
    if let Some(description) = changes.description {
        set.insert("description", description);
    }
    if let Some(priority) = changes.priority {
        set.insert("priority", priority.as_str());
    }
    if let Some(ids) = changes.assignee_ids {
        set.insert(
            "assignee_ids",
            Bson::Array(ids.into_iter().map(Bson::ObjectId).collect()),
        );
    }
    if let Some(tags) = changes.tags {
        set.insert(
            "tags",
            Bson::Array(tags.into_iter().map(Bson::String).collect()),
        );
    }
    let updated = tasks::update_fields(&state.db, id, set)
        .await?
        .ok_or_else(task_not_found)?;
    Ok(Json(task_response(&updated)))
}

/// Finds `hex` among the tasks of the target column; anything else is a 422 on `field`.
fn neighbour(
    column: &[tasks::Slot],
    hex: Option<&str>,
    field: &'static str,
) -> Result<Option<usize>, ApiError> {
    let Some(hex) = hex else {
        return Ok(None);
    };
    let wrong = || ApiError::validation([(field, "must be another task in the target column")]);
    let id = ObjectId::parse_str(hex.trim()).map_err(|_| wrong())?;
    column
        .iter()
        .position(|slot| slot.id == id)
        .map(Some)
        .ok_or_else(wrong)
}

pub async fn move_task(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ApiJson(input): ApiJson<MoveTaskInput>,
) -> Result<Json<Task>, ApiError> {
    let id = parse_id(&id)?;
    let (task, _) = require_task(&state, id, user.id, Role::Editor).await?;
    let to = input.to_status;
    // The target column without the task itself, so a task can be moved inside its own column.
    let column = tasks::column(&state.db, task.board_id, to.as_str(), Some(task.id)).await?;
    let before = neighbour(&column, input.before_id.as_deref(), "before_id")?;
    let after = neighbour(&column, input.after_id.as_deref(), "after_id")?;
    // Where the task goes, as the indexes of the tasks above and below it. A missing one is
    // taken from the column: a task after `before_id` goes just before whatever follows it, and
    // with neither the task goes to the end.
    let (above, below) = match (before, after) {
        (Some(b), Some(a)) if b >= a => {
            return Err(ApiError::validation([(
                "before_id",
                "must come before after_id in the column",
            )]));
        }
        (Some(b), Some(a)) => (Some(b), Some(a)),
        (Some(b), None) => (Some(b), Some(b + 1).filter(|next| *next < column.len())),
        (None, Some(a)) => (a.checked_sub(1), Some(a)),
        (None, None) => (column.len().checked_sub(1), None),
    };
    let mut orders: Vec<f64> = column.iter().map(|slot| slot.order).collect();
    let pick = |orders: &[f64], index: Option<usize>| index.and_then(|i| orders.get(i).copied());
    let mut order = position_between(pick(&orders, above), pick(&orders, below));
    if order.is_none() {
        // The neighbours are too close to split: renumber the column 1.0, 2.0, ... and ask again.
        let ids: Vec<ObjectId> = column.iter().map(|slot| slot.id).collect();
        tasks::renumber(&state.db, &ids).await?;
        orders = (1..=ids.len()).map(|n| n as f64).collect();
        order = position_between(pick(&orders, above), pick(&orders, below));
    }
    let order =
        order.ok_or_else(|| ApiError::internal("no room in a freshly renumbered column"))?;
    let updated = tasks::set_position(&state.db, task.id, to.as_str(), order)
        .await?
        .ok_or_else(task_not_found)?;
    audit::record(
        &state.db,
        task.workspace_id,
        user.id,
        &AuditEvent::TaskMove {
            task_id: task.id,
            from_column: task.status,
            to_column: to.as_str().to_string(),
            order,
        },
    )
    .await?;
    Ok(Json(task_response(&updated)))
}

pub async fn delete_task(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id = parse_id(&id)?;
    let (task, _) = require_task(&state, id, user.id, Role::Editor).await?;
    if !tasks::delete(&state.db, id).await? {
        return Err(task_not_found());
    }
    audit::record(
        &state.db,
        task.workspace_id,
        user.id,
        &AuditEvent::TaskDelete {
            task_id: id,
            title: task.title,
        },
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

//! Boards and tasks (`docs/API.md` section 2, "Boards and tasks"): the REST side of the Kanban
//! board. A board is a named set of five fixed columns; a task sits in one column at a
//! server-computed `order`.
//!
//! Who may do what is decided in [`crate::rbac`]; TASK_CREATE, TASK_MOVE and TASK_DELETE are
//! audited through [`crate::audit`]. Live updates of an open board (`BOARD_EVENT` over a
//! workspace socket) are not built: the web page reloads the tasks after its own changes.

pub mod handlers;
pub mod ordering;
pub mod validation;

use crate::api_types::{Board, Priority, Status, StatusCounts, Task};
use crate::db::boards::BoardRecord;
use crate::db::tasks::TaskRecord;
use crate::workspaces::handlers::iso;

/// A board as the API shows it.
pub fn board_response(record: &BoardRecord, counts: Option<StatusCounts>) -> Board {
    Board {
        id: record.id.to_hex(),
        workspace_id: record.workspace_id.to_hex(),
        name: record.name.clone(),
        task_counts: counts.unwrap_or(StatusCounts {
            backlog: 0,
            todo: 0,
            in_progress: 0,
            review: 0,
            done: 0,
        }),
        created_at: iso(record.created_at),
    }
}

/// A task as the API shows it. A stored status or priority this code does not know (it cannot be
/// written by the API) falls back to the first column and the middle priority instead of failing
/// the whole list.
pub fn task_response(record: &TaskRecord) -> Task {
    Task {
        id: record.id.to_hex(),
        board_id: record.board_id.to_hex(),
        title: record.title.clone(),
        description: record.description.clone(),
        status: Status::parse(&record.status).unwrap_or(Status::Backlog),
        priority: Priority::parse(&record.priority).unwrap_or(Priority::Medium),
        assignee_ids: record.assignee_ids.iter().map(|id| id.to_hex()).collect(),
        tags: record.tags.clone(),
        order: record.order,
        created_at: iso(record.created_at),
        updated_at: iso(record.updated_at),
    }
}

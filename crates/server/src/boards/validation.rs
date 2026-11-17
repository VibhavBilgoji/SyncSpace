//! Input rules for boards and tasks (`docs/DATA_MODEL.md` section 7). Every broken rule is
//! reported at once and lengths count characters.

use mongodb::bson::oid::ObjectId;
use serde::Deserialize;
use ts_rs::TS;

use crate::api_types::{Priority, Status};
use crate::db::workspaces::Workspace;
use crate::error::ApiError;

pub const MAX_BOARD_NAME_CHARS: usize = 60;
pub const MAX_TITLE_CHARS: usize = 200;
pub const MAX_DESCRIPTION_CHARS: usize = 5000;
pub const MAX_TAGS: usize = 10;
pub const MAX_TAG_CHARS: usize = 24;
pub const DEFAULT_STATUS: Status = Status::Backlog;
pub const DEFAULT_PRIORITY: Priority = Priority::Medium;

const NAME_PROBLEM: &str = "must be 1 to 60 characters and contain no control characters";
const TITLE_PROBLEM: &str = "must be 1 to 200 characters and contain no control characters";
const DESCRIPTION_PROBLEM: &str = "must be at most 5000 characters";
const TAGS_PROBLEM: &str = "must be at most 10 tags of 1 to 24 characters each";
const ASSIGNEES_PROBLEM: &str = "must be ids of current members of this workspace";

#[derive(Debug, Deserialize, TS)]
#[ts(rename = "CreateBoardRequest")]
pub struct CreateBoardInput {
    pub name: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(rename = "CreateTaskRequest")]
pub struct CreateTaskInput {
    pub title: String,
    #[ts(optional)]
    pub description: Option<String>,
    #[ts(optional)]
    pub status: Option<Status>,
    #[ts(optional)]
    pub priority: Option<Priority>,
    #[ts(optional)]
    pub assignee_ids: Option<Vec<String>>,
    #[ts(optional)]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(rename = "UpdateTaskRequest")]
pub struct PatchTaskInput {
    #[ts(optional)]
    pub title: Option<String>,
    #[ts(optional)]
    pub description: Option<String>,
    #[ts(optional)]
    pub priority: Option<Priority>,
    #[ts(optional)]
    pub assignee_ids: Option<Vec<String>>,
    #[ts(optional)]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(rename = "MoveTaskRequest")]
pub struct MoveTaskInput {
    pub to_status: Status,
    /// The task that will be directly above the moved one in the target column.
    #[ts(optional)]
    pub before_id: Option<String>,
    /// The task that will be directly below the moved one in the target column.
    #[ts(optional)]
    pub after_id: Option<String>,
}

fn clean_line(raw: &str, max_chars: usize) -> Option<String> {
    let text = raw.trim();
    let length = text.chars().count();
    if length == 0 || length > max_chars || text.chars().any(char::is_control) {
        None
    } else {
        Some(text.to_string())
    }
}

pub fn clean_board_name(raw: &str) -> Option<String> {
    clean_line(raw, MAX_BOARD_NAME_CHARS)
}

pub fn clean_title(raw: &str) -> Option<String> {
    clean_line(raw, MAX_TITLE_CHARS)
}

/// A description may have several lines; other control characters are refused.
pub fn clean_description(raw: &str) -> Option<String> {
    let text = raw.trim();
    if text.chars().count() > MAX_DESCRIPTION_CHARS
        || text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        None
    } else {
        Some(text.to_string())
    }
}

/// Lower-cased, trimmed, without duplicates (first one wins), at most 10 of 1 to 24 characters.
pub fn clean_tags(raw: &[String]) -> Option<Vec<String>> {
    let mut tags: Vec<String> = Vec::new();
    for tag in raw {
        let tag = clean_line(tag, MAX_TAG_CHARS)?.to_lowercase();
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    (tags.len() <= MAX_TAGS).then_some(tags)
}

/// Parses the ids and checks that each is a current member of `workspace`; no duplicates.
pub fn clean_assignees(raw: &[String], workspace: &Workspace) -> Option<Vec<ObjectId>> {
    let mut ids: Vec<ObjectId> = Vec::new();
    for item in raw {
        let id = ObjectId::parse_str(item.trim()).ok()?;
        workspace.member(id)?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Some(ids)
}

pub struct NewTask {
    pub title: String,
    pub description: String,
    pub status: Status,
    pub priority: Priority,
    pub assignee_ids: Vec<ObjectId>,
    pub tags: Vec<String>,
}

pub fn validate_create_board(input: CreateBoardInput) -> Result<String, ApiError> {
    clean_board_name(&input.name).ok_or_else(|| ApiError::validation([("name", NAME_PROBLEM)]))
}

pub fn validate_create_task(
    input: CreateTaskInput,
    workspace: &Workspace,
) -> Result<NewTask, ApiError> {
    let mut problems: Vec<(&str, &str)> = Vec::new();
    let title = clean_title(&input.title);
    if title.is_none() {
        problems.push(("title", TITLE_PROBLEM));
    }
    let description = match input.description.as_deref() {
        None => Some(String::new()),
        Some(raw) => clean_description(raw),
    };
    if description.is_none() {
        problems.push(("description", DESCRIPTION_PROBLEM));
    }
    let tags = match input.tags.as_deref() {
        None => Some(Vec::new()),
        Some(raw) => clean_tags(raw),
    };
    if tags.is_none() {
        problems.push(("tags", TAGS_PROBLEM));
    }
    let assignees = match input.assignee_ids.as_deref() {
        None => Some(Vec::new()),
        Some(raw) => clean_assignees(raw, workspace),
    };
    if assignees.is_none() {
        problems.push(("assignee_ids", ASSIGNEES_PROBLEM));
    }
    match (title, description, tags, assignees) {
        (Some(title), Some(description), Some(tags), Some(assignee_ids)) if problems.is_empty() => {
            Ok(NewTask {
                title,
                description,
                status: input.status.unwrap_or(DEFAULT_STATUS),
                priority: input.priority.unwrap_or(DEFAULT_PRIORITY),
                assignee_ids,
                tags,
            })
        }
        _ => Err(ApiError::validation(problems)),
    }
}

/// The fields to set; at least one must be present and each present one must be valid.
#[derive(Debug, Default)]
pub struct TaskChanges {
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<Priority>,
    pub assignee_ids: Option<Vec<ObjectId>>,
    pub tags: Option<Vec<String>>,
}

pub fn validate_patch_task(
    input: PatchTaskInput,
    workspace: &Workspace,
) -> Result<TaskChanges, ApiError> {
    if input.title.is_none()
        && input.description.is_none()
        && input.priority.is_none()
        && input.assignee_ids.is_none()
        && input.tags.is_none()
    {
        return Err(ApiError::validation([(
            "body",
            "send at least one of title, description, priority, assignee_ids and tags",
        )]));
    }
    let mut problems: Vec<(&str, &str)> = Vec::new();
    let mut changes = TaskChanges {
        priority: input.priority,
        ..TaskChanges::default()
    };
    if let Some(raw) = input.title.as_deref() {
        match clean_title(raw) {
            Some(title) => changes.title = Some(title),
            None => problems.push(("title", TITLE_PROBLEM)),
        }
    }
    if let Some(raw) = input.description.as_deref() {
        match clean_description(raw) {
            Some(text) => changes.description = Some(text),
            None => problems.push(("description", DESCRIPTION_PROBLEM)),
        }
    }
    if let Some(raw) = input.tags.as_deref() {
        match clean_tags(raw) {
            Some(tags) => changes.tags = Some(tags),
            None => problems.push(("tags", TAGS_PROBLEM)),
        }
    }
    if let Some(raw) = input.assignee_ids.as_deref() {
        match clean_assignees(raw, workspace) {
            Some(ids) => changes.assignee_ids = Some(ids),
            None => problems.push(("assignee_ids", ASSIGNEES_PROBLEM)),
        }
    }
    if problems.is_empty() {
        Ok(changes)
    } else {
        Err(ApiError::validation(problems))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rbac::Role;

    fn workspace_with(member: ObjectId) -> Workspace {
        let owner = ObjectId::new();
        let mut workspace = Workspace::new("Team".into(), "team".into(), owner);
        workspace.members.push(crate::db::workspaces::MemberDoc {
            user_id: member,
            role: Role::Editor,
            joined_at: mongodb::bson::DateTime::now(),
        });
        workspace
    }

    #[test]
    fn board_names_are_trimmed_and_limited() {
        assert_eq!(clean_board_name("  Sprint  ").as_deref(), Some("Sprint"));
        assert!(clean_board_name("   ").is_none());
        assert!(clean_board_name(&"x".repeat(61)).is_none());
        assert!(clean_board_name(&"x".repeat(60)).is_some());
        assert!(clean_board_name("two\nlines").is_none());
        // Lengths count characters, not bytes.
        assert!(clean_board_name(&"é".repeat(60)).is_some());
    }

    #[test]
    fn titles_have_200_characters_and_no_control_characters() {
        assert!(clean_title(&"x".repeat(200)).is_some());
        assert!(clean_title(&"x".repeat(201)).is_none());
        assert!(clean_title("").is_none());
        assert!(clean_title("a\u{7}b").is_none());
    }

    #[test]
    fn descriptions_may_be_empty_and_multi_line_but_not_huge() {
        assert_eq!(clean_description("").as_deref(), Some(""));
        assert_eq!(
            clean_description("one\ntwo\tthree").as_deref(),
            Some("one\ntwo\tthree")
        );
        assert!(clean_description(&"x".repeat(5000)).is_some());
        assert!(clean_description(&"x".repeat(5001)).is_none());
        assert!(clean_description("bell\u{7}").is_none());
    }

    #[test]
    fn tags_are_lower_cased_unique_and_limited() {
        let raw = |items: &[&str]| items.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        assert_eq!(
            clean_tags(&raw(&[" Backend ", "API", "backend"])),
            Some(raw(&["backend", "api"]))
        );
        assert!(clean_tags(&raw(&[""])).is_none());
        assert!(clean_tags(&["x".repeat(25)]).is_none());
        let eleven: Vec<String> = (0..11).map(|n| format!("t{n}")).collect();
        assert!(clean_tags(&eleven).is_none());
        assert!(clean_tags(&eleven[..10]).is_some());
    }

    #[test]
    fn assignees_must_be_members_and_are_deduplicated() {
        let member = ObjectId::new();
        let workspace = workspace_with(member);
        let id = member.to_hex();
        assert_eq!(
            clean_assignees(&[id.clone(), id.clone()], &workspace),
            Some(vec![member])
        );
        assert!(clean_assignees(&[ObjectId::new().to_hex()], &workspace).is_none());
        assert!(clean_assignees(&["not an id".to_string()], &workspace).is_none());
        assert_eq!(clean_assignees(&[], &workspace), Some(vec![]));
    }

    #[test]
    fn a_new_task_takes_defaults_and_reports_every_problem_at_once() {
        let workspace = workspace_with(ObjectId::new());
        let task = validate_create_task(
            CreateTaskInput {
                title: " Ship it ".into(),
                description: None,
                status: None,
                priority: None,
                assignee_ids: None,
                tags: None,
            },
            &workspace,
        )
        .unwrap();
        assert_eq!(task.title, "Ship it");
        assert_eq!(task.status, Status::Backlog);
        assert_eq!(task.priority, Priority::Medium);
        assert!(task.assignee_ids.is_empty() && task.tags.is_empty());

        let error = validate_create_task(
            CreateTaskInput {
                title: String::new(),
                description: Some("x".repeat(6000)),
                status: None,
                priority: None,
                assignee_ids: Some(vec!["nope".into()]),
                tags: Some(vec![String::new()]),
            },
            &workspace,
        )
        .err()
        .unwrap();
        let ApiError::Validation(fields) = error else {
            panic!("expected a validation error");
        };
        let names: Vec<&str> = fields.keys().map(String::as_str).collect();
        for field in ["title", "description", "tags", "assignee_ids"] {
            assert!(names.contains(&field), "{field} missing from {names:?}");
        }
    }

    #[test]
    fn a_patch_needs_at_least_one_field() {
        let workspace = workspace_with(ObjectId::new());
        let empty = PatchTaskInput {
            title: None,
            description: None,
            priority: None,
            assignee_ids: None,
            tags: None,
        };
        assert!(validate_patch_task(empty, &workspace).is_err());
        let one = PatchTaskInput {
            title: None,
            description: Some(String::new()),
            priority: Some(Priority::Urgent),
            assignee_ids: None,
            tags: None,
        };
        let changes = validate_patch_task(one, &workspace).unwrap();
        assert_eq!(changes.description.as_deref(), Some(""));
        assert_eq!(changes.priority, Some(Priority::Urgent));
        assert!(changes.title.is_none());
    }
}

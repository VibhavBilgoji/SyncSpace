//! Demo data for the presentation (`cargo run -p server --bin seed`): one workspace with three
//! members (an admin who owns it, an editor and a viewer), three documents with content and
//! saved versions, a board with tasks in every column, and the audit rows those actions leave.
//!
//! Everything goes through the same repositories and audit writer the API uses, so the data looks
//! exactly like data made by people. It refuses to run twice for the same workspace.

use mongodb::Database;
use mongodb::bson::DateTime;
use mongodb::bson::oid::ObjectId;
use sync_crdt::{BlockType, Document as CrdtDocument};

use crate::audit::{self, AuditEvent};
use crate::db::boards::{self, BoardRecord};
use crate::db::documents;
use crate::db::tasks::{self, TaskRecord};
use crate::db::users::{self, InsertError, User};
use crate::db::versions::{self, NewVersion};
use crate::db::workspaces::{self, MemberDoc, Workspace};
use crate::documents::{SERVER_CLIENT_ID, new_block_id, record_from_crdt};
use crate::error::ApiError;
use crate::rbac::Role;
use crate::workspaces::validation::{clean_name, is_valid_slug};

#[derive(Debug, thiserror::Error)]
pub enum SeedError {
    #[error("the workspace name is not valid (1 to 60 characters, no control characters)")]
    InvalidName,
    #[error("the name does not make a valid slug (3 to 40 lower-case letters, digits and hyphens)")]
    InvalidSlug,
    #[error("a workspace with the slug \"{0}\" already exists; nothing was changed")]
    WorkspaceExists(String),
    #[error("could not hash a password: {0}")]
    Password(String),
    #[error("{0}")]
    Api(String),
    #[error("database error: {0}")]
    Database(#[from] mongodb::error::Error),
}

impl From<ApiError> for SeedError {
    fn from(error: ApiError) -> Self {
        Self::Api(format!("{error:?}"))
    }
}

/// What to create.
#[derive(Debug, Clone)]
pub struct SeedOptions {
    pub workspace_name: String,
    /// The password of the three demo accounts that do not exist yet.
    pub password: String,
}

/// One of the demo accounts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeededUser {
    pub name: String,
    pub email: String,
    pub role: Role,
    /// False when the account existed already: its password was left alone.
    pub created: bool,
}

#[derive(Debug, Clone)]
pub struct SeedReport {
    pub workspace_id: ObjectId,
    pub slug: String,
    pub users: Vec<SeededUser>,
    pub documents: usize,
    pub versions: usize,
    pub tasks: usize,
    pub audit_rows: usize,
}

/// The three demo people: the owner and admin, an editor and a viewer.
pub const PEOPLE: [(&str, &str, Role); 3] = [
    ("Vibhav Bilgoji", "vibhav@syncspace.demo", Role::Admin),
    ("Sparsh Pai Bir", "sparsh@syncspace.demo", Role::Editor),
    ("Suyash Berde", "suyash@syncspace.demo", Role::Viewer),
];

/// `Engineering Team` becomes `engineering-team`.
pub fn slug_of(name: &str) -> String {
    let mut slug = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_string()
}

struct Block {
    block_type: BlockType,
    content: &'static str,
    props: Option<&'static str>,
}

const fn block(block_type: BlockType, content: &'static str) -> Block {
    Block {
        block_type,
        content,
        props: None,
    }
}

const fn with(block_type: BlockType, content: &'static str, props: &'static str) -> Block {
    Block {
        block_type,
        content,
        props: Some(props),
    }
}

struct DocSpec {
    title: &'static str,
    icon: &'static str,
    /// The blocks of the first saved version, of the second, and then of the document as it is.
    first: Vec<Block>,
    second: Vec<Block>,
    last: Vec<Block>,
}

fn documents_spec() -> Vec<DocSpec> {
    use BlockType::{Callout, Checklist, Code, Heading1, Heading2, Paragraph};
    vec![
        DocSpec {
            title: "Product Requirements",
            icon: "📋",
            first: vec![
                block(Heading1, "Product Requirements"),
                block(
                    Paragraph,
                    "SyncSpace is a real-time collaborative workspace: documents several people edit at once, a Kanban board and an audit trail.",
                ),
            ],
            second: vec![
                block(Heading2, "Goals"),
                with(
                    Checklist,
                    "Edits from different people merge without losing anyone's text",
                    r#"{"checked":true}"#,
                ),
                with(
                    Checklist,
                    "Viewers can read and preview but never change anything",
                    r#"{"checked":true}"#,
                ),
            ],
            last: vec![
                with(
                    Checklist,
                    "Every important action is written to the audit trail",
                    r#"{"checked":false}"#,
                ),
                with(
                    Callout,
                    "Out of scope: comments, offline mode and sharing outside the workspace.",
                    r#"{"tone":"info"}"#,
                ),
                block(Heading2, "Open questions"),
                block(
                    Paragraph,
                    "Do we keep automatic versions for the next release?",
                ),
            ],
        },
        DocSpec {
            title: "Sprint 12 Planning",
            icon: "🗓️",
            first: vec![
                block(Heading1, "Sprint 12 Planning"),
                block(Paragraph, "Two weeks, five people, one demo on Friday."),
            ],
            second: vec![
                block(Heading2, "Commitments"),
                with(Checklist, "Board with five columns", r#"{"checked":true}"#),
                with(Checklist, "Members and roles screen", r#"{"checked":true}"#),
            ],
            last: vec![
                with(
                    Checklist,
                    "Audit trail with filters",
                    r#"{"checked":false}"#,
                ),
                with(
                    Callout,
                    "Risk: the free Render instance sleeps after 15 minutes, so open the app before the demo.",
                    r#"{"tone":"warning"}"#,
                ),
                block(Heading2, "Retro"),
                block(Paragraph, "What went well, what to change, what to try."),
            ],
        },
        DocSpec {
            title: "Architecture Overview",
            icon: "🏗️",
            first: vec![
                block(Heading1, "Architecture Overview"),
                block(
                    Paragraph,
                    "A Rust server holds each open document in memory and relays changes between the browsers.",
                ),
            ],
            second: vec![
                block(Heading2, "How edits sync"),
                with(
                    Code,
                    "let update = doc.encode_state_as_update();\nbroadcast(update);",
                    r#"{"language":"rust"}"#,
                ),
            ],
            last: vec![
                block(
                    Paragraph,
                    "The server writes the state to MongoDB every few seconds and when the last person leaves.",
                ),
                with(
                    Callout,
                    "Every restore saves a copy of the current content first, so it can be undone.",
                    r#"{"tone":"success"}"#,
                ),
            ],
        },
    ]
}

/// Adds `blocks` after the ones already in `crdt`, with their properties.
fn add_blocks(crdt: &CrdtDocument, blocks: &[Block]) -> Result<(), SeedError> {
    let existing = crdt.blocks().len();
    for (offset, spec) in blocks.iter().enumerate() {
        let id = new_block_id();
        crdt.create_block(
            &id,
            spec.block_type,
            (existing + offset + 1) as f64,
            spec.content,
        )
        .map_err(|e| SeedError::Api(e.to_string()))?;
        if let Some(props) = spec.props {
            crdt.set_props(&id, props)
                .map_err(|e| SeedError::Api(e.to_string()))?;
        }
    }
    Ok(())
}

struct TaskSpec {
    title: &'static str,
    status: &'static str,
    priority: &'static str,
    tags: &'static [&'static str],
    /// Indexes into [`PEOPLE`].
    assignees: &'static [usize],
    /// Who created it.
    by: usize,
}

const TASKS: [TaskSpec; 9] = [
    TaskSpec {
        title: "Research export formats",
        status: "BACKLOG",
        priority: "LOW",
        tags: &["research"],
        assignees: &[],
        by: 0,
    },
    TaskSpec {
        title: "Plan the load test",
        status: "BACKLOG",
        priority: "MEDIUM",
        tags: &["performance"],
        assignees: &[0],
        by: 0,
    },
    TaskSpec {
        title: "Write the API documentation",
        status: "TODO",
        priority: "HIGH",
        tags: &["docs", "api"],
        assignees: &[1],
        by: 1,
    },
    TaskSpec {
        title: "Design the audit screen",
        status: "TODO",
        priority: "MEDIUM",
        tags: &["design"],
        assignees: &[0],
        by: 0,
    },
    TaskSpec {
        title: "Build the board page",
        status: "IN_PROGRESS",
        priority: "HIGH",
        tags: &["frontend"],
        assignees: &[1],
        by: 1,
    },
    TaskSpec {
        title: "Fix the reconnect banner",
        status: "IN_PROGRESS",
        priority: "URGENT",
        tags: &["bug", "frontend"],
        assignees: &[0, 1],
        by: 0,
    },
    TaskSpec {
        title: "Review the version history panel",
        status: "REVIEW",
        priority: "MEDIUM",
        tags: &["review"],
        assignees: &[1],
        by: 1,
    },
    TaskSpec {
        title: "Set up CI",
        status: "DONE",
        priority: "MEDIUM",
        tags: &["infra"],
        assignees: &[0],
        by: 0,
    },
    TaskSpec {
        title: "Deploy the first build",
        status: "DONE",
        priority: "HIGH",
        tags: &["infra"],
        assignees: &[0, 1],
        by: 0,
    },
];

/// Creates the demo workspace and everything in it. Refuses when the slug is taken.
pub async fn seed(db: &Database, options: &SeedOptions) -> Result<SeedReport, SeedError> {
    let name = clean_name(&options.workspace_name).ok_or(SeedError::InvalidName)?;
    let slug = slug_of(&name);
    if !is_valid_slug(&slug) {
        return Err(SeedError::InvalidSlug);
    }
    if workspaces::collection(db)
        .find_one(mongodb::bson::doc! { "slug": &slug })
        .await?
        .is_some()
    {
        return Err(SeedError::WorkspaceExists(slug));
    }

    // The people: new accounts get the demo password, existing ones are kept as they are.
    let mut people: Vec<(ObjectId, SeededUser)> = Vec::new();
    for (person_name, email, role) in PEOPLE {
        let (id, created) = match users::find_by_email(db, email).await? {
            Some(existing) => (existing.id, false),
            None => {
                let user = User {
                    id: ObjectId::new(),
                    email: email.to_string(),
                    name: person_name.to_string(),
                    password_hash: Some(
                        crate::auth::password::hash(&options.password)
                            .map_err(|e| SeedError::Password(e.to_string()))?,
                    ),
                    github_id: None,
                    github_login: None,
                    created_at: DateTime::now(),
                };
                match users::insert(db, &user).await {
                    Ok(()) => {}
                    // Someone registered it a moment ago: use that account.
                    Err(InsertError::DuplicateEmail) => {}
                    Err(InsertError::Database(e)) => return Err(e.into()),
                }
                (user.id, true)
            }
        };
        people.push((
            id,
            SeededUser {
                name: person_name.to_string(),
                email: email.to_string(),
                role,
                created,
            },
        ));
    }
    let owner = people[0].0;
    let ids: Vec<ObjectId> = people.iter().map(|(id, _)| *id).collect();

    // The workspace, with all three members at once.
    let mut workspace = Workspace::new(name, slug.clone(), owner);
    for (id, person) in people.iter().skip(1) {
        workspace.members.push(MemberDoc {
            user_id: *id,
            role: person.role,
            joined_at: DateTime::now(),
        });
    }
    match workspaces::insert(db, &workspace).await {
        Ok(()) => {}
        // Someone created it while the people were being set up.
        Err(workspaces::WriteError::DuplicateSlug) => return Err(SeedError::WorkspaceExists(slug)),
        Err(workspaces::WriteError::Database(error)) => return Err(error.into()),
    }
    let mut audit_rows = 0;
    for (id, person) in people.iter().skip(1) {
        audit::record(
            db,
            workspace.id,
            owner,
            &AuditEvent::MemberAdd {
                user_id: *id,
                role: person.role,
            },
        )
        .await?;
        audit_rows += 1;
    }

    // Documents: each with two saved versions, written by the editor.
    let editor = ids[1];
    let mut versions_written = 0;
    let specs = documents_spec();
    for spec in &specs {
        let crdt =
            CrdtDocument::new(SERVER_CLIENT_ID).map_err(|e| SeedError::Api(e.to_string()))?;
        crdt.initialise();
        add_blocks(&crdt, &spec.first)?;
        let first_state = crdt.encode_state_as_update();
        add_blocks(&crdt, &spec.second)?;
        let second_state = crdt.encode_state_as_update();
        add_blocks(&crdt, &spec.last)?;
        let record = record_from_crdt(
            workspace.id,
            editor,
            spec.title.to_string(),
            spec.icon.to_string(),
            &crdt,
        );
        documents::insert(db, &record).await?;
        audit::record(
            db,
            workspace.id,
            editor,
            &AuditEvent::DocCreate {
                document_id: record.id,
                title: record.title.clone(),
            },
        )
        .await?;
        audit_rows += 1;
        for (label, state) in [
            ("Outline", &first_state),
            ("Ready for review", &second_state),
        ] {
            let saved = versions::insert_next(
                db,
                NewVersion {
                    document_id: record.id,
                    workspace_id: workspace.id,
                    change_type: versions::MANUAL,
                    label: Some(label.to_string()),
                    restored_from: None,
                    created_by: Some(editor),
                    state,
                },
                50,
            )
            .await
            .map_err(|e| SeedError::Api(e.to_string()))?;
            audit::record(
                db,
                workspace.id,
                editor,
                &AuditEvent::VersionSave {
                    document_id: record.id,
                    version_number: saved.version_number,
                    label: label.to_string(),
                },
            )
            .await?;
            versions_written += 1;
            audit_rows += 1;
        }
    }

    // A board with tasks in every column.
    let board = BoardRecord {
        id: ObjectId::new(),
        workspace_id: workspace.id,
        name: "Sprint 12".to_string(),
        created_by: owner,
        created_at: DateTime::now(),
    };
    boards::insert(db, &board).await?;
    let mut per_column: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
    for spec in &TASKS {
        let order = per_column.entry(spec.status).or_insert(0.0);
        *order += 1.0;
        let now = DateTime::now();
        let record = TaskRecord {
            id: ObjectId::new(),
            workspace_id: workspace.id,
            board_id: board.id,
            title: spec.title.to_string(),
            description: String::new(),
            status: spec.status.to_string(),
            priority: spec.priority.to_string(),
            assignee_ids: spec.assignees.iter().map(|i| ids[*i]).collect(),
            tags: spec.tags.iter().map(|t| (*t).to_string()).collect(),
            order: *order,
            created_by: ids[spec.by],
            created_at: now,
            updated_at: now,
        };
        tasks::insert(db, &record).await?;
        // The audit row says where the task started: the first column.
        audit::record(
            db,
            workspace.id,
            ids[spec.by],
            &AuditEvent::TaskCreate {
                task_id: record.id,
                title: record.title.clone(),
                status: "BACKLOG".to_string(),
                priority: record.priority.clone(),
            },
        )
        .await?;
        audit_rows += 1;
        if spec.status != "BACKLOG" {
            audit::record(
                db,
                workspace.id,
                ids[spec.by],
                &AuditEvent::TaskMove {
                    task_id: record.id,
                    from_column: "BACKLOG".to_string(),
                    to_column: spec.status.to_string(),
                    order: record.order,
                },
            )
            .await?;
            audit_rows += 1;
        }
    }

    Ok(SeedReport {
        workspace_id: workspace.id,
        slug,
        users: people.into_iter().map(|(_, person)| person).collect(),
        documents: specs.len(),
        versions: versions_written,
        tasks: TASKS.len(),
        audit_rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_becomes_a_slug() {
        assert_eq!(slug_of("Engineering Team"), "engineering-team");
        assert_eq!(slug_of("  Team  --  A!  "), "team-a");
        assert_eq!(slug_of("Équipe 42"), "quipe-42");
        assert_eq!(slug_of("???"), "");
    }

    #[test]
    fn the_people_are_an_admin_an_editor_and_a_viewer_with_valid_emails() {
        let roles: Vec<Role> = PEOPLE.iter().map(|(_, _, r)| *r).collect();
        assert_eq!(roles, [Role::Admin, Role::Editor, Role::Viewer]);
        for (_, email, _) in PEOPLE {
            assert!(email.is_ascii() && email.contains('@') && email == email.to_lowercase());
        }
    }

    #[test]
    fn every_task_uses_known_columns_priorities_and_people() {
        for task in &TASKS {
            assert!(
                crate::api_types::Status::parse(task.status).is_some(),
                "{}",
                task.title
            );
            assert!(
                crate::api_types::Priority::parse(task.priority).is_some(),
                "{}",
                task.title
            );
            assert!(task.by < PEOPLE.len());
            assert!(task.assignees.iter().all(|i| *i < PEOPLE.len()));
            assert!(task.tags.len() <= 10 && task.tags.iter().all(|t| t.len() <= 24));
        }
        // Every column has at least one task, so the board looks full.
        for status in crate::api_types::Status::ALL {
            assert!(TASKS.iter().any(|t| t.status == status.as_str()));
        }
    }

    #[test]
    fn every_document_builds_and_passes_the_crdt_checks() {
        for spec in documents_spec() {
            let crdt = CrdtDocument::new(SERVER_CLIENT_ID).unwrap();
            crdt.initialise();
            add_blocks(&crdt, &spec.first).unwrap();
            add_blocks(&crdt, &spec.second).unwrap();
            add_blocks(&crdt, &spec.last).unwrap();
            let count = spec.first.len() + spec.second.len() + spec.last.len();
            assert_eq!(crdt.blocks().len(), count, "{}", spec.title);
            let orders: Vec<f64> = crdt.blocks().iter().map(|b| b.order).collect();
            assert!(orders.windows(2).all(|w| w[0] < w[1]), "{}", spec.title);
        }
    }
}

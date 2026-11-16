//! The audit writer: the only code that touches `audit_logs` (`docs/DATA_MODEL.md` section 3,
//! CLAUDE.md rule 5).
//!
//! Rows are only ever inserted. Nothing in the server updates or deletes one, and a test scans
//! the source to keep it that way. Each [`AuditEvent`] maps to one row exactly as the table in
//! `DATA_MODEL.md` says; an action that is not an event here is not audited.

use mongodb::Database;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime, Document, doc};

use crate::error::ApiError;
use crate::rbac::Role;

pub const COLLECTION: &str = "audit_logs";

/// What happened. Variants are added as the features that cause them arrive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEvent {
    /// A user was added to the workspace with `role`.
    MemberAdd { user_id: ObjectId, role: Role },
    /// A user was removed from the workspace; `role` is the one they had.
    MemberRemove { user_id: ObjectId, role: Role },
    /// A member's role changed.
    RoleChange {
        user_id: ObjectId,
        from: Role,
        to: Role,
    },
    /// A document was created.
    DocCreate {
        document_id: ObjectId,
        title: String,
    },
    /// A document was deleted; its title is kept because the document is gone.
    DocDelete {
        document_id: ObjectId,
        title: String,
    },
    /// A named version was saved.
    VersionSave {
        document_id: ObjectId,
        version_number: i32,
        label: String,
    },
    /// A version was restored; `version_number` is the new RESTORE row.
    VersionRestore {
        document_id: ObjectId,
        restored_from: i32,
        version_number: i32,
    },
}

impl AuditEvent {
    pub fn action(&self) -> &'static str {
        match self {
            Self::MemberAdd { .. } => "MEMBER_ADD",
            Self::MemberRemove { .. } => "MEMBER_REMOVE",
            Self::RoleChange { .. } => "ROLE_CHANGE",
            Self::DocCreate { .. } => "DOC_CREATE",
            Self::DocDelete { .. } => "DOC_DELETE",
            Self::VersionSave { .. } => "VERSION_SAVE",
            Self::VersionRestore { .. } => "VERSION_RESTORE",
        }
    }

    pub fn entity_type(&self) -> &'static str {
        match self {
            Self::MemberAdd { .. } | Self::MemberRemove { .. } => "WORKSPACE",
            Self::RoleChange { .. } => "MEMBER",
            Self::DocCreate { .. }
            | Self::DocDelete { .. }
            | Self::VersionSave { .. }
            | Self::VersionRestore { .. } => "DOCUMENT",
        }
    }

    /// The id of the item acted on: the workspace for member add and remove, the member for a
    /// role change.
    pub fn entity_id(&self, workspace_id: ObjectId) -> String {
        match self {
            Self::MemberAdd { .. } | Self::MemberRemove { .. } => workspace_id.to_hex(),
            Self::RoleChange { user_id, .. } => user_id.to_hex(),
            Self::DocCreate { document_id, .. }
            | Self::DocDelete { document_id, .. }
            | Self::VersionSave { document_id, .. }
            | Self::VersionRestore { document_id, .. } => document_id.to_hex(),
        }
    }

    /// The action-specific details (the "state delta").
    pub fn metadata(&self) -> Document {
        match self {
            Self::MemberAdd { user_id, role } => {
                doc! { "user_id": user_id.to_hex(), "assigned": role.as_str() }
            }
            Self::MemberRemove { user_id, role } => {
                doc! { "user_id": user_id.to_hex(), "role": role.as_str() }
            }
            Self::RoleChange { from, to, .. } => {
                doc! { "from": from.as_str(), "to": to.as_str() }
            }
            Self::DocCreate { title, .. } | Self::DocDelete { title, .. } => {
                doc! { "title": title }
            }
            Self::VersionSave {
                version_number,
                label,
                ..
            } => doc! { "version_number": *version_number, "label": label },
            Self::VersionRestore {
                restored_from,
                version_number,
                ..
            } => doc! { "restored_from": *restored_from, "version_number": *version_number },
        }
    }

    /// The row this event writes.
    pub fn to_row(&self, workspace_id: ObjectId, actor_id: ObjectId) -> Document {
        doc! {
            "_id": ObjectId::new(),
            "workspace_id": workspace_id,
            "actor_id": actor_id,
            "action": self.action(),
            "entity_type": self.entity_type(),
            "entity_id": self.entity_id(workspace_id),
            "metadata": self.metadata(),
            "timestamp": DateTime::now(),
        }
    }
}

/// Appends one audit row. Call it after the change it records has succeeded.
pub async fn record(
    db: &Database,
    workspace_id: ObjectId,
    actor_id: ObjectId,
    event: &AuditEvent,
) -> Result<(), ApiError> {
    db.collection::<Document>(COLLECTION)
        .insert_one(event.to_row(workspace_id, actor_id))
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> (ObjectId, ObjectId, ObjectId) {
        (ObjectId::new(), ObjectId::new(), ObjectId::new())
    }

    #[test]
    fn member_add_follows_the_documented_mapping() {
        let (workspace, actor, user) = ids();
        let row = AuditEvent::MemberAdd {
            user_id: user,
            role: Role::Editor,
        }
        .to_row(workspace, actor);
        assert_eq!(row.get_str("action").unwrap(), "MEMBER_ADD");
        assert_eq!(row.get_str("entity_type").unwrap(), "WORKSPACE");
        assert_eq!(row.get_str("entity_id").unwrap(), workspace.to_hex());
        assert_eq!(
            row.get_document("metadata").unwrap(),
            &doc! { "user_id": user.to_hex(), "assigned": "EDITOR" }
        );
        assert_eq!(row.get_object_id("workspace_id").unwrap(), workspace);
        assert_eq!(row.get_object_id("actor_id").unwrap(), actor);
        assert!(row.get_datetime("timestamp").is_ok());
    }

    #[test]
    fn member_remove_follows_the_documented_mapping() {
        let (workspace, actor, user) = ids();
        let row = AuditEvent::MemberRemove {
            user_id: user,
            role: Role::Viewer,
        }
        .to_row(workspace, actor);
        assert_eq!(row.get_str("action").unwrap(), "MEMBER_REMOVE");
        assert_eq!(row.get_str("entity_type").unwrap(), "WORKSPACE");
        assert_eq!(row.get_str("entity_id").unwrap(), workspace.to_hex());
        assert_eq!(
            row.get_document("metadata").unwrap(),
            &doc! { "user_id": user.to_hex(), "role": "VIEWER" }
        );
    }

    #[test]
    fn role_change_follows_the_documented_mapping() {
        let (workspace, actor, user) = ids();
        let row = AuditEvent::RoleChange {
            user_id: user,
            from: Role::Viewer,
            to: Role::Admin,
        }
        .to_row(workspace, actor);
        assert_eq!(row.get_str("action").unwrap(), "ROLE_CHANGE");
        assert_eq!(row.get_str("entity_type").unwrap(), "MEMBER");
        assert_eq!(row.get_str("entity_id").unwrap(), user.to_hex());
        assert_eq!(
            row.get_document("metadata").unwrap(),
            &doc! { "from": "VIEWER", "to": "ADMIN" }
        );
    }

    #[test]
    fn document_events_follow_the_documented_mapping() {
        let (workspace, actor, document) = ids();
        for (event, action) in [
            (
                AuditEvent::DocCreate {
                    document_id: document,
                    title: "Plan".into(),
                },
                "DOC_CREATE",
            ),
            (
                AuditEvent::DocDelete {
                    document_id: document,
                    title: "Plan".into(),
                },
                "DOC_DELETE",
            ),
        ] {
            let row = event.to_row(workspace, actor);
            assert_eq!(row.get_str("action").unwrap(), action);
            assert_eq!(row.get_str("entity_type").unwrap(), "DOCUMENT");
            assert_eq!(row.get_str("entity_id").unwrap(), document.to_hex());
            assert_eq!(
                row.get_document("metadata").unwrap(),
                &doc! { "title": "Plan" }
            );
        }
    }

    #[test]
    fn every_row_gets_its_own_id() {
        let (workspace, actor, user) = ids();
        let event = AuditEvent::MemberAdd {
            user_id: user,
            role: Role::Viewer,
        };
        assert_ne!(
            event.to_row(workspace, actor).get_object_id("_id").unwrap(),
            event.to_row(workspace, actor).get_object_id("_id").unwrap()
        );
    }

    /// Rule 5: audit rows are append-only. Only this file and the index table may name the
    /// collection, and this file may only insert.
    #[test]
    fn only_this_module_writes_audit_logs_and_it_only_inserts() {
        fn visit(dir: &std::path::Path, hits: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    visit(&path, hits);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    if text.contains("audit_logs") || text.contains("audit::COLLECTION") {
                        hits.push(path.file_name().unwrap().to_string_lossy().into_owned());
                    }
                }
            }
        }
        let mut hits = Vec::new();
        visit(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut hits,
        );
        hits.sort();
        assert_eq!(
            hits,
            ["audit.rs", "mod.rs"],
            "who names `audit_logs`: {hits:?}"
        );

        let text = include_str!("audit.rs");
        let code = &text[..text.find("#[cfg(test)]").unwrap()];
        for forbidden in [
            "update_one",
            "update_many",
            "delete_one",
            "delete_many",
            "replace_one",
            "find_one_and",
            "drop(",
        ] {
            assert!(
                !code.contains(forbidden),
                "audit.rs must not call {forbidden}"
            );
        }
    }
}

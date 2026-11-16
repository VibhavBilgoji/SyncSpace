//! Roles and the one place that decides who may touch a workspace (`docs/ARCHITECTURE.md`
//! section 2, `docs/API.md` section 1).
//!
//! Every handler under `/workspaces/:wid` calls [`require_role`] before it does anything else.
//! A caller who is not a member gets the same 404 as for a workspace that does not exist, so the
//! API never reveals which workspaces exist; a member whose role is too low gets 403.

use mongodb::Database;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

use crate::db::workspaces::{self, Workspace};
use crate::error::ApiError;

/// A member's role in one workspace. The order is the order of power: `Viewer < Editor < Admin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Role {
    Viewer,
    Editor,
    Admin,
}

impl Role {
    /// The name used in the API and in MongoDB.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "VIEWER",
            Self::Editor => "EDITOR",
            Self::Admin => "ADMIN",
        }
    }
}

/// Who may call a route: the "Role" column of `docs/API.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Anyone, signed in or not.
    Public,
    /// Any signed-in user.
    User,
    /// A member of the workspace in the path with at least this role.
    Member(Role),
}

/// A workspace the caller may use, and the role they have in it.
#[derive(Debug)]
pub struct Granted {
    pub workspace: Workspace,
    pub role: Role,
}

fn workspace_not_found() -> ApiError {
    ApiError::not_found("workspace not found")
}

/// Loads the workspace and checks that `user` is a member with at least `min`.
///
/// Unknown workspace and non-member are indistinguishable (404). A member below `min` is 403.
pub async fn require_role(
    db: &Database,
    workspace_id: ObjectId,
    user: ObjectId,
    min: Role,
) -> Result<Granted, ApiError> {
    let workspace = workspaces::find(db, workspace_id)
        .await?
        .ok_or_else(workspace_not_found)?;
    let role = workspace.role_of(user).ok_or_else(workspace_not_found)?;
    if role < min {
        return Err(ApiError::forbidden(format!(
            "this needs the {} role",
            min.as_str()
        )));
    }
    Ok(Granted { workspace, role })
}

/// Parses a 24-character hex id from a path. Anything else is a 400.
pub fn parse_id(raw: &str) -> Result<ObjectId, ApiError> {
    ObjectId::parse_str(raw).map_err(|_| ApiError::bad_request("malformed id"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_are_ordered_by_power() {
        assert!(Role::Viewer < Role::Editor);
        assert!(Role::Editor < Role::Admin);
        assert!(Role::Viewer < Role::Admin);
    }

    #[test]
    fn roles_serialise_with_the_api_names() {
        for (role, name) in [
            (Role::Viewer, "VIEWER"),
            (Role::Editor, "EDITOR"),
            (Role::Admin, "ADMIN"),
        ] {
            assert_eq!(role.as_str(), name);
            assert_eq!(serde_json::to_string(&role).unwrap(), format!("\"{name}\""));
            assert_eq!(
                serde_json::from_str::<Role>(&format!("\"{name}\"")).unwrap(),
                role
            );
        }
    }

    #[test]
    fn unknown_or_lower_case_roles_are_rejected() {
        for bad in ["\"admin\"", "\"OWNER\"", "\"\"", "1", "null"] {
            assert!(serde_json::from_str::<Role>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn ids_must_be_24_hex_characters() {
        assert!(parse_id("0123456789abcdef01234567").is_ok());
        for bad in [
            "",
            "xyz",
            "0123456789abcdef0123456",
            "0123456789abcdef012345678",
            "zzzzzzzzzzzzzzzzzzzzzzzz",
        ] {
            let error = parse_id(bad).unwrap_err();
            assert_eq!(error.status().as_u16(), 400, "{bad}");
        }
    }
}

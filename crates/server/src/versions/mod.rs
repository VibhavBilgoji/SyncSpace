//! Document versions over HTTP (`docs/API.md` section 2, "Versions", tasks 4.3 and 4.4).

pub mod handlers;

use mongodb::bson::oid::ObjectId;

use crate::api_types::{ChangeType, UserRef, VersionSummary};
use crate::db::versions::{self, VersionRecord};
use crate::error::ApiError;
use crate::workspaces::handlers::iso;

pub const MAX_LABEL_CHARS: usize = 80;
const LABEL_PROBLEM: &str = "must be 1 to 80 characters and contain no control characters";

/// The label of a manual version: trimmed, 1 to 80 characters, no control characters.
pub fn validate_label(raw: &str) -> Result<String, ApiError> {
    let text = raw.trim();
    let length = text.chars().count();
    if length == 0 || length > MAX_LABEL_CHARS || text.chars().any(char::is_control) {
        return Err(ApiError::validation([("label", LABEL_PROBLEM)]));
    }
    Ok(text.to_string())
}

/// The change type of a stored row; a row with an unknown value is treated as automatic.
fn change_type(record: &VersionRecord) -> ChangeType {
    match record.change_type.as_str() {
        versions::MANUAL => ChangeType::Manual,
        versions::RESTORE => ChangeType::Restore,
        _ => ChangeType::Automatic,
    }
}

/// The `VersionSummary` of a stored row. `creator` is the user who saved it (none for automatic
/// versions); `kept` says whether the restored version still exists (only for restore rows).
pub fn summary(
    record: &VersionRecord,
    creator: Option<UserRef>,
    kept: Option<bool>,
) -> VersionSummary {
    let label = record
        .label
        .clone()
        .or_else(|| match record.change_type.as_str() {
            versions::RESTORE => record
                .restored_from
                .map(|n| format!("Restored from Version {n}")),
            versions::AUTOMATIC => Some("Periodic Snapshot".to_string()),
            _ => None,
        });
    VersionSummary {
        version_number: record.version_number,
        change_type: change_type(record),
        label,
        restored_from: record.restored_from,
        restored_from_kept: record.restored_from.and(kept),
        created_by: creator,
        created_at: iso(record.created_at),
    }
}

/// Resolves who saved each version, one lookup for all of them.
pub async fn creators(
    db: &mongodb::Database,
    records: &[VersionRecord],
) -> Result<std::collections::HashMap<ObjectId, UserRef>, ApiError> {
    let ids: Vec<ObjectId> = records.iter().filter_map(|r| r.created_by).collect();
    let users = crate::db::users::find_many(db, &ids).await?;
    Ok(users
        .into_iter()
        .map(|u| {
            (
                u.id,
                UserRef {
                    id: u.id.to_hex(),
                    name: u.name,
                },
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_trimmed_and_bounded() {
        assert_eq!(
            validate_label("  Before launch  ").unwrap(),
            "Before launch"
        );
        assert!(validate_label("   ").is_err());
        assert!(validate_label("").is_err());
        assert!(validate_label("bad\nlabel").is_err());
        assert!(validate_label(&"x".repeat(80)).is_ok());
        assert!(validate_label(&"x".repeat(81)).is_err());
        assert!(
            validate_label(&"é".repeat(80)).is_ok(),
            "characters, not bytes"
        );
    }
}

//! The `workspaces` collection (`docs/DATA_MODEL.md` section 2) and the counts the workspace
//! picker shows.
//!
//! Membership lives inside the workspace document (ADR-007), so one read gives both the
//! membership and the role. Every membership change is a single update whose filter carries the
//! rule it protects (the owner is never touched, nobody is added twice), so two requests that
//! race cannot break an invariant.

use std::collections::HashMap;

use futures_util::TryStreamExt;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime, Document, doc};
use mongodb::options::ReturnDocument;
use mongodb::{Collection, Database};
use serde::{Deserialize, Serialize};

use super::is_duplicate_key;
use crate::rbac::Role;

pub const COLLECTION: &str = "workspaces";

/// One entry of `workspaces.members`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemberDoc {
    pub user_id: ObjectId,
    pub role: Role,
    pub joined_at: DateTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub name: String,
    pub slug: String,
    pub owner_id: ObjectId,
    pub members: Vec<MemberDoc>,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}

impl Workspace {
    /// A new workspace whose only member is its owner, as an ADMIN (invariant 1).
    pub fn new(name: String, slug: String, owner: ObjectId) -> Self {
        let now = DateTime::now();
        Self {
            id: ObjectId::new(),
            name,
            slug,
            owner_id: owner,
            members: vec![MemberDoc {
                user_id: owner,
                role: Role::Admin,
                joined_at: now,
            }],
            created_at: now,
            updated_at: now,
        }
    }

    pub fn member(&self, user: ObjectId) -> Option<&MemberDoc> {
        self.members.iter().find(|member| member.user_id == user)
    }

    pub fn role_of(&self, user: ObjectId) -> Option<Role> {
        self.member(user).map(|member| member.role)
    }
}

#[derive(Debug)]
pub enum WriteError {
    /// The unique index on `slug` rejected the value.
    DuplicateSlug,
    Database(mongodb::error::Error),
}

impl From<mongodb::error::Error> for WriteError {
    fn from(error: mongodb::error::Error) -> Self {
        if is_duplicate_key(&error) {
            Self::DuplicateSlug
        } else {
            Self::Database(error)
        }
    }
}

pub fn collection(db: &Database) -> Collection<Workspace> {
    db.collection(COLLECTION)
}

pub async fn insert(db: &Database, workspace: &Workspace) -> Result<(), WriteError> {
    collection(db).insert_one(workspace).await?;
    Ok(())
}

pub async fn find(db: &Database, id: ObjectId) -> Result<Option<Workspace>, mongodb::error::Error> {
    collection(db).find_one(doc! { "_id": id }).await
}

/// Every workspace `user` is a member of, by name.
pub async fn list_for_user(
    db: &Database,
    user: ObjectId,
) -> Result<Vec<Workspace>, mongodb::error::Error> {
    let mut found: Vec<Workspace> = collection(db)
        .find(doc! { "members.user_id": user })
        .await?
        .try_collect()
        .await?;
    found.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    Ok(found)
}

/// How many rows of `collection` (`documents` or `boards`) each workspace has. A workspace with
/// none is absent from the map.
pub async fn count_by_workspace(
    db: &Database,
    collection: &str,
    workspaces: &[ObjectId],
) -> Result<HashMap<ObjectId, u64>, mongodb::error::Error> {
    let pipeline = vec![
        doc! { "$match": { "workspace_id": { "$in": workspaces.to_vec() } } },
        doc! { "$group": { "_id": "$workspace_id", "n": { "$sum": 1 } } },
    ];
    let mut cursor = db
        .collection::<Document>(collection)
        .aggregate(pipeline)
        .await?;
    let mut counts = HashMap::new();
    while let Some(row) = cursor.try_next().await? {
        let count = match row.get("n") {
            Some(mongodb::bson::Bson::Int32(n)) => u64::try_from(*n).unwrap_or(0),
            Some(mongodb::bson::Bson::Int64(n)) => u64::try_from(*n).unwrap_or(0),
            _ => 0,
        };
        if let Ok(id) = row.get_object_id("_id") {
            counts.insert(id, count);
        }
    }
    Ok(counts)
}

/// Changes the name and/or slug and returns the updated workspace, or `None` when it is gone.
pub async fn update_details(
    db: &Database,
    id: ObjectId,
    name: Option<&str>,
    slug: Option<&str>,
) -> Result<Option<Workspace>, WriteError> {
    let mut set = doc! { "updated_at": DateTime::now() };
    if let Some(name) = name {
        set.insert("name", name);
    }
    if let Some(slug) = slug {
        set.insert("slug", slug);
    }
    Ok(collection(db)
        .find_one_and_update(doc! { "_id": id }, doc! { "$set": set })
        .return_document(ReturnDocument::After)
        .await?)
}

/// Adds `member` unless that user is already in the workspace. `false` means they were.
pub async fn add_member(
    db: &Database,
    id: ObjectId,
    member: &MemberDoc,
) -> Result<bool, mongodb::error::Error> {
    let entry = doc! {
        "user_id": member.user_id,
        "role": member.role.as_str(),
        "joined_at": member.joined_at,
    };
    let result = collection(db)
        .update_one(
            doc! { "_id": id, "members.user_id": { "$ne": member.user_id } },
            doc! { "$push": { "members": entry } },
        )
        .await?;
    Ok(result.matched_count == 1)
}

/// Sets a member's role and returns the workspace as it was before, or `None` when the user is
/// not a member or is the owner (whose role is locked).
pub async fn set_member_role(
    db: &Database,
    id: ObjectId,
    user: ObjectId,
    role: Role,
) -> Result<Option<Workspace>, mongodb::error::Error> {
    collection(db)
        .find_one_and_update(
            doc! { "_id": id, "owner_id": { "$ne": user }, "members.user_id": user },
            doc! { "$set": { "members.$[member].role": role.as_str() } },
        )
        .array_filters(vec![doc! { "member.user_id": user }])
        .return_document(ReturnDocument::Before)
        .await
}

/// Removes a member and returns the workspace as it was before, or `None` when the user is not
/// a member or is the owner.
pub async fn remove_member(
    db: &Database,
    id: ObjectId,
    user: ObjectId,
) -> Result<Option<Workspace>, mongodb::error::Error> {
    collection(db)
        .find_one_and_update(
            doc! { "_id": id, "owner_id": { "$ne": user }, "members.user_id": user },
            doc! { "$pull": { "members": { "user_id": user } } },
        )
        .return_document(ReturnDocument::Before)
        .await
}

/// Takes `user` off every task of this workspace (`docs/DATA_MODEL.md` invariant 6).
pub async fn clear_assignments(
    db: &Database,
    workspace: ObjectId,
    user: ObjectId,
) -> Result<(), mongodb::error::Error> {
    db.collection::<Document>("tasks")
        .update_many(
            doc! { "workspace_id": workspace, "assignee_ids": user },
            doc! { "$pull": { "assignee_ids": user } },
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_workspace_has_its_owner_as_the_only_admin() {
        let owner = ObjectId::new();
        let workspace = Workspace::new("Team".into(), "team".into(), owner);
        assert_eq!(workspace.owner_id, owner);
        assert_eq!(workspace.members.len(), 1);
        assert_eq!(workspace.role_of(owner), Some(Role::Admin));
        assert_eq!(workspace.role_of(ObjectId::new()), None);
        assert_eq!(workspace.created_at, workspace.updated_at);
    }
}

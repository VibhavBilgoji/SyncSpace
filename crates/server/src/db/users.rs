//! The `users` collection (`docs/DATA_MODEL.md` section 2).

use mongodb::bson::DateTime;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use mongodb::{Collection, Database};
use serde::{Deserialize, Serialize};

use super::is_duplicate_key;

pub const COLLECTION: &str = "users";

/// A user document. `email` is stored lower-cased, which is what makes the unique index on it
/// case-insensitive; `password_hash` is an argon2id PHC string, or absent for an account that
/// only signs in with GitHub (ADR-015). `github_id` and `github_login` are set when a GitHub
/// account is linked; no GitHub token is ever stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub email: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_login: Option<String>,
    pub created_at: DateTime,
}

#[derive(Debug)]
pub enum InsertError {
    /// The unique index on `email` rejected the user.
    DuplicateEmail,
    Database(mongodb::error::Error),
}

/// Why linking a GitHub account to a user failed.
#[derive(Debug)]
pub enum LinkError {
    /// Another user already has this GitHub account.
    AlreadyLinked,
    Database(mongodb::error::Error),
}

pub fn collection(db: &Database) -> Collection<User> {
    db.collection(COLLECTION)
}

pub async fn insert(db: &Database, user: &User) -> Result<(), InsertError> {
    match collection(db).insert_one(user).await {
        Ok(_) => Ok(()),
        Err(error) if is_duplicate_key(&error) => Err(InsertError::DuplicateEmail),
        Err(error) => Err(InsertError::Database(error)),
    }
}

/// `email` must already be normalised (trimmed and lower-cased).
pub async fn find_by_email(
    db: &Database,
    email: &str,
) -> Result<Option<User>, mongodb::error::Error> {
    collection(db).find_one(doc! { "email": email }).await
}

pub async fn find_by_id(
    db: &Database,
    id: ObjectId,
) -> Result<Option<User>, mongodb::error::Error> {
    collection(db).find_one(doc! { "_id": id }).await
}

/// The users with these ids, in no particular order. Ids with no user are simply absent.
pub async fn find_many(
    db: &Database,
    ids: &[ObjectId],
) -> Result<Vec<User>, mongodb::error::Error> {
    use futures_util::TryStreamExt;
    collection(db)
        .find(doc! { "_id": { "$in": ids.to_vec() } })
        .await?
        .try_collect()
        .await
}

pub async fn find_by_github_id(
    db: &Database,
    github_id: i64,
) -> Result<Option<User>, mongodb::error::Error> {
    collection(db)
        .find_one(doc! { "github_id": github_id })
        .await
}

/// Links a GitHub account to `user_id`, or refreshes the login of the link it already has. It
/// changes nothing when the user is already linked to a different GitHub account, and it
/// reports `AlreadyLinked` when another user has this one (the unique index decides a race).
pub async fn link_github(
    db: &Database,
    user_id: ObjectId,
    github_id: i64,
    github_login: &str,
) -> Result<bool, LinkError> {
    let filter = doc! {
        "_id": user_id,
        "$or": [
            { "github_id": { "$exists": false } },
            { "github_id": github_id },
        ],
    };
    let update = doc! { "$set": { "github_id": github_id, "github_login": github_login } };
    match collection(db).update_one(filter, update).await {
        Ok(result) => Ok(result.matched_count == 1),
        Err(error) if is_duplicate_key(&error) => Err(LinkError::AlreadyLinked),
        Err(error) => Err(LinkError::Database(error)),
    }
}

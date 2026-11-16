//! The `users` collection (`docs/DATA_MODEL.md` section 2).

use mongodb::bson::DateTime;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use mongodb::{Collection, Database};
use serde::{Deserialize, Serialize};

use super::is_duplicate_key;

pub const COLLECTION: &str = "users";

/// A user document. `email` is stored lower-cased, which is what makes the unique index on it
/// case-insensitive; `password_hash` is an argon2id PHC string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub email: String,
    pub name: String,
    pub password_hash: String,
    pub created_at: DateTime,
}

#[derive(Debug)]
pub enum InsertError {
    /// The unique index on `email` rejected the user.
    DuplicateEmail,
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

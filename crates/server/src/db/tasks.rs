//! The `tasks` collection (`docs/DATA_MODEL.md` section 7). A task sits in one column of one
//! board; `order` is its position inside that column and only means something next to the other
//! tasks of the same `(board_id, status)`.

use futures_util::TryStreamExt;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime, Document, doc};
use mongodb::options::{FindOneAndUpdateOptions, FindOptions, ReturnDocument};
use mongodb::{Collection, Database};
use serde::{Deserialize, Serialize};

pub const COLLECTION: &str = "tasks";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskRecord {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub workspace_id: ObjectId,
    pub board_id: ObjectId,
    pub title: String,
    pub description: String,
    /// `BACKLOG`, `TODO`, `IN_PROGRESS`, `REVIEW` or `DONE`.
    pub status: String,
    /// `LOW`, `MEDIUM`, `HIGH` or `URGENT`.
    pub priority: String,
    pub assignee_ids: Vec<ObjectId>,
    pub tags: Vec<String>,
    pub order: f64,
    pub created_by: ObjectId,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}

/// One neighbour in a column: just what the position calculation needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    pub id: ObjectId,
    pub order: f64,
}

pub fn collection(db: &Database) -> Collection<TaskRecord> {
    db.collection(COLLECTION)
}

pub async fn insert(db: &Database, record: &TaskRecord) -> Result<(), mongodb::error::Error> {
    collection(db).insert_one(record).await?;
    Ok(())
}

pub async fn find(
    db: &Database,
    id: ObjectId,
) -> Result<Option<TaskRecord>, mongodb::error::Error> {
    collection(db).find_one(doc! { "_id": id }).await
}

pub async fn delete(db: &Database, id: ObjectId) -> Result<bool, mongodb::error::Error> {
    Ok(collection(db)
        .delete_one(doc! { "_id": id })
        .await?
        .deleted_count
        > 0)
}

/// The tasks of a board, optionally only those of one assignee and/or one priority, in column
/// order (BACKLOG first), then by `order`, then by id.
pub async fn list_for_board(
    db: &Database,
    board_id: ObjectId,
    assignee: Option<ObjectId>,
    priority: Option<&str>,
) -> Result<Vec<TaskRecord>, mongodb::error::Error> {
    let mut filter = doc! { "board_id": board_id };
    if let Some(user) = assignee {
        filter.insert("assignee_ids", user);
    }
    if let Some(priority) = priority {
        filter.insert("priority", priority);
    }
    let mut tasks: Vec<TaskRecord> = collection(db).find(filter).await?.try_collect().await?;
    tasks.sort_by(|a, b| {
        let rank = |t: &TaskRecord| {
            crate::api_types::Status::parse(&t.status)
                .map(crate::api_types::Status::rank)
                .unwrap_or(usize::MAX)
        };
        rank(a)
            .cmp(&rank(b))
            .then(a.order.total_cmp(&b.order))
            .then(a.id.cmp(&b.id))
    });
    Ok(tasks)
}

/// The tasks of one column, by `order` then id, leaving out `except` (the task being moved).
pub async fn column(
    db: &Database,
    board_id: ObjectId,
    status: &str,
    except: Option<ObjectId>,
) -> Result<Vec<Slot>, mongodb::error::Error> {
    let mut filter = doc! { "board_id": board_id, "status": status };
    if let Some(id) = except {
        filter.insert("_id", doc! { "$ne": id });
    }
    let options = FindOptions::builder()
        .projection(doc! { "order": 1 })
        .sort(doc! { "order": 1, "_id": 1 })
        .build();
    let rows: Vec<Document> = db
        .collection::<Document>(COLLECTION)
        .find(filter)
        .with_options(options)
        .await?
        .try_collect()
        .await?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            Some(Slot {
                id: row.get_object_id("_id").ok()?,
                order: row.get_f64("order").ok()?,
            })
        })
        .collect())
}

/// Sets the given fields and `updated_at`, and returns the task as it is now (`None` when gone).
pub async fn update_fields(
    db: &Database,
    id: ObjectId,
    mut set: Document,
) -> Result<Option<TaskRecord>, mongodb::error::Error> {
    set.insert("updated_at", DateTime::now());
    let options = FindOneAndUpdateOptions::builder()
        .return_document(ReturnDocument::After)
        .build();
    collection(db)
        .find_one_and_update(doc! { "_id": id }, doc! { "$set": set })
        .with_options(options)
        .await
}

/// Puts a task into a column at `order`.
pub async fn set_position(
    db: &Database,
    id: ObjectId,
    status: &str,
    order: f64,
) -> Result<Option<TaskRecord>, mongodb::error::Error> {
    update_fields(db, id, doc! { "status": status, "order": order }).await
}

/// Renumbers a column 1.0, 2.0, ... in the given order (`docs/API.md`, move rule).
pub async fn renumber(db: &Database, ids: &[ObjectId]) -> Result<(), mongodb::error::Error> {
    let tasks = db.collection::<Document>(COLLECTION);
    for (index, id) in ids.iter().enumerate() {
        tasks
            .update_one(
                doc! { "_id": id },
                doc! { "$set": { "order": (index + 1) as f64 } },
            )
            .await?;
    }
    Ok(())
}

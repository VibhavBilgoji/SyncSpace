//! The `boards` collection (`docs/DATA_MODEL.md` section 7). A board is a name in a workspace;
//! its tasks live in `tasks` and are counted per column when boards are listed.

use std::collections::HashMap;

use futures_util::TryStreamExt;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime, Document, doc};
use mongodb::options::FindOptions;
use mongodb::{Collection, Database};
use serde::{Deserialize, Serialize};

use crate::api_types::{Status, StatusCounts};

pub const COLLECTION: &str = "boards";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoardRecord {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub workspace_id: ObjectId,
    pub name: String,
    pub created_by: ObjectId,
    pub created_at: DateTime,
}

pub fn collection(db: &Database) -> Collection<BoardRecord> {
    db.collection(COLLECTION)
}

pub async fn insert(db: &Database, record: &BoardRecord) -> Result<(), mongodb::error::Error> {
    collection(db).insert_one(record).await?;
    Ok(())
}

pub async fn find(
    db: &Database,
    id: ObjectId,
) -> Result<Option<BoardRecord>, mongodb::error::Error> {
    collection(db).find_one(doc! { "_id": id }).await
}

/// The boards of a workspace, oldest first (the order they were made in).
pub async fn list_for_workspace(
    db: &Database,
    workspace_id: ObjectId,
) -> Result<Vec<BoardRecord>, mongodb::error::Error> {
    let options = FindOptions::builder()
        .sort(doc! { "created_at": 1, "_id": 1 })
        .build();
    collection(db)
        .find(doc! { "workspace_id": workspace_id })
        .with_options(options)
        .await?
        .try_collect()
        .await
}

/// How many tasks each of `boards` has in each column, in one grouped query. A board without
/// tasks is absent from the map.
pub async fn task_counts(
    db: &Database,
    boards: &[ObjectId],
) -> Result<HashMap<ObjectId, StatusCounts>, mongodb::error::Error> {
    let mut counts: HashMap<ObjectId, StatusCounts> = HashMap::new();
    if boards.is_empty() {
        return Ok(counts);
    }
    let pipeline = vec![
        doc! { "$match": { "board_id": { "$in": boards } } },
        doc! { "$group": { "_id": { "board": "$board_id", "status": "$status" }, "n": { "$sum": 1 } } },
    ];
    let mut cursor = db
        .collection::<Document>(super::tasks::COLLECTION)
        .aggregate(pipeline)
        .await?;
    while let Some(row) = cursor.try_next().await? {
        let Ok(key) = row.get_document("_id") else {
            continue;
        };
        let (Ok(board), Ok(status)) = (key.get_object_id("board"), key.get_str("status")) else {
            continue;
        };
        let n = row
            .get_i32("n")
            .ok()
            .map(i64::from)
            .or_else(|| row.get_i64("n").ok())
            .unwrap_or(0);
        let n = u32::try_from(n).unwrap_or(0);
        let entry = counts.entry(board).or_insert(StatusCounts {
            backlog: 0,
            todo: 0,
            in_progress: 0,
            review: 0,
            done: 0,
        });
        match Status::parse(status) {
            Some(Status::Backlog) => entry.backlog = n,
            Some(Status::Todo) => entry.todo = n,
            Some(Status::InProgress) => entry.in_progress = n,
            Some(Status::Review) => entry.review = n,
            Some(Status::Done) => entry.done = n,
            None => {}
        }
    }
    Ok(counts)
}

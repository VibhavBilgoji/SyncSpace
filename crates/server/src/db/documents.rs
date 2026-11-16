//! The `documents` collection (`docs/DATA_MODEL.md` section 2).
//!
//! The CRDT snapshot is the source of truth for content; `blocks` is a projection of it that is
//! rewritten with every snapshot write (task 2.4), used for lists, previews and search. Reads
//! that do not need the snapshot leave it out, because it can be large.

use futures_util::TryStreamExt;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Binary, Bson, DateTime, Document, doc};
use mongodb::options::{FindOneAndUpdateOptions, FindOptions, ReturnDocument};
use mongodb::{Collection, Database};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const COLLECTION: &str = "documents";
/// Where the versions of a document live; deleting a document deletes them (invariant 8).
const VERSIONS_COLLECTION: &str = "document_versions";

/// One entry of `documents.blocks`, as `docs/DATA_MODEL.md` says: a projection of the snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockRecord {
    pub block_id: String,
    #[serde(rename = "type")]
    pub block_type: String,
    pub content: String,
    pub properties: Document,
    pub order: f64,
}

impl From<&sync_crdt::Block> for BlockRecord {
    fn from(block: &sync_crdt::Block) -> Self {
        let mut properties = Document::new();
        for (key, value) in &block.properties {
            match value {
                Value::String(text) => properties.insert(key.clone(), text.clone()),
                Value::Bool(flag) => properties.insert(key.clone(), *flag),
                _ => None,
            };
        }
        Self {
            block_id: block.block_id.clone(),
            block_type: block.block_type.as_str().to_string(),
            content: block.content.clone(),
            properties,
            order: block.order,
        }
    }
}

impl BlockRecord {
    /// The properties as a JSON object.
    pub fn properties_json(&self) -> Map<String, Value> {
        self.properties
            .iter()
            .filter_map(|(key, value)| match value {
                Bson::String(text) => Some((key.clone(), Value::String(text.clone()))),
                Bson::Boolean(flag) => Some((key.clone(), Value::Bool(*flag))),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentRecord {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub workspace_id: ObjectId,
    pub title: String,
    pub icon: String,
    pub revision: i64,
    /// Left out by reads that do not ask for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crdt_snapshot: Option<Binary>,
    #[serde(default)]
    pub blocks: Vec<BlockRecord>,
    pub last_version_at: Option<DateTime>,
    pub next_version: i32,
    pub changed_since_version: bool,
    pub created_by: ObjectId,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}

pub fn collection(db: &Database) -> Collection<DocumentRecord> {
    db.collection(COLLECTION)
}

pub async fn insert(db: &Database, record: &DocumentRecord) -> Result<(), mongodb::error::Error> {
    collection(db).insert_one(record).await?;
    Ok(())
}

/// What a read leaves out: the snapshot always, and the blocks for lists.
fn without_snapshot() -> Document {
    doc! { "crdt_snapshot": 0 }
}

/// The document without its snapshot.
pub async fn find(
    db: &Database,
    id: ObjectId,
) -> Result<Option<DocumentRecord>, mongodb::error::Error> {
    collection(db)
        .find_one(doc! { "_id": id })
        .projection(without_snapshot())
        .await
}

/// The document with its snapshot, for the hub.
pub async fn find_with_snapshot(
    db: &Database,
    id: ObjectId,
) -> Result<Option<DocumentRecord>, mongodb::error::Error> {
    collection(db).find_one(doc! { "_id": id }).await
}

/// The documents of a workspace, most recently changed first, without snapshots and blocks.
pub async fn list_for_workspace(
    db: &Database,
    workspace_id: ObjectId,
) -> Result<Vec<DocumentRecord>, mongodb::error::Error> {
    let options = FindOptions::builder()
        .projection(doc! { "crdt_snapshot": 0, "blocks": 0 })
        .sort(doc! { "updated_at": -1, "_id": -1 })
        .build();
    collection(db)
        .find(doc! { "workspace_id": workspace_id })
        .with_options(options)
        .await?
        .try_collect()
        .await
}

/// Changes the title and/or icon and returns the updated document (without its snapshot), or
/// `None` when it is gone.
pub async fn update_details(
    db: &Database,
    id: ObjectId,
    title: Option<&str>,
    icon: Option<&str>,
) -> Result<Option<DocumentRecord>, mongodb::error::Error> {
    let mut set = doc! { "updated_at": DateTime::now() };
    if let Some(title) = title {
        set.insert("title", title);
    }
    if let Some(icon) = icon {
        set.insert("icon", icon);
    }
    let options = FindOneAndUpdateOptions::builder()
        .return_document(ReturnDocument::After)
        .projection(without_snapshot())
        .build();
    collection(db)
        .find_one_and_update(doc! { "_id": id }, doc! { "$set": set })
        .with_options(options)
        .await
}

/// Deletes a document and its versions. `false` when it did not exist.
pub async fn delete(db: &Database, id: ObjectId) -> Result<bool, mongodb::error::Error> {
    let result = collection(db).delete_one(doc! { "_id": id }).await?;
    if result.deleted_count == 0 {
        return Ok(false);
    }
    db.collection::<Document>(VERSIONS_COLLECTION)
        .delete_many(doc! { "document_id": id })
        .await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sync_crdt::{BlockType, Document as CrdtDocument};

    #[test]
    fn a_block_converts_to_a_record_and_back_to_the_same_properties() {
        let doc = CrdtDocument::new(1).unwrap();
        doc.create_block("blk000000001", BlockType::Code, 2.5, "fn main() {}")
            .unwrap();
        doc.set_props("blk000000001", r#"{"language": "rust"}"#)
            .unwrap();
        doc.create_block("blk000000002", BlockType::Checklist, 3.0, "todo")
            .unwrap();
        doc.set_props("blk000000002", r#"{"checked": true}"#)
            .unwrap();

        let records: Vec<BlockRecord> = doc.blocks().iter().map(BlockRecord::from).collect();
        assert_eq!(records[0].block_type, "CODE");
        assert_eq!(records[0].order, 2.5);
        assert_eq!(
            Value::Object(records[0].properties_json()),
            json!({"language": "rust"})
        );
        assert_eq!(
            Value::Object(records[1].properties_json()),
            json!({"checked": true})
        );
    }
}

//! The `document_versions` collection (`docs/DATA_MODEL.md` section 2, task 4.1).
//!
//! A version is a zstd-compressed copy of the document's CRDT state. Numbers are 1, 2, 3 ... per
//! document and are handed out by a counter on the document (`documents.next_version`) with one
//! atomic `$inc`, so two saves at once never get the same number (invariant 4). Automatic
//! versions are pruned to a fixed count after every write (invariant 5); manual and restore
//! versions are never pruned.

use futures_util::TryStreamExt;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::spec::BinarySubtype;
use mongodb::bson::{Binary, DateTime, Document, doc};
use mongodb::options::{FindOneAndUpdateOptions, FindOptions, ReturnDocument};
use mongodb::{Collection, Database};
use serde::{Deserialize, Serialize};

use super::documents;

const COLLECTION: &str = "document_versions";
/// zstd level: states are small and saves are rare, so a middle level is plenty.
const LEVEL: i32 = 3;
/// A stored version never decompresses to more than this (the document cap plus headroom), so a
/// corrupt or hostile row cannot exhaust memory.
pub const MAX_STATE_BYTES: usize = 2 * 1024 * 1024;

pub const AUTOMATIC: &str = "AUTOMATIC";
pub const MANUAL: &str = "MANUAL";
pub const RESTORE: &str = "RESTORE";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VersionRecord {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub document_id: ObjectId,
    pub workspace_id: ObjectId,
    pub version_number: i32,
    pub change_type: String,
    pub label: Option<String>,
    pub restored_from: Option<i32>,
    pub created_by: Option<ObjectId>,
    pub created_at: DateTime,
    /// zstd-compressed `encode_state_as_update_v1`. Absent in list reads.
    #[serde(default)]
    pub crdt_snapshot: Option<Binary>,
    /// Compressed size, for the storage budget.
    pub size_bytes: i32,
}

/// What a caller supplies for a new version.
pub struct NewVersion<'a> {
    pub document_id: ObjectId,
    pub workspace_id: ObjectId,
    pub change_type: &'static str,
    pub label: Option<String>,
    pub restored_from: Option<i32>,
    pub created_by: Option<ObjectId>,
    /// The uncompressed CRDT state.
    pub state: &'a [u8],
}

#[derive(Debug, thiserror::Error)]
pub enum VersionError {
    #[error("the document does not exist")]
    NoDocument,
    #[error("the stored state could not be compressed or decompressed")]
    Corrupt,
    #[error(transparent)]
    Mongo(#[from] mongodb::error::Error),
}

fn collection(db: &Database) -> Collection<VersionRecord> {
    db.collection(COLLECTION)
}

pub fn compress(state: &[u8]) -> Result<Vec<u8>, VersionError> {
    zstd::bulk::compress(state, LEVEL).map_err(|_| VersionError::Corrupt)
}

/// The state of a stored version. Bounded: a row that would expand past [`MAX_STATE_BYTES`] is
/// refused, whatever it claims about itself.
pub fn decompress(compressed: &[u8]) -> Result<Vec<u8>, VersionError> {
    zstd::bulk::decompress(compressed, MAX_STATE_BYTES).map_err(|_| VersionError::Corrupt)
}

/// Writes a version and returns it (without the snapshot bytes). For `AUTOMATIC` versions the
/// oldest ones beyond `retention` are deleted afterwards.
pub async fn insert_next(
    db: &Database,
    new: NewVersion<'_>,
    retention: u32,
) -> Result<VersionRecord, VersionError> {
    // The number is claimed first, atomically. A crash after this leaves a gap in the numbers,
    // never a duplicate; the history only needs numbers to be unique and increasing.
    let options = FindOneAndUpdateOptions::builder()
        .return_document(ReturnDocument::Before)
        .projection(doc! { "next_version": 1 })
        .build();
    let before = db
        .collection::<Document>(documents::COLLECTION)
        .find_one_and_update(
            doc! { "_id": new.document_id },
            doc! { "$inc": { "next_version": 1_i32 } },
        )
        .with_options(options)
        .await?
        .ok_or(VersionError::NoDocument)?;
    let number = before
        .get_i32("next_version")
        .map_err(|_| VersionError::Corrupt)?;
    let compressed = compress(new.state)?;
    let size_bytes = i32::try_from(compressed.len()).map_err(|_| VersionError::Corrupt)?;
    let record = VersionRecord {
        id: ObjectId::new(),
        document_id: new.document_id,
        workspace_id: new.workspace_id,
        version_number: number,
        change_type: new.change_type.to_string(),
        label: new.label,
        restored_from: new.restored_from,
        created_by: new.created_by,
        created_at: DateTime::now(),
        crdt_snapshot: Some(Binary {
            subtype: BinarySubtype::Generic,
            bytes: compressed,
        }),
        size_bytes,
    };
    collection(db).insert_one(&record).await?;
    if new.change_type == AUTOMATIC {
        prune_automatic(db, new.document_id, retention).await?;
    }
    Ok(VersionRecord {
        crdt_snapshot: None,
        ..record
    })
}

/// Deletes automatic versions beyond the newest `keep`.
pub async fn prune_automatic(
    db: &Database,
    document_id: ObjectId,
    keep: u32,
) -> Result<u64, mongodb::error::Error> {
    let options = FindOptions::builder()
        .sort(doc! { "version_number": -1 })
        .skip(u64::from(keep))
        .projection(doc! { "_id": 1 })
        .build();
    let stale: Vec<ObjectId> = db
        .collection::<Document>(COLLECTION)
        .find(doc! { "document_id": document_id, "change_type": AUTOMATIC })
        .with_options(options)
        .await?
        .try_collect::<Vec<_>>()
        .await?
        .into_iter()
        .filter_map(|d| d.get_object_id("_id").ok())
        .collect();
    if stale.is_empty() {
        return Ok(0);
    }
    let result = db
        .collection::<Document>(COLLECTION)
        .delete_many(doc! { "_id": { "$in": stale } })
        .await?;
    Ok(result.deleted_count)
}

/// One page of versions, newest first, without snapshots. `before` is the number of the last
/// version of the previous page.
pub async fn list(
    db: &Database,
    document_id: ObjectId,
    limit: i64,
    before: Option<i32>,
) -> Result<Vec<VersionRecord>, mongodb::error::Error> {
    let mut filter = doc! { "document_id": document_id };
    if let Some(number) = before {
        filter.insert("version_number", doc! { "$lt": number });
    }
    let options = FindOptions::builder()
        .sort(doc! { "version_number": -1 })
        .limit(limit)
        .projection(doc! { "crdt_snapshot": 0 })
        .build();
    collection(db)
        .find(filter)
        .with_options(options)
        .await?
        .try_collect()
        .await
}

/// One version with its compressed snapshot.
pub async fn get(
    db: &Database,
    document_id: ObjectId,
    number: i32,
) -> Result<Option<VersionRecord>, mongodb::error::Error> {
    collection(db)
        .find_one(doc! { "document_id": document_id, "version_number": number })
        .await
}

/// Which of `numbers` still exist (for `restored_from_kept`).
pub async fn existing_numbers(
    db: &Database,
    document_id: ObjectId,
    numbers: &[i32],
) -> Result<Vec<i32>, mongodb::error::Error> {
    if numbers.is_empty() {
        return Ok(Vec::new());
    }
    let options = FindOptions::builder()
        .projection(doc! { "version_number": 1 })
        .build();
    Ok(db
        .collection::<Document>(COLLECTION)
        .find(doc! { "document_id": document_id, "version_number": { "$in": numbers.to_vec() } })
        .with_options(options)
        .await?
        .try_collect::<Vec<_>>()
        .await?
        .into_iter()
        .filter_map(|d| d.get_i32("version_number").ok())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compression_round_trips_and_shrinks_repetitive_state() {
        let state = vec![7u8; 50_000];
        let packed = compress(&state).unwrap();
        assert!(packed.len() < 1_000);
        assert_eq!(decompress(&packed).unwrap(), state);
    }

    #[test]
    fn decompression_refuses_garbage_and_bombs() {
        assert!(decompress(&[1, 2, 3, 4]).is_err());
        let bomb = compress(&vec![0u8; MAX_STATE_BYTES + 1]).unwrap();
        assert!(bomb.len() < 1_000);
        assert!(decompress(&bomb).is_err(), "must not expand past the cap");
    }
}

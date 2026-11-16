//! Documents (`docs/API.md` section 2, "Documents"): the REST side. The live editing side is the
//! hub (task 2.3); content is the CRDT snapshot of `docs/DATA_MODEL.md` section 2.
//!
//! Who may do what is decided in [`crate::rbac`]; DOC_CREATE and DOC_DELETE are audited through
//! [`crate::audit`].

pub mod handlers;
pub mod validation;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::spec::BinarySubtype;
use mongodb::bson::{Binary, DateTime};
use sync_crdt::{BlockType, Document as CrdtDocument};

use crate::api_types::{self, Document, DocumentSummary};
use crate::db::documents::{BlockRecord, DocumentRecord};
use crate::error::ApiError;
use crate::workspaces::handlers::iso;

/// The CRDT client id of the server's own replica when it creates a document. Browsers pick random
/// 53-bit ids, so a collision with this one is astronomically unlikely, and the server writes
/// nothing else to a document after creating it (all later edits are the clients').
pub const SERVER_CLIENT_ID: u64 = 1;

/// A new random block id: 12 letters or digits (`docs/DATA_MODEL.md` section 6).
pub fn new_block_id() -> String {
    uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(12)
        .collect()
}

/// A new document with `title` and `icon`: the CRDT is initialised and holds one empty paragraph,
/// so the editor can be used at once. Nothing is stored yet.
pub fn new_record(
    workspace_id: ObjectId,
    created_by: ObjectId,
    title: String,
    icon: String,
) -> Result<DocumentRecord, ApiError> {
    let crdt = CrdtDocument::new(SERVER_CLIENT_ID).map_err(ApiError::internal)?;
    crdt.initialise();
    crdt.create_block(&new_block_id(), BlockType::Paragraph, 1.0, "")
        .map_err(ApiError::internal)?;
    let now = DateTime::now();
    Ok(DocumentRecord {
        id: ObjectId::new(),
        workspace_id,
        title,
        icon,
        revision: 0,
        crdt_snapshot: Some(Binary {
            subtype: BinarySubtype::Generic,
            bytes: crdt.encode_state_as_update(),
        }),
        blocks: crdt.blocks().iter().map(BlockRecord::from).collect(),
        last_version_at: None,
        next_version: 1,
        changed_since_version: false,
        created_by,
        created_at: now,
        updated_at: now,
    })
}

/// The `DocumentSummary` of `docs/API.md` section 4.
pub fn summary(record: &DocumentRecord) -> DocumentSummary {
    DocumentSummary {
        id: record.id.to_hex(),
        title: record.title.clone(),
        icon: record.icon.clone(),
        revision: record.revision,
        updated_at: iso(record.updated_at),
    }
}

/// One stored block as the API shows it; a block with an unknown type (which the CRDT checks keep
/// out) is left out rather than failing the whole document.
fn api_block(record: &BlockRecord) -> Option<api_types::Block> {
    let block_type =
        serde_json::from_value(serde_json::Value::String(record.block_type.clone())).ok()?;
    Some(api_types::Block {
        block_id: record.block_id.clone(),
        block_type,
        content: record.content.clone(),
        properties: serde_json::Value::Object(record.properties_json()),
        order: record.order,
    })
}

/// The `Document` of `docs/API.md` section 4: the blocks as of the last snapshot, never the CRDT.
pub fn detail(record: &DocumentRecord) -> Document {
    Document {
        id: record.id.to_hex(),
        workspace_id: record.workspace_id.to_hex(),
        title: record.title.clone(),
        icon: record.icon.clone(),
        revision: record.revision,
        blocks: record.blocks.iter().filter_map(api_block).collect(),
        created_by: record.created_by.to_hex(),
        created_at: iso(record.created_at),
        updated_at: iso(record.updated_at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::BlockType as ApiBlockType;

    fn record() -> DocumentRecord {
        new_record(ObjectId::new(), ObjectId::new(), "Plan".into(), "📄".into()).unwrap()
    }

    #[test]
    fn block_ids_are_twelve_letters_or_digits_and_differ() {
        let first = new_block_id();
        assert_eq!(first.len(), 12);
        assert!(first.bytes().all(|b| b.is_ascii_alphanumeric()));
        assert_ne!(first, new_block_id());
    }

    #[test]
    fn a_new_document_holds_one_empty_paragraph_and_a_snapshot_a_replica_accepts() {
        let record = record();
        assert_eq!((record.revision, record.next_version), (0, 1));
        assert!(!record.changed_since_version);
        assert_eq!(record.blocks.len(), 1);
        assert_eq!(record.blocks[0].block_type, "PARAGRAPH");
        assert_eq!(record.blocks[0].content, "");

        // The snapshot is what the hub loads: it must pass every check of an incoming update.
        let snapshot = record.crdt_snapshot.as_ref().unwrap().bytes.clone();
        let replica = CrdtDocument::new(2).unwrap();
        replica.apply_update(&snapshot).unwrap();
        assert_eq!(replica.blocks().len(), 1);
        assert_eq!(replica.blocks()[0].block_id, record.blocks[0].block_id);
    }

    #[test]
    fn the_detail_has_the_blocks_and_no_snapshot() {
        let record = record();
        let detail = detail(&record);
        assert_eq!(detail.id, record.id.to_hex());
        assert_eq!(detail.blocks.len(), 1);
        assert_eq!(detail.blocks[0].block_type, ApiBlockType::Paragraph);
        assert_eq!(detail.revision, 0);
        assert!(detail.created_at.ends_with('Z'));
        let json = serde_json::to_value(&detail).unwrap();
        assert!(json.get("crdt_snapshot").is_none());
    }

    #[test]
    fn the_summary_has_only_the_documented_fields() {
        let json = serde_json::to_value(summary(&record())).unwrap();
        let keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let mut keys = keys;
        keys.sort_unstable();
        assert_eq!(keys, ["icon", "id", "revision", "title", "updated_at"]);
    }

    #[test]
    fn a_stored_block_with_an_unknown_type_is_left_out_of_the_detail() {
        let mut record = record();
        record.blocks.push(BlockRecord {
            block_id: "blk000000009".into(),
            block_type: "HEADING_9".into(),
            content: "x".into(),
            properties: mongodb::bson::Document::new(),
            order: 2.0,
        });
        assert_eq!(detail(&record).blocks.len(), 1);
    }
}

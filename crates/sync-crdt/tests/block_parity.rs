//! Native half of the parity test for the block document (task 2.1). `tests/wasm.rs` replays the
//! same fixture in the Wasm build and compares against the same expected files, so the two builds
//! must produce byte-identical state and identical blocks.
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code may unwrap (CLAUDE.md rule 3 covers non-test code)

use std::path::Path;

use sync_crdt::parity::{block_doc_from_fixture, encode_fixture};
use sync_crdt::{BlockType, Document};

const UPDATES: &[u8] = include_bytes!("fixtures/block_parity_updates.bin");
const EXPECTED_STATE: &[u8] = include_bytes!("fixtures/block_parity_expected_state.bin");
const EXPECTED_BLOCKS: &str = include_str!("fixtures/block_parity_expected_blocks.json");

fn id(n: u32) -> String {
    format!("blk{n:09}")
}

/// Three replicas with fixed client ids build a document: a shared start, then edits that happen
/// at the same time (text in one block, a type change and a text cut in another, a move, a new
/// callout), then edits after everyone merged. The updates are returned in an order in which each
/// one comes after the ones it builds on, which is what a block document accepts.
fn build_updates() -> Vec<Vec<u8>> {
    let a = Document::new(11).unwrap();
    let b = Document::new(22).unwrap();
    let c = Document::new(33).unwrap();
    // A starts the document.
    let mut log: Vec<Vec<u8>> = vec![
        a.initialise(),
        a.create_block(&id(1), BlockType::Heading1, 1.0, "Quarterly plan")
            .unwrap(),
        a.create_block(&id(2), BlockType::Paragraph, 2.0, "Ship the editor 🙂")
            .unwrap(),
        a.create_block(&id(3), BlockType::Code, 3.0, "fn main() {}")
            .unwrap(),
        a.set_props(&id(3), r#"{"language": "rust"}"#).unwrap(),
        a.create_block(&id(4), BlockType::Checklist, 4.0, "write tests")
            .unwrap(),
        a.set_props(&id(4), r#"{"checked": true}"#).unwrap(),
        a.create_block(&id(6), BlockType::Paragraph, 6.0, "scratch")
            .unwrap(),
    ];
    for update in &log {
        b.apply_update(update).unwrap();
        c.apply_update(update).unwrap();
    }

    // Concurrent edits.
    let from_b = vec![
        b.insert_text(&id(2), 5, "नमस्ते ").unwrap(),
        b.set_type(&id(1), BlockType::Heading2).unwrap(),
        b.create_block(&id(5), BlockType::Callout, 5.0, "heads up")
            .unwrap(),
        b.set_props(&id(5), r#"{"tone": "warning"}"#).unwrap(),
    ];
    let from_c = vec![
        c.insert_text(&id(2), 0, "→ ").unwrap(),
        c.move_block(&id(3), 0.5).unwrap(),
        c.delete_text(&id(1), 0, 10).unwrap(),
        c.insert_text(&id(4), 11, " too").unwrap(),
    ];
    let from_a = vec![
        a.move_block(&id(4), 1.5).unwrap(),
        a.set_props(&id(4), r#"{"checked": false}"#).unwrap(),
    ];
    for update in from_b.iter().chain(&from_c) {
        a.apply_update(update).unwrap();
    }
    for update in from_a.iter().chain(&from_c) {
        b.apply_update(update).unwrap();
    }
    log.extend(from_b);
    log.extend(from_c);
    log.extend(from_a);

    // After everyone has merged, A carries on.
    log.push(a.delete_block(&id(6)).unwrap());
    log.push(
        a.create_block(&id(7), BlockType::Paragraph, 7.0, "done")
            .unwrap(),
    );
    log.push(
        a.create_block(&id(8), BlockType::Heading1, 0.25, "Appendix")
            .unwrap(),
    );
    log
}

#[test]
fn block_parity_native_matches_fixture() {
    let doc = block_doc_from_fixture(UPDATES).unwrap();
    assert_eq!(doc.blocks_json(), EXPECTED_BLOCKS.trim_end());
    assert_eq!(doc.encode_state_as_update(), EXPECTED_STATE);
}

#[test]
fn the_fixture_covers_every_block_type_and_the_scripts_it_is_meant_to() {
    let doc = block_doc_from_fixture(UPDATES).unwrap();
    let blocks = doc.blocks();
    for block_type in BlockType::ALL {
        assert!(
            blocks.iter().any(|b| b.block_type == block_type)
                || block_type == BlockType::Heading2 && blocks.iter().any(|b| b.block_id == id(1)),
            "{block_type:?} is missing from the fixture"
        );
    }
    assert!(EXPECTED_BLOCKS.contains('🙂'));
    assert!(EXPECTED_BLOCKS.contains("नमस्ते"));
    assert!(EXPECTED_BLOCKS.contains("\"language\":\"rust\""));
    assert!(EXPECTED_BLOCKS.contains("\"tone\":\"warning\""));
    assert!(
        !EXPECTED_BLOCKS.contains("scratch"),
        "the deleted block is gone"
    );
    assert!(blocks.len() >= 6);
}

/// The script above still describes the fixture. The bytes of freshly built updates can differ
/// from the checked-in ones (a map is created from a hash map, so the order of its entries
/// varies), so this compares what the document contains, not the encoding.
#[test]
fn the_fixture_script_still_produces_the_fixture_blocks() {
    let encoded = encode_fixture(&build_updates()).unwrap();
    let doc = block_doc_from_fixture(&encoded).unwrap();
    assert_eq!(doc.blocks_json(), EXPECTED_BLOCKS.trim_end());
}

/// Regenerate only when the fixture should change (for example after a `yrs` upgrade), and review
/// the diff on purpose:
///
///     cargo test -p sync-crdt --test block_parity -- --ignored generate_block_parity_fixtures
#[test]
#[ignore = "writes the checked-in fixture files"]
fn generate_block_parity_fixtures() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let updates = encode_fixture(&build_updates()).unwrap();
    let doc = block_doc_from_fixture(&updates).unwrap();
    std::fs::write(dir.join("block_parity_updates.bin"), &updates).unwrap();
    std::fs::write(
        dir.join("block_parity_expected_state.bin"),
        doc.encode_state_as_update(),
    )
    .unwrap();
    std::fs::write(
        dir.join("block_parity_expected_blocks.json"),
        format!("{}\n", doc.blocks_json()),
    )
    .unwrap();
}

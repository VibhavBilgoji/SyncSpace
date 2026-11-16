//! Tests of the block document (task 2.1): ops, projection, ordering, convergence, hostile input,
//! structural validation and the y-sync messages. See `docs/TESTING.md`, "sync-crdt".

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code may unwrap (CLAUDE.md rule 3 covers non-test code)

use std::collections::BTreeSet;

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use serde_json::json;
use sync_crdt::schema::{MAX_BLOCK_TEXT_UNITS, MAX_BLOCKS};
use sync_crdt::{
    Block, BlockType, CrdtError, Document, FrameKind, MAX_UPDATE_BYTES, check_state_vector,
    frame_kind, is_empty_update, order_between, update_frame,
};
use yrs::block::ClientID;
use yrs::sync::awareness::AwarenessUpdate;
use yrs::sync::{Message, SyncMessage};
use yrs::updates::encoder::Encode;
use yrs::{Any, Doc, In, Map, MapPrelim, OffsetKind, Options, Text, TextPrelim, Transact};

/// Same idea as in `convergence.rs`: a fixed seed so a CI run cannot fail by luck, and the
/// nightly workflow sets `PROPTEST_RNG_SEED` and `PROPTEST_CASES` to explore.
const FIXED_SEED: u64 = 0x5EED_0021;

/// `release_cases` is the size of a real run (the release step of CI, where the 10 000-case
/// fuzz belongs); a debug build, which `cargo test` uses, runs `debug_cases` so the suite stays
/// quick on a laptop. `PROPTEST_CASES` overrides both (the nightly workflow sets it big).
fn proptest_config(release_cases: u32, debug_cases: u32) -> ProptestConfig {
    let mut config = ProptestConfig::default();
    if std::env::var_os("PROPTEST_CASES").is_none() {
        config.cases = if cfg!(debug_assertions) {
            debug_cases
        } else {
            release_cases
        };
    }
    if std::env::var_os("PROPTEST_RNG_SEED").is_none() {
        config.rng_seed = RngSeed::Fixed(FIXED_SEED);
    }
    config
}

fn doc(client_id: u64) -> Document {
    Document::new(client_id).unwrap()
}

/// A valid block id from a number: `blk` plus nine digits.
fn id(n: u32) -> String {
    format!("blk{n:09}")
}

fn ids(document: &Document) -> Vec<String> {
    document.blocks().into_iter().map(|b| b.block_id).collect()
}

/// What a rejected edit or update must not change.
fn snapshot(document: &Document) -> (Vec<u8>, Vec<u8>, Vec<Block>) {
    (
        document.state_vector(),
        document.encode_state_as_update(),
        document.blocks(),
    )
}

// ---------------------------------------------------------------------------------- projection

#[test]
fn blocks_come_back_sorted_by_order_then_id() {
    let d = doc(1);
    d.create_block(&id(3), BlockType::Paragraph, 3.0, "three")
        .unwrap();
    d.create_block(&id(1), BlockType::Heading1, 1.0, "one")
        .unwrap();
    d.create_block(&id(2), BlockType::Code, 2.0, "two").unwrap();
    // Two blocks with the same order: the id breaks the tie.
    d.create_block(&id(9), BlockType::Callout, 2.0, "tie")
        .unwrap();

    let blocks = d.blocks();
    assert_eq!(
        blocks
            .iter()
            .map(|b| b.content.as_str())
            .collect::<Vec<_>>(),
        ["one", "two", "tie", "three"]
    );
    assert_eq!(blocks[1].block_id, id(2));
    assert_eq!(blocks[2].block_id, id(9));
}

#[test]
fn an_empty_document_has_no_blocks() {
    let d = doc(1);
    assert!(d.blocks().is_empty());
    assert_eq!(d.blocks_json(), "[]");
}

#[test]
fn every_block_type_round_trips_with_its_wire_name() {
    let d = doc(1);
    for (n, block_type) in BlockType::ALL.into_iter().enumerate() {
        d.create_block(&id(n as u32), block_type, n as f64, "x")
            .unwrap();
    }
    let json: serde_json::Value = serde_json::from_str(&d.blocks_json()).unwrap();
    let names: Vec<&str> = json
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "PARAGRAPH",
            "HEADING_1",
            "HEADING_2",
            "CODE",
            "CHECKLIST",
            "CALLOUT"
        ]
    );
    // The JSON has exactly the fields of `api_types::Block`.
    let keys: BTreeSet<&str> = json[0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        BTreeSet::from(["block_id", "type", "content", "properties", "order"])
    );
}

#[test]
fn properties_round_trip_for_the_types_that_have_them() {
    let d = doc(1);
    d.create_block(&id(1), BlockType::Code, 1.0, "fn main() {}")
        .unwrap();
    d.create_block(&id(2), BlockType::Checklist, 2.0, "task")
        .unwrap();
    d.create_block(&id(3), BlockType::Callout, 3.0, "note")
        .unwrap();
    d.set_props(&id(1), r#"{"language": "rust"}"#).unwrap();
    d.set_props(&id(2), r#"{"checked": true}"#).unwrap();
    d.set_props(&id(3), r#"{"tone": "warning"}"#).unwrap();

    let blocks = d.blocks();
    assert_eq!(blocks[0].properties["language"], json!("rust"));
    assert_eq!(blocks[1].properties["checked"], json!(true));
    assert_eq!(blocks[2].properties["tone"], json!("warning"));

    // Replacing the properties replaces them: the old key goes away.
    d.set_props(&id(2), r#"{"checked": false}"#).unwrap();
    assert_eq!(d.blocks()[1].properties["checked"], json!(false));
    d.set_props(&id(2), "{}").unwrap();
    assert!(d.blocks()[1].properties.is_empty());
}

#[test]
fn changing_the_type_drops_properties_that_no_longer_belong() {
    let d = doc(1);
    d.create_block(&id(1), BlockType::Code, 1.0, "x").unwrap();
    d.set_props(&id(1), r#"{"language": "rust"}"#).unwrap();
    d.set_type(&id(1), BlockType::Paragraph).unwrap();
    let block = &d.blocks()[0];
    assert_eq!(block.block_type, BlockType::Paragraph);
    assert!(
        block.properties.is_empty(),
        "language does not belong to PARAGRAPH"
    );
    // It was dropped, not just hidden: back to CODE it does not come back.
    d.set_type(&id(1), BlockType::Code).unwrap();
    assert!(
        d.blocks()[0].properties.is_empty(),
        "the old language must be gone"
    );

    // A property that still belongs is kept.
    d.set_type(&id(1), BlockType::Callout).unwrap();
    d.set_props(&id(1), r#"{"tone": "info"}"#).unwrap();
    d.set_type(&id(1), BlockType::Callout).unwrap();
    assert_eq!(d.blocks()[0].properties["tone"], json!("info"));
}

#[test]
fn text_edits_use_utf16_offsets() {
    let d = doc(1);
    d.create_block(&id(1), BlockType::Paragraph, 1.0, "a🙂b")
        .unwrap();
    // The emoji is two UTF-16 units, so "b" starts at index 3.
    d.insert_text(&id(1), 3, "X").unwrap();
    assert_eq!(d.block_content(&id(1)).unwrap(), "a🙂Xb");
    d.delete_text(&id(1), 1, 2).unwrap();
    assert_eq!(d.block_content(&id(1)).unwrap(), "aXb");
    d.insert_text(&id(1), 3, " नमस्ते").unwrap();
    assert_eq!(d.blocks()[0].content, "aXb नमस्ते");
}

#[test]
fn initialise_writes_the_schema_marker_once() {
    let d = doc(1);
    let first = d.initialise();
    assert!(!first.is_empty());
    let state = d.encode_state_as_update();
    let _second = d.initialise();
    assert_eq!(
        d.encode_state_as_update(),
        state,
        "the marker is written once"
    );

    // A replica that loads the stored state accepts it (meta is part of the schema).
    let other = doc(2);
    other.apply_update(&state).unwrap();
    assert!(other.blocks().is_empty());
}

// ---------------------------------------------------------------------------------- bad edits

#[test]
fn edits_with_bad_arguments_fail_and_change_nothing() {
    let d = doc(1);
    d.create_block(&id(1), BlockType::Code, 1.0, "hello")
        .unwrap();
    let before = snapshot(&d);

    let long = "x".repeat(MAX_BLOCK_TEXT_UNITS as usize + 1);
    let cases: Vec<(&str, Result<Vec<u8>, CrdtError>)> = vec![
        (
            "short id",
            d.create_block("short", BlockType::Paragraph, 1.0, ""),
        ),
        (
            "dash in id",
            d.create_block("aaaaaaaaaaa-", BlockType::Paragraph, 1.0, ""),
        ),
        (
            "NaN order",
            d.create_block(&id(2), BlockType::Paragraph, f64::NAN, ""),
        ),
        (
            "infinite order",
            d.create_block(&id(2), BlockType::Paragraph, f64::INFINITY, ""),
        ),
        (
            "duplicate id",
            d.create_block(&id(1), BlockType::Paragraph, 1.0, ""),
        ),
        (
            "text over the cap",
            d.create_block(&id(2), BlockType::Paragraph, 1.0, &long),
        ),
        ("delete unknown", d.delete_block(&id(7))),
        ("delete with a bad id", d.delete_block("nope")),
        ("insert into unknown", d.insert_text(&id(7), 0, "x")),
        ("insert past the end", d.insert_text(&id(1), 6, "x")),
        ("insert over the cap", d.insert_text(&id(1), 0, &long)),
        ("delete range past the end", d.delete_text(&id(1), 3, 3)),
        (
            "delete range overflow",
            d.delete_text(&id(1), u32::MAX, u32::MAX),
        ),
        ("set type of unknown", d.set_type(&id(7), BlockType::Code)),
        ("props not json", d.set_props(&id(1), "{nope")),
        ("props not an object", d.set_props(&id(1), "[1]")),
        (
            "props key of another type",
            d.set_props(&id(1), r#"{"checked": true}"#),
        ),
        (
            "props wrong value kind",
            d.set_props(&id(1), r#"{"language": 5}"#),
        ),
        (
            "props language too long",
            d.set_props(&id(1), &format!(r#"{{"language": "{}"}}"#, "x".repeat(33))),
        ),
        ("props on unknown block", d.set_props(&id(7), "{}")),
        ("move unknown", d.move_block(&id(7), 1.0)),
        ("move to NaN", d.move_block(&id(1), f64::NAN)),
        ("move to infinity", d.move_block(&id(1), f64::NEG_INFINITY)),
    ];
    for (name, result) in cases {
        assert!(result.is_err(), "{name} should have failed");
        assert_eq!(snapshot(&d), before, "{name} changed the document");
    }
}

#[test]
fn the_error_kinds_are_the_documented_ones() {
    let d = doc(1);
    d.create_block(&id(1), BlockType::Paragraph, 1.0, "hello")
        .unwrap();
    assert_eq!(
        d.create_block("short", BlockType::Paragraph, 1.0, ""),
        Err(CrdtError::InvalidBlockId)
    );
    assert_eq!(
        d.create_block(&id(1), BlockType::Paragraph, 1.0, ""),
        Err(CrdtError::BlockExists(id(1)))
    );
    assert_eq!(d.delete_block(&id(2)), Err(CrdtError::BlockNotFound(id(2))));
    assert_eq!(
        d.create_block(&id(2), BlockType::Paragraph, f64::NAN, ""),
        Err(CrdtError::InvalidOrder)
    );
    assert!(matches!(
        d.insert_text(&id(1), 99, "x"),
        Err(CrdtError::OutOfRange { .. })
    ));
    assert!(matches!(
        d.set_props(&id(1), r#"{"tone": "info"}"#),
        Err(CrdtError::InvalidProps(_))
    ));
}

#[test]
fn a_document_cannot_hold_more_blocks_than_the_limit() {
    let d = doc(1);
    for n in 0..MAX_BLOCKS as u32 {
        d.create_block(&id(n), BlockType::Paragraph, f64::from(n), "")
            .unwrap();
    }
    let before = snapshot(&d);
    assert!(matches!(
        d.create_block(&id(MAX_BLOCKS as u32), BlockType::Paragraph, 0.0, ""),
        Err(CrdtError::Limit(_))
    ));
    assert_eq!(snapshot(&d), before);
    // Deleting one makes room again.
    d.delete_block(&id(0)).unwrap();
    d.create_block(&id(MAX_BLOCKS as u32), BlockType::Paragraph, 0.0, "")
        .unwrap();
}

#[test]
fn client_id_must_fit_in_53_bits() {
    assert!(Document::new((1 << 53) - 1).is_ok());
    assert!(matches!(
        Document::new(1 << 53),
        Err(CrdtError::InvalidClientId)
    ));
}

// ---------------------------------------------------------------------------- order and moves

#[test]
fn many_inserts_at_the_head_stay_distinct_and_ordered() {
    let d = doc(1);
    d.create_block(&id(0), BlockType::Paragraph, 1.0, "")
        .unwrap();
    let mut first = 1.0;
    for n in 1..=40 {
        let order = order_between(None, Some(first)).unwrap();
        d.create_block(&id(n), BlockType::Paragraph, order, "")
            .unwrap();
        first = order;
    }
    let found = ids(&d);
    let expected: Vec<String> = (0..=40).rev().map(id).collect();
    assert_eq!(found, expected, "each new head block comes first");
    let orders: Vec<f64> = d.blocks().iter().map(|b| b.order).collect();
    assert!(
        orders.windows(2).all(|w| w[0] < w[1]),
        "orders are distinct and increasing"
    );
}

#[test]
fn renumber_fixes_a_collapsed_gap_and_keeps_the_reading_order() {
    let d = doc(1);
    d.create_block(&id(1), BlockType::Paragraph, 1.0, "a")
        .unwrap();
    d.create_block(&id(2), BlockType::Paragraph, 2.0, "z")
        .unwrap();
    // Insert between the first block and the previous insert until there is no room.
    let mut upper = 2.0;
    let mut n = 10;
    let error = loop {
        match order_between(Some(1.0), Some(upper)) {
            Ok(order) => {
                d.create_block(&id(n), BlockType::Paragraph, order, "m")
                    .unwrap();
                upper = order;
                n += 1;
            }
            Err(error) => break error,
        }
    };
    assert_eq!(error, CrdtError::NoRoom);
    assert!(n > 20, "the gap lasts a few dozen halvings");

    let before = ids(&d);
    d.renumber();
    assert_eq!(ids(&d), before, "renumbering keeps the reading order");
    let orders: Vec<f64> = d.blocks().iter().map(|b| b.order).collect();
    let expected: Vec<f64> = (1..=orders.len()).map(|i| i as f64).collect();
    assert_eq!(orders, expected);
    // There is room again.
    assert!(order_between(Some(orders[0]), Some(orders[1])).is_ok());
}

#[test]
fn concurrent_moves_of_one_block_converge_to_one_position() {
    let a = doc(2);
    let b = doc(3);
    let seed = a
        .create_block(&id(1), BlockType::Paragraph, 1.0, "one")
        .unwrap();
    let seed2 = a
        .create_block(&id(2), BlockType::Paragraph, 2.0, "two")
        .unwrap();
    b.apply_update(&seed).unwrap();
    b.apply_update(&seed2).unwrap();

    let ua = a.move_block(&id(1), 5.0).unwrap();
    let ub = b.move_block(&id(1), 7.0).unwrap();
    a.apply_update(&ub).unwrap();
    b.apply_update(&ua).unwrap();

    assert_eq!(a.blocks(), b.blocks());
    assert_eq!(a.encode_state_as_update(), b.encode_state_as_update());
    assert_eq!(a.blocks().len(), 2, "no duplicate block");
    let moved = a
        .blocks()
        .into_iter()
        .find(|x| x.block_id == id(1))
        .unwrap();
    assert!(
        moved.order == 5.0 || moved.order == 7.0,
        "one of the two orders wins"
    );
}

#[test]
fn concurrent_creates_with_the_same_order_keep_both_blocks() {
    let a = doc(2);
    let b = doc(3);
    let ua = a
        .create_block(&id(1), BlockType::Paragraph, 1.0, "from a")
        .unwrap();
    let ub = b
        .create_block(&id(2), BlockType::Paragraph, 1.0, "from b")
        .unwrap();
    a.apply_update(&ub).unwrap();
    b.apply_update(&ua).unwrap();
    assert_eq!(a.blocks(), b.blocks());
    assert_eq!(
        ids(&a),
        [id(1), id(2)],
        "equal orders: the smaller id first"
    );
}

#[test]
fn a_block_created_with_the_same_id_on_two_replicas_ends_as_one_complete_block() {
    let a = doc(2);
    let b = doc(3);
    let ua = a.create_block(&id(1), BlockType::Code, 1.0, "aaa").unwrap();
    let ub = b
        .create_block(&id(1), BlockType::Callout, 2.0, "bbb")
        .unwrap();
    a.apply_update(&ub).unwrap();
    b.apply_update(&ua).unwrap();
    assert_eq!(a.blocks(), b.blocks());
    assert_eq!(a.blocks().len(), 1);
}

// ------------------------------------------------------------------------------- convergence

#[test]
fn two_replicas_converge_after_concurrent_text_edits_in_one_block() {
    let a = doc(2);
    let b = doc(3);
    let seed = a
        .create_block(&id(1), BlockType::Paragraph, 1.0, "hello world")
        .unwrap();
    b.apply_update(&seed).unwrap();

    let ua = a.insert_text(&id(1), 5, " AAA").unwrap();
    let ub = b.insert_text(&id(1), 5, " BBB").unwrap();
    a.apply_update(&ub).unwrap();
    b.apply_update(&ua).unwrap();
    b.apply_update(&ua).unwrap(); // a duplicate

    assert_eq!(a.blocks(), b.blocks());
    assert_eq!(a.encode_state_as_update(), b.encode_state_as_update());
    let text = &a.blocks()[0].content;
    assert!(text.contains("AAA") && text.contains("BBB"));
}

#[test]
fn a_late_joiner_receives_the_full_state() {
    let a = doc(2);
    a.initialise();
    a.create_block(&id(1), BlockType::Heading1, 1.0, "title")
        .unwrap();
    a.create_block(&id(2), BlockType::Checklist, 2.0, "todo")
        .unwrap();
    a.set_props(&id(2), r#"{"checked": true}"#).unwrap();
    a.insert_text(&id(1), 5, " नमस्ते 🙂").unwrap();
    a.delete_block(&id(2)).unwrap();

    let late = doc(9);
    late.apply_update(&a.encode_state_as_update()).unwrap();
    assert_eq!(late.blocks(), a.blocks());
    assert_eq!(late.encode_state_as_update(), a.encode_state_as_update());
}

#[test]
fn an_update_that_builds_on_missing_changes_is_rejected_and_waits_for_them() {
    // Found while testing: Yrs keeps such an update pending, and re-encoding a document with a
    // pending update loses data (see `validate.rs`). So the block document refuses it, the
    // sender (the hub, after a re-sync) delivers the missing changes first, and the same update
    // is then accepted.
    let author = doc(101);
    let u1 = author
        .create_block(&id(0), BlockType::Code, 1.0, "")
        .unwrap();
    let u2 = author.move_block(&id(0), 2.0).unwrap();
    let u3 = author
        .create_block(&id(1), BlockType::Heading1, 0.0, "")
        .unwrap();
    let u4 = author.set_type(&id(1), BlockType::Code).unwrap();

    let d = doc(1);
    let before = snapshot(&d);
    for late in [&u2, &u4] {
        assert_eq!(d.apply_update(late), Err(CrdtError::MissingDependencies));
        assert_eq!(snapshot(&d), before, "a refused update changes nothing");
    }
    d.apply_update(&u3).unwrap();
    d.apply_update(&u4).unwrap(); // now u3 is there
    assert_eq!(d.apply_update(&u2), Err(CrdtError::MissingDependencies));
    d.apply_update(&u1).unwrap();
    d.apply_update(&u2).unwrap();
    assert_eq!(d.blocks(), author.blocks());
    assert_eq!(d.encode_state_as_update(), author.encode_state_as_update());
}

/// One step of the convergence test. `sel` picks a block of the replica as it is at that moment,
/// so most steps hit an existing block; the ones that do not simply fail and are skipped.
#[derive(Debug, Clone)]
enum Op {
    Create { slot: u32, kind: u8, order: i32 },
    Delete { sel: usize },
    Insert { sel: usize, at: u32, text: String },
    DeleteText { sel: usize, at: u32, len: u32 },
    SetType { sel: usize, kind: u8 },
    SetProps { sel: usize, flag: bool },
    Move { sel: usize, order: i32 },
    Deliver { pick: usize },
}

fn op_strategy() -> impl Strategy<Value = (usize, Op)> {
    let op = prop_oneof![
        3 => (0u32..6, any::<u8>(), -5i32..20).prop_map(|(slot, kind, order)| Op::Create { slot, kind, order }),
        1 => any::<usize>().prop_map(|sel| Op::Delete { sel }),
        4 => (any::<usize>(), 0u32..12, "[a-c🙂é]{1,3}").prop_map(|(sel, at, text)| Op::Insert { sel, at, text }),
        2 => (any::<usize>(), 0u32..12, 1u32..4).prop_map(|(sel, at, len)| Op::DeleteText { sel, at, len }),
        1 => (any::<usize>(), any::<u8>()).prop_map(|(sel, kind)| Op::SetType { sel, kind }),
        1 => (any::<usize>(), any::<bool>()).prop_map(|(sel, flag)| Op::SetProps { sel, flag }),
        2 => (any::<usize>(), -5i32..20).prop_map(|(sel, order)| Op::Move { sel, order }),
        3 => any::<usize>().prop_map(|pick| Op::Deliver { pick }),
    ];
    (0usize..5, op)
}

fn kind(n: u8) -> BlockType {
    BlockType::ALL[usize::from(n) % BlockType::ALL.len()]
}

proptest! {
    #![proptest_config(proptest_config(200, 40))]

    /// 3 to 5 replicas apply random edits and exchange updates in random orders, with
    /// duplicates, then everything is delivered. All must end with identical blocks and
    /// identical encoded state, and no valid update may be rejected by the schema checks.
    #[test]
    fn replicas_converge_under_random_edits_and_delivery(
        replicas_count in 3usize..=5,
        steps in proptest::collection::vec(op_strategy(), 1..45),
        seeds in proptest::collection::vec(any::<u64>(), 5),
    ) {
        let replicas: Vec<Document> = (0..replicas_count).map(|n| doc(100 + n as u64)).collect();
        // Updates a replica has not received yet, in the order they were made.
        let mut pending: Vec<Vec<Vec<u8>>> = vec![Vec::new(); replicas_count];

        for (who, op) in &steps {
            let who = who % replicas_count;
            let r = &replicas[who];
            let blocks = r.blocks();
            let target = |sel: usize| blocks.get(sel % blocks.len().max(1)).map(|b| b.block_id.clone());
            let update = match op {
                Op::Create { slot, kind: k, order } => {
                    r.create_block(&id(*slot), kind(*k), f64::from(*order), "").ok()
                }
                Op::Delete { sel } => target(*sel).and_then(|t| r.delete_block(&t).ok()),
                Op::Insert { sel, at, text } => target(*sel).and_then(|t| {
                    let len = blocks.iter().find(|b| b.block_id == t).map_or(0, |b| b.content.encode_utf16().count() as u32);
                    let mut at = (*at).min(len);
                    // Do not split a surrogate pair.
                    let units: Vec<u16> = blocks.iter().find(|b| b.block_id == t).unwrap().content.encode_utf16().collect();
                    if at > 0 && at < len && (0xDC00..=0xDFFF).contains(&units[at as usize]) { at -= 1; }
                    r.insert_text(&t, at, text).ok()
                }),
                Op::DeleteText { sel, at, len } => target(*sel).and_then(|t| {
                    let units: Vec<u16> = blocks.iter().find(|b| b.block_id == t).unwrap().content.encode_utf16().collect();
                    let n = units.len() as u32;
                    let mut at = (*at).min(n);
                    if at > 0 && at < n && (0xDC00..=0xDFFF).contains(&units[at as usize]) { at -= 1; }
                    let mut end = (at + *len).min(n);
                    if end < n && (0xDC00..=0xDFFF).contains(&units[end as usize]) { end += 1; }
                    if end == at { None } else { r.delete_text(&t, at, end - at).ok() }
                }),
                Op::SetType { sel, kind: k } => target(*sel).and_then(|t| r.set_type(&t, kind(*k)).ok()),
                Op::SetProps { sel, flag } => target(*sel).and_then(|t| {
                    let block = blocks.iter().find(|b| b.block_id == t).unwrap();
                    let props = match block.block_type {
                        BlockType::Code => json!({"language": if *flag { "rust" } else { "ts" }}),
                        BlockType::Checklist => json!({"checked": flag}),
                        BlockType::Callout => json!({"tone": if *flag { "info" } else { "danger" }}),
                        _ => json!({}),
                    };
                    r.set_props(&t, &props.to_string()).ok()
                }),
                Op::Move { sel, order } => target(*sel).and_then(|t| r.move_block(&t, f64::from(*order)).ok()),
                Op::Deliver { pick } => {
                    if !pending[who].is_empty() {
                        let n = pick % pending[who].len();
                        let delivered = pending[who].remove(n);
                        match r.apply_update(&delivered) {
                            Ok(()) => {}
                            // It builds on something that has not arrived yet: try again later.
                            Err(CrdtError::MissingDependencies) => pending[who].push(delivered),
                            Err(error) => panic!("a valid update was rejected: {error}"),
                        }
                    }
                    None
                }
            };
            if let Some(update) = update {
                for (other, queue) in pending.iter_mut().enumerate() {
                    if other != who {
                        queue.push(update.clone());
                    }
                }
            }
        }

        // Deliver the rest, each replica in a different random order, plus one duplicate.
        for (n, r) in replicas.iter().enumerate() {
            let queue = &mut pending[n];
            let mut s = seeds[n] | 1;
            for i in (1..queue.len()).rev() {
                s ^= s << 13; s ^= s >> 7; s ^= s << 17;
                queue.swap(i, (s % (i as u64 + 1)) as usize);
            }
            // Deliver in passes: an update that is not ready yet waits for the next pass. The
            // order of creation is always a valid order, so every pass makes progress.
            let mut last_applied = None;
            while !queue.is_empty() {
                let before = queue.len();
                let mut waiting = Vec::new();
                for update in queue.drain(..) {
                    match r.apply_update(&update) {
                        Ok(()) => last_applied = Some(update),
                        Err(CrdtError::MissingDependencies) => waiting.push(update),
                        Err(error) => panic!("a valid update was rejected: {error}"),
                    }
                }
                *queue = waiting;
                prop_assert!(queue.len() < before, "no update could be delivered");
            }
            if let Some(last) = last_applied {
                r.apply_update(&last).unwrap(); // a duplicate
            }
        }

        let first = replicas[0].blocks();
        for r in &replicas[1..] {
            prop_assert_eq!(&r.blocks(), &first);
            // The same changes and deletions (state vector and delete set). The encoded bytes
            // can differ: the ranges of deleted structs are split differently depending on the
            // order updates arrived in, and a state vector lists its clients in hash order.
            prop_assert!(r.same_state_as(&replicas[0]));
        }
    }
}

// ---------------------------------------------------------------------------- hostile input

proptest! {
    #![proptest_config(proptest_config(10_000, 300))]

    #[test]
    fn apply_update_rejects_garbage_without_panicking(
        bytes in proptest::collection::vec(any::<u8>(), 0..512)
    ) {
        let d = doc(1);
        d.create_block(&id(1), BlockType::Paragraph, 1.0, "keep").unwrap();
        let before = snapshot(&d);
        if d.apply_update(&bytes).is_err() {
            prop_assert_eq!(snapshot(&d), before);
        }
        let _ = d.encode_state_as_update();
    }

    #[test]
    fn handle_message_rejects_garbage_without_panicking(
        bytes in proptest::collection::vec(any::<u8>(), 0..512)
    ) {
        let d = doc(1);
        d.create_block(&id(1), BlockType::Paragraph, 1.0, "keep").unwrap();
        let before = snapshot(&d);
        if d.handle_message(&bytes).is_err() {
            prop_assert_eq!(snapshot(&d), before);
        }
        let _ = d.encode_state_as_update();
    }

    #[test]
    fn state_vectors_from_the_network_never_panic(
        bytes in proptest::collection::vec(any::<u8>(), 0..128)
    ) {
        let d = doc(1);
        let _ = d.encode_state_as_update_from(&bytes);
        let _ = check_state_vector(&bytes);
    }

    #[test]
    fn truncated_real_updates_never_panic(cut in 0usize..400) {
        let src = doc(2);
        let mut update = src.create_block(&id(1), BlockType::Code, 1.0, "hello 🙂 नमस्ते").unwrap();
        update.extend(src.set_props(&id(1), r#"{"language": "rust"}"#).unwrap());
        let d = doc(1);
        let before = snapshot(&d);
        let cut = cut.min(update.len());
        if d.apply_update(&update[..cut]).is_err() {
            prop_assert_eq!(snapshot(&d), before);
        }
    }
}

/// A valid history with bytes flipped and cut: the update a hostile peer could send to a
/// replica that already has content. The outcome may be Ok or Err; it must never panic, a
/// rejected update changes nothing, and the replica can still encode its state for a joiner.
fn apply_mutated(flips: &[(usize, u8)], cut: Option<usize>) {
    let author = doc(2);
    author
        .create_block(&id(1), BlockType::Callout, 1.0, "hello 🙂")
        .unwrap();
    author
        .create_block(&id(2), BlockType::Code, 2.0, "world")
        .unwrap();
    author.set_props(&id(2), r#"{"language": "rust"}"#).unwrap();
    author.insert_text(&id(1), 2, "xyz").unwrap();
    let mut hostile = author.encode_state_as_update();
    for (position, byte) in flips {
        let len = hostile.len();
        hostile[position % len] = *byte;
    }
    if let Some(cut) = cut {
        hostile.truncate(cut % (hostile.len() + 1));
    }

    let replica = doc(1);
    replica
        .create_block(&id(9), BlockType::Paragraph, 9.0, "existing")
        .unwrap();
    let before = snapshot(&replica);
    if replica.apply_update(&hostile).is_err() {
        assert_eq!(snapshot(&replica), before);
    }
    let _ = replica.blocks();
    let _ = replica.encode_state_as_update();
}

proptest! {
    #![proptest_config(proptest_config(10_000, 300))]

    #[test]
    fn mutated_valid_updates_never_panic_and_rejections_change_nothing(
        flips in proptest::collection::vec((any::<usize>(), any::<u8>()), 1..4),
        cut in proptest::option::of(any::<usize>()),
    ) {
        apply_mutated(&flips, cut);
    }
}

#[test]
fn updates_over_the_size_limit_are_rejected_and_the_limit_itself_is_not() {
    let d = doc(1);
    let big = vec![0u8; MAX_UPDATE_BYTES + 1];
    assert_eq!(
        d.apply_update(&big),
        Err(CrdtError::TooLarge(MAX_UPDATE_BYTES + 1))
    );
    let exact = vec![0u8; MAX_UPDATE_BYTES];
    assert!(!matches!(
        d.apply_update(&exact),
        Err(CrdtError::TooLarge(_))
    ));
    assert!(matches!(
        d.handle_message(&vec![0u8; MAX_UPDATE_BYTES + 65]),
        Err(CrdtError::TooLarge(_))
    ));
}

/// Bytes that decode to one struct whose client id needs 54 bits. Yrs 0.28.1 trips a
/// `debug_assert!` on it; release builds accept such ids, so this is debug-only (as in
/// `convergence.rs`).
#[cfg(debug_assertions)]
#[test]
fn a_hostile_client_id_is_rejected_and_leaves_the_document_untouched() {
    let mut bytes = vec![1, 1];
    let mut client: u64 = 1 << 60;
    while client >= 0x80 {
        bytes.push((client as u8 & 0x7F) | 0x80);
        client >>= 7;
    }
    bytes.push(client as u8);
    bytes.extend_from_slice(&[0, 0, 1, 0]);

    let d = doc(1);
    d.create_block(&id(1), BlockType::Paragraph, 1.0, "keep me")
        .unwrap();
    let before = snapshot(&d);
    assert_eq!(d.apply_update(&bytes), Err(CrdtError::Decode));
    assert_eq!(snapshot(&d), before);
    d.insert_text(&id(1), 7, "!").unwrap();
    assert_eq!(d.blocks()[0].content, "keep me!");
}

#[test]
fn a_rejected_update_does_not_move_the_state_vector() {
    let d = doc(1);
    d.create_block(&id(1), BlockType::Paragraph, 1.0, "x")
        .unwrap();
    let before = d.state_vector();
    assert!(d.apply_update(&[1, 2, 3]).is_err());
    assert!(d.apply_update(&[]).is_err() || d.state_vector() == before);
    assert_eq!(d.state_vector(), before);
}

// --------------------------------------------------------------------- structural validation

/// A raw Yrs replica: the kind of client a hostile peer controls. It can write anything.
fn raw() -> Doc {
    let mut options =
        Options::with_guid_and_client_id("syncspace-document".into(), ClientID::new(77));
    options.offset_kind = OffsetKind::Utf16;
    Doc::with_options(options)
}

fn raw_block(block_type: In, order: In, text: &str, extra: Vec<(&'static str, In)>) -> MapPrelim {
    let mut fields: Vec<(&'static str, In)> = vec![
        ("type", block_type),
        ("order", order),
        ("text", In::from(TextPrelim::new(text))),
        ("props", In::from(MapPrelim::default())),
    ];
    fields.extend(extra);
    MapPrelim::from_iter(fields)
}

fn text_in(s: &str) -> In {
    In::Any(Any::from(s))
}

fn number_in(n: f64) -> In {
    In::Any(Any::from(n))
}

/// The update with which a raw replica adds one block under `key`.
fn raw_update(key: &str, block: MapPrelim) -> Vec<u8> {
    let d = raw();
    let blocks = d.get_or_insert_map("blocks");
    let mut txn = d.transact_mut();
    blocks.insert(&mut txn, key, block);
    txn.encode_update_v1()
}

fn assert_rejected_as_invalid(update: &[u8], what: &str) {
    let d = doc(1);
    d.create_block(&id(1), BlockType::Paragraph, 1.0, "keep")
        .unwrap();
    let before = snapshot(&d);
    let result = d.apply_update(update);
    assert!(
        matches!(result, Err(CrdtError::Invalid(_))),
        "{what}: expected Invalid, got {result:?}"
    );
    assert_eq!(snapshot(&d), before, "{what}: the document changed");
}

#[test]
fn a_hand_built_valid_block_is_accepted() {
    let update = raw_update(
        &id(5),
        raw_block(text_in("PARAGRAPH"), number_in(3.0), "hi", vec![]),
    );
    let d = doc(1);
    d.apply_update(&update).unwrap();
    assert_eq!(d.blocks()[0].content, "hi");
}

#[test]
fn updates_that_break_the_schema_are_rejected_and_change_nothing() {
    let ok_type = || text_in("PARAGRAPH");
    let ok_order = || number_in(1.0);

    assert_rejected_as_invalid(
        &raw_update(
            &id(5),
            raw_block(text_in("HEADING_3"), ok_order(), "", vec![]),
        ),
        "unknown block type",
    );
    assert_rejected_as_invalid(
        &raw_update(
            &id(5),
            raw_block(ok_type(), number_in(f64::NAN), "", vec![]),
        ),
        "NaN order",
    );
    assert_rejected_as_invalid(
        &raw_update(
            &id(5),
            raw_block(ok_type(), number_in(f64::INFINITY), "", vec![]),
        ),
        "infinite order",
    );
    assert_rejected_as_invalid(
        &raw_update(&id(5), raw_block(ok_type(), text_in("first"), "", vec![])),
        "order is a string",
    );
    assert_rejected_as_invalid(
        &raw_update(&id(5), raw_block(number_in(1.0), ok_order(), "", vec![])),
        "type is a number",
    );
    assert_rejected_as_invalid(
        &raw_update("short", raw_block(ok_type(), ok_order(), "", vec![])),
        "bad block id",
    );
    assert_rejected_as_invalid(
        &raw_update(
            &id(5),
            raw_block(ok_type(), ok_order(), "", vec![("extra", text_in("x"))]),
        ),
        "unknown field in a block",
    );
    assert_rejected_as_invalid(
        &raw_update(
            &id(5),
            raw_block(
                ok_type(),
                ok_order(),
                &"a".repeat(MAX_BLOCK_TEXT_UNITS as usize + 1),
                vec![],
            ),
        ),
        "text over the cap",
    );
}

#[test]
fn a_block_with_missing_fields_is_rejected() {
    let missing_order = MapPrelim::from_iter([
        ("type", text_in("PARAGRAPH")),
        ("text", In::from(TextPrelim::new(""))),
    ]);
    assert_rejected_as_invalid(&raw_update(&id(5), missing_order), "missing order");

    let not_a_map = {
        let d = raw();
        let blocks = d.get_or_insert_map("blocks");
        let mut txn = d.transact_mut();
        blocks.insert(&mut txn, id(5).as_str(), Any::from("just a string"));
        txn.encode_update_v1()
    };
    assert_rejected_as_invalid(&not_a_map, "a block that is not a map");
}

#[test]
fn properties_with_bad_values_or_unknown_names_are_rejected() {
    for (block_type, props, what) in [
        (
            "CODE",
            vec![("language", Any::from(5.0))],
            "numeric language",
        ),
        (
            "CALLOUT",
            vec![("tone", Any::from("purple"))],
            "unknown tone",
        ),
        (
            "CODE",
            vec![("language", Any::from("x".repeat(33)))],
            "language too long",
        ),
        ("CODE", vec![("language", Any::Null)], "null value"),
        (
            "CHECKLIST",
            vec![("checked", Any::from("yes"))],
            "checked is not a bool",
        ),
        (
            "PARAGRAPH",
            vec![("evil", Any::from("x"))],
            "unknown property name",
        ),
    ] {
        let d = raw();
        let blocks = d.get_or_insert_map("blocks");
        let mut txn = d.transact_mut();
        let props = MapPrelim::from_iter(props.into_iter().map(|(k, v)| (k, In::Any(v))));
        blocks.insert(
            &mut txn,
            id(5).as_str(),
            MapPrelim::from_iter([
                ("type", text_in(block_type)),
                ("order", number_in(1.0)),
                ("text", In::from(TextPrelim::new(""))),
                ("props", In::from(props)),
            ]),
        );
        assert_rejected_as_invalid(&txn.encode_update_v1(), what);
    }
}

#[test]
fn a_concurrent_type_change_and_property_change_is_accepted_and_converges() {
    // One person turns a checklist item into a heading while another ticks it: the merged block
    // is a HEADING_1 that still holds `checked`. That is legitimate, so it is accepted; the
    // projection shows only the properties of the block's type, the same on every replica.
    let a = doc(2);
    let b = doc(3);
    let seed = a
        .create_block(&id(1), BlockType::Checklist, 1.0, "todo")
        .unwrap();
    b.apply_update(&seed).unwrap();

    let ua = a.set_type(&id(1), BlockType::Heading1).unwrap();
    let ub = b.set_props(&id(1), r#"{"checked": true}"#).unwrap();
    a.apply_update(&ub).unwrap();
    b.apply_update(&ua).unwrap();

    assert_eq!(a.blocks(), b.blocks());
    assert!(a.same_state_as(&b));
    let block = &a.blocks()[0];
    assert_eq!(block.block_type, BlockType::Heading1);
    assert!(
        block.properties.is_empty(),
        "checked does not belong to a heading"
    );
}

#[test]
fn unknown_root_types_are_rejected() {
    let d = raw();
    let evil = d.get_or_insert_map("evil");
    let mut txn = d.transact_mut();
    evil.insert(&mut txn, "k", Any::from("v"));
    assert_rejected_as_invalid(&txn.encode_update_v1(), "unknown root map");

    let d = raw();
    let text = d.get_or_insert_text("text");
    let mut txn = d.transact_mut();
    text.insert(&mut txn, 0, "spike text");
    assert_rejected_as_invalid(&txn.encode_update_v1(), "a root text");
}

#[test]
fn a_bad_meta_entry_is_rejected() {
    let d = raw();
    let meta = d.get_or_insert_map("meta");
    let mut txn = d.transact_mut();
    meta.insert(&mut txn, "schema", Any::from(2.0));
    assert_rejected_as_invalid(&txn.encode_update_v1(), "schema 2");

    let d = raw();
    let meta = d.get_or_insert_map("meta");
    let mut txn = d.transact_mut();
    meta.insert(&mut txn, "other", Any::from(1.0));
    assert_rejected_as_invalid(&txn.encode_update_v1(), "unknown meta key");
}

#[test]
fn an_update_that_would_pass_the_block_limit_is_rejected() {
    let hostile = {
        let d = raw();
        let blocks = d.get_or_insert_map("blocks");
        let mut txn = d.transact_mut();
        for n in 0..=MAX_BLOCKS as u32 {
            blocks.insert(
                &mut txn,
                id(n).as_str(),
                raw_block(text_in("PARAGRAPH"), number_in(f64::from(n)), "", vec![]),
            );
        }
        txn.encode_update_v1()
    };
    assert!(
        hostile.len() < MAX_UPDATE_BYTES,
        "the test update must pass the size limit"
    );
    assert_rejected_as_invalid(&hostile, "more blocks than the limit");

    // The same count minus one is fine.
    let allowed = {
        let d = raw();
        let blocks = d.get_or_insert_map("blocks");
        let mut txn = d.transact_mut();
        for n in 0..MAX_BLOCKS as u32 {
            blocks.insert(
                &mut txn,
                id(n).as_str(),
                raw_block(text_in("PARAGRAPH"), number_in(f64::from(n)), "", vec![]),
            );
        }
        txn.encode_update_v1()
    };
    doc(1).apply_update(&allowed).unwrap();
}

// ------------------------------------------------------------------------------------ sync

#[test]
fn sync_step1_gets_a_step2_that_brings_the_peer_up_to_date() {
    let server = doc(1);
    server.initialise();
    server
        .create_block(&id(1), BlockType::Heading1, 1.0, "title")
        .unwrap();
    server
        .create_block(&id(2), BlockType::Paragraph, 2.0, "body 🙂")
        .unwrap();

    let client = doc(2);
    let handled = server.handle_message(&client.sync_step1_frame()).unwrap();
    let reply = handled.reply.expect("a SyncStep2");
    assert!(handled.update.is_none());

    let applied = client.handle_message(&reply).unwrap();
    assert!(
        applied.update.is_some(),
        "the update is handed back for broadcast"
    );
    assert_eq!(client.blocks(), server.blocks());

    // A client that is already up to date gets an empty-ish step 2 and stays equal.
    let again = server.handle_message(&client.sync_step1_frame()).unwrap();
    client.handle_message(&again.reply.unwrap()).unwrap();
    assert_eq!(client.blocks(), server.blocks());
}

#[test]
fn an_update_frame_applies_and_is_returned_for_broadcast() {
    let a = doc(2);
    let b = doc(3);
    let update = a
        .create_block(&id(1), BlockType::Callout, 1.0, "hi")
        .unwrap();
    let handled = b.handle_message(&update_frame(&update)).unwrap();
    assert_eq!(handled.update.as_deref(), Some(update.as_slice()));
    assert!(handled.reply.is_none() && handled.awareness.is_none());
    assert_eq!(b.blocks(), a.blocks());
}

#[test]
fn an_update_frame_that_breaks_the_schema_is_rejected_like_a_bare_update() {
    let hostile = raw_update(
        &id(5),
        raw_block(text_in("NOPE"), number_in(1.0), "", vec![]),
    );
    let d = doc(1);
    let before = snapshot(&d);
    assert!(matches!(
        d.handle_message(&update_frame(&hostile)),
        Err(CrdtError::Invalid(_))
    ));
    assert_eq!(snapshot(&d), before);
}

#[test]
fn awareness_frames_are_passed_through_and_other_frames_are_errors() {
    let d = doc(1);
    let awareness = Message::Awareness(AwarenessUpdate {
        clients: Default::default(),
    })
    .encode_v1();
    let handled = d.handle_message(&awareness).unwrap();
    assert!(handled.awareness.is_some());
    assert!(handled.update.is_none() && handled.reply.is_none());

    for (frame, what) in [
        (Vec::new(), "empty"),
        (vec![0], "sync tag without a body"),
        (vec![0, 9], "unknown sync message"),
        (vec![99, 1, 2], "unknown message type"),
        (Message::AwarenessQuery.encode_v1(), "awareness query"),
        (Message::Auth(None).encode_v1(), "auth"),
        (
            Message::Sync(SyncMessage::Update(vec![1, 2, 3])).encode_v1(),
            "an update that is not an update",
        ),
        (
            Message::Sync(SyncMessage::SyncStep1(Default::default()))
                .encode_v1()
                .into_iter()
                .take(1)
                .collect(),
            "a truncated step 1",
        ),
    ] {
        assert!(
            d.handle_message(&frame).is_err(),
            "{what} should be an error"
        );
    }
}

#[test]
fn encoding_the_state_from_a_state_vector_gives_only_what_is_missing() {
    let a = doc(2);
    a.create_block(&id(1), BlockType::Paragraph, 1.0, "one")
        .unwrap();
    let b = doc(3);
    b.apply_update(&a.encode_state_as_update()).unwrap();
    let new = a
        .create_block(&id(2), BlockType::Paragraph, 2.0, "two")
        .unwrap();

    let diff = a.encode_state_as_update_from(&b.state_vector()).unwrap();
    assert!(
        diff.len() <= new.len() + 16,
        "the diff is about the size of the new update"
    );
    b.apply_update(&diff).unwrap();
    assert_eq!(a.blocks(), b.blocks());
    assert!(a.encode_state_as_update_from(&[0xFF, 0xFF]).is_err());
}

#[test]
fn frames_are_classified_without_being_applied() {
    let a = doc(2);
    let update = a
        .create_block(&id(1), BlockType::Paragraph, 1.0, "x")
        .unwrap();
    assert_eq!(frame_kind(&a.sync_step1_frame()), Ok(FrameKind::SyncStep1));
    assert_eq!(frame_kind(&update_frame(&update)), Ok(FrameKind::Update));
    let step2 = Message::Sync(SyncMessage::SyncStep2(update)).encode_v1();
    assert_eq!(frame_kind(&step2), Ok(FrameKind::SyncStep2));
    let awareness = Message::Awareness(AwarenessUpdate {
        clients: Default::default(),
    })
    .encode_v1();
    assert_eq!(frame_kind(&awareness), Ok(FrameKind::Awareness));

    // Only the two frames that carry changes are writes.
    for (kind, write) in [
        (FrameKind::SyncStep1, false),
        (FrameKind::Awareness, false),
        (FrameKind::SyncStep2, true),
        (FrameKind::Update, true),
    ] {
        assert_eq!(kind.is_write(), write, "{kind:?}");
    }

    // Classifying never applies anything.
    let b = doc(3);
    let before = snapshot(&b);
    let _ = frame_kind(&update_frame(&a.encode_state_as_update()));
    assert_eq!(snapshot(&b), before);

    for (frame, what) in [
        (Vec::new(), "empty"),
        (vec![0, 9], "unknown sync message"),
        (vec![99, 1, 2], "unknown message type"),
        (Message::AwarenessQuery.encode_v1(), "awareness query"),
        (Message::Auth(None).encode_v1(), "auth"),
        (vec![0u8; MAX_UPDATE_BYTES + 65], "oversized"),
    ] {
        assert!(frame_kind(&frame).is_err(), "{what} should be an error");
    }
}

proptest! {
    #![proptest_config(proptest_config(10_000, 300))]

    #[test]
    fn classifying_garbage_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
        let _ = frame_kind(&bytes);
    }
}

#[test]
fn an_update_with_no_change_is_recognised_as_empty() {
    let a = doc(2);
    let b = doc(3);
    // What a client with nothing to add answers to a SyncStep1.
    let nothing = b.encode_state_as_update_from(&a.state_vector()).unwrap();
    assert!(is_empty_update(&nothing));
    let update = a
        .create_block(&id(1), BlockType::Paragraph, 1.0, "x")
        .unwrap();
    assert!(!is_empty_update(&update));
    assert!(!is_empty_update(&[]));
    assert!(!is_empty_update(&[0, 0, 0]));
}

#[test]
fn restoring_turns_the_content_into_the_target_and_converges_on_every_replica() {
    // The target: three blocks, one with properties.
    let old = doc(2);
    old.create_block(&id(1), BlockType::Heading1, 1.0, "Plan")
        .unwrap();
    old.create_block(&id(2), BlockType::Checklist, 2.0, "Ship it")
        .unwrap();
    old.set_props(&id(2), r#"{"checked":true}"#).unwrap();
    old.create_block(&id(3), BlockType::Paragraph, 3.0, "gone later")
        .unwrap();
    let archived = doc(3);
    archived
        .apply_update(&old.encode_state_as_update())
        .unwrap();

    // The live document drifted: block 1 edited and retyped, block 2 deleted, block 3 moved,
    // block 4 added.
    let live = doc(4);
    live.apply_update(&old.encode_state_as_update()).unwrap();
    let peer = doc(5);
    peer.apply_update(&old.encode_state_as_update()).unwrap();
    let edits = vec![
        live.insert_text(&id(1), 4, " and more").unwrap(),
        live.set_type(&id(1), BlockType::Paragraph).unwrap(),
        live.delete_block(&id(2)).unwrap(),
        live.move_block(&id(3), 0.5).unwrap(),
        live.create_block(&id(4), BlockType::Code, 9.0, "fn main() {}")
            .unwrap(),
    ];
    for update in edits {
        peer.apply_update(&update).unwrap();
    }
    assert_eq!(live.blocks(), peer.blocks());

    let update = live.restore_update(&archived, 77).unwrap();
    live.apply_update(&update).unwrap();
    peer.apply_update(&update).unwrap();

    assert_eq!(
        live.blocks(),
        archived.blocks(),
        "the content is the archived content"
    );
    assert_eq!(
        peer.blocks(),
        archived.blocks(),
        "a replica that only got the update agrees"
    );
    assert_eq!(ids(&live), vec![id(1), id(2), id(3)]);
    assert_eq!(
        live.blocks()[1].properties.get("checked"),
        Some(&json!(true))
    );
}

#[test]
fn restoring_an_identical_document_changes_nothing() {
    let a = doc(2);
    a.create_block(&id(1), BlockType::Paragraph, 1.0, "same")
        .unwrap();
    let b = doc(3);
    b.apply_update(&a.encode_state_as_update()).unwrap();
    let update = a.restore_update(&b, 77).unwrap();
    assert!(is_empty_update(&update));
    let before = a.blocks();
    a.apply_update(&update).unwrap();
    assert_eq!(a.blocks(), before);
}

#[test]
fn restoring_leaves_the_document_alone_until_the_update_is_applied() {
    let live = doc(2);
    live.create_block(&id(1), BlockType::Paragraph, 1.0, "live")
        .unwrap();
    let target = doc(3);
    target
        .create_block(&id(2), BlockType::Paragraph, 1.0, "archived")
        .unwrap();
    let state = live.encode_state_as_update();
    let _ = live.restore_update(&target, 77).unwrap();
    assert_eq!(live.encode_state_as_update(), state);
    assert_eq!(live.block_content(&id(1)).unwrap(), "live");
    assert!(
        live.restore_update(&target, 1 << 53).is_err(),
        "a bad scratch client id is an error"
    );
}

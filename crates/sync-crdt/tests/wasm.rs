//! Wasm half of the parity test. Runs under Node: `wasm-pack test --node crates/sync-crdt`.
//! It replays the same fixture as `tests/parity.rs` and compares against the same files.
#![cfg(target_arch = "wasm32")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code may unwrap (CLAUDE.md rule 3 covers non-test code)

use sync_crdt::SpikeDoc;
use sync_crdt::parity::doc_from_fixture;
use sync_crdt::wasm::WasmDoc;
use wasm_bindgen_test::wasm_bindgen_test;

const UPDATES: &[u8] = include_bytes!("fixtures/parity_updates.bin");
const EXPECTED_STATE: &[u8] = include_bytes!("fixtures/parity_expected_state.bin");
const EXPECTED_TEXT: &str = include_str!("fixtures/parity_expected_text.txt");

#[wasm_bindgen_test]
fn parity_wasm_matches_fixture() {
    let doc = doc_from_fixture(UPDATES).unwrap();
    assert_eq!(doc.text(), EXPECTED_TEXT);
    assert_eq!(doc.encode_state_as_update(), EXPECTED_STATE);
}

#[wasm_bindgen_test]
fn parity_through_the_js_api_matches_fixture() {
    let doc = WasmDoc::from_fixture(UPDATES).ok().unwrap();
    assert_eq!(doc.text(), EXPECTED_TEXT);
    assert_eq!(doc.encode_state_as_update(), EXPECTED_STATE);
}

#[wasm_bindgen_test]
fn wasm_doc_round_trip() {
    let a = WasmDoc::new(101.0).ok().unwrap();
    let b = WasmDoc::new(202.0).ok().unwrap();

    let u1 = a.insert(0, "hi 🙂").ok().unwrap();
    b.apply_update(&u1).ok().unwrap();
    let u2 = b.insert(5, "!").ok().unwrap();
    a.apply_update(&u2).ok().unwrap();

    assert_eq!(a.text(), "hi 🙂!");
    assert_eq!(a.text(), b.text());
    assert_eq!(a.encode_state_as_update(), b.encode_state_as_update());

    let u3 = a.delete(3, 2).ok().unwrap();
    b.apply_update(&u3).ok().unwrap();
    assert_eq!(b.text(), "hi !");
}

#[wasm_bindgen_test]
fn wasm_doc_rejects_bad_client_ids_and_bad_updates() {
    assert!(WasmDoc::new(-1.0).is_err());
    assert!(WasmDoc::new(1.5).is_err());
    assert!(WasmDoc::new(f64::NAN).is_err());
    assert!(WasmDoc::new(9_007_199_254_740_992.0).is_err());

    let doc = WasmDoc::new(7.0).ok().unwrap();
    assert!(doc.apply_update(&[255, 255, 255, 255, 255]).is_err());
    assert!(doc.insert(5, "x").is_err());
    assert_eq!(doc.text(), "");
}

#[wasm_bindgen_test]
fn two_clients_with_distinct_ids_converge_on_concurrent_edits() {
    let a = SpikeDoc::new(1001).unwrap();
    let b = SpikeDoc::new(1002).unwrap();
    let ua = a.insert(0, "A").unwrap();
    let ub = b.insert(0, "B").unwrap();
    a.apply_update(&ub).unwrap();
    b.apply_update(&ua).unwrap();
    assert_eq!(a.text(), b.text());
    assert_eq!(a.encode_state_as_update(), b.encode_state_as_update());
}

#[wasm_bindgen_test]
fn from_fixture_reports_truncated_and_lying_fixtures_as_errors() {
    assert!(WasmDoc::from_fixture(&UPDATES[..UPDATES.len() - 1]).is_err());
    assert!(WasmDoc::from_fixture(&[1, 2]).is_err());
    let mut lying = 1000u32.to_le_bytes().to_vec();
    lying.extend_from_slice(&[1, 2, 3]);
    assert!(WasmDoc::from_fixture(&lying).is_err());
    assert!(WasmDoc::from_fixture(&[]).is_ok());
}

#[wasm_bindgen_test]
fn wasm_doc_edit_errors_leave_the_text_unchanged() {
    let doc = WasmDoc::new(8.0).ok().unwrap();
    doc.insert(0, "abc").ok().unwrap();
    assert!(doc.delete(2, 5).is_err());
    assert!(doc.insert(4, "x").is_err());
    assert!(WasmDoc::new(f64::INFINITY).is_err());
    assert!(WasmDoc::new(0.0).is_ok());
    assert_eq!(doc.text(), "abc");
}

#[wasm_bindgen_test]
fn wasm_doc_late_joiner_matches_native_state_encoding() {
    let a = WasmDoc::new(1.0).ok().unwrap();
    a.insert(0, "नमस्ते 🙂").ok().unwrap();
    let late = WasmDoc::new(2.0).ok().unwrap();
    late.apply_update(&a.encode_state_as_update()).ok().unwrap();
    assert_eq!(late.text(), a.text());
    assert_eq!(late.encode_state_as_update(), a.encode_state_as_update());
}

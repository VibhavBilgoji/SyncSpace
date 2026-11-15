#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code may unwrap (CLAUDE.md rule 3 covers non-test code)

use proptest::prelude::*;
use sync_crdt::{CrdtError, MAX_UPDATE_BYTES, SpikeDoc};

fn doc(client_id: u64) -> SpikeDoc {
    SpikeDoc::new(client_id).unwrap()
}

#[test]
fn insert_and_delete_use_utf16_offsets() {
    let d = doc(1);
    d.insert(0, "a🙂b").unwrap();
    // The emoji is two UTF-16 units, so "b" starts at index 3.
    assert_eq!(d.len(), 4);
    d.insert(3, "X").unwrap();
    assert_eq!(d.text(), "a🙂Xb");
    // Deleting two units removes exactly the emoji.
    d.delete(1, 2).unwrap();
    assert_eq!(d.text(), "aXb");
}

#[test]
fn out_of_range_edits_return_errors_and_change_nothing() {
    let d = doc(1);
    d.insert(0, "abc").unwrap();
    assert!(matches!(
        d.insert(4, "x"),
        Err(CrdtError::OutOfRange { .. })
    ));
    assert!(matches!(d.delete(2, 2), Err(CrdtError::OutOfRange { .. })));
    assert!(matches!(
        d.delete(u32::MAX, u32::MAX),
        Err(CrdtError::OutOfRange { .. })
    ));
    assert_eq!(d.text(), "abc");
}

#[test]
fn client_id_must_fit_in_53_bits() {
    assert!(SpikeDoc::new((1 << 53) - 1).is_ok());
    assert!(matches!(
        SpikeDoc::new(1 << 53),
        Err(CrdtError::InvalidClientId)
    ));
}

#[test]
fn two_docs_converge_after_exchanging_concurrent_updates() {
    let base = doc(1);
    let seed = base.insert(0, "hello world").unwrap();

    let a = doc(2);
    let b = doc(3);
    a.apply_update(&seed).unwrap();
    b.apply_update(&seed).unwrap();

    // Both edit the same index at the same time.
    let ua = a.insert(5, " AAA").unwrap();
    let ub = b.insert(5, " BBB").unwrap();

    // Opposite delivery orders, plus a duplicate.
    a.apply_update(&ub).unwrap();
    b.apply_update(&ua).unwrap();
    b.apply_update(&ua).unwrap();

    assert_eq!(a.text(), b.text());
    assert_eq!(a.encode_state_as_update(), b.encode_state_as_update());
    assert!(a.text().contains("AAA") && a.text().contains("BBB"));
}

#[test]
fn concurrent_delete_and_insert_converge() {
    let a = doc(2);
    let b = doc(3);
    let seed = a.insert(0, "0123456789").unwrap();
    b.apply_update(&seed).unwrap();

    let ua = a.delete(2, 5).unwrap();
    let ub = b.insert(4, "🙂").unwrap();
    a.apply_update(&ub).unwrap();
    b.apply_update(&ua).unwrap();

    assert_eq!(a.text(), b.text());
    assert!(a.text().contains('🙂'));
}

#[test]
fn late_joiner_receives_full_state() {
    let a = doc(2);
    a.insert(0, "hello").unwrap();
    a.insert(5, " नमस्ते").unwrap();
    a.delete(0, 1).unwrap();

    let late = doc(9);
    late.apply_update(&a.encode_state_as_update()).unwrap();
    assert_eq!(late.text(), a.text());
    assert_eq!(late.encode_state_as_update(), a.encode_state_as_update());
}

#[test]
fn update_over_the_size_limit_is_rejected() {
    let d = doc(1);
    let big = vec![0u8; MAX_UPDATE_BYTES + 1];
    assert_eq!(
        d.apply_update(&big),
        Err(CrdtError::TooLarge(MAX_UPDATE_BYTES + 1))
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn apply_update_rejects_garbage_without_panicking(
        bytes in proptest::collection::vec(any::<u8>(), 0..512)
    ) {
        let d = doc(1);
        // Any Ok or Err is fine; a panic or abort fails the test.
        let _ = d.apply_update(&bytes);
        let _ = d.text();
    }

    #[test]
    fn truncated_real_updates_never_panic(cut in 0usize..200) {
        let src = doc(2);
        let update = src.insert(0, "hello 🙂 नमस्ते world").unwrap();
        let cut = cut.min(update.len());
        let d = doc(1);
        let _ = d.apply_update(&update[..cut]);
        let _ = d.text();
    }
}

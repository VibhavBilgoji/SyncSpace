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

/// Bytes that decode to one struct whose client id needs 54 bits. Yrs 0.28.1 trips a
/// `debug_assert!` on it, so this pins the `catch_unwind` guard in `SpikeDoc::apply_update`.
/// Release builds compile the assert out and accept such ids, so this is debug-only.
#[cfg(debug_assertions)]
fn update_with_oversized_client_id() -> Vec<u8> {
    // 1 client block, 1 struct, client id 2^60 (varint), clock 0, GC info byte 0, length 1.
    let mut bytes = vec![1, 1];
    let mut id: u64 = 1 << 60;
    while id >= 0x80 {
        bytes.push((id as u8 & 0x7F) | 0x80);
        id >>= 7;
    }
    bytes.push(id as u8);
    bytes.extend_from_slice(&[0, 0, 1, 0]);
    bytes
}

#[cfg(debug_assertions)]
#[test]
fn hostile_client_id_is_rejected_and_leaves_existing_content_untouched() {
    let d = doc(1);
    d.insert(0, "keep me").unwrap();
    let before = d.encode_state_as_update();
    let result = d.apply_update(&update_with_oversized_client_id());
    // Yrs panics (debug_assert) on this input; the guard turns that into Decode.
    assert_eq!(result, Err(CrdtError::Decode));
    assert_eq!(d.text(), "keep me");
    assert_eq!(d.encode_state_as_update(), before);
    // Still usable afterwards.
    d.insert(7, "!").unwrap();
    assert_eq!(d.text(), "keep me!");
}

#[test]
fn apply_update_on_a_document_that_already_has_content_merges_both_histories() {
    let a = doc(2);
    let b = doc(3);
    a.insert(0, "alpha ").unwrap();
    b.insert(0, "beta ").unwrap();
    // Neither saw the other: applying full states merges, and both ends agree.
    a.apply_update(&b.encode_state_as_update()).unwrap();
    b.apply_update(&a.encode_state_as_update()).unwrap();
    assert_eq!(a.text(), b.text());
    assert!(a.text().contains("alpha ") && a.text().contains("beta "));
    assert_eq!(a.len(), 11);
}

#[test]
fn applying_the_same_update_twice_is_idempotent() {
    let src = doc(2);
    let update = src.insert(0, "once").unwrap();
    let d = doc(1);
    d.apply_update(&update).unwrap();
    let state = d.encode_state_as_update();
    d.apply_update(&update).unwrap();
    assert_eq!(d.text(), "once");
    assert_eq!(d.encode_state_as_update(), state);
}

#[test]
fn update_of_exactly_the_limit_is_not_reported_as_too_large() {
    let d = doc(1);
    let at_limit = vec![0u8; MAX_UPDATE_BYTES];
    // All zeros is a valid, empty update, so it is accepted rather than rejected for size.
    assert_eq!(d.apply_update(&at_limit), Ok(()));
}

#[test]
fn rejected_update_leaves_the_document_unchanged() {
    let d = doc(1);
    d.insert(0, "abc").unwrap();
    let before = d.encode_state_as_update();
    assert_eq!(d.apply_update(&[0xFF; 9]), Err(CrdtError::Decode));
    assert_eq!(d.apply_update(&[]), Err(CrdtError::Decode));
    assert_eq!(d.text(), "abc");
    assert_eq!(d.encode_state_as_update(), before);
}

#[test]
fn empty_insert_and_zero_length_delete_are_accepted_at_the_edges() {
    let d = doc(1);
    d.insert(0, "").unwrap();
    assert!(d.is_empty());
    d.insert(0, "ab").unwrap();
    d.insert(2, "").unwrap(); // index == len is the tail, which is valid
    d.delete(2, 0).unwrap();
    d.delete(0, 2).unwrap();
    assert!(d.is_empty());
    assert!(matches!(d.delete(1, 0), Err(CrdtError::OutOfRange { .. })));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Three replicas make random edits, then every update is delivered to every replica in a
    /// different random order. All must end identical.
    #[test]
    fn replicas_converge_under_any_delivery_order(
        ops in proptest::collection::vec((0usize..3, 0u32..40, any::<bool>(), 1u32..4, "[a-c🙂é]{1,3}"), 1..25),
        seeds in proptest::collection::vec(any::<u64>(), 3),
    ) {
        let replicas = [doc(11), doc(22), doc(33)];
        let mut log: Vec<Vec<u8>> = Vec::new();
        for (who, at, is_delete, len, chunk) in &ops {
            let r = &replicas[*who];
            let n = r.len();
            // Index in UTF-16 units; clamp, and skip edits that would split a surrogate pair.
            let text: Vec<u16> = r.text().encode_utf16().collect();
            let mut idx = (*at).min(n);
            if idx > 0 && idx < n && (0xDC00..=0xDFFF).contains(&text[idx as usize]) {
                idx -= 1;
            }
            if *is_delete && n > 0 {
                let mut end = (idx + *len).min(n);
                if end < n && (0xDC00..=0xDFFF).contains(&text[end as usize]) {
                    end += 1;
                }
                log.push(r.delete(idx, end - idx).unwrap());
            } else {
                log.push(r.insert(idx, chunk).unwrap());
            }
        }
        for (r, seed) in replicas.iter().zip(&seeds) {
            let mut order: Vec<usize> = (0..log.len()).collect();
            let mut s = *seed | 1;
            for i in (1..order.len()).rev() {
                s ^= s << 13; s ^= s >> 7; s ^= s << 17;
                order.swap(i, (s % (i as u64 + 1)) as usize);
            }
            for i in order {
                r.apply_update(&log[i]).unwrap();
            }
        }
        prop_assert_eq!(replicas[0].text(), replicas[1].text());
        prop_assert_eq!(replicas[1].text(), replicas[2].text());
        prop_assert_eq!(replicas[0].encode_state_as_update(), replicas[2].encode_state_as_update());
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    /// Garbage mostly dies at decode. Mutating a valid update keeps it decodable far more
    /// often, so this reaches the integration step, where hostile but well-formed updates
    /// (missing origins, impossible delete ranges) must also be rejected without a panic.
    #[test]
    fn mutated_valid_updates_never_panic_when_applied(
        edits in proptest::collection::vec((0u32..20, "[a-z🙂]{0,4}", 0u32..4), 1..6),
        flips in proptest::collection::vec((any::<usize>(), any::<u8>()), 1..4),
        drops in proptest::option::of(any::<usize>()),
    ) {
        let author = doc(2);
        let mut bytes = Vec::new();
        for (at, chunk, del) in &edits {
            let at = (*at).min(author.len());
            bytes = author.insert(at, chunk).unwrap();
            let del = (*del).min(author.len().saturating_sub(at));
            if del > 0 {
                bytes = author.delete(at, del).unwrap();
            }
        }
        // Deliver the whole history, mutated, to a replica that already has content.
        let mut hostile = author.encode_state_as_update();
        hostile.extend_from_slice(&bytes);
        for (pos, byte) in &flips {
            let len = hostile.len();
            hostile[pos % len] = *byte;
        }
        if let Some(cut) = drops {
            hostile.truncate(cut % (hostile.len() + 1));
        }

        let replica = doc(1);
        replica.insert(0, "existing").unwrap();
        let _ = replica.apply_update(&hostile);
        let _ = replica.text();
        let _ = replica.encode_state_as_update();
    }
}

/// Regression for a bug the mutation property test found: one flipped byte in a valid update
/// was accepted, left an unencodable block in Yrs' pending queue, and the next
/// `encode_state_as_update` panicked inside Yrs. The relay encodes the state for every
/// client that joins, so one hostile update could break the room for everyone.
#[test]
fn single_flipped_byte_cannot_poison_later_state_encoding() {
    let author = doc(2);
    author.insert(0, "aa").unwrap();
    author.delete(0, 1).unwrap();
    author.insert(1, "🙂").unwrap();
    let last = author.delete(1, 1).unwrap();

    let mut hostile = author.encode_state_as_update();
    hostile.extend_from_slice(&last);
    let at = 7026184586290134428usize % hostile.len();
    hostile[at] = 128;

    let replica = doc(1);
    replica.insert(0, "existing").unwrap();
    // Accepting or rejecting is fine; what matters is that the document stays usable.
    let _ = replica.apply_update(&hostile);
    let state = replica.encode_state_as_update();
    let joiner = doc(9);
    joiner.apply_update(&state).unwrap();
    assert_eq!(joiner.text(), replica.text());
}

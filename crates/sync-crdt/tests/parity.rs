//! Native half of the parity test. `tests/wasm.rs` replays the same fixture in the Wasm
//! build and compares against the same expected files.
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code may unwrap (CLAUDE.md rule 3 covers non-test code)

use sync_crdt::SpikeDoc;
use sync_crdt::parity::{doc_from_fixture, encode_fixture};

const UPDATES: &[u8] = include_bytes!("fixtures/parity_updates.bin");
const EXPECTED_STATE: &[u8] = include_bytes!("fixtures/parity_expected_state.bin");
const EXPECTED_TEXT: &str = include_str!("fixtures/parity_expected_text.txt");

#[test]
fn parity_native_matches_fixture() {
    let doc = doc_from_fixture(UPDATES).unwrap();
    assert_eq!(doc.text(), EXPECTED_TEXT);
    assert_eq!(doc.encode_state_as_update(), EXPECTED_STATE);
}

#[test]
fn fixture_is_not_trivial() {
    let doc = doc_from_fixture(UPDATES).unwrap();
    assert!(
        doc.len() > 20,
        "fixture text is too short to be a useful check"
    );
    // Later edits interleave with earlier chunks, so check for the scripts, not exact words.
    assert!(EXPECTED_TEXT.contains('🙂'));
    assert!(
        EXPECTED_TEXT
            .chars()
            .any(|c| ('\u{0900}'..='\u{097F}').contains(&c))
    );
}

/// Deterministic pseudo-random numbers, so the fixture is reproducible.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

/// UTF-16 offsets that fall between characters, so edits never split a surrogate pair.
fn boundaries(text: &str) -> Vec<u32> {
    let mut out = vec![0];
    let mut at = 0u32;
    for c in text.chars() {
        at += c.len_utf16() as u32;
        out.push(at);
    }
    out
}

/// Regenerates the fixture files. Run on purpose, never in CI:
/// `cargo test -p sync-crdt --test parity -- --ignored generate_parity_fixtures`
#[test]
#[ignore = "writes tests/fixtures; run manually when the fixture should change"]
fn generate_parity_fixtures() {
    const CHUNKS: [&str; 6] = ["a", "bc", " ", "🙂", "नमस्ते", "é"];
    let replicas: Vec<SpikeDoc> = [11, 22, 33]
        .iter()
        .map(|id| SpikeDoc::new(*id).unwrap())
        .collect();
    let mut rng = Lcg(42);
    let mut log: Vec<Vec<u8>> = Vec::new();
    let mut delivered = [0usize; 3];

    for step in 0..48 {
        let who = rng.below(3) as usize;
        let text = replicas[who].text();
        let marks = boundaries(&text);
        let update = if marks.len() > 3 && rng.below(4) == 0 {
            let from = rng.below(marks.len() as u64 - 1) as usize;
            let to = from + 1 + rng.below((marks.len() - from - 1).min(4) as u64) as usize;
            replicas[who]
                .delete(marks[from], marks[to] - marks[from])
                .unwrap()
        } else {
            let at = marks[rng.below(marks.len() as u64) as usize];
            replicas[who]
                .insert(at, CHUNKS[rng.below(CHUNKS.len() as u64) as usize])
                .unwrap()
        };
        log.push(update);

        // Partial syncs every few steps leave concurrent edits in the log.
        if step % 5 == 4 {
            let target = rng.below(3) as usize;
            for update in &log[delivered[target]..] {
                replicas[target].apply_update(update).unwrap();
            }
            delivered[target] = log.len();
        }
    }

    // Deliver in a fixed shuffled order so some updates arrive before their dependencies.
    let mut order: Vec<usize> = (0..log.len()).collect();
    order.sort_by_key(|i| (i * 37) % 101);
    let shuffled: Vec<Vec<u8>> = order.iter().map(|i| log[*i].clone()).collect();
    let bytes = encode_fixture(&shuffled).unwrap();

    let doc = doc_from_fixture(&bytes).unwrap();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::write(dir.join("parity_updates.bin"), &bytes).unwrap();
    std::fs::write(
        dir.join("parity_expected_state.bin"),
        doc.encode_state_as_update(),
    )
    .unwrap();
    std::fs::write(dir.join("parity_expected_text.txt"), doc.text()).unwrap();
}

#[test]
fn empty_fixture_gives_an_empty_document() {
    let doc = doc_from_fixture(&[]).unwrap();
    assert!(doc.is_empty());
}

#[test]
fn fixture_round_trips_through_encode_fixture() {
    let src = SpikeDoc::new(2).unwrap();
    let updates = vec![src.insert(0, "ab").unwrap(), src.insert(2, "🙂").unwrap()];
    let doc = doc_from_fixture(&encode_fixture(&updates).unwrap()).unwrap();
    assert_eq!(doc.text(), "ab🙂");
}

#[test]
fn fixture_with_a_short_length_header_is_a_decode_error() {
    for n in 1..4 {
        assert_eq!(
            doc_from_fixture(&vec![1u8; n]).err(),
            Some(sync_crdt::CrdtError::Decode)
        );
    }
}

#[test]
fn fixture_whose_length_prefix_lies_is_a_decode_error() {
    // Claims 1000 bytes, delivers 3.
    let mut lying = 1000u32.to_le_bytes().to_vec();
    lying.extend_from_slice(&[1, 2, 3]);
    assert_eq!(
        doc_from_fixture(&lying).err(),
        Some(sync_crdt::CrdtError::Decode)
    );
    // Claims u32::MAX bytes.
    let mut huge = u32::MAX.to_le_bytes().to_vec();
    huge.push(0);
    assert_eq!(
        doc_from_fixture(&huge).err(),
        Some(sync_crdt::CrdtError::Decode)
    );
}

#[test]
fn fixture_containing_an_oversized_update_reports_too_large() {
    let mut bytes = ((sync_crdt::MAX_UPDATE_BYTES + 1) as u32)
        .to_le_bytes()
        .to_vec();
    bytes.resize(4 + sync_crdt::MAX_UPDATE_BYTES + 1, 0);
    assert!(matches!(
        doc_from_fixture(&bytes).err(),
        Some(sync_crdt::CrdtError::TooLarge(_))
    ));
}

#[test]
fn fixture_with_a_garbage_update_is_an_error_not_a_panic() {
    let mut bytes = 5u32.to_le_bytes().to_vec();
    bytes.extend_from_slice(&[0xFF; 5]);
    assert!(doc_from_fixture(&bytes).is_err());
}

#[test]
fn truncating_the_real_fixture_at_any_point_never_panics() {
    let mut errors = 0;
    for cut in (0..UPDATES.len()).step_by(7) {
        if doc_from_fixture(&UPDATES[..cut]).is_err() {
            errors += 1;
        }
    }
    // Most cuts land inside an update or a header, so most must be reported.
    assert!(errors > 0);
}

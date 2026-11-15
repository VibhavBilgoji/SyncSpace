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
    let bytes = encode_fixture(&shuffled);

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

# Parity fixtures

`parity_updates.bin` is a list of Yrs v1 updates (each prefixed with its length as a
little-endian `u32`) produced by three replicas with fixed client ids and a seeded generator.
It includes concurrent edits, deletes, emoji, Devanagari text and out-of-order delivery.

`parity_expected_state.bin` and `parity_expected_text.txt` are what replaying the updates must
produce. The native test (`tests/parity.rs`) and the Wasm test (`tests/wasm.rs`) both compare
against these files, so the two builds must be byte-identical.

Regenerate only when the fixture should change (for example after a `yrs` upgrade), and review
the diff on purpose:

    cargo test -p sync-crdt --test parity -- --ignored generate_parity_fixtures

## Block document fixture (task 2.1)

`block_parity_updates.bin` is a list of updates (same length-prefixed format) built by
`build_updates` in `tests/block_parity.rs`: three replicas with fixed client ids create blocks of
all six types, edit them at the same time (text, type, order, properties), merge, and carry on.
The updates are in an order in which each one comes after the ones it builds on, because a block
document refuses an update that builds on changes it does not have (`CrdtError::MissingDependencies`).
`block_parity_expected_state.bin` and `block_parity_expected_blocks.json` are what replaying them
must produce, byte for byte in the native test and in the Wasm test.

Regenerate only on purpose (for example after a `yrs` upgrade), and review the diff:

    cargo test -p sync-crdt --test block_parity -- --ignored generate_block_parity_fixtures

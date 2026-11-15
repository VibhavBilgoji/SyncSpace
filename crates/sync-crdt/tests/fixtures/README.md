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

# Testing strategy

Tests are written in the same change as the code they cover. Each roadmap task lists the tests it must add. CI blocks merging when any gate below fails.

## 1. Layers and tools

| Layer | Tool | Location |
|---|---|---|
| CRDT logic (native) | `cargo test`, `proptest` | `crates/sync-crdt/src/**` (unit), `crates/sync-crdt/tests/` |
| CRDT logic (Wasm) | `wasm-bindgen-test` under Node | `crates/sync-crdt/tests/wasm.rs` |
| Server unit | `cargo test` | next to the code (`#[cfg(test)]`) |
| Server integration | `cargo test` + a real MongoDB **8.0** at `MONGODB_URI` (default `mongodb://localhost:27017`: a native install, the compose service, or the CI service container; ADR-011) + `reqwest` and `tokio-tungstenite` clients | `crates/server/tests/` |
| Web unit and components | Vitest + Testing Library + `jsdom` | `apps/web/src/**/*.test.ts(x)` |
| End to end | Playwright (Chromium, Firefox), two browser contexts for collaboration | `e2e/` |
| Load | `tools/loadtest` (Rust, N simulated editors over WebSocket) | manual and nightly |

Each integration test creates its own database (`syncspace_test_<uuid>`) and drops it at the end (also when the test fails), so tests can run in parallel. The MongoDB major version is pinned: `mongodb_major_version_matches_production` fails when the tests run on a version other than the one Atlas free clusters run (`docs/DEPLOYMENT.md` section 3). Tests that do not need a database build the app against an unreachable MongoDB instead.

## 2. Required tests

### sync-crdt
- **Convergence (property test):** 3 to 5 replicas apply random sequences of ops (create, delete, insert, delete text, set type, set props, move) and exchange updates in random orders, with duplicates and delays. All replicas end with identical `blocks()` and identical encoded state.
- **Native and Wasm parity:** a fixed update sequence (checked-in fixture) produces byte-identical `encode_state_as_update_v1` and identical `blocks()` in both builds.
- **Projection:** ordering by `(order, block_id)`, ties, every block type, properties round-trip.
- **Move:** midpoint ordering; concurrent moves of the same block converge to one position with no duplicates.
- **Restore:** `replace_content(live, target)` makes `blocks(live) == blocks(target)`; a concurrent edit applied during the restore merges; restoring an empty target and restoring into an empty doc both work.
- **Validation:** random bytes, truncated updates and oversized updates return errors and never panic (fuzz-style property test, at least 10 000 cases).

### server
- **Auth:** register, duplicate email (409), login success, wrong password and unknown email return the same 401, expired JWT returns 401, logout clears the cookie, argon2 hash is not the password. Implemented in `crates/server/tests/auth.rs` and the unit tests under `crates/server/src/auth/`, which also cover: forged, tampered, `alg: none` and other-algorithm tokens, and expiry with no clock leeway; two simultaneous registrations of one email; the validation rules at every edge (characters, not bytes); the cookie attributes and the `Secure` flag; the hashing capacity limit (429, then recovery) and that an unknown email takes the same hashing path as a known one; CORS on the auth routes; that nothing is audited; and that the log never contains a password, token, hash or email.
- **Tickets:** valid ticket connects; expired, reused, tampered, wrong scope and wrong Origin are rejected.
- **RBAC matrix:** one table-driven test that calls every REST endpoint as ADMIN, EDITOR, VIEWER, non-member and anonymous, and asserts the status from the matrix in `API.md`. Adding an endpoint without adding it to this table fails the test (keep a list of routes and compare with the router). Implemented in `crates/server/tests/rbac_matrix.rs`: the routes come from `app::api_route_table()`, which the router is built from, each needs a fixture in the test, each is checked against the Role column of `API.md`, and a documented route that is neither in the table nor on the test's "not built yet" list fails it. Callers that are non-members get 404 (not 403), see `API.md` section 1.
- **WebSocket frames:** VIEWER Update is rejected with `READ_ONLY` and not applied or broadcast; VIEWER awareness is broadcast; demoting an EDITOR with an open socket makes the next update fail without reconnecting; removing a member closes with 4003; oversized frame closes with 1009; garbage bytes close with 1008.
- **Sync:** two clients converge; a client that edits offline and reconnects merges; a late joiner receives the full state.
- **Snapshots:** dirty doc written within 6 s; clean doc not written; final flush when the last client leaves; the lost-edit race (an edit lands during encoding) is written in the next tick; MongoDB failure restores the dirty flag; `revision` increases; `blocks` equals the projection.
- **Restart durability:** stop the server mid-edit, restart, reconnect clients, final state equals the union of all edits.
- **Versions:** manual save with label and audit row; automatic version only when changed and at most every 10 minutes (use a controllable clock); retention keeps 50 automatic and all manual and restore; numbering has no gaps under 20 concurrent saves; preview returns the archived blocks; restore creates one RESTORE row holding the pre-restore content, applies the target content live to connected clients, writes VERSION_RESTORE, and can be undone by restoring the RESTORE row; `restored_from_kept` false after pruning; VIEWER cannot save or restore.
- **Tasks:** create places at column end; move between neighbours, to head, to tail, into empty column; renumbering when the gap is under `1e-9`; concurrent moves of different cards both persist; assignees must be members; removing a member clears their assignments; BOARD_EVENT reaches workspace sockets.
- **Members:** add by email (404 for unknown email, 409 for duplicate), change role, remove; the owner cannot be demoted or removed (409). Implemented in `crates/server/tests/members.rs` (17 tests: also the case-insensitive email, two simultaneous adds, a role set to itself writing no row, removal clearing task assignments of that workspace only, an admin demoting themselves, editors and viewers refused, malformed ids, the database update refusing the owner on its own) and `tests/workspaces.rs` (12 tests: create, slug and name edges, concurrent slug, list with counts, get, patch, 404 for outsiders identical to an unknown id).
- **Audit:** every one of the 10 actions writes exactly one row with the mapping in `DATA_MODEL.md` §3; no other action writes a row; non-admins get 403; filters and cursor pagination work; there is no update or delete route (assert the router has none for audit).
- **Documents:** delete removes versions and closes sockets with 4004; resources in another workspace return 404.

### web
- Components: every state in `UI_SPEC.md` §6 renders (Button loading and disabled, TaskCard dragging, BlockItem RemoteEdit, VersionCard variants including "no longer kept").
- Role gating: the editor, board, history panel and settings render the right controls for each role.
- Editor logic: minimal diff (prefix and suffix) for insert, delete, replace, emoji and multi-byte text; slash menu filtering and keyboard; caret restoration after a remote update.
- Socket client: reconnect backoff, ticket refresh, handling of every JSON frame type and close code (4001 opens frame 41, 4003 frame 42, 4004 redirect).
- Board: optimistic move and rollback on error; filters update the URL.
- Audit: CSV and JSON export of loaded rows (escaping commas, quotes and new lines).

### end to end (Playwright)
1. Register, create a workspace, create a document, type in three block types, reload, content persists.
2. Two contexts (Admin and Editor) type in the same block at the same time; both end with the same text; each sees the other's cursor.
3. Viewer opens the document: no edit controls; a forced update over the socket is rejected.
4. Admin demotes the Editor while the Editor has the document open; the Editor sees the toast and can no longer type.
5. Go offline in one context (`context.setOffline(true)`), type, go online; the edits merge.
6. Save a named version, edit, restore it; the other context sees the restored content live; the history shows the RESTORE card; restore that RESTORE card to undo.
7. Board: create a task, drag it to another column; the other context sees it move; reload keeps the order.
8. Admin adds and removes a member; audit trail shows MEMBER_ADD and MEMBER_REMOVE; export JSON contains them.
9. Command palette opens with Ctrl/Cmd + K and navigates.
10. Accessibility smoke: `@axe-core/playwright` on login, documents list, editor, board, settings and audit with no serious or critical violations.

### load (report for the project)
`tools/loadtest --editors 20 --duration 60s --rate 5` prints edits/s received, MongoDB writes/s, broadcast latency p50 and p95, and memory per document. Record results in `docs/PROGRESS.md`. Target: writes/s at least 20 times lower than edits/s, p95 under 100 ms.

## 3. Coverage and quality gates (CI)

| Gate | Threshold |
|---|---|
| `cargo fmt --check`, `cargo clippy -D warnings` | must pass |
| `cargo test --workspace` (with MongoDB service) | must pass |
| `wasm-pack test --node crates/sync-crdt` | must pass |
| Line coverage, `sync-crdt` (`cargo llvm-cov`) | 90% or more |
| Line coverage, `server` | 75% or more |
| Web: `lint`, `typecheck`, `test --coverage` | pass; 70% or more lines on `src/lib` and `src/components` |
| Generated TypeScript types up to date | `cargo run -p server --bin gen-types`, then `git diff --exit-code` and no untracked file in `apps/web/src/types/generated` (the `rust` job); the same comparison is the test `the_committed_types_are_up_to_date` in `crates/server/tests/typegen.rs`, so `cargo test` fails locally first |
| Playwright e2e | must pass on `main` and on PRs that touch `apps/web` or `crates/server` |

**How coverage is measured.** `cargo llvm-cov`, line coverage of the native build (the commands are in `CLAUDE.md`). It needs the `llvm-tools-preview` component, which `rust-toolchain.toml` installs, and for `server` a MongoDB 8.0 at `MONGODB_URI`. Three things are not in the numbers: `sync-crdt`'s `wasm.rs`, which only compiles for `wasm32` and is covered by `wasm-pack test`; code that runs only in a child process (`main.rs`, `telemetry.rs`, the spike relay binary), which `crates/server/tests/binary.rs` exercises but whose coverage data is not collected, so those files read 0%; and test code itself. CI runs the `sync-crdt` property tests with `PROPTEST_CASES=500` for the coverage step because the percentage does not depend on the case count; the full runs are in the normal test step and the nightly workflow. Both gates block the build. The current numbers are in `docs/PROGRESS.md`.

## 4. Rules for writing tests

- Test behaviour through public interfaces; no tests of private helpers unless they hold tricky logic (fractional ordering, diffing).
- Use a controllable clock (`trait Clock`) for the 5 s and 10 min workers; never `sleep` for real minutes.
- Each test sets up its own data with builders (`TestWorkspace::new().with_member(Role::Viewer)`); no shared fixtures that tests mutate.
- Name tests after the behaviour: `viewer_update_is_rejected_and_not_broadcast`.
- A bug fix starts with a failing test that reproduces it.
- Property tests are deterministic in CI. They use a fixed seed (`FIXED_SEED` in `crates/sync-crdt/tests/convergence.rs`) and a fixed case count, so a pull request cannot fail by luck: a red run points at a change. Saved failing cases live in `convergence.proptest-regressions`, which is committed and replays first on every run. Never delete lines from it.
- Random exploration is separate. `.github/workflows/nightly-proptest.yml` runs the same tests in release mode with a new seed every night (`PROPTEST_RNG_SEED`, `PROPTEST_CASES`), with a timeout so a hang counts as a failure, and opens an issue on failure instead of blocking pull requests. To reproduce a nightly failure: `PROPTEST_RNG_SEED=<seed> PROPTEST_CASES=<cases> cargo test -p sync-crdt --release --test convergence`. When it finds a bug, add the minimal case as a plain test with its input spelled out (do not rely on the seed) and commit the `cc` line from the regressions file.
- Hostile-input tests run in release mode as well as debug (CI does both for `sync-crdt`): release builds drop `debug_assert!` and wrap integer overflow unless it is switched on, so the same input can be rejected in one profile and accepted in the other.

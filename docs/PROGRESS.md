# Progress log

Claude updates this file at the end of every task. Newest entries at the top of each section.

## Status

| Phase | Status | Tag |
|---|---|---|
| 0 Bootstrap and spike | complete | `phase-0` on `e2bb9b1` |
| 1 Accounts, workspaces, design system | in progress (1.1 to 1.4 done; 1.5 and 1.6 are web-track tasks for Monday) | |
| 2 Documents and live editing | not started | |
| 3 Collaboration details | not started | |
| 4 Version history | not started | |
| 5 Kanban | not started | |
| 6 Settings and audit | not started | |
| 7 Hardening, measurement, deployment | not started | |
| 8 Redis (optional) | not started | |

## Task log

| Date | Task | Result | Tests added | Notes |
|---|---|---|---|---|
| 2026-11-15 | 0.1 Monorepo bootstrap | done | 4 (2 Rust, 1 Vitest, 1 Playwright x2 browsers) | `cargo test`, web lint/typecheck/test/build and e2e smoke all green. Docker compose file not run (Docker not installed) |
| 2026-11-15 | 0.2 CI workflow | done (PR #1, green) | 0 (config only) | First run failed at `typecheck` (generated `LayoutProps` missing on a clean checkout); fixed with `next typegen` |
| 2026-11-15 | 0.3 Yrs native + Wasm spike | done (PR #2, green) | 9 convergence + 2 parity + 6 relay, 5 Wasm, 16 web, 2 e2e specs x 2 browsers; replaced 2 placeholders | **Yrs works in Wasm: parity passes, byte-identical.** See "Spike 0.3 findings" |
| 2026-11-15 | Phase 0 review | done (PR #3, green; tagged `phase-0` on `e2bb9b1`) | +31 tests from `test-writer`; +14 from the review (6 in sync-crdt: mutation property test, 3 regressions, fixture drift guard, state-unchanged tests; 3 relay integration; 5 relay unit) | Final counts: native 22 convergence (20 in release, two tests are debug-only) + 9 parity + 14 relay + 5 relay unit (+2 ignored fixture generators), 8 Wasm, 24 web, 6 e2e. See "Phase 0 review" |
| 2026-11-16 | 1.1 Server skeleton | done (PR #5, green, merged as `dfd11ab`) | 59 new: 22 unit (config 12, error 6, db 4), 24 API integration, 9 index integration, 4 process-level; all pass locally on MongoDB 8.0.23 | Config, tracing, `/healthz`, MongoDB connection and indexes, error format, 256 KiB body limit, CORS, panic net, graceful shutdown. Each protection was disabled in turn and a test failed every time. CI now has a `mongo:8.0` service. See "MongoDB version alignment" |
| 2026-11-16 | Coverage gates and ADR-011 | done (PR #6, green, merged as `144e57e`) | 0 (CI and docs) | `sync-crdt` 99.07% and `server` 88.37% line coverage, both gates blocking from now. Testcontainers question closed by ADR-011 |
| 2026-11-16 | 1.2 Auth | done (PR #7, `rust`, `web` and `e2e` green, merged as `64fa51e`) | 66 new: 37 unit (password 7, session 12, cookie 9, validation 9) and 29 integration against MongoDB 8.0.23; 1.1 tests unchanged | Register, login, logout, `me`; argon2id; HS256 JWT in an `HttpOnly` cookie; `AuthUser` extractor. Ten protections were disabled in turn and a test failed every time. A real-binary walkthrough with `curl` and a cookie jar passed and its log held no secret. GitGuardian flagged the fake passwords in the test fixtures; they were replaced with plain-word phrases, the old commit still holds them, and the findings were marked as false positives in the GitGuardian dashboard before the merge |
| 2026-11-16 | 1.3 Workspaces, members, RBAC, audit writer | done (PR #8, all four checks green, merged as `924c98f`) | 50 new: 15 unit (rbac 4, audit 5, workspace validation 5, db 1) and 35 integration against MongoDB 8.0.23 (`workspaces.rs` 12, `members.rs` 17, `rbac_matrix.rs` 6) | Eight `/workspaces` endpoints, `rbac::require_role`, `audit::record` for MEMBER_ADD, MEMBER_REMOVE and ROLE_CHANGE, and a route table the router is built from. Nine protections were disabled in turn (owner guard in the handler and in the database update, role check, non-member 404, audit insert, audit content, duplicate-member filter, assignment cleanup, the table against `API.md`) and a test failed every time. Server line coverage 94.51%. A real-binary walkthrough (register two users, create, add, refuse, promote, list, refuse the owner, remove, 404 afterwards) passed and its log held no email, password or cookie. One full `cargo test --workspace` run failed in `rbac_matrix` at `ensure_indexes` after a 43-minute stall of the machine; the rerun passed (see open questions) |
| 2026-11-16 | [server] 1.4 `ts-rs` types and staleness check | implemented, **CI run pending** | 9 integration tests (`tests/typegen.rs`) and 3 unit tests (`api_types.rs`) | `cargo run -p server --bin gen-types` writes 28 types plus `index.ts` to `apps/web/src/types/generated/`; the same check runs as a test and as a CI step. Five protections were disabled in turn (stale file, dropped export, stale cleaning, serde rename, bigint) and a test failed every time |
| 2026-11-16 | Friday submission plan | done (docs only) | 0 | `docs/ROADMAP.md` rewritten for the deadline of Friday 20 November: MUST, SHOULD and post-submission lists, a server track and a web track with daily batches (about 36 h and 39 h against 48 h each), deploy on Wednesday, tests scaled to the deadline, and the trims approved by the project lead. See "Friday plan" below |

## Decisions made during implementation

(Short notes. Anything that changes architecture becomes an ADR in `DECISIONS.md`.)

### Spike 0.3 findings

- **Result: the ADR-002 bet holds.** `yrs` 0.28.1 (pinned `=0.28.1`) compiles and runs on `wasm32-unknown-unknown`. The parity fixture (48 edits from 3 replicas, concurrent edits, deletes, emoji, Devanagari, shuffled delivery) replays to a byte-identical 452-byte state and identical text natively and in Wasm. The Yjs fallback ADR was **not** needed.
- Two browser contexts converge through the Rust relay in Chromium and Firefox, including concurrent typing, a late joiner (reload) and an emoji replaced by another sharing its first UTF-16 unit.
- Size: the `--release` `.wasm` is 425 KB after `wasm-opt`.
- **Offsets are UTF-16** (`OffsetKind::Utf16`) in both builds, so indices match JS strings and textarea selection. Yrs defaults to bytes; the default would corrupt non-ASCII edits.
- **Client ids and the guid are always passed in.** On `wasm32-unknown-unknown`, `fastrand` has no entropy source (its seed is `None`), so Yrs' "random" client id and UUID would be the same in every tab, and two tabs sharing a client id corrupt the document. The page takes a 53-bit id from `crypto.getRandomValues`; the doc guid is a constant. Use `Options::with_guid_and_client_id`, never `Doc::new()`, in Phase 2.
- **Yrs panics on hostile input.** (1) Decoding an update whose client id does not fit in 53 bits trips a `debug_assert!` in debug builds (`yrs` `block.rs:93`). (2) One flipped byte in a valid update can decode and apply, then make the next `encode_state_as_update` panic inside Yrs (`merge_pending_v1` unwraps, `transaction.rs:390`); the relay encodes the state for every joining client, so one hostile update could break the room. Found by the property tests. `SpikeDoc::apply_update` now decodes inside `catch_unwind` and only accepts updates that survive an encode and decode round trip. Panics abort on Wasm, so only the server accepts untrusted bytes, and it validates before relaying. (3) **A hostile update can make Yrs panic, wrap or hang while integrating it.** `block_store.rs:320` does an unchecked `u32` subtraction (`skip.next_clock() - block.next_clock()`) when an update's clocks do not line up. Debug builds panic (`attempt to subtract with overflow`). **Release builds wrap silently, return `Ok` and leave a corrupt document, and one release run of the mutation test ran for over 60 s (normally about 6 s).** (An earlier note here called this an `unwrap`; that was wrong, the standalone reproduction below shows the real cause.) It surfaced after `cargo test` failed on CI once and passed on the next run of the same code. The CI log was not readable from here, so the failing test was never seen directly; the mutation property test (fresh random seeds every run) is the likely cause, because under CPU load it failed locally on exactly this path about once per 2 000 cases. CI now annotates failing tests, and the property tests use a fixed seed, so a run can no longer fail by luck. Mitigations, each with a test that fails without it: `[profile.release.package.yrs] overflow-checks = true` in `Cargo.toml` (the overflow becomes an ordinary panic in release and in the Wasm release build too); `SpikeDoc::apply_update` **rehearses** the update on a scratch replica built from its own state (apply, then encode) inside `catch_unwind`, and only if that is clean touches the real document; CI also runs the hostile-input tests in release mode. A compile-time guard refuses `panic = "abort"` on native targets, since `catch_unwind` would silently stop working.
  - **Cost and alternatives (decide in the Phase 2 load test):** the rehearsal costs one extra encode and decode of the room state per update, so it grows with document size. The alternative is a **shadow document**: keep a persistent shadow replica per room, apply each update to the shadow first inside `catch_unwind`, and only if that succeeds apply it to the live document; after a panic the shadow may be half integrated, so it is rebuilt from the live document (cost only when someone attacks) and the update is rejected. That removes the per-update copy at the price of double the memory per room and one more thing to keep in sync. Measure edits/s and CPU for both at the expected document sizes, and prefer an upstream fix or a pinned patch if one exists.
  - **The room lock:** the rehearsal runs while the room lock is held. It never crosses an `.await` (CLAUDE.md rule 4), and a panic caught inside it does not poison the lock. Rehearsing outside the lock would let another update change the state between the rehearsal and the real apply, so the rehearsal would no longer prove the real apply is safe; an optimistic variant (rehearse on a snapshot, re-check a version counter under the lock, retry on change) is possible if lock hold time shows up in the load test.
  - **Wasm cannot catch panics.** The browser relies on the server rejecting bad updates before they are broadcast; see `docs/ARCHITECTURE.md` section 4.
  - Phase 2 must keep all of the above, and add fuzzing (Phase 7).
- **Frame size limit:** the WebSocket library drops the TCP connection without a close frame when a frame exceeds its limit, so a clean `1009` needs our own check below a higher library cap (1 MiB check, 4 MiB cap). The Phase 2 hub should copy this.
- Turbopack bundles the `wasm-bindgen --target web` output as a hashed static asset (`sync_crdt_bg.<hash>.wasm`); the public-folder fallback was not needed.
- `wasm-pack` 0.15.1 installs with `cargo install wasm-pack --locked`; CI uses `taiki-e/install-action`.

### Decisions

- 0.1: `unwrap_used` and `expect_used` are denied workspace-wide in clippy (allowed in tests) to enforce CLAUDE.md rule 3.
- 0.1: scaffolded with the current `create-next-app` (Next 16, React 19, Tailwind 4). Its `cacheComponents` and `partialPrefetching` flags are kept as generated. `apps/web/AGENTS.md` was generated by Next and is kept.
- 0.1: the `/api/*` rewrite in `next.config.ts` is only active when `API_ORIGIN` is set, so builds work without it.
- 0.3: local edits return their update bytes (`insert`/`delete` give `Vec<u8>` / `Uint8Array`) instead of exposing a Yrs observer callback: the observer needs `Send + Sync` callbacks, which a JS function is not, and returning bytes is simpler and cannot re-enter.
- Review: property tests use a fixed seed in CI and locally (`FIXED_SEED`), with saved failing cases in the committed `convergence.proptest-regressions`; `.github/workflows/nightly-proptest.yml` explores new seeds in release mode every night and opens an issue on failure instead of blocking pull requests. CI also runs the hostile-input tests in release mode. See `docs/TESTING.md` section 4.
- Review: `[profile.release.package.yrs] overflow-checks = true` in `Cargo.toml` (see the Yrs finding); the Wasm size is unchanged (426 192 bytes).
- 0.3: e2e runs with one worker because the throwaway relay holds a single shared document.
- 0.3: native test files use an inner `#![allow(clippy::unwrap_used, clippy::expect_used)]`, since clippy's `allow-unwrap-in-tests` does not cover helper functions in integration tests.
- 0.2: gates that cannot pass yet are `# TODO(task)` stubs in `ci.yml` (llvm-cov thresholds, web coverage, generated types, Mongo service). `DEPLOYMENT.md` §4 explains the current state.
- 0.2: `typecheck` now runs `next typegen` first. `LayoutProps` is a global type that Next generates into `.next/types`, so `tsc` failed on a clean checkout (first CI run) while passing locally where `.next` existed.
- 0.2: `.gitattributes` (`eol=lf`) added so `prettier --check` is identical on Windows and Linux runners.
- 0.1: the `wasm` script for `apps/web` was deferred to 0.3 and is added there.

- 1.1: `GET /healthz` is served at the server root, not under `/api/v1` (`API.md` lists it without the prefix; Render's health check and the deploy checklist call `/healthz`). It answers 200 `{"status":"ok","mongo":"ok"}`, or 503 `{"status":"degraded","mongo":"error"}` when a MongoDB ping fails or takes over 2 s; the cause goes to the log, never to the client. `API.md` now documents both.
- 1.1: every response the server produces uses `{"error":{"code","message","details"?}}`, including unknown paths (404), wrong methods (405), oversized bodies (413) and caught panics (500). Three codes were added to the `API.md` table: `METHOD_NOT_ALLOWED`, `PAYLOAD_TOO_LARGE`, `INTERNAL`. An internal error's cause is logged and never returned. Malformed JSON and a wrong `Content-Type` are 400; valid JSON of the wrong shape is 422 with the reason in `details.body`.
- 1.1: configuration fails fast and reports every problem at once. It requires `MONGODB_URI`, `JWT_SECRET`, `TICKET_SECRET` and `ALLOWED_ORIGINS` already (task 1.2 uses the secrets), so a bad deploy fails at start. Secrets are base64 of at least 32 bytes and must differ; origins are `scheme://host[:port]` with no path and no wildcard; `COOKIE_SECURE` defaults to true. `Secret<T>` prints as `<redacted>`. Exit code 2 means bad configuration, 1 means start-up failed (MongoDB unreachable, indexes failed).
- 1.1: middleware order, outermost first: request logging, CORS, 256 KiB body limit, panic catching. CORS is outside so 404, 413 and 500 carry its headers; panic catching is innermost so a panic becomes a 500 in the error format. CORS grants `Allow-Origin` **and** `Allow-Credentials` to the configured origins only (`AllowCredentials::predicate`); a foreign origin gets no CORS header at all.
- 1.1: request logs hold method, path, status and latency, and never the query string (WebSocket tickets travel there), headers, cookies or bodies. Output goes to stdout; `RUST_LOG` is honoured. A grep of a real run's log for the ticket, the cookie, both signing keys and the MongoDB URI found none of them.
- 1.1: the driver connects lazily with a 5 s server-selection timeout (unless the URI sets one); start-up pings and exits non-zero when MongoDB is unreachable. All ten indexes of `DATA_MODEL.md` section 4 are created at start from one table (`db::INDEXES`); a test parses the document and compares it with that table, so the two cannot drift apart.
- 1.1: the workspace `rust-version` is now `1.94.1`, the minimum the driver's release notes give for `mongodb` 3.10 (it was a stale `1.85`). Side effect: clippy lints that depend on the declared minimum version are active, for example `manual_is_multiple_of`.
- 1.1: server integration tests read `MONGODB_URI` (default `mongodb://localhost:27017`), give each test a database `syncspace_test_<uuid>` and drop it with a guard that also runs when a test fails (after many runs and killed processes no test database was left behind). Tests that do not need a database build the app against an unreachable MongoDB.
- ADR-011 (replaces the earlier "testcontainers deferred" open question): server integration tests use the MongoDB at `MONGODB_URI`, a native `mongod` 8.0 locally and a `mongo:8.0` service container in CI, with a database per test. No test starts containers, so no Docker is needed. `docs/TESTING.md` and `docs/DEPLOYMENT.md` say the same.

- 1.2: passwords are hashed with argon2id (19 MiB, 2 iterations, 1 lane, PHC string in `users.password_hash`) on blocking threads, at most **4 at once** (about 76 MiB), because Render's free instance has 512 MB and an unbounded pool would let a few dozen parallel logins kill it. A request that cannot get its turn within 5 s gets 429 `RATE_LIMITED`. This bounds memory; it is not per-user throttling. Measured on this machine in a debug build with the hashing crates optimised: about 80 ms per hash.
- 1.2: login for an unknown email verifies a dummy hash, and every failed login returns the byte-identical 401 `invalid email or password`. In the real run, wrong password took 80 ms, unknown email 91 ms and a correct login 80 ms. The over-long-input shortcut (a password over 128 characters, an email over 254) skips hashing and returns the same 401.
- 1.2: the session is an HS256 JWT (`sub`, `iat`, `exp` = 24 h) in `ss_session=...; Max-Age=86400; Path=/; HttpOnly; SameSite=Lax` (`Secure` when `COOKIE_SECURE`). Verification accepts only HS256, requires `exp` and `sub`, and uses **zero leeway** (the library default is 60 s). `jsonwebtoken` 12 has no default crypto backend; the pure-Rust `rust_crypto` feature is used, so no C toolchain is needed. `argon2` 0.6 reads the cost parameters from the stored hash, so raising them later does not lock anyone out.
- 1.2: `AuthUser` (the extractor every protected route will use) checks the token and nothing else, with no database call; `GET /auth/me` loads the user, and a token for an account that no longer exists is 401. Missing, expired, forged and malformed sessions all give the same `not signed in`.
- 1.2: emails are stored trimmed and lower-cased, ASCII only; that is what makes the unique index case-insensitive. Registration checks for the email first (so a repeat costs a lookup and not a hash) and the unique index settles two requests that race: one 201, one 409.
- 1.2: `db.rs` became `db/mod.rs` so the users repository lives in `db/users.rs`; routes are nested under `/api/v1` (`/healthz` stays at the root). `AppState` gained `auth` (the hashing service) and a constructor `AppState::new`.
- 1.2: in debug builds the `argon2` and `blake2` crates are compiled with `opt-level = 3` (`[profile.dev.package.*]`); without it every test that registers a user would take seconds.
- 1.2 (test harness): log capture in the integration tests is now one process-wide `tracing` subscriber with each test reading only what was logged after it started. The earlier per-thread subscriber lost every line when other test threads reached a call site first (`tracing` caches per call site whether anyone listens): the auth log test failed in 2 of 3 parallel runs with an empty log. The task 1.1 log tests had the same weakness and were switched too; 10 consecutive parallel runs of the auth and API suites pass.
- 1.3: **a non-member gets 404, a member with too low a role gets 403.** `API.md` listed "not a member" under 403 and also said a resource in a workspace the caller is not in is 404 so existence does not leak; the two disagree. The 404 rule won (a 403 would tell an outsider that the workspace exists). The 403 row of `API.md` now says "role too low". `rbac::require_role` is the one place that decides it, and a test checks that an outsider's answer for a real workspace equals the answer for an id that does not exist, byte for byte.
- 1.3: `rbac::Role` is `Viewer < Editor < Admin` (serialised `VIEWER`, `EDITOR`, `ADMIN`). A malformed `:wid` or `:uid` is 400 in the error format (the path parameters are read as strings and parsed by `rbac::parse_id`, so axum's own plain-text rejection never appears). The role is checked before the body is read, so a refused caller never learns whether their body was valid.
- 1.3: **the router is built from a route table** (`app::api_route_table()`, one `RouteSpec { method, path, access, handler }` per route). A route cannot exist without an entry. `tests/rbac_matrix.rs` walks the table: it needs a fixture for each entry, calls each route as an anonymous caller, a signed-in outsider, a VIEWER, an EDITOR and an ADMIN and compares the status with the matrix (anonymous 401, outsider 404, role too low 403, otherwise success), checks every entry against the Role column of `docs/API.md`, and fails if `API.md` documents a route that is neither in the table nor on an explicit "not built yet" list.
- 1.3: membership changes are one MongoDB update each with the rule in the filter: add is `$push` filtered on "not already a member", change role and remove are `findOneAndUpdate` filtered on "not the owner" and "is a member" and return the document as it was, which gives the `from` role and the removed role for the audit row. The handler checks the same rules first to pick the status (409 owner, 404 not a member); the filter makes a race between two requests unable to break them, and `the_database_updates_themselves_refuse_to_touch_the_owner` tests that second guard alone.
- 1.3: setting a member to the role they already have is 200 with no audit row (nothing changed). Removing a member also pulls them from `assignee_ids` of that workspace's tasks (invariant 6; the collection is empty until Phase 5, the test inserts task documents directly). Creating or patching a workspace is not audited because it is not in the audit table.
- 1.3: `audit::record` is the only code that names `audit_logs` (plus the index table); a unit test scans the source and fails if another file does, or if `audit.rs` calls an update, replace or delete.
- 1.3: `GET /workspaces` counts documents and boards with two grouped aggregations for all the caller's workspaces, not one query per card. Both collections are empty until Phases 2 and 5, so those counts are 0 in practice; the integration test inserts rows to prove the counting.
- 1.3: the member list keeps the array's own order (which is join order, because members are appended) and only lifts the owner to the front. Sorting by `joined_at` was flaky: two members added in the same millisecond tie.
- 1.3 (test harness): `tests/common` gained `TestApp::account` (inserts a user and signs a session with no password hashing), `TestApp::call`, `TestApp::audit_rows` and `TestWorkspace::new(&app).await.with_member(Role::Viewer).await`, as `TESTING.md` asks.

## Friday plan (written Monday 16 November, updated the same day)

The full plan is `docs/ROADMAP.md`. Summary:

- **State.** Tasks 0.1 to 0.3 and 1.1 to 1.3 are merged on `main` (`924c98f`). **1.4 is not merged and not started**, although the deadline message said it was; it is the first task of the server track on Monday.
- **Two tracks, split by directory.** Server track (`crates/`, the project lead): 1.4, 2.1 to 2.4, 3.1 and 3.3 server parts, 4.1 to 4.4, 5.1, 6.1, 7.4 server part, 7.5. Web track (`apps/web/`, `e2e/`, a teammate in a parallel session): 1.5, 1.6, 2.5, 2.6, 3.1 to 3.3 web parts, 4.5, 5.2, 5.3, 6.2, 6.3, 7.4 Vercel. The web track builds against `docs/API.md` and the generated types, with typed mocks until each endpoint merges (hand-off table in the roadmap).
- **Estimated load per track** (Mon / Tue / Wed / Thu): server 9 / 10.5 / 11.5 / 5 = **36 h**; web 9 / 7 / 11 / 12 = **39 h**; each against about 48 h available. These are estimates, not measurements; the web numbers are the least certain (only the spike page exists today). The earlier single-track estimate of 63.5 h against 48 h no longer applies: the parallel split removes the overload, and its first draft also double-counted the savings of the cut ladder (6.3 and 5.2 were each freeing as many hours as the whole task was estimated at).
- **Deploy on Wednesday:** server on Render with Atlas (server track), web on Vercel (web track).
- **Tests scaled.** Server logic and every permission check keep unit and integration tests; component tests for the main states; e2e only scenarios 1, 2, 3, 6 and 7. Scenarios 4, 5, 8, 9 and 10 are post-submission.

## Decisions (Friday plan, Monday 16 November)

- **Trims approved by the project lead:** 6.3 is the audit list with filters (JSON inspector and export are post-submission); 5.2 without filters; 6.2 members screen only (general settings and danger zone are post-submission); 3.2 without "Waiting to sync"; 3.3 reduced to "a demoted or removed user stops editing at once".
- **Kept in full:** 2.4 with the SIGTERM flush and the metrics counters (they produce the edits/s versus writes/s number) and 4.2 automatic versions (FR15). **Re-check on Wednesday evening:** if the server track is behind, 4.2 is the first drop and the project lead decides then.
- **Coverage:** the two Rust gates (`sync-crdt` 90%, `server` 75%) stay blocking as they are; no web coverage gate is added.

## Contract changes (server track writes here when `docs/API.md` or a close code changes)

- **2026-11-16, task 1.4:** the TypeScript types of the API now exist in `apps/web/src/types/generated/` (import from `index.ts`). They are generated by `cargo run -p server --bin gen-types` and committed; **do not edit them by hand**, and if one is wrong ask for a change under "Contract requests". Delete `apps/web/src/types/pending.ts` if you created one.
- **2026-11-16, task 1.4:** two one-line edits in `apps/web` made by the server track so that lint stays green: `src/types/generated` in `.prettierignore` and `src/types/generated/**` in the `eslint.config.mjs` ignores.
- **2026-11-16, task 1.4, provisional:** `Document`, `DocumentSummary`, `Board`, `StatusCounts`, `UserRef` and `Page<T>` were written from the tables of `API.md` section 2 where section 4 gave no shape; they may change when tasks 2.2 and 5.1 build the endpoints (the generated files are then regenerated). `ErrorCode` and `ApiErrorBody` describe the error body. The WebSocket frame types arrive with task 2.3.

## Contract requests (web track writes here; the server track decides)

(none yet)

## Open questions (Friday plan)

- **The web track's hours are the least certain estimate of the plan.** If the editor (2.6, 6 h) overruns on Tuesday, Wednesday's deploy day is the next to slip; the server track's spare Thursday hours can take over integration work that is blocked on the server.
- **Who runs the e2e job when both tracks merge on the same day?** The e2e job needs the web build and the API; `ci.yml` is owned by the server track, so a failing e2e on a web PR may be waiting on a server merge. Merge server PRs first on days when a web task depends on them (the hand-off table).

## Open questions (earlier)

(Conflicts between docs, unclear requirements, things the team must decide.)

- **No transaction around a membership change and its audit row.** CI and local MongoDB run as a standalone server, which has no transactions. A change is committed first and the audit row second, so a failure between them (the request is a 500) leaves the change without its row. Fix with a transaction once the tests can run on a replica set; Atlas supports them.
- **Role changes and removals do not reach open sessions yet.** `API.md` promises `ROLE_CHANGED` and close code 4003, which need the WebSocket hub of task 2.3. The handlers have a marked place for it. Until then nothing is open, and REST re-reads membership on every request, so a removed or demoted user is refused immediately.
- **A local test run once failed on a MongoDB stall.** `cargo test --workspace` ran for 2571 s (the machine appears to have been suspended) and `rbac_matrix` panicked on the `ensure_indexes().await.unwrap()` in `tests/common/mod.rs` when the connection timed out; the immediate rerun passed (all suites, three more repeated runs of the new ones). Not a product bug, but it shows the harness reports a MongoDB hiccup as a bare unwrap; say if you want a clearer message there.
- **`WorkspaceSummary` is my reading of "cards with role and counts"** (`id, name, slug, role, is_owner, member_count, document_count, board_count`). The Figma frame 05 could not be opened here (no Figma MCP); check the card against the frame in task 1.6.

- **Delete the spike in Phase 2** (when the real hub lands): `crates/server/src/spike.rs`, `crates/server/src/bin/spike_relay.rs`, `crates/server/tests/spike_relay.rs`, `apps/web/src/app/spike/`, `e2e/tests/spike.spec.ts`, the relay `webServer` in `e2e/playwright.config.ts`, and the `SpikeDoc`/`WasmDoc` API in `sync-crdt` once the block schema replaces it. Keep `textDiff`, the parity fixture approach and the size-limit pattern.
- The garbage-input property test takes about 50 s because every caught panic prints a message. Acceptable for now; revisit if CI time matters.
- Docker is not installed on the dev machine, so `infrastructure/docker-compose.yml` is unverified. Run `docker compose -f infrastructure/docker-compose.yml config` and `up -d mongo` once available. Needed before Phase 1 integration tests.

- **Atlas can change the MongoDB major version under us.** Atlas upgrades free clusters to the newest major version after several patch releases of it, and you cannot upgrade one yourself. The version guard test only sees local and CI. Check the cluster's version in Atlas at the first deploy and whenever Atlas announces an upgrade, and follow the bump procedure below if it moved.
- **`wasm-pack test --node` sometimes exits 1 on Windows after all tests passed: Node crashes while shutting down.** Captured verbatim (2026-11-16, local, Node 24.15.0, `wasm-pack` 0.15.1):
  ```
  test result: ok. 8 passed; 0 failed; 0 ignored; 0 filtered out; finished in 0.07s

  Assertion failed: !(handle->flags & UV_HANDLE_CLOSING), file srcwinasync.c, line 76
  Error: Node failed with exit_code -1073740791
  error: test failed, to rerun pass `--test wasm`
  ```
  `-1073740791` is `0xC0000409` (a fast-fail abort). It is a libuv assertion in Node's Windows-only source (`srcwinasync.c`) at process exit, after the results are printed, so it is not a failing test and not our code. It is intermittent: 2 failures, both immediately after a ~100 s CPU-heavy release-mode test run, against 13 consecutive standalone passes. The Linux CI runs of `wasm-pack test` passed every time (PRs 5 and 6), which fits a Windows-only shutdown race. If it blocks you locally, run the command again; if it ever shows up in CI, record that output here and look at pinning the Node version for that step.
- **Windows Firewall may ask about `server.exe`.** The process-level tests start the real binary, which binds `0.0.0.0:$PORT` (Render needs that). Allowing or cancelling the prompt does not matter for the tests, which connect over loopback.
- The empty folder `C:\Program Files\MongoDB\Server\9.0` is left over from the removed 9.0 install. It is harmless; the running service is 8.0.23.

- **Sessions cannot be revoked.** The JWT is stateless: logout clears the cookie, but a stolen token stays valid until it expires (24 h). A revocation list or a per-user token version would fix it; it is not in scope for 1.2.
- **No login throttling per account or IP yet.** Only the memory cap on concurrent hashing exists, so online password guessing is not slowed. Planned for Phase 7 hardening; say if it should come sooner.
- **Logout needs a valid session** (as `API.md` says). A client whose cookie has already expired gets 401 from logout; the browser drops an expired cookie by itself, so this is harmless.
- **Emails are ASCII only** (no internationalised addresses). Simple and strict.
- **Registration reveals whether an email is taken** (the 409 required by `API.md`). Login does not.

## MongoDB version alignment (task 1.1)

Checked on 2026-11-16, before changing any version:

| Question | Finding | Source |
|---|---|---|
| What do Atlas free (M0) clusters run? | **MongoDB 8.0.** "Atlas uses MongoDB 8.0 for Free clusters." You cannot upgrade a free cluster yourself; Atlas upgrades them to the newest version after several patch releases of it. | Atlas docs, "Atlas Free Cluster Limits" |
| Does the pinned Rust driver support it? | `mongodb` **3.10.1**. The compatibility table lists driver 3.9 to 3.10 as fully compatible (all features) with MongoDB **9.0, 8.0 and 7.0**; 3.6 to 3.8 are only partly compatible with 9.0. The driver is therefore not what limits the choice. | MongoDB docs, "Client Library Compatibility Tables" (Rust) and the Rust driver release notes |
| Does the `mongo:9` image exist? | Yes. `mongo:9.0`, `mongo:9`, `mongo:8.0`, `mongo:8`, `mongo:7.0` and `latest` all exist on Docker Hub (linux/amd64, linux/arm64, windows/amd64), last updated 2026-11-12. | Docker Hub API, `library/mongo` tags |
| What was installed locally? | MongoDB 9.0.4 first. It was removed on 2026-11-16 and 8.0.23 installed instead (Windows service `MongoDB`, `localhost:27017`, no auth), verified by service state, listening port and `mongod --version`. | local checks |

**Decision: MongoDB 8.0 everywhere**, so no test passes on a version production does not run. Chosen by the project owner after the Atlas finding, as they had said they would prefer 8.x locally to testing on a version production does not run.

| Where | Now |
|---|---|
| Local development | MongoDB Community Server 8.0.23 |
| CI | `mongo:8.0` service container (`.github/workflows/ci.yml`) |
| `infrastructure/docker-compose.yml` | `mongo:8.0` |
| Docs | `DEPLOYMENT.md` sections 3, 4, 10, 11; `TESTING.md`; `README.md`; `CLAUDE.md`; `ROADMAP.md` row 0.1; `.env.example` |
| Enforced by | `mongodb_major_version_matches_production` in `crates/server/tests/db_indexes.rs` (constant `EXPECTED_MONGODB_MAJOR` in `tests/common/mod.rs`) fails when tests run on another major version |

**Bump procedure** (when Atlas moves the cluster to a newer major version): change `EXPECTED_MONGODB_MAJOR`, the CI image, the compose image, the local install, the paragraph in `DEPLOYMENT.md` section 3 and this section, in one change, and run the full tests on the new version first. The driver already supports 9.0 fully, so no driver change is expected.

Confirmed on a real run (PR #5): the `mongo:8.0` service container and its `mongosh` health check initialise on the GitHub runner and every Mongo-backed test passes there.

Not verified: the Rust driver's own minimum Rust version is inconsistent between sources (the 3.10 release notes say 1.94.1, the crates.io metadata for 3.10.1 says 1.88); I used the higher figure for `rust-version`. The toolchain is pinned to 1.100.0, so builds are unaffected either way.

## Coverage (measured 2026-11-16)

Line coverage of the native build with `cargo llvm-cov` 0.9.1 on the code of PR #5, against MongoDB 8.0.23. Both gates are **blocking in CI from this change**: the server crate is above its 75% threshold, so the report-only fallback was not needed.

| Crate | Lines | Regions | Functions | Gate (lines) | Status |
|---|---|---|---|---|---|
| `sync-crdt` | **99.07%** (107 of 108) | 95.20% | 94.12% | 90% | blocking |
| `server` | **88.37%** (889 of 1006) | 87.09% | 85.44% | 75% | blocking |

`server` by file (lines): `db.rs` 100%, `error.rs` 98.3%, `config.rs` 95.8%, `spike.rs` 94.5%, `extract.rs` 93.3%, `health.rs` 86.7%, `app.rs` 75.0%, `main.rs` 0%, `telemetry.rs` 0%, `bin/spike_relay.rs` 0%. 64 of the 117 missed lines are in the last three files, which run only in a child process: `crates/server/tests/binary.rs` exercises them, but the child's coverage data is not collected. `app.rs` is low mostly because `shutdown_signal` and the production `router()` wrapper are not called by the tests (the tests build the same app through `apply_layers`). The one missed `sync-crdt` line is the over-4 GiB error branch in `parity.rs`.

How it is run in CI (the `rust` job, after `wasm-pack test`): `bash .github/scripts/coverage-gate.sh sync-crdt 90` (with `PROPTEST_CASES=500`), then `... server 75`. The script runs `cargo llvm-cov -p <crate> --fail-under-lines <n>`, prints the table, and emits the measured percentage as a `::notice` annotation, and a `::error` annotation when the gate fails; `cargo-llvm-cov` 0.9.1 itself fails a run under the threshold with exit code 1 and no message, which GitHub would show only as "Process completed with exit code 1". With 500 cases the `sync-crdt` run takes 10 s instead of 2 min locally and reports the same percentage (99.07%). Running these gates is what first proves, on a real runner, that `cargo-llvm-cov` 0.9.1 and the `llvm-tools-preview` component work in CI; this PR's CI run is that proof.

**Confirmed on the CI runner** (PR #6, read from the gate script's annotations): `sync-crdt` 99.07% and `server` 88.22% line coverage, so the script, `cargo-llvm-cov` 0.9.1 and the `llvm-tools-preview` component all work in CI; the local `server` number was 88.37%.

**After task 1.2** (local, MongoDB 8.0.23): `server` **92.49%** (1565 of 1692 lines; regions 91.65%, functions 90.37%). The auth files: `cookie.rs`, `extractor.rs`, `session.rs` 100%, `validation.rs` 99.5%, `handlers.rs` 97.5%, `mod.rs` 94.6%, `password.rs` 89.3% (the missed lines are the branches that handle a failure to build the hasher, which cannot be provoked with the fixed parameters). `main.rs`, `telemetry.rs` and the spike binary still read 0% for the reason given above. The gates stay blocking.

**After task 1.3** (local, MongoDB 8.0.23): `server` **94.51%** (2412 of 2552 lines; regions 92.78%, functions 92.71%). New files: `rbac.rs` 100%, `audit.rs` 99.3%, `workspaces/validation.rs` 99.4%, `db/workspaces.rs` 97.7%, `workspaces/handlers.rs` 97.6%.

## Phase 0 review
`test-writer` added 31 tests and found no bugs. `code-reviewer` returned "changes required" (no blockers). Every finding is fixed; dispositions:

- **Fixed:** weak assertions in `convergence.rs` replaced by real expectations; the oversized-client-id test is debug-only (release builds compile the assert out); `encode_fixture` uses `u32::try_from`; the e2e spec waits for the relay's first frame (`data-synced`) instead of racing it; the spike page reports Wasm-load and apply errors instead of a silent empty editor, nulls its socket handlers on cleanup and no longer touches a freed document; `@types/node` is 22; CI runs `push` only on `main` (PRs ran twice) and pins `wasm-pack@0.15.1`; README quick start no longer implies `cargo run -p server` serves anything; missing task-log rows added.
- **M3 (panic while applying a hostile update): confirmed and fixed.** The reviewer could not verify it; a mutation property test did (see the second bullet of the Yrs finding above). Debug runs missed it at first by chance, release found it at once. A deterministic regression test and the saved proptest seed keep it fixed. **A second path then surfaced as a flaky CI run** (see finding 3 above): the most likely cause of the intermittent `cargo test` failure is this Yrs panic, not the timing-based relay tests I first suspected. That is an inference from a local reproduction, not from the CI log. Both panic paths now have an explicit regression test that fails without its guard.
- **M1, M2, m5, m6: fixed in the relay** (`crates/server/src/spike.rs`), each with a test that fails when the protection is removed (checked by disabling each one):
  - **M1** Every send goes through `send_within` with a timeout (10 s), the relay pings every 20 s and closes a peer that sends nothing, not even a pong, for 60 s. A peer that stops reading used to block its task inside `send`, so it could neither reach the lag check nor free the broadcast backlog. Tests: three unit tests on `send_within` and `close` with a sink that never completes (each with an outer deadline so a missing timeout fails fast instead of hanging), and `relay_drops_a_peer_that_stops_answering_pings_and_keeps_a_live_one`.
  - **M2** The document has a size cap (768 KiB, below the 1 MiB frame limit) checked before an update is applied, so the full state a late joiner receives always fits the limit the client enforces. An update past the cap closes the connection with 1009. Tests: `relay_refuses_updates_past_the_size_cap_and_late_joiners_still_load` and `apply_refuses_an_update_that_could_pass_the_size_cap`. The spike page also shows an error instead of an empty editor when it cannot apply the state.
  - **m5** The broadcast backlog is 64 frames instead of 256 (at most 64 MiB pinned by a stuck peer, down from 256 MiB), and the size cap bounds the document. A 1 MiB all-zero frame is still relayed to everyone without changing the document; that is harmless here.
  - **m6** Updates over 16 KiB are applied with `spawn_blocking` instead of on a Tokio worker.
  - The two new timing-based relay tests were also made deterministic as a precaution (the size-cap test no longer waits on a timer; the idle test has wide margins); that was not the cause but removes a real fragility. Two existing tests were adapted, not weakened: the 1 MiB frame-limit test now raises the size cap so it checks the frame limit alone, and the lagging-consumer test sends no-op frames with a long send timeout so it still exercises the lag path (the cap would refuse 36 MB of real text). The size cap has its own test.
  - The same rules are written into `docs/ARCHITECTURE.md` section 4 and `docs/API.md` (Limits) as requirements for the Phase 2 hub, which must keep them, plus a real answer to state sync past 1 MiB (chunking or a per-room size policy), since the cap is a spike shortcut.
- **m4: fixed.** The Rust toolchain is pinned to 1.100.0 in `rust-toolchain.toml` and in CI (`toolchain: 1.100.0`), and wasm-pack to 0.15.1, so a new release cannot turn CI red without a code change. Bump them together on purpose. Floating major tags for actions (`@v7`, `@v2`) stay: read-only token, no secrets, no `pull_request_target`.
- **Not covered:** coverage gates (`cargo llvm-cov` is not installed locally and CI does not run it yet), the Wasm no-`catch_unwind` path, and rate limits (the relay has none).

## Upstream report: yrs 0.28.1 overflows while integrating an update

Ready to file against the Yrs repository (y-crdt/y-crdt). Everything below was run; the same bytes are checked in as `crates/sync-crdt/tests/fixtures/yrs_integration_panic.bin` and covered by tests.

**Summary.** `Update::decode_v1` accepts a 28-byte update, and `TransactionMut::apply_update` then panics with `attempt to subtract with overflow` at `src/block_store.rs:320:40` (`let diff_end = skip.next_clock() - block.next_clock();`). With overflow checks off, which is the default in release builds, there is no panic: the subtraction wraps, `apply_update` returns `Ok(())`, and the document is left in an undefined state (in our mutation tests one such run did not finish within 60 s). Applying remote updates is the untrusted-input path of every sync server, so this should return an error (or keep the block pending) instead.

**Environment.** `yrs = "=0.28.1"` (default features), rustc 1.100.0, Windows x86_64. Observed locally; we have not run the reproduction on Linux.

**Reproduction** (`Cargo.toml` has only `yrs = "=0.28.1"`):

```rust
use yrs::updates::decoder::Decode;
use yrs::{Doc, Text, Transact, Update};

const UPDATE: [u8; 28] = [
    0x02, 0x02, 0x02, 0x00, 0x01, 0x01, 0x04, 0x74, 0x65, 0x78, 0x74, 0x02, 0x84, 0x02, 0x01,
    0x05, 0xf0, 0x9f, 0x99, 0x82, 0x61, 0x01, 0x02, 0x01, 0x00, 0x02, 0x00, 0x00,
];

fn main() {
    let doc = Doc::with_client_id(1);
    let text = doc.get_or_insert_text("text");
    text.insert(&mut doc.transact_mut(), 0, "existing"); // optional: it also fails on an empty doc
    let update = Update::decode_v1(&UPDATE).expect("decodes fine");
    let mut txn = doc.transact_mut();
    println!("{:?}", txn.apply_update(update)); // panics in debug, prints Ok(()) in release
}
```

| Build | Result |
|---|---|
| `cargo run` (debug) | panic: `attempt to subtract with overflow`, `block_store.rs:320:40` |
| `cargo run --release` | prints `Ok(())`; the overflow wrapped |
| release with `[profile.release.package.yrs] overflow-checks = true` | the same panic as debug |

Expected: `Err(UpdateError::..)`, or the block stays pending, in every build profile.

**How it was found.** A property test builds a valid update history, flips one to three bytes and applies the result to a replica that already has content. The bytes above come from a single flipped byte (`edits = [(0, "🙂🙂a", 2), (3, "", 0)]`, `flips = [(17921346986639153400, 2)]`; see `hostile_update` in `crates/sync-crdt/tests/convergence.rs`).

**Two related panics from the same testing**, both reproducible with the tests named here:

1. `src/block.rs:93`: decoding an update whose client id needs more than 53 bits trips `debug_assert!(value & Self::MASK == 0)` in debug builds, so hostile bytes panic inside `Update::decode_v1`. Test: `hostile_client_id_is_rejected_and_leaves_existing_content_untouched`. Release builds compile the assert out and silently mask the id.
2. `src/transaction.rs:390` (`merge_pending_v1`): after a mutated update was accepted into the pending queue, a later `encode_state_as_update_v1` panics on `merge_updates_v1(merge).unwrap()` with `EndOfBuffer(1)`, because the pending block cannot be re-encoded. Test: `single_flipped_byte_cannot_poison_later_state_encoding`.

## Measurements

| Date | Editors | Edits/s | Mongo writes/s | Ratio | Broadcast p50 / p95 | Memory per doc |
|---|---|---|---|---|---|---|

## Hosted URLs

- Web:
- API:
- Atlas cluster:

# Roadmap: Friday submission plan

**Deadline: Friday 20 November 2026.** This plan was rewritten on Monday 16 November from the real repository state and updated the same day with the team's decisions (two parallel tracks, the approved trims). The original phase tables (deliverables and acceptance criteria) are kept in section 7 with a track, a priority and a day added to every row. Nothing was deleted: what will not be done by Friday is marked **post-submission**.

Each task is still one plan-and-implement cycle in Claude Code (plan with Opus, implement with Sonnet), and a task is done when its acceptance criteria hold and its tests pass (`CLAUDE.md`, definition of done).

## 1. Where we are (Monday 16 November)

| Done and merged | Tasks |
|---|---|
| Phase 0 | 0.1, 0.2, 0.3 (tagged `phase-0`) |
| Phase 1 | 1.1 server skeleton (PR #5), 1.2 auth (PR #7), 1.3 workspaces, members, RBAC and the audit writer (PR #8) |

**Task 1.4 (`ts-rs` types and the CI staleness check) is not merged and not started.** It is the first task of the server track on Monday because the web track consumes the generated types.

Nothing exists yet for documents, the WebSocket hub, versions, boards and tasks, the audit endpoint, the web UI, the deployment files or the seed script.

## 2. Priorities

| Priority | Tasks |
|---|---|
| **MUST** (demo-critical) | 1.4, 1.5, 1.6, 2.1 to 2.6, 3.1, 3.2 (trimmed), 3.3 (trimmed), 4.1 to 4.5, 5.1, 5.2 (trimmed), 5.3, 6.1, 6.2 (trimmed), 6.3 (trimmed), 7.4 (deploy), 7.5 (seed) |
| **SHOULD** (only if time) | 3.4 (simple: on close code 4001 redirect to login), 3.5 (share dialog and command palette), 7.2 (a 60-second load test run to get the edits/s versus writes/s number) |
| **Post-submission** (not deleted) | 7.1 full RBAC and frame sweep, 7.3 accessibility pass, all of Phase 8 (Redis), and the trimmed parts listed in section 5 |

## 3. Two parallel tracks

The work is split by directory so that two people (and two Claude Code sessions) can work at the same time without touching the same files.

| | **Server track** | **Web track** |
|---|---|---|
| Owner | the project lead | a teammate, in a parallel session |
| Directories | `crates/`, `tools/`, `infrastructure/`, `Dockerfile`, `.github/workflows/`, `docs/API.md`, `DATA_MODEL.md`, `ARCHITECTURE.md`, `DEPLOYMENT.md` | `apps/web/`, `e2e/`, `docs/UI_SPEC.md` |
| Builds against | the contract in `docs/API.md` | the same contract, plus **typed mocks** until each endpoint merges |
| Reads | `CLAUDE.md` and every doc named in it | `CLAUDE.md` (rules 9 and 11 apply: only `docs/UI_SPEC.md` and the Figma frames for the UI) and `docs/API.md`, `docs/UI_SPEC.md`, `docs/TESTING.md` |

**Rules between the tracks**
- **The contract is `docs/API.md` plus the generated types.** After task 1.4 (Monday morning) the types in `apps/web/src/types/generated/` are produced by `cargo run -p server --bin gen-types` and committed; the web track never edits them by hand. Until 1.4 merges, the web track writes the few types it needs by hand in one file, `apps/web/src/types/pending.ts`, and deletes it when the generated ones land.
- **Mocks are typed with the contract types** (`apps/web/src/lib/api/`: one interface per endpoint group, a real implementation and a mock one, switched by an environment variable). If a mock's shape drifts from the generated type, `pnpm typecheck` fails. A mock is replaced by the real call the day the endpoint merges, and that swap is part of the web task, not a separate one.
- **A change to the contract** (an endpoint, a field, a close code) is made by the server track in `docs/API.md` in the same PR as the endpoint, and announced in the "Contract changes" list of `docs/PROGRESS.md`. The web track asks for a change by adding a line to "Contract requests" in the same file; it does not edit `API.md`.
- **`docs/PROGRESS.md` and `docs/TESTING.md`** are edited by both. Each track adds only its own rows (prefix `[server]` or `[web]`) and its own bullets, so merge conflicts are limited to adjacent lines. Whoever merges second rebases.
- **The Wasm package** is built from `crates/sync-crdt` by `pnpm --dir apps/web wasm`. Its JavaScript API is server-track work (task 2.1); until it merges, the web track codes against a `DocEngine` TypeScript interface and a fake implementation.
- **CI** (`ci.yml`) belongs to the server track; the web track asks for changes. The e2e job runs on both.

**Batches.** Each track has one batch per day; one batch is one pull request. Rule 8 of `CLAUDE.md` says one roadmap task per commit or PR, so **each task is its own commit** (conventional message, tests in the same commit). A PR is merged only when CI is green.

### Hand-offs: which server merge unblocks which web task

| Server task merged | Day | Web task that switches from mock to real |
|---|---|---|
| 1.4 generated types | Mon morning | all (replaces `pending.ts`) |
| 2.1 Wasm document API | Mon | 2.6 editor (replaces the fake `DocEngine`) |
| 2.2 documents REST | Mon | 2.5 documents list |
| 2.3 WebSocket hub, 3.1 awareness | Tue | 2.6 editor sync, 3.1 cursors, 3.2 reconnect |
| 4.3 versions endpoints | Tue | 4.5 history panel (list, save, preview) |
| 4.4 restore | Wed | 4.5 restore |
| 3.3 live role changes | Wed | 3.3 viewer mode |
| 5.1 boards and tasks | Wed | 5.2 and 5.3 (Thursday) |
| 6.1 audit list | Thu morning (first task of the day) | 6.3 audit screen |

## 4. The week, per track (hours are my estimates for code plus tests, not measurements)

### Server track

| Day | Batch (one PR) | Tasks (hours) | Hours | Done when |
|---|---|---|---|---|
| **Mon 16** | S1 | 1.4 types (1) first, then 2.1 `sync-crdt` document schema and Wasm bindings (4), 2.2 documents REST (2), 4.1 versions repository (2) | **9** | Types merged by mid-morning; documents can be created through the API; the CRDT crate converges under the property test |
| **Tue 17** | S2 | 2.3 WebSocket hub (4), 2.4 snapshot worker, final flush, SIGTERM flush and metrics (2), 3.1 awareness relay (1), 4.3 save, list and preview endpoints (2), 7.4 part 1: Dockerfile and deploy workflow (1.5) | **10.5** | Two sockets converge and the snapshot persists across a restart; the metrics give edits/s and writes/s |
| **Wed 18** | S3 | 4.4 restore (3), 3.3 live role changes (1), 4.2 automatic versions (1.5), 5.1 boards and tasks REST (4), 7.4 part 2: Render and Atlas, smoke checklist (2) | **11.5** | **Server deployed on Render with Atlas, `DEPLOYMENT.md` section 8 smoke checklist passing.** Friday is never the first deploy |
| **Thu 19** | S4 | 6.1 audit list endpoint (1.5, first), 7.5 seed script (1.5), `code-reviewer` and `test-writer` runs and fixes (2) | **5** | Seed fills a fresh database; reviewers' findings fixed; `main` redeployed. About 1.5 h spare for 7.2 (the load test, SHOULD) |
| **Fri 20 am** | | Final checks only | | Demo script on the hosted URLs, tag `submission` |

Server total about **36 hours** against about 48 available.

### Web track

| Day | Batch (one PR) | Tasks (hours) | Hours | Done when |
|---|---|---|---|---|
| **Mon 16** | W1 | 1.5 components (3), 1.6 login, register, onboarding, picker, app shell and the `/api/*` rewrite (4), 2.5 documents list and create dialog (2) | **9** | A user registers, logs in, creates a workspace and a document in the browser, against the real auth and workspace API and the mocked documents API |
| **Tue 17** | W2 | 2.6 block editor with the socket client (6), integration of 1.6 and 2.5 with the real API (1) | **7** | e2e scenario 1: type in three block types, reload, content persists |
| **Wed 18** | W3 | 3.1 presence and cursors (2.5), 3.2 reconnect with backoff and the banner (1.5), 3.3 viewer mode (1), 4.5 history panel (4), 7.4 Vercel (1), integration (1) | **11** | **Web deployed on Vercel against the Render API.** e2e scenario 2 (two contexts, same block) |
| **Thu 19** | W4 | 5.2 board page (2.5), 5.3 tasks and drag and drop (4), 6.2 members screen (2.5), 6.3 audit list with filters (2), integration (1) | **12** | e2e scenarios 3, 6 and 7 pass; web redeployed from `main` |
| **Fri 20 am** | | Final checks only | | Demo script on the hosted URLs |

Web total about **39 hours** against about 48 available. The web hours are the least certain: only the spike page exists today, and the editor (2.6) is the biggest single task of the week.

### Both tracks together, per day

| | Mon | Tue | Wed | Thu | Total | Available per track |
|---|---|---|---|---|---|---|
| Server | 9 | 10.5 | 11.5 | 5 | 36 | 48 (4 days of about 12 hours) |
| Web | 9 | 7 | 11 | 12 | 39 | 48 |

Wednesday is the heaviest day on both tracks (deploy day) and Thursday is the heaviest for the web track. The server track's spare hours on Thursday are the buffer: it can take over web integration work that is blocked on the server.

## 5. Trims and cuts

**Approved on Monday 16 November** (already applied in the tables of section 7):

| Task | Friday version | Post-submission |
|---|---|---|
| 6.3 audit screen | Audit list with the action, actor and entity-type filters | JSON inspector, CSV and JSON export (frame 40) |
| 5.2 board page | Columns, task cards, create board, empty board | Filters |
| 6.2 settings | Members and access (add, change role, remove) | General settings tab and danger zone |
| 3.2 reconnect | Reconnect with backoff, banner and connection pill | "Waiting to sync" state (frame 13) |
| 3.3 live role changes | A demoted or removed user stops editing at once (viewer mode, or sent away on removal) | Toasts and frame 42 |

**Kept in full by decision:** 2.4 with the SIGTERM flush and the metrics counters (they produce the edits/s versus writes/s number) and 4.2 automatic versions (requirement FR15).

**Re-check on Wednesday evening.** If the server track is behind then, 4.2 is the first thing to drop; the project lead is told at that point and decides. No other cut is planned.

**Coverage.** The two Rust gates (`sync-crdt` 90%, `server` 75%) stay blocking as they are. No web coverage gate is added; web coverage is reported only.

## 6. Tests, scaled to the deadline

Tests stay mandatory; what changes is how many.

| Layer | What is required |
|---|---|
| Server | Unit and integration tests for **all** server logic and **every permission check**: each new endpoint or frame type is added to the RBAC matrix (`crates/server/tests/rbac_matrix.rs`, which fails if a route has no entry). Each audited action has a test for its row. |
| `sync-crdt` | The convergence property test, projection and validation tests, the native and Wasm parity test. Unchanged. |
| Web | Component tests for the main states of each screen: loading, empty, error, populated, and the viewer, editor and admin controls. Not every pixel state. Mocks are typed with the generated types. |
| End to end | **Only scenarios 1, 2, 3, 6 and 7** of `TESTING.md` section 2: 1 create and persist, 2 two users in one block, 3 viewer is read-only, 6 save, restore and undo a version, 7 board drag. Scenarios 4, 5, 8, 9 and 10 are post-submission; their logic is covered by server and socket-client tests (a demoted editor's next update is rejected, reconnect backoff, member add and remove with their audit rows). |
| Coverage | Rust gates blocking as above; web report-only. |

Review agents, scaled: the server track runs the `code-reviewer` agent on the Tuesday and Wednesday batches (WebSocket hub, snapshots, versions) before merging, and the `test-writer` agent once on Thursday; the web track runs `code-reviewer` once on Thursday. Phase tags other than `phase-1` and the final `submission` tag are skipped.

## 7. Original phase tables with track, priority and day

The deliverable and acceptance text is the original, except where it asks for tests or screens that the scaled plan moves to post-submission (marked in the last column).

### Phase 0: bootstrap and spike (done)

| Task | Status | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 0.1 | done | Monorepo: Cargo workspace (`sync-crdt`, `server`), Next.js app in `apps/web` (pnpm, TypeScript strict, ESLint, Prettier, Vitest), `e2e/` with Playwright, `infrastructure/docker-compose.yml` with MongoDB 8.0 (the major version Atlas free clusters run), `.env.example`, `.gitignore`, `README.md` | `cargo test`, `pnpm test`, `pnpm lint` run green on an empty project |
| 0.2 | done | CI workflow `ci.yml` (`DEPLOYMENT.md` §4) running every gate in `TESTING.md` §3 that applies so far | CI green on a PR |
| 0.3 | done | Spike: `sync-crdt` with one shared Y.Text, compiled natively and to Wasm; a throwaway Axum WebSocket relay; two browser tabs converge | Parity test passes; a short note in `PROGRESS.md`. If Yrs in Wasm fails, stop and record the fallback (Yjs on the client) as an ADR before going on |

### Phase 1: accounts, workspaces, design system

| Task | Track, priority, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 1.1 | done | Server skeleton: config, tracing, `/healthz`, Mongo connection, index creation, error type and format, request body limit, CORS | Integration test for `/healthz` and the error format |
| 1.2 | done | Auth: register, login, logout, me; argon2id; JWT cookie | Auth tests in `TESTING.md` |
| 1.3 | done | Workspaces and members endpoints with RBAC module and audit writer (MEMBER_ADD, MEMBER_REMOVE, ROLE_CHANGE) | RBAC matrix test (for routes so far), member and owner rules, audit rows |
| 1.4 | server, MUST, Mon | `ts-rs` type generation (output `apps/web/src/types/generated/`) and CI staleness check | CI fails if types are stale |
| 1.5 | web, MUST, Mon | Web: tokens, fonts, component inventory part 1 (Button, Input, Select, Dialog, Toast, Badge, Avatar, NavItem, Banner, EmptyState) on `/dev/components` | Component tests for the main states of each component (default, disabled, loading, error) |
| 1.6 | web, MUST, Mon | Web: `/login`, `/register`, `/onboarding`, `/w` picker, app shell (sidebar, header, connection pill placeholder), Next.js rewrite of `/api/*` to the server | Frames 01 to 05 match; component tests. The standalone e2e "register, create workspace, see it in the picker" is covered by scenario 1 |

### Phase 2: documents and live editing

| Task | Track, priority, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 2.1 | server, MUST, Mon | `sync-crdt`: schema, ops, projection, validation, sync wrappers, Wasm bindings | Convergence property test, projection, move, validation fuzz, parity |
| 2.2 | server, MUST, Mon | Documents REST (list, create, get, rename, delete) with DOC_CREATE and DOC_DELETE audit | Integration and RBAC tests |
| 2.3 | server, MUST, Tue | WebSocket hub: tickets, doc rooms, handshake, broadcast, frame role checks, rate limits, size limit, close codes | Ticket and frame tests; two clients converge; late joiner |
| 2.4 | server, MUST, Tue | Snapshot worker, final flush, SIGTERM flush, `SNAPSHOT_SAVED`, metrics counters | Snapshot and restart durability tests |
| 2.5 | web, MUST, Mon | Web: documents list, empty state, create dialog (frames 06 to 08) | Component tests (the e2e create-document is covered by scenario 1) |
| 2.6 | web, MUST, Tue | Web: block editor with the six block types, minimal-diff input, slash menu, hover and focus states, code copy, checklist toggle (frames 09 to 11), and the socket client | Editor logic tests; e2e scenario 1 |

### Phase 3: collaboration details

| Task | Track, priority, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 3.1 | server Tue (awareness relay, `PRESENCE_COUNT`), web Wed, MUST | Awareness: presence avatars, remote cursors and selections with sticky indices, `PRESENCE_COUNT` | e2e scenario 2 |
| 3.2 | web, MUST, Wed | Reconnect with backoff and ticket refresh; reconnecting banner and pill. **Trimmed:** no "Waiting to sync" state (frame 13) | Socket client tests (e2e scenario 5 is post-submission) |
| 3.3 | server Wed (live role updates, close 4003), web Wed, MUST | Live role changes and removal; viewer mode. **Trimmed:** a demoted or removed user stops editing at once; toasts and frame 42 are post-submission | Frame tests; e2e scenario 3 (scenario 4 is post-submission; the server test "a demoted editor's next update is rejected" covers its logic) |
| 3.4 | server and web, SHOULD | Session expiry: on close code 4001 redirect to login (the full flow keeping unsynced edits, frame 41, is post-submission) | Socket client test for 4001 |
| 3.5 | web, SHOULD | Share dialog (15), delete dialog (16), command palette (17), workspace socket with `DOC_LIST_EVENT` and `MEMBER_EVENT` | Component tests (e2e scenario 9 is post-submission) |

### Phase 4: version history

| Task | Track, priority, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 4.1 | server, MUST, Mon | `document_versions` repository, zstd compression, version counter, retention | Numbering under concurrency, retention tests |
| 4.2 | server, MUST, Wed (dropped first if behind on Wednesday evening) | Automatic versions worker (10 min, last editor leaves) with controllable clock | Automatic version tests |
| 4.3 | server, MUST, Tue | Manual save, list, preview endpoints with VERSION_SAVE audit and `VERSION_CREATED` broadcast | Integration and RBAC tests |
| 4.4 | server, MUST, Wed | `replace_content` in `sync-crdt` and the restore endpoint with VERSION_RESTORE and `DOC_RESTORED` | Restore tests (crate and server) |
| 4.5 | web, MUST, Wed | Web: history panel, create named version, preview mode, restore confirmation and success, viewer and empty variants, "no longer kept" (frames 18 to 24b) | Component tests; e2e scenario 6 (Thursday, when the restore endpoint has been integrated) |

### Phase 5: Kanban

| Task | Track, priority, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 5.1 | server, MUST, Wed | Boards and tasks REST, move ordering and renumbering, member checks, TASK_CREATE, TASK_MOVE, TASK_DELETE audit, `BOARD_EVENT` | Task tests in `TESTING.md` |
| 5.2 | web, MUST, Thu | Web: board page, columns, task card states, create board, empty board (frames 25 to 27b). **Trimmed:** filters are post-submission | Component tests |
| 5.3 | web, MUST, Thu | Web: create task, details and edit, delete, drag and drop with keyboard support, viewer board (frames 28 to 32) | e2e scenario 7 |

### Phase 6: settings and audit

| Task | Track, priority, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 6.1 | server, MUST, Thu (first) | Audit list endpoint with filters and cursor | Audit tests |
| 6.2 | web, MUST, Thu | Web: members and access, add member, change role, remove member (frames 34 to 37). **Trimmed:** general settings and the danger zone (frames 33, 38) are post-submission | Component tests (e2e scenario 8 is post-submission; its logic is covered by the member and audit server tests) |
| 6.3 | web, MUST, Thu | Web: audit trail list with the action, actor and entity-type filters (frame 39). **Trimmed:** the JSON inspector and the CSV and JSON export (frame 40) are post-submission | Component tests for the list and the filters |

### Phase 7: hardening, measurement, deployment

| Task | Track, priority, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 7.1 | post-submission | Full RBAC matrix and frame test sweep; fix gaps reported by `test-writer` | Coverage gates in `TESTING.md` §3 met |
| 7.2 | server, SHOULD (Thu spare hours) | Load test tool and results (a 60-second run) | Numbers recorded in `PROGRESS.md`; targets met or explained |
| 7.3 | post-submission | Accessibility pass | e2e scenario 10 |
| 7.4 | server Tue and Wed (Dockerfile, Render, Atlas), web Wed (Vercel), MUST | Dockerfile (multi-stage, distroless or debian-slim), deploy workflows, Render, Vercel and Atlas setup per `DEPLOYMENT.md` | Smoke test checklist in `DEPLOYMENT.md` §8 passes on the hosted URLs |
| 7.5 | server, MUST, Thu | Seed script for the demo (workspace "Engineering Team", three members, documents and a board like the Figma frames) | Runs against a fresh database |

### Phase 8 (optional): several nodes with Redis (post-submission)

| Task | Priority | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 8.1 | post-submission | `Bus` trait, in-memory and Redis implementations | Unit tests for both |
| 8.2 | post-submission | State-vector catch-up between nodes; conditional snapshot writes | Integration test with two server processes |
| 8.3 | post-submission | docker compose demo with two nodes, Redis and a reverse proxy | e2e scenario 2 with the two contexts on different nodes |

## 8. Working rules for the week

- A PR is merged only when CI is green; a failing check stops the day's work on that PR, it is not merged past.
- After each task: update `docs/PROGRESS.md` (task log, decisions, open questions), as before, with your track's prefix.
- Anything not on the MUST or SHOULD list that someone asks for during the week needs a decision before work starts; anything out of scope in `PRD.md` still needs an ADR (`CLAUDE.md` rule 10).

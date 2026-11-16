# Roadmap: Friday submission plan

**Deadline: Friday 20 November 2026.** This plan was rewritten on Monday 16 November from the real repository state. The original phase tables (deliverables and acceptance criteria) are kept below, with a priority and a day added to every row. Nothing was deleted: what will not be done by Friday is marked **post-submission**.

Each task is still one plan-and-implement cycle in Claude Code (plan with Opus, implement with Sonnet), and a task is done when its acceptance criteria hold and its tests pass (`CLAUDE.md`, definition of done).

## 1. Where we are (Monday 16 November)

| Done and merged | Tasks |
|---|---|
| Phase 0 | 0.1, 0.2, 0.3 (tagged `phase-0`) |
| Phase 1 | 1.1 server skeleton (PR #5), 1.2 auth (PR #7), 1.3 workspaces, members, RBAC and the audit writer (PR #8) |

**Task 1.4 (`ts-rs` types and the CI staleness check) is not merged and not started.** The deadline message said 0.1 to 1.4 were merged; the repository says 0.1 to 1.3. 1.4 is on this plan as the first task of Monday because the web app imports the generated types.

Nothing exists yet for documents, the WebSocket hub, versions, boards and tasks, the audit endpoint, the web UI, the deployment files or the seed script.

## 2. Priorities

| Priority | Tasks |
|---|---|
| **MUST** (demo-critical) | 1.4, 1.5, 1.6, 2.1 to 2.6, 3.1, 3.2, 3.3, 4.1 to 4.5, 5.1, 5.2, 5.3, 6.1, 6.2, 6.3, 7.4 (deploy), 7.5 (seed). Within these, section 5 lists what is cut first if time runs out. |
| **SHOULD** (only if time) | 3.4 (simple: on close code 4001 redirect to login), 3.5 (share dialog and command palette), 7.2 (a 60-second load test run to get the edits/s versus writes/s number) |
| **Post-submission** (not deleted) | 7.1 full RBAC and frame sweep, 7.3 accessibility pass, all of Phase 8 (Redis) |

## 3. The week in batches

One batch is one pull request and one day. Rule 8 of `CLAUDE.md` says one roadmap task per commit or PR; with a batch per PR, **each task is its own commit** (conventional message, tests in the same commit), so the history stays reviewable even though the PR is big. Each batch is merged only when CI is green.

The hours are my estimates for code plus its tests, not measurements. They exist so the plan can be checked against the time there is.

| Day | Batch (one PR) | Tasks (estimated hours) | Done when |
|---|---|---|---|
| **Mon 16** | **B1 Foundations** | 1.4 types (1), 1.5 components (2.5), 1.6 auth screens and app shell (3), 2.1 `sync-crdt` document schema and Wasm bindings (4), 2.2 documents REST (2) = **12.5 h** | A user registers, logs in, creates a workspace and sees it in the picker in the browser; documents can be created through the API; the CRDT crate converges under the property test |
| **Tue 17** | **B2 Live documents** | 2.3 WebSocket hub (4), 2.4 snapshot worker and flush (2), 2.5 documents list screens (2), 2.6 block editor (4) = **12 h** | Scenario 1: type in three block types, reload, content persists; two browser tabs edit one document |
| **Wed 18** | **B3 Collaboration and first deploy** | 3.1 presence and cursors (2), 3.2 reconnect (2.5), 4.1 versions repository (2), 4.2 automatic versions worker (1.5), 4.3 save, list and preview endpoints (2), 5.1 boards and tasks REST (4), **7.4 deploy (4)** = **18 h** | **Deployed on Render, Vercel and Atlas, with the smoke checklist of `DEPLOYMENT.md` section 8 passing on the hosted URLs.** Friday is never the first deploy. Scenario 2 (two contexts, same block) passes |
| **Thu 19** | **B4 Versions, board, settings, audit, seed** | 3.3 live role changes and viewer mode (2.5), 4.4 restore (3), 4.5 history panel (3), 5.2 board page (2), 5.3 task create, edit and drag (3), 6.1 audit list endpoint (1.5), 6.2 settings and members screens (3), 6.3 audit trail screen (1.5), 7.5 seed script (1.5) = **21 h** | Scenarios 3, 6 and 7 pass; the seed fills a fresh database; the hosted site is redeployed from `main` |
| **Fri 20, morning** | **Final checks** (no new features) | Run the demo script on the hosted URLs, fix only what it finds, update `PROGRESS.md` and `README.md`, tag `submission` | Smoke checklist green on the hosted URLs; `main` is green |

**Mon to Thu total: about 63.5 hours of work against about 48 hours of working time (four days of about 12 hours).** The plan does not fit as written, and Wednesday and especially Thursday are overloaded. Section 5 says what is cut first, in order. This needs a decision from you, not a silent assumption: see `docs/PROGRESS.md`, "Open questions".

## 4. Tests, scaled to the deadline

Tests stay mandatory; what changes is how many.

| Layer | What is required |
|---|---|
| Server | Unit and integration tests for **all** server logic and **every permission check**: each new endpoint or frame type is added to the RBAC matrix (`crates/server/tests/rbac_matrix.rs`, which fails if a route has no entry). Each audited action has a test for its row. |
| `sync-crdt` | The convergence property test, projection and validation tests, the native and Wasm parity test. Unchanged. |
| Web | Component tests for the main states of each screen: loading, empty, error, populated, and the viewer versus editor versus admin controls. Not every pixel state. |
| End to end | **Only scenarios 1, 2, 3, 6 and 7** of `TESTING.md` section 2: 1 create and persist, 2 two users in one block, 3 viewer is read-only, 6 save, restore and undo a version, 7 board drag. Scenarios 4, 5, 8, 9 and 10 are post-submission; their logic is covered by server and socket-client tests (a demoted editor's next update is rejected, reconnect backoff, member add and remove with their audit rows). |
| Coverage | **Report-only for the web app**: no web coverage gate is added. The two Rust gates that exist since PR #6 (`sync-crdt` 90%, `server` 75%) are still blocking: see "Open questions" in `PROGRESS.md`, because the deadline message asked for report-only. |

## 5. If we fall behind: the cut ladder

Do these in order, one at a time, as soon as a day finishes late. Each line says what is dropped and how many hours it frees (estimates). Dropped work moves to post-submission and is recorded in `PROGRESS.md`; the tests of everything that stays are not reduced.

| Order | Cut | Frees |
|---|---|---|
| 1 | 4.2 automatic versions worker (manual versions stay; the history panel shows manual and restore versions only) | 1.5 h |
| 2 | 6.3 audit export (the audit screen stays) | 1.5 h |
| 3 | 5.2 board filters and the empty-board state polish | 2 h |
| 4 | 6.2 general settings tab and danger zone (the members tab stays: scenario "admin adds and removes a member") | 1.5 h |
| 5 | 2.4 SIGTERM flush and the metrics counters (the periodic snapshot stays) | 1 h |
| 6 | 3.2 reduced to reconnect with backoff and the banner (no "Waiting to sync" state) | 1 h |
| 7 | 3.3 reduced to "demoted or removed users stop editing" (no toasts and no frame 42) | 1 h |

These seven free about 9.5 hours, which still leaves roughly 6 hours over the 48 available. The next step is a whole-feature choice that only you can make: demote the **audit screens** (6.1 stays for the data, 6.3 and the audit part of 6.2 go) or the **Kanban board** (5.1 to 5.3, about 9 hours) to post-submission. The rest of the gap has to come from longer days.

## 6. Original phase tables with priorities

Priority and day are added to every row; the deliverable and acceptance text is the original, except where it asks for tests that the scaled plan in section 4 moves to post-submission (marked in the last column).

### Phase 0: bootstrap and spike (done)

| Task | Status | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 0.1 | done | Monorepo: Cargo workspace (`sync-crdt`, `server`), Next.js app in `apps/web` (pnpm, TypeScript strict, ESLint, Prettier, Vitest), `e2e/` with Playwright, `infrastructure/docker-compose.yml` with MongoDB 8.0 (the major version Atlas free clusters run), `.env.example`, `.gitignore`, `README.md` | `cargo test`, `pnpm test`, `pnpm lint` run green on an empty project |
| 0.2 | done | CI workflow `ci.yml` (`DEPLOYMENT.md` §4) running every gate in `TESTING.md` §3 that applies so far | CI green on a PR |
| 0.3 | done | Spike: `sync-crdt` with one shared Y.Text, compiled natively and to Wasm; a throwaway Axum WebSocket relay; two browser tabs converge | Parity test passes; a short note in `PROGRESS.md`. If Yrs in Wasm fails, stop and record the fallback (Yjs on the client) as an ADR before going on |

### Phase 1: accounts, workspaces, design system

| Task | Priority, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 1.1 | done | Server skeleton: config, tracing, `/healthz`, Mongo connection, index creation, error type and format, request body limit, CORS | Integration test for `/healthz` and the error format |
| 1.2 | done | Auth: register, login, logout, me; argon2id; JWT cookie | Auth tests in `TESTING.md` |
| 1.3 | done | Workspaces and members endpoints with RBAC module and audit writer (MEMBER_ADD, MEMBER_REMOVE, ROLE_CHANGE) | RBAC matrix test (for routes so far), member and owner rules, audit rows |
| 1.4 | MUST, Mon | `ts-rs` type generation and CI staleness check | CI fails if types are stale |
| 1.5 | MUST, Mon | Web: tokens, fonts, component inventory part 1 (Button, Input, Select, Dialog, Toast, Badge, Avatar, NavItem, Banner, EmptyState) on `/dev/components` | Component tests for the main states of each component (default, disabled, loading, error) |
| 1.6 | MUST, Mon | Web: `/login`, `/register`, `/onboarding`, `/w` picker, app shell (sidebar, header, connection pill placeholder), Next.js rewrite of `/api/*` to the server | Frames 01 to 05 match; component tests. The standalone e2e "register, create workspace, see it in the picker" is covered by scenario 1 |

### Phase 2: documents and live editing

| Task | Priority, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 2.1 | MUST, Mon | `sync-crdt`: schema, ops, projection, validation, sync wrappers, Wasm bindings | Convergence property test, projection, move, validation fuzz, parity |
| 2.2 | MUST, Mon | Documents REST (list, create, get, rename, delete) with DOC_CREATE and DOC_DELETE audit | Integration and RBAC tests |
| 2.3 | MUST, Tue | WebSocket hub: tickets, doc rooms, handshake, broadcast, frame role checks, rate limits, size limit, close codes | Ticket and frame tests; two clients converge; late joiner |
| 2.4 | MUST, Tue | Snapshot worker, final flush, SIGTERM flush, `SNAPSHOT_SAVED`, metrics counters | Snapshot and restart durability tests |
| 2.5 | MUST, Tue | Web: documents list, empty state, create dialog (frames 06 to 08) | Component tests (the e2e create-document is covered by scenario 1) |
| 2.6 | MUST, Tue | Web: block editor with the six block types, minimal-diff input, slash menu, hover and focus states, code copy, checklist toggle (frames 09 to 11) | Editor logic tests; e2e scenario 1 |

### Phase 3: collaboration details

| Task | Priority, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 3.1 | MUST, Wed | Awareness: presence avatars, remote cursors and selections with sticky indices, `PRESENCE_COUNT` | e2e scenario 2 |
| 3.2 | MUST, Wed | Reconnect with backoff and ticket refresh; reconnecting banner and pill; "Waiting to sync" (frame 13) | Socket client tests (e2e scenario 5 is post-submission) |
| 3.3 | MUST, Thu | Live role changes and removal; viewer mode (frames 12, 14, 42) | Frame tests; e2e scenario 3 (scenario 4 is post-submission; the server test "a demoted editor's next update is rejected" covers its logic) |
| 3.4 | SHOULD | Session expiry flow: on close code 4001 redirect to login (the full flow keeping unsynced edits, frame 41, is post-submission) | Socket client test for 4001 |
| 3.5 | SHOULD | Share dialog (15), delete dialog (16), command palette (17), workspace socket with `DOC_LIST_EVENT` and `MEMBER_EVENT` | Component tests (e2e scenario 9 is post-submission) |

### Phase 4: version history

| Task | Priority, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 4.1 | MUST, Wed | `document_versions` repository, zstd compression, version counter, retention | Numbering under concurrency, retention tests |
| 4.2 | MUST, Wed (first on the cut ladder) | Automatic versions worker (10 min, last editor leaves) with controllable clock | Automatic version tests |
| 4.3 | MUST, Wed | Manual save, list, preview endpoints with VERSION_SAVE audit and `VERSION_CREATED` broadcast | Integration and RBAC tests |
| 4.4 | MUST, Thu | `replace_content` in `sync-crdt` and the restore endpoint with VERSION_RESTORE and `DOC_RESTORED` | Restore tests (crate and server) |
| 4.5 | MUST, Thu | Web: history panel, create named version, preview mode, restore confirmation and success, viewer and empty variants, "no longer kept" (frames 18 to 24b) | Component tests; e2e scenario 6 |

### Phase 5: Kanban

| Task | Priority, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 5.1 | MUST, Wed | Boards and tasks REST, move ordering and renumbering, member checks, TASK_CREATE, TASK_MOVE, TASK_DELETE audit, `BOARD_EVENT` | Task tests in `TESTING.md` |
| 5.2 | MUST, Thu | Web: board page, columns, task card states, filters, create board, empty board (frames 25 to 27b) | Component tests |
| 5.3 | MUST, Thu | Web: create task, details and edit, delete, drag and drop with keyboard support, viewer board (frames 28 to 32) | e2e scenario 7 |

### Phase 6: settings and audit

| Task | Priority, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 6.1 | MUST, Thu | Audit list endpoint with filters and cursor | Audit tests |
| 6.2 | MUST, Thu | Web: general settings, members and access, add member, change role, remove member, danger zone (frames 33 to 38) | Component tests (e2e scenario 8 is post-submission; its logic is covered by the member and audit server tests) |
| 6.3 | MUST, Thu | Web: audit trail, JSON inspector, CSV and JSON export of loaded rows (frames 39, 40) | Export tests |

### Phase 7: hardening, measurement, deployment

| Task | Priority, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 7.1 | post-submission | Full RBAC matrix and frame test sweep; fix gaps reported by `test-writer` | Coverage gates in `TESTING.md` §3 met |
| 7.2 | SHOULD | Load test tool and results (a 60-second run) | Numbers recorded in `PROGRESS.md`; targets met or explained |
| 7.3 | post-submission | Accessibility pass | e2e scenario 10 |
| 7.4 | MUST, Wed | Dockerfile (multi-stage, distroless or debian-slim), deploy workflows, Render, Vercel and Atlas setup per `DEPLOYMENT.md` | Smoke test checklist in `DEPLOYMENT.md` §8 passes on the hosted URLs |
| 7.5 | MUST, Thu | Seed script for the demo (workspace "Engineering Team", three members, documents and a board like the Figma frames) | Runs against a fresh database |

### Phase 8 (optional): several nodes with Redis (post-submission)

| Task | Priority | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 8.1 | post-submission | `Bus` trait, in-memory and Redis implementations | Unit tests for both |
| 8.2 | post-submission | State-vector catch-up between nodes; conditional snapshot writes | Integration test with two server processes |
| 8.3 | post-submission | docker compose demo with two nodes, Redis and a reverse proxy | e2e scenario 2 with the two contexts on different nodes |

## 7. Working rules for the week

- Review agents, scaled: run the `code-reviewer` agent on the Wednesday and Thursday batches before merging (they touch the WebSocket hub, snapshots, versions and the audit endpoint, which `CLAUDE.md` names), and the `test-writer` agent once on Thursday. Fix what they report. Phase tags other than `phase-1` and the final `submission` tag are skipped.
- A batch PR is merged only when CI is green; a failing check stops the day's work on that PR, it is not merged past.
- After each task: update `docs/PROGRESS.md` (task log, decisions, open questions), as before.
- Anything not on the MUST or SHOULD list that someone asks for during the week needs a decision before work starts; anything out of scope in `PRD.md` still needs an ADR (`CLAUDE.md` rule 10).

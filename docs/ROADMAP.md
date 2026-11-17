# Roadmap: submission plan

**Deadline: Friday 20 November 2026.** The plan has changed twice. On Monday 16 November it was split into a server track and a web track for two people. On Tuesday 17 November the project became **one developer with one Claude session and one laptop**, and the two tracks were merged into **vertical slices**: each feature is built server first and then its screens, in one sequence, against the real API (no mocks). The original phase tables (deliverables and acceptance criteria) are kept in section 8 with a status and a day on every row. Nothing was deleted: what will not be done by Friday is marked **post-submission**.

Each task is still one plan-and-implement cycle in Claude Code (plan with Opus, implement with Sonnet), and a task is done when its acceptance criteria hold and its tests pass (`CLAUDE.md`, definition of done).

## 1. Where we are (Tuesday 17 November, morning)

| Done and merged | Tasks |
|---|---|
| Phase 0 | 0.1, 0.2, 0.3 (tagged `phase-0`) |
| Phase 1 | 1.1 server skeleton (PR #5), 1.2 auth (#7), 1.3 workspaces, members, RBAC and the audit writer (#8), 1.4 `ts-rs` types (#10), 1.5 tokens and UI kit (#11), 1.6 auth screens, picker and shell (#12) |

**Task 2.1 (`sync-crdt` block document and its Wasm API) is implemented** on the local branch `feat/2.1-crdt-document` (two WIP commits, not pushed): the suites that were run pass, but the full `cargo test --workspace`, the coverage gate and the web package build have not been run yet because the laptop ran out of memory twice. Finishing and merging 2.1 is the first job of Tuesday.

Nothing exists yet for the documents API, the WebSocket hub, versions, boards and tasks, the audit endpoint, the document screens, the deployment files or the seed script.

## 2. Priorities

| Priority | Tasks |
|---|---|
| **MUST** (demo-critical) | 2.1 to 2.6, 3.1 (presence), 3.2 (reconnect), 4.1, 4.3, 4.4, 4.5, 6.1, 6.2, 6.3, 7.4 (deploy), 7.5 (seed); 3.3 and 5.1 to 5.3 are MUST in the Tuesday plan but **the first things on the cut list** (section 5) |
| **SHOULD** (only if time) | 3.4 (on close code 4001 redirect to login), 3.5 (share dialog, command palette), 7.2 (a 60-second load test run) |
| **Post-submission** (not deleted) | 4.2 automatic versions (FR15, **future work, `DECISIONS.md` ADR-012**), 7.1 full RBAC and frame sweep, 7.3 accessibility pass, all of Phase 8 (Redis), and the trimmed parts listed in section 6 |

## 3. How the work is organised

- **One slice at a time, server first, then its screens.** A feature's endpoints and sockets are built and tested, then the screens use them directly (the generated types of `apps/web/src/types/generated/` and the Wasm package are the contract). There are no typed mocks any more and no hand-offs between tracks.
- **One pull request per slice**, with **one commit per task** (conventional message, tests in the same commit), merged only when CI is green. A slice that grows too large for one review is split at a task boundary.
- **`docs/PROGRESS.md` is updated after every task**, as before. The `[server]` and `[web]` prefixes are no longer needed.
- **The review agents run on the slices that touch the hub, versions and audit**: `code-reviewer` before merging the live-documents slice and the versions slice, `test-writer` once on Thursday.
- **Thursday night is a freeze.** After it only fixes found by the Friday morning checks are made. Phase tags other than the final `submission` tag are skipped.
- **Memory.** The laptop has run out of memory twice during full builds. `.cargo/config.toml` limits Cargo to two jobs (task 2.1). Close the browser and the other editors before a full `cargo test --workspace`, a release build or `wasm-pack`, and do not run them at the same time.

## 4. Estimated hours (my estimates for code plus tests, not measurements)

Capacity assumed: about 12 working hours on each of Tuesday, Wednesday and Thursday, so **36 hours**; Friday morning is for checks only.

Task estimates (hours, server and screens together):

| Slice | Tasks | Hours |
|---|---|---|
| Live documents | 2.1 finish and merge 1.5, 2.2 documents REST 2, 2.3 hub 4, 2.4 snapshots, SIGTERM flush, metrics 2, 2.5 documents screens 2, 2.6 editor and socket client 6 | **17.5** |
| Presence and reconnect | 3.1 awareness relay 1 and avatars and cursors 2.5, 3.2 reconnect (reduced) 1.5 | **5** |
| Versions end to end | 4.1 repository 2, 4.3 save, list, preview 2, 4.4 restore 3, 4.5 history panel 4 | **11** |
| Deploy | 7.4 Dockerfile, Render, Atlas, Vercel, smoke checklist 4.5, and a redeploy on Thursday 1 | **5.5** |
| Kanban | 5.1 boards and tasks REST 4, 5.2 board page 2.5, 5.3 tasks and drag and drop 4 | **10.5** |
| Members and audit | 6.1 audit endpoint 1.5, 6.2 members screen 2.5, 6.3 audit list 2 | **6** |
| Live role change | 3.3 (reduced) server 1 and screens 1 | **2** |
| Seed | 7.5 | **1.5** |
| **Total** | | **59** |

**59 hours do not fit in 36.** Every day of the plan as you described it is over: Tuesday 17.5 h, Wednesday 20.5 h (presence 5, versions 11, deploy 4.5), Thursday 21 h (Kanban 10.5, members and audit 6, role change 2, seed 1.5, redeploy 1). The estimates are also likely to be low: task 2.1 was planned at 4 hours and took about twice that, mostly on two findings in Yrs (see `PROGRESS.md`).

## 5. The next cut: proposal, needs a decision

Cut items, with the hours each frees and what the demo loses:

| Item | Cut | Frees | What is lost |
|---|---|---|---|
| K1 | Presence shows avatars and the count only (no remote cursors and selections) | 2 h | Colleagues' cursors are not drawn |
| K2 | Versions without preview mode (list, save a named version, restore) | 2 h | No read-only preview before restoring |
| K3 | 3.3 live role change not built: REST checks apply at once, but an editor who is demoted keeps editing an open document until it reloads | 2 h | FR11 is only half met (a known gap) |
| K4 | **Kanban deferred** (5.1 to 5.3) | 10.5 h | No board, no tasks |
| K5 | The members screen and the audit list screen deferred (the endpoints and their tests stay: 6.1) | 4.5 h | Members and the audit trail can only be shown through the API |
| K6 | Kanban kept but reduced: no task edit dialog or delete, no empty-board polish (alternative to K4) | 2.5 h | A thin board |

Packages, and what each needs:

| Package | Cuts | Total hours | Per day (3 days) |
|---|---|---|---|
| A: keep everything | K1, K2, K6 | 52.5 | 17.5 h |
| **B** | K1, K2, K3, K4 | 42.5 | 14.2 h |
| **C (recommended if days are 12 hours)** | K1, K2, K3, K4, K5 | 38 | 12.7 h |

Recommendation: **package C**, because it is the only one close to the 36 hours there are, and the 2 hours over are covered by the spare hour of Friday morning. If you can work about 14 hours a day, **package B** keeps the members and audit screens (the DBMS-heavy part of the demo: roles, the audit log). Either way, check on **Wednesday evening**: if the live documents are not deployed by then, apply the next item (B becomes C) without asking again.

The day boundaries also move, because a day cannot hold 20 hours: the editor (2.6) cannot be split, so it spills from Tuesday to Wednesday, and versions move from Wednesday to Thursday. The first deploy stays on Wednesday.

| Day | Package C (and B) |
|---|---|
| **Tue 17** (11.5 h) | 2.1 finish and merge, 2.2, 2.3, 2.4, 2.5 |
| **Wed 18** (13.5 h) | 2.6 editor, 3.1 presence (avatars and count), 3.2 reconnect, **7.4 first deploy** |
| **Thu 19** (13 h in C; 17.5 h in B) | 4.1, 4.3, 4.4, 4.5 versions; 7.5 seed; redeploy; (B: also 6.1, 6.2, 6.3) |
| **Thu night** | Freeze |
| **Fri 20 am** | Final checks only |

In package C, 6.1 (the audit endpoint, 1.5 h) is still built and tested, because the audit rows already written by 1.3 need a way to be read; it is the first item to drop if Thursday runs late.

**Decision (Tuesday 17 November, evening): package D.** Ahead of plan, so nothing built is removed (K1 and the simple live role change stay as built; the version preview is kept). The rest is built in a **reduced form**: **Kanban** with a status dropdown or arrow buttons to move a task, **no drag and drop and no filters**; **audit** screen as a **plain table with an action filter only**; no remote cursors. **Drag and drop only at the very end**, if everything else, the seed and the deploy are done. Order: Kanban 5.1 to 5.3, then 6.1 to 6.3, then 7.5. The first deploy moves up to today.

## 6. Cuts and trims already applied

| Task | Friday version | Post-submission |
|---|---|---|
| 4.2 automatic versions | Dropped on Tuesday 17 November. Manual and restore versions stay. **FR15 is future work (ADR-012)** | The worker (10 minutes, last editor leaves, keeps the newest 50) |
| 6.3 audit screen | Plain table with an action filter | Actor and entity-type filters, JSON inspector, CSV and JSON export (frame 40) |
| 5.2 board page | Columns, task cards, create board, empty board | Filters |
| 6.2 settings | Members and access (add, change role, remove) | General settings tab and danger zone |
| 3.2 reconnect | Reconnect with backoff, banner and connection pill | "Waiting to sync" state (frame 13) |
| 3.3 live role changes | A demoted or removed user stops editing at once | Toasts and frame 42 |

2.4 stays complete (the SIGTERM flush and the metrics counters give the edits per second versus writes per second number).

## 7. Tests, scaled to the deadline

| Layer | What is required |
|---|---|
| Server | Unit and integration tests for **all** server logic and **every permission check**: each new endpoint or frame type is added to the RBAC matrix (`crates/server/tests/rbac_matrix.rs`, which fails if a route has no entry). Each audited action has a test for its row. |
| `sync-crdt` | The convergence property test, projection, validation and sync tests, the native and Wasm parity tests. Done in 2.1; the debug run of the suite takes seconds, the release step of CI runs the full 10 000-case fuzz. |
| Web | Component tests for the main states of each screen: loading, empty, error, populated, and the viewer, editor and admin controls. |
| End to end | **Only scenarios 1, 2, 3, 6 and 7** of `TESTING.md` section 2 (create and persist; two users in one block; viewer is read-only; save, restore and undo a version; board drag), and only for the features that are built. Scenarios 4, 5, 8, 9 and 10 are post-submission. |
| Coverage | The two Rust gates (`sync-crdt` 90%, `server` 75%) stay blocking. No web coverage gate; web coverage is reported only. |

## 8. Phase tables with status and day

The deliverable and acceptance text is the original, except where it asks for tests or screens that the scaled plan moves to post-submission (marked in the text). Days follow the package-C layout of section 5; the packages differ only in what is cut.

### Phase 0: bootstrap and spike (done)

| Task | Status | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 0.1 | done | Monorepo: Cargo workspace (`sync-crdt`, `server`), Next.js app in `apps/web` (pnpm, TypeScript strict, ESLint, Prettier, Vitest), `e2e/` with Playwright, `infrastructure/docker-compose.yml` with MongoDB 8.0 (the major version Atlas free clusters run), `.env.example`, `.gitignore`, `README.md` | `cargo test`, `pnpm test`, `pnpm lint` run green on an empty project |
| 0.2 | done | CI workflow `ci.yml` (`DEPLOYMENT.md` §4) running every gate in `TESTING.md` §3 that applies so far | CI green on a PR |
| 0.3 | done | Spike: `sync-crdt` with one shared Y.Text, compiled natively and to Wasm; a throwaway Axum WebSocket relay; two browser tabs converge | Parity test passes; a short note in `PROGRESS.md`. If Yrs in Wasm fails, stop and record the fallback (Yjs on the client) as an ADR before going on |

### Phase 1: accounts, workspaces, design system

| Task | Status, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 1.1 | done | Server skeleton: config, tracing, `/healthz`, Mongo connection, index creation, error type and format, request body limit, CORS | Integration test for `/healthz` and the error format |
| 1.2 | done | Auth: register, login, logout, me; argon2id; JWT cookie | Auth tests in `TESTING.md` |
| 1.3 | done | Workspaces and members endpoints with RBAC module and audit writer (MEMBER_ADD, MEMBER_REMOVE, ROLE_CHANGE) | RBAC matrix test (for routes so far), member and owner rules, audit rows |
| 1.4 | done (PR #10) | `ts-rs` type generation (output `apps/web/src/types/generated/`) and CI staleness check | CI fails if types are stale |
| 1.5 | done (PR #11) | Web: tokens, fonts, component inventory part 1 (Button, Input, Select, Dialog, Toast, Badge, Avatar, NavItem, Banner, EmptyState) on `/dev/components` | Component tests for the main states of each component (default, disabled, loading, error) |
| 1.6 | done (PR #12) | Web: `/login`, `/register`, `/onboarding`, `/w` picker, app shell (sidebar, header, connection pill placeholder), Next.js rewrite of `/api/*` to the server | Frames 01 to 05 match; component tests. The standalone e2e "register, create workspace, see it in the picker" is covered by scenario 1 |

### Phase 2: documents and live editing

| Task | Status, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 2.1 | done (PR #14) | `sync-crdt`: schema, ops, projection, validation, sync wrappers, Wasm bindings | Convergence property test, projection, move, validation fuzz, parity |
| 2.2 | done | Documents REST (list, create, get, rename, delete) with DOC_CREATE and DOC_DELETE audit | Integration and RBAC tests |
| 2.3 | done | WebSocket hub: tickets, doc rooms, handshake, broadcast, frame role checks, rate limits, size limit, close codes | Ticket and frame tests; two clients converge; late joiner |
| 2.4 | done | Snapshot worker, final flush, SIGTERM flush, `SNAPSHOT_SAVED`, metrics counters | Snapshot and restart durability tests |
| 2.5 | done | Web: documents list, empty state, create dialog (frames 06 to 08) | Component tests (the e2e create-document is covered by scenario 1) |
| 2.6 | done | Web: block editor with the six block types, minimal-diff input, slash menu, hover and focus states, code copy, checklist toggle (frames 09 to 11), and the socket client | Editor logic tests; e2e scenario 1 |

### Phase 3: collaboration details

| Task | Status, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 3.1 | done (PR #23; avatars and count only, K1) | Awareness: presence avatars, remote cursors and selections with sticky indices, `PRESENCE_COUNT` | e2e scenario 2 |
| 3.2 | done (PR #29) | Reconnect with backoff and ticket refresh; reconnecting banner and pill. **Trimmed:** no "Waiting to sync" state (frame 13) | Socket client tests (e2e scenario 5 is post-submission) |
| 3.3 | done (PR #20, ADR-014; the simple version) | Live role changes and removal; viewer mode. **Trimmed:** a demoted or removed user stops editing at once; toasts and frame 42 are post-submission | Frame tests; e2e scenario 3 (scenario 4 is post-submission; the server test "a demoted editor's next update is rejected" covers its logic) |
| 3.4 | SHOULD | Session expiry: on close code 4001 redirect to login (the full flow keeping unsynced edits, frame 41, is post-submission) | Socket client test for 4001 |
| 3.5 | SHOULD | Share dialog (15), delete dialog (16), command palette (17), workspace socket with `DOC_LIST_EVENT` and `MEMBER_EVENT` | Component tests (e2e scenario 9 is post-submission) |

### Phase 4: version history

| Task | Status, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 4.1 | done (PR #22) | `document_versions` repository, zstd compression, version counter, retention | Numbering under concurrency, retention tests |
| 4.2 | **post-submission (FR15 is future work, ADR-012)**, dropped on 17 November | Automatic versions worker (10 min, last editor leaves) with controllable clock | Automatic version tests |
| 4.3 | done (PR #27) | Manual save, list, preview endpoints with VERSION_SAVE audit and `VERSION_CREATED` broadcast | Integration and RBAC tests |
| 4.4 | done (PR #25) | `replace_content` in `sync-crdt` and the restore endpoint with VERSION_RESTORE and `DOC_RESTORED` | Restore tests (crate and server) |
| 4.5 | done (PR #31; package D: with preview mode) | Web: history panel, create named version, preview mode, restore confirmation and success, viewer and empty variants, "no longer kept" (frames 18 to 24b) | Component tests; e2e scenario 6 (Thursday, when the restore endpoint has been integrated) |

### Phase 5: Kanban

| Task | Status, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 5.1 | implemented, PR to open (Tue) | Boards and tasks REST, move ordering and renumbering, member checks, TASK_CREATE, TASK_MOVE, TASK_DELETE audit, `BOARD_EVENT` | Task tests in `TESTING.md` |
| 5.2 | implemented, PR to open (Tue); package D: no filters | Web: board page, columns, task card states, create board, empty board (frames 25 to 27b). **Trimmed:** filters are post-submission | Component tests |
| 5.3 | implemented, PR to open (Tue); package D: arrow buttons and status dropdown, **no drag and drop** (only at the very end) | Web: create task, details and edit, delete, move between columns with a status dropdown or arrow buttons (keyboard accessible), viewer board (frames 28 to 32). **Trimmed:** drag and drop is post-submission unless time is left at the very end | e2e scenario 7 (moving a task with the status control) |

### Phase 6: settings and audit

| Task | Status, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 6.1 | implemented, PR to open (Tue) | Audit list endpoint with filters and cursor | Audit tests |
| 6.2 | implemented, PR to open (Tue); package D | Web: members and access, add member, change role, remove member (frames 34 to 37). **Trimmed:** general settings and the danger zone (frames 33, 38) are post-submission | Component tests (e2e scenario 8 is post-submission; its logic is covered by the member and audit server tests) |
| 6.3 | implemented, PR to open (Tue); package D: a plain table with an action filter only | Web: audit trail as a plain table with an action filter (frame 39). **Trimmed:** the actor and entity-type filters, the JSON inspector and the CSV and JSON export (frame 40) are post-submission | Component tests for the table and the action filter |

### Phase 7: hardening, measurement, deployment

| Task | Status, day | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 7.1 | post-submission | Full RBAC matrix and frame test sweep; fix gaps reported by `test-writer` | Coverage gates in `TESTING.md` §3 met |
| 7.2 | SHOULD | Load test tool and results (a 60-second run) | Numbers recorded in `PROGRESS.md`; targets met or explained |
| 7.3 | post-submission | Accessibility pass | e2e scenario 10 |
| 7.4 | MUST, Tue evening (first deploy, moved up), redeploy Thu; hardening done (PR #21) | Dockerfile (multi-stage, distroless or debian-slim), deploy workflows, Render, Vercel and Atlas setup per `DEPLOYMENT.md` | Smoke test checklist in `DEPLOYMENT.md` §8 passes on the hosted URLs |
| 7.5 | implemented, PR to open (Tue) | Seed script for the demo (workspace "Engineering Team", three members, documents and a board like the Figma frames) | Runs against a fresh database |

### Phase 8 (optional): several nodes with Redis (post-submission)

| Task | Priority | Deliverable | Acceptance criteria and tests |
|---|---|---|---|
| 8.1 | post-submission | `Bus` trait, in-memory and Redis implementations | Unit tests for both |
| 8.2 | post-submission | State-vector catch-up between nodes; conditional snapshot writes | Integration test with two server processes |
| 8.3 | post-submission | docker compose demo with two nodes, Redis and a reverse proxy | e2e scenario 2 with the two contexts on different nodes |

## 9. Working rules for the week

- A PR is merged only when CI is green; a failing check stops the day's work on that PR, it is not merged past.
- After each task: update `docs/PROGRESS.md` (task log, decisions, open questions), as before.
- Wednesday evening is a checkpoint: if the live documents are not deployed, the next cut of section 5 is applied.
- Anything not on the MUST or SHOULD list that someone asks for during the week needs a decision before work starts; anything out of scope in `PRD.md` still needs an ADR (`CLAUDE.md` rule 10).

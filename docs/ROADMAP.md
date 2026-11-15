# Roadmap

Work through the phases in order. Each task is one plan-and-implement cycle in Claude Code (plan with Opus, implement with Sonnet). A task is done when its acceptance criteria hold and its tests pass (see `CLAUDE.md`, definition of done). At the end of each phase, run the `test-writer` and `code-reviewer` agents and fix what they report, then tag the commit `phase-N`.

## Phase 0: bootstrap and spike

| Task | Deliverable | Acceptance criteria and tests |
|---|---|---|
| 0.1 | Monorepo: Cargo workspace (`sync-crdt`, `server`), Next.js app in `apps/web` (pnpm, TypeScript strict, ESLint, Prettier, Vitest), `e2e/` with Playwright, `infrastructure/docker-compose.yml` with MongoDB 8.0 (the major version Atlas free clusters run), `.env.example`, `.gitignore`, `README.md` | `cargo test`, `pnpm test`, `pnpm lint` run green on an empty project |
| 0.2 | CI workflow `ci.yml` (`DEPLOYMENT.md` §4) running every gate in `TESTING.md` §3 that applies so far | CI green on a PR |
| 0.3 | Spike: `sync-crdt` with one shared Y.Text, compiled natively and to Wasm; a throwaway Axum WebSocket relay; two browser tabs converge | Parity test passes; a short note in `PROGRESS.md`. If Yrs in Wasm fails, stop and record the fallback (Yjs on the client) as an ADR before going on |

## Phase 1: accounts, workspaces, design system

| Task | Deliverable | Acceptance criteria and tests |
|---|---|---|
| 1.1 | Server skeleton: config, tracing, `/healthz`, Mongo connection, index creation, error type and format, request body limit, CORS | Integration test for `/healthz` and the error format |
| 1.2 | Auth: register, login, logout, me; argon2id; JWT cookie | Auth tests in `TESTING.md` |
| 1.3 | Workspaces and members endpoints with RBAC module and audit writer (MEMBER_ADD, MEMBER_REMOVE, ROLE_CHANGE) | RBAC matrix test (for routes so far), member and owner rules, audit rows |
| 1.4 | `ts-rs` type generation and CI staleness check | CI fails if types are stale |
| 1.5 | Web: tokens, fonts, component inventory part 1 (Button, Input, Select, Dialog, Toast, Badge, Avatar, NavItem, Banner, EmptyState) on `/dev/components` | Component tests for every state |
| 1.6 | Web: `/login`, `/register`, `/onboarding`, `/w` picker, app shell (sidebar, header, connection pill placeholder), Next.js rewrite of `/api/*` to the server | Frames 01 to 05 match; e2e: register, create workspace, see it in the picker |

## Phase 2: documents and live editing

| Task | Deliverable | Acceptance criteria and tests |
|---|---|---|
| 2.1 | `sync-crdt`: schema, ops, projection, validation, sync wrappers, Wasm bindings | Convergence property test, projection, move, validation fuzz, parity |
| 2.2 | Documents REST (list, create, get, rename, delete) with DOC_CREATE and DOC_DELETE audit | Integration and RBAC tests |
| 2.3 | WebSocket hub: tickets, doc rooms, handshake, broadcast, frame role checks, rate limits, size limit, close codes | Ticket and frame tests; two clients converge; late joiner |
| 2.4 | Snapshot worker, final flush, SIGTERM flush, `SNAPSHOT_SAVED`, metrics counters | Snapshot and restart durability tests |
| 2.5 | Web: documents list, empty state, create dialog (frames 06 to 08) | Component tests; e2e create document |
| 2.6 | Web: block editor with the six block types, minimal-diff input, slash menu, hover and focus states, code copy, checklist toggle (frames 09 to 11) | Editor logic tests; e2e scenario 1 |

## Phase 3: collaboration details

| Task | Deliverable | Acceptance criteria and tests |
|---|---|---|
| 3.1 | Awareness: presence avatars, remote cursors and selections with sticky indices, `PRESENCE_COUNT` | e2e scenario 2 |
| 3.2 | Reconnect with backoff and ticket refresh; reconnecting banner and pill; "Waiting to sync" (frame 13) | Socket client tests; e2e scenario 5 |
| 3.3 | Live role changes and removal; viewer mode (frames 12, 14, 42) | Frame tests; e2e scenarios 3 and 4 |
| 3.4 | Session expiry flow keeping unsynced edits (frame 41) | Socket client test for 4001; manual check noted in PROGRESS |
| 3.5 | Share dialog (15), delete dialog (16), command palette (17), workspace socket with `DOC_LIST_EVENT` and `MEMBER_EVENT` | Component tests; e2e scenario 9 |

## Phase 4: version history

| Task | Deliverable | Acceptance criteria and tests |
|---|---|---|
| 4.1 | `document_versions` repository, zstd compression, version counter, retention | Numbering under concurrency, retention tests |
| 4.2 | Automatic versions worker (10 min, last editor leaves) with controllable clock | Automatic version tests |
| 4.3 | Manual save, list, preview endpoints with VERSION_SAVE audit and `VERSION_CREATED` broadcast | Integration and RBAC tests |
| 4.4 | `replace_content` in `sync-crdt` and the restore endpoint with VERSION_RESTORE and `DOC_RESTORED` | Restore tests (crate and server) |
| 4.5 | Web: history panel, create named version, preview mode, restore confirmation and success, viewer and empty variants, "no longer kept" (frames 18 to 24b) | Component tests; e2e scenario 6 |

## Phase 5: Kanban

| Task | Deliverable | Acceptance criteria and tests |
|---|---|---|
| 5.1 | Boards and tasks REST, move ordering and renumbering, member checks, TASK_CREATE, TASK_MOVE, TASK_DELETE audit, `BOARD_EVENT` | Task tests in `TESTING.md` |
| 5.2 | Web: board page, columns, task card states, filters, create board, empty board (frames 25 to 27b) | Component tests |
| 5.3 | Web: create task, details and edit, delete, drag and drop with keyboard support, viewer board (frames 28 to 32) | e2e scenario 7 |

## Phase 6: settings and audit

| Task | Deliverable | Acceptance criteria and tests |
|---|---|---|
| 6.1 | Audit list endpoint with filters and cursor | Audit tests |
| 6.2 | Web: general settings, members and access, add member, change role, remove member, danger zone (frames 33 to 38) | Component tests; e2e scenario 8 |
| 6.3 | Web: audit trail, JSON inspector, CSV and JSON export of loaded rows (frames 39, 40) | Export tests |

## Phase 7: hardening, measurement, deployment

| Task | Deliverable | Acceptance criteria and tests |
|---|---|---|
| 7.1 | Full RBAC matrix and frame test sweep; fix gaps reported by `test-writer` | Coverage gates in `TESTING.md` §3 met |
| 7.2 | Load test tool and results | Numbers recorded in `PROGRESS.md`; targets met or explained |
| 7.3 | Accessibility pass | e2e scenario 10 |
| 7.4 | Dockerfile (multi-stage, distroless or debian-slim), deploy workflows, Render, Vercel and Atlas setup per `DEPLOYMENT.md` | Smoke test checklist in `DEPLOYMENT.md` §8 passes on the hosted URLs |
| 7.5 | Seed script for the demo (workspace "Engineering Team", three members, documents and a board like the Figma frames) | Runs against a fresh database |

## Phase 8 (optional): several nodes with Redis

| Task | Deliverable | Acceptance criteria and tests |
|---|---|---|
| 8.1 | `Bus` trait, in-memory and Redis implementations | Unit tests for both |
| 8.2 | State-vector catch-up between nodes; conditional snapshot writes | Integration test with two server processes |
| 8.3 | docker compose demo with two nodes, Redis and a reverse proxy | e2e scenario 2 with the two contexts on different nodes |

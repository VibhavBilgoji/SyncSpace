# SyncSpace: project memory for Claude Code

SyncSpace is a real-time collaborative workspace: block-based documents edited by several people at once, Kanban boards, Admin / Editor / Viewer roles, document version history and an append-only audit log. It is a DBMS mini project by Vibhav Bilgoji (24B-CO-076), Sparsh Pai Bir (24B-CO-069) and Suyash Berde (24B-CO-070).

## Read before any work

| File | What it decides |
|---|---|
| `docs/PRD.md` | Users, roles, functional requirements FR1 to FR19, scope |
| `docs/ARCHITECTURE.md` | Components, flows, concurrency rules, crate and folder layout |
| `docs/DATA_MODEL.md` | MongoDB collections, indexes, invariants, audit event mapping |
| `docs/API.md` | REST endpoints, WebSocket protocol, error and close codes |
| `docs/UI_SPEC.md` | Design tokens, routes, screens mapped to the Figma frames |
| `docs/TESTING.md` | Required tests per layer and CI gates |
| `docs/ROADMAP.md` | Phases, tasks and acceptance criteria |
| `docs/DECISIONS.md` | Architecture decision records. Do not reverse one without adding a new ADR |
| `docs/DEPLOYMENT.md` | Hosting, CI/CD and environment variables |
| `docs/PROGRESS.md` | Running log. Update it at the end of every task |

If two documents disagree, the order of authority is: DECISIONS, DATA_MODEL, API, ARCHITECTURE, UI_SPEC, PRD. Report the conflict in `docs/PROGRESS.md` under "Open questions" and do not silently pick one.

Figma reference (wireframes v4.1, dark theme): https://www.figma.com/design/SjNEVcjUYzfuuD9AklAcHM/SyncSpace-%C2%B7-Wireframes-v4.1
If a Figma MCP server is connected, use it to inspect frames. Otherwise `docs/UI_SPEC.md` is the source of truth for the UI.

## Repository layout

```
syncspace/
├── Cargo.toml                    # Rust workspace
├── crates/
│   ├── sync-crdt/                # shared Yrs logic, built natively and to wasm32
│   └── server/                   # Axum + Tokio API, WebSocket hub, snapshot and version workers
├── apps/web/                     # Next.js (App Router, TypeScript strict)
├── e2e/                          # Playwright end-to-end tests
├── tools/loadtest/               # Rust load generator for the edits/s vs writes/s metric
├── infrastructure/docker-compose.yml
├── .github/workflows/            # ci.yml, deploy-api.yml, deploy-web.yml
└── docs/
```

## Commands

| Task | Command |
|---|---|
| Start MongoDB locally | `docker compose -f infrastructure/docker-compose.yml up -d mongo` |
| Run the server | `cargo run -p server` |
| Rust tests | `cargo test --workspace` |
| Rust lint | `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings` |
| Build the Wasm package | `pnpm --dir apps/web wasm` (runs wasm-pack for `crates/sync-crdt`, outputs to `apps/web/src/wasm/pkg`) |
| Wasm tests | `wasm-pack test --node crates/sync-crdt` |
| Web dev server | `pnpm --dir apps/web dev` |
| Web unit tests | `pnpm --dir apps/web test` |
| Web lint and types | `pnpm --dir apps/web lint && pnpm --dir apps/web typecheck` |
| End-to-end tests | `pnpm --dir e2e test` (needs server, web and MongoDB running) |
| Regenerate TypeScript types from Rust | `cargo run -p server --bin gen-types` |
| Seed demo data | `cargo run -p server --bin seed -- --workspace "Engineering Team"` |
| Coverage | `cargo llvm-cov -p sync-crdt --fail-under-lines 90` and `cargo llvm-cov -p server --fail-under-lines 75` |

Keep this table correct. If you add or rename a script, update it here in the same change.

## Rules

1. **Tests are part of every task.** Write or update tests in the same change as the code. A task is not done until its tests exist, pass locally and would pass in CI. Follow `docs/TESTING.md`. Never delete or weaken a failing test to make CI green; fix the code or record the reason in `docs/PROGRESS.md`.
2. **Server is the authority.** Every REST handler and every WebSocket frame checks the caller's role (`docs/API.md` permission matrix). The UI hides disabled actions, but never relies on that.
3. **The server never panics on client input.** No `unwrap()` or `expect()` on data from the network or the database outside tests. Decode errors close the socket with code 1008.
4. **Never hold a document lock across `.await`.** Encode, then release, then write to MongoDB.
5. **Audit rows are append-only.** No code path updates or deletes `audit_logs`. Every action in the audit table of `docs/DATA_MODEL.md` writes exactly one row.
6. **One `yrs` version.** Pin the exact same `yrs` version for the native and Wasm builds (workspace dependency). The parity test must pass.
7. **Secrets never in git.** Use `.env` (ignored) and `.env.example` (committed, no values).
8. **Small, reviewable changes.** One roadmap task per commit or PR, with a conventional commit message (`feat(server): ...`, `test(crdt): ...`).
9. **UI matches the Figma.** Use only the design tokens in `docs/UI_SPEC.md`. No new colours, no light theme unless asked.
10. **Ask before scope changes.** Anything listed as out of scope in `docs/PRD.md` needs a new ADR and the team's approval.

## Working with opusplan

- Plan each roadmap task in plan mode (Opus). The plan must list: files to create or change, tests to write, and how the acceptance criteria will be checked.
- After the plan is approved, implement (Sonnet), run the commands above, and update `docs/PROGRESS.md`.
- At the end of each phase, run the `test-writer` agent to find missing tests and the `code-reviewer` agent to review the phase diff, then fix what they report.

## Definition of done (every task)

- [ ] Acceptance criteria in `docs/ROADMAP.md` met
- [ ] Unit and integration tests added; `cargo test`, web tests and lint pass
- [ ] Permission checks covered by a test when the task adds an endpoint or frame type
- [ ] Audit row written and tested when the task adds an audited action
- [ ] Docs updated if behaviour, endpoints, schema or commands changed
- [ ] `docs/PROGRESS.md` updated

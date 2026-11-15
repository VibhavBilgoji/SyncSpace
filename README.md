# SyncSpace

A real-time collaborative workspace: block-based documents edited by several people at once, Kanban boards, Admin / Editor / Viewer roles, document version history and an append-only audit log.

A DBMS mini project by Vibhav Bilgoji (24B-CO-076), Sparsh Pai Bir (24B-CO-069) and Suyash Berde (24B-CO-070).

Stack: Rust (Axum, Tokio, Yrs CRDT, compiled natively and to WebAssembly), MongoDB, Next.js (App Router, TypeScript strict), Playwright.

## Prerequisites

- Rust (rustup). `rust-toolchain.toml` pins the toolchain (1.100.0) and installs the `wasm32-unknown-unknown` target.
- Node 22 or newer and pnpm
- MongoDB 8.0 for local development and the server integration tests: either Docker (`docker compose -f infrastructure/docker-compose.yml up -d mongo`) or a native MongoDB Community Server 8.0 on `localhost:27017`. It is the version Atlas free clusters run; do not use another major version (a test checks it).
- `wasm-pack` (`cargo install wasm-pack --locked`), for the browser CRDT package

## Quick start

```bash
cp .env.example .env                                      # fill in local values
docker compose -f infrastructure/docker-compose.yml up -d mongo
cargo run -p server --bin spike_relay                     # throwaway CRDT relay on 127.0.0.1:8080; the real API arrives in Phase 1
pnpm --dir apps/web install
pnpm --dir apps/web wasm                                  # build the browser CRDT package
pnpm --dir apps/web dev                                   # http://localhost:3000
```

## Commands

| Task | Command |
|---|---|
| Rust tests | `cargo test --workspace` |
| Rust lint | `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings` |
| Wasm tests (parity with the native build) | `wasm-pack test --node crates/sync-crdt` |
| Build the Wasm package | `pnpm --dir apps/web wasm` |
| Web unit tests | `pnpm --dir apps/web test` |
| Web lint and types | `pnpm --dir apps/web lint && pnpm --dir apps/web typecheck` |
| Web format | `pnpm --dir apps/web format` |
| End-to-end tests | `pnpm --dir e2e install && pnpm --dir e2e exec playwright install chromium firefox && pnpm --dir e2e test` |

The full list, including coverage and code generation, is in [`CLAUDE.md`](CLAUDE.md).

## Documentation

All design documents are in [`docs/`](docs): PRD, architecture, data model, API, UI spec, testing, roadmap, decisions, deployment and the running progress log.

## How this repo was built

It is built task by task with Claude Code following `docs/ROADMAP.md`. `MASTER_PROMPT.md` holds the prompts used to start each phase.

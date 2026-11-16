# Architecture decision records

Add a new record instead of editing an accepted one. Format: context, decision, consequences.

## ADR-001: CRDTs (Yrs), not operational transformation
**Context.** Concurrent edits must merge without a central sequencer, including edits made offline.
**Decision.** Use Yrs (the Rust port of Yjs, YATA algorithm). Say "CRDT updates" in the report and viva, not "operational transformations".
**Consequences.** Merges are automatic and commutative. Deleted content leaves tombstones, so documents grow; acceptable at project scale.

## ADR-002: one CRDT crate for server and browser through WebAssembly
**Decision.** `crates/sync-crdt` is compiled natively and with wasm-bindgen for the browser. Both pin the same `yrs` version. A parity test guards it.
**Consequences.** One implementation of block ops, projection and restore. Wasm is not claimed to be faster than JavaScript. wasm-pack moved out of the archived rustwasm organisation; if it does not install, build with `cargo build --target wasm32-unknown-unknown` plus `wasm-bindgen-cli` of the same version as the `wasm-bindgen` crate.

## ADR-003: throttled snapshots instead of an update log
**Decision.** Write the full document state at most every 5 seconds per dirty document and on the last disconnect.
**Consequences.** Far fewer database writes. A crash loses nothing while a client stays connected, because clients resend missing updates on reconnect.

## ADR-004: blocks stored as a derived projection
**Decision.** `crdt_snapshot` is the source of truth; `documents.blocks` is rebuilt at every snapshot and is read-only.
**Consequences.** Listings, previews and search do not need the CRDT. Block is a weak entity in the ER diagram.

## ADR-005: version history as a separate collection (supersedes the v4 "not built" decision)
**Context.** The team added version history to the scope; the Figma frames 18 to 24b define it.
**Decision.** `document_versions` stores compressed full snapshots with `version_number`, `change_type` (AUTOMATIC, MANUAL, RESTORE), `label`, `restored_from`, `created_by`. Automatic versions at most every 10 minutes, newest 50 kept; manual and restore kept. A restore first stores the current content as a RESTORE version, then applies the target content as a normal CRDT edit.
**Consequences.** History is never rewritten and a restore can be undone. Storage grows with versions, so snapshots are zstd-compressed and automatic versions are capped. `revision` (snapshot counter) and `version_number` are separate numbers.

## ADR-006: REST through a Next.js rewrite, WebSockets direct with tickets
**Context.** The web app (Vercel) and the API (Render) are on different domains. Browsers increasingly block third-party cookies, and Vercel rewrites do not proxy WebSockets.
**Decision.** The browser calls `/api/*` on the web domain, which Next.js rewrites to the API, so the session cookie is first-party. For WebSockets the browser gets a 30-second single-use ticket over REST and connects directly to the API host; the server checks the ticket and the `Origin` header.
**Consequences.** No cookies on the WebSocket and no tokens in long-lived URLs or logs. The API must allow the web origin in `ALLOWED_ORIGINS`.

## ADR-007: roles stored in the embedded members array
**Decision.** `workspaces.members[{user_id, role, joined_at}]`; the owner is an ADMIN member and is locked.
**Consequences.** One read gives membership and role. Live sessions are updated in process on role change.

## ADR-008: server-computed fractional order for tasks and blocks
**Decision.** Tasks: the server computes `order` from neighbours; renumber when gaps get tiny. Blocks: the client computes `order` inside the CRDT (the CRDT handles concurrency); ties break on `block_id`.
**Consequences.** No list rewrite on every move; card moves are last-write-wins per card.

## ADR-009: no due dates, nested documents, workspace deletion or public links
**Context.** The Figma file shows due dates on cards and nested documents in the sidebar as design references, and its scope notes exclude public sharing, workspace deletion, version deletion and diffs.
**Decision.** Not modeled. Cards show the creation date; the sidebar list is flat; the danger zone links to document and task deletion only.

## ADR-010: dark theme only
**Decision.** Ship the dark theme from the Figma tokens. Keep colours in CSS variables so a light theme could be added later.

## ADR-011: server integration tests use `MONGODB_URI`, not testcontainers
**Decision.** Integration tests run against the MongoDB at `MONGODB_URI` (default `mongodb://localhost:27017`): a native `mongod` 8.0 locally and a `mongo:8.0` service container in CI, each test in a database of its own; no test starts containers itself, so they need no Docker.

## ADR-012: automatic versions (FR15, task 4.2) are future work
**Context.** The project has one developer and a deadline of Friday 20 November 2026. The remaining work was estimated at about 59 hours for 36 available (`docs/ROADMAP.md` section 4), and the automatic versions worker (a 10-minute timer, a "last editor leaves" trigger, retention of the newest 50) is the least visible part of version history: the demo shows saving a named version, listing versions and restoring one.
**Decision.** The worker is not built for the submission. Versions are manual (`MANUAL`) and restore (`RESTORE`) only. The `AUTOMATIC` change type stays in the schema (`DATA_MODEL.md`) and in the generated types, so adding the worker later changes no contract.
**Consequences.** FR15 is unmet and is listed as future work in `PRD.md`. The "Automatic version" flow in `ARCHITECTURE.md` section 3.4 is the design for it. Until it exists no version is ever pruned, so `restored_from_kept` is always true. Data-model invariant 5 (at most 50 automatic versions) has nothing to check. Reversing this decision means implementing task 4.2 as written in the roadmap.


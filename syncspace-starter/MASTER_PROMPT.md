# Master prompt

Start Claude Code in the repository root with the opusplan model (`claude --model opusplan`, or `/model opusplan` inside a session). Opus plans in plan mode and Sonnet implements after you approve. Paste everything between the lines below as your first message, in plan mode (Shift+Tab until the footer shows plan mode).

---

You are the lead engineer building **SyncSpace**, a real-time collaborative workspace for block-based documents and Kanban boards, for our DBMS mini project. You will build it end to end, phase by phase, with tests for everything, and deploy it on free tiers.

## Sources of truth
Read these now, fully, before planning anything:
1. `CLAUDE.md`: rules, commands, definition of done, how we work.
2. `docs/PRD.md`: users, roles, FR1 to FR19, scope and out of scope.
3. `docs/ARCHITECTURE.md`: components, flows, concurrency and security rules.
4. `docs/DATA_MODEL.md`: collections, invariants, audit mapping, CRDT structure.
5. `docs/API.md`: REST endpoints, WebSocket protocol, close codes, shared types.
6. `docs/UI_SPEC.md`: design tokens and every screen, mapped to our Figma wireframes v4.1: https://www.figma.com/design/SjNEVcjUYzfuuD9AklAcHM/SyncSpace-%C2%B7-Wireframes-v4.1 . If a Figma MCP server is connected, use it to check spacing and sizes against the frames listed there; otherwise UI_SPEC.md is authoritative.
7. `docs/TESTING.md`: required tests per layer and the CI gates.
8. `docs/ROADMAP.md`: phases 0 to 8 with tasks and acceptance criteria.
9. `docs/DECISIONS.md`: accepted decisions. Do not reverse one without proposing a new ADR and getting my approval.
10. `docs/DEPLOYMENT.md`: hosting, CI/CD, environment variables.

## Stack
Rust stable (edition 2021 or newer): Axum, Tokio, Yrs, mongodb driver, serde, argon2, jsonwebtoken, tracing, ts-rs, zstd. `crates/sync-crdt` compiled natively and to `wasm32-unknown-unknown` with wasm-bindgen. Next.js App Router with TypeScript strict, React, Tailwind CSS, TanStack Query, dnd-kit, cmdk. Tests: cargo test, proptest, wasm-bindgen-test, testcontainers, Vitest with Testing Library, Playwright with axe-core. MongoDB 7 locally; Atlas in production.

Version policy: check the current stable versions yourself (`cargo search`, `cargo add`, `npm view <pkg> version`) and pin them; do not rely on remembered version numbers. Starting points when this kit was written: axum 0.8, tokio 1, mongodb 3, Next.js 16 with React 19, Tailwind 4. Use one exact `yrs` version as a workspace dependency for both builds. If `wasm-pack` cannot be installed, use `wasm-bindgen-cli` matching the `wasm-bindgen` crate version (ADR-002).

## How we work (every task)
1. Pick the next task from `docs/ROADMAP.md` (or the one I name).
2. In plan mode, give me a plan: files to create or change, the tests you will write (test names and what each asserts), how each acceptance criterion will be checked, and risks or questions. Wait for my approval.
3. Implement with tests in the same change. Tests are not optional and not a later phase: unit tests next to the code, integration tests for every endpoint and frame type, property tests for the CRDT, component tests for every UI state, Playwright scenarios as listed in TESTING.md. A bug fix starts with a failing test.
4. Run the commands in `CLAUDE.md` for everything you touched until they pass. Never weaken or delete a failing test to get green; fix the cause or stop and tell me.
5. Update `docs/PROGRESS.md` and any doc whose content changed. Suggest a conventional commit message.
6. At the end of each phase: run the `test-writer` agent, then the `code-reviewer` agent (both in `.claude/agents/`), fix what they find, re-run all suites, and tell me the phase is ready to tag `phase-N`.

You can also run a single task with the `/next-task` command.

## Non-negotiables
- The server enforces every permission on every REST call and every WebSocket frame. The RBAC matrix test covers every route.
- The server never panics on client bytes and never holds a document lock across `.await`.
- Audit rows are append-only and follow the mapping in DATA_MODEL.md exactly.
- `crdt_snapshot` is the source of truth; `blocks` is derived.
- A restore is a normal CRDT edit preceded by a RESTORE version; history is never rewritten.
- The UI uses only the tokens in UI_SPEC.md and matches the Figma frames, dark theme.
- Secrets only in env vars; `.env.example` lists every variable.
- Stay inside the scope in PRD.md. If something seems missing or the docs disagree, write it under "Open questions" in PROGRESS.md and ask me; do not guess silently.

## Communication
Keep updates short: what you did, test results, what is next, questions. When you need a decision from me, give the options with a recommendation.

## First step
Read all the files above. Then, in plan mode, give me:
1. a summary of the system in at most 10 bullet points, so I can check you understood it;
2. any contradictions or gaps you found in the docs;
3. the plan for Phase 0, tasks 0.1 to 0.3 (repo bootstrap, CI, and the Yrs-in-Wasm spike), with the tests for each.
Do not write code until I approve the plan.

---

## Prompts for later

- **Next task:** `/next-task` (or `/next-task 4.4`)
- **End of phase:** "Phase N is implemented. Run test-writer, then code-reviewer, fix their findings, re-run every suite, update PROGRESS.md and tell me if it is ready to tag."
- **Bug:** "Bug: <what happens, steps>. Write a failing test that reproduces it first, then fix it, then show me both."
- **UI check:** "Compare the <screen> with Figma frame <number> in UI_SPEC.md and list every difference in layout, tokens, copy and states; then fix them."
- **Deploy:** "Follow docs/DEPLOYMENT.md to prepare the production deploy: Dockerfile, the three workflows, next.config rewrites, seed script. List every secret and setting I must create by hand in GitHub, Render, Vercel and Atlas, in order."
- **Viva prep:** "Explain, with references to our code, how a VIEWER's edit is rejected, how a restore reaches other users live, and why MongoDB writes are much fewer than edits."

# SyncSpace starter kit

Blueprint for building SyncSpace with Claude Code (opusplan). Copy everything in this folder into the root of an empty Git repository, commit it, then follow the steps below.

## What is here

```
CLAUDE.md                     project memory Claude Code loads automatically
MASTER_PROMPT.md              the first message to paste, plus follow-up prompts
.env.example                  every environment variable
.claude/agents/test-writer.md subagent that finds and writes missing tests
.claude/agents/code-reviewer.md subagent that reviews each phase
.claude/commands/next-task.md /next-task: plan + implement one roadmap task with tests
docs/PRD.md                   users, roles, requirements FR1-FR19, scope (v5: version history in scope)
docs/ARCHITECTURE.md          system design, flows, concurrency, security, failure modes
docs/DATA_MODEL.md            MongoDB collections, invariants, audit mapping, CRDT layout
docs/API.md                   REST endpoints, WebSocket protocol, close codes, shared types
docs/UI_SPEC.md               design tokens and all 44 Figma frames mapped to routes
docs/TESTING.md               required tests, tools, CI gates
docs/ROADMAP.md               phases 0-8 with tasks and acceptance criteria
docs/DECISIONS.md             architecture decision records
docs/DEPLOYMENT.md            Atlas + Render + Vercel + GitHub Actions guide
docs/PROGRESS.md              running log Claude keeps up to date
```

## Steps

1. Create a GitHub repo `syncspace`, clone it, copy these files in, commit.
2. Install: Rust (rustup, with the `wasm32-unknown-unknown` target), Node 22 with pnpm, Docker, Claude Code.
3. In the repo root run `claude --model opusplan`, switch to plan mode (Shift+Tab), paste the prompt from `MASTER_PROMPT.md`.
4. Review the summary and the Phase 0 plan it returns, answer its questions, approve.
5. Then repeat `/next-task` for each task; at the end of each phase let it run the two agents and tag the phase.
6. For deployment, use the "Deploy" prompt in `MASTER_PROMPT.md` and do the manual steps in `docs/DEPLOYMENT.md`.

## Changes from design v4

- Version history added (`document_versions`, FR14 to FR17, Figma frames 18 to 24b), with a new "Saved by" relationship between User and Document version.
- `documents.version` renamed `revision` (the snapshot counter shown as `rev_N`).
- Owner is stored as an ADMIN member and cannot be changed or removed.
- Audit actions: 10 (adds VERSION_SAVE and VERSION_RESTORE), with entity mapping from the Figma audit frames.
- Workspace WebSocket added for board, document-list and member events; tickets now have a scope.
- REST goes through a Next.js rewrite so the session cookie is first-party (ADR-006).
- Due dates, nested documents, workspace deletion and public links stay out of scope (Figma scope notes).

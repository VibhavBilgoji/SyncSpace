---
description: Plan and implement the next unfinished SyncSpace roadmap task, with tests
argument-hint: "[task id, e.g. 2.3 — optional]"
---

Work on SyncSpace roadmap task $ARGUMENTS. If no task id is given, pick the first task in `docs/ROADMAP.md` that `docs/PROGRESS.md` does not mark as done.

1. Read `CLAUDE.md`, the task's row in `docs/ROADMAP.md`, and the sections of `docs/ARCHITECTURE.md`, `docs/DATA_MODEL.md`, `docs/API.md`, `docs/UI_SPEC.md` and `docs/TESTING.md` that the task touches.
2. In plan mode, write a plan with: files to create or change; the tests you will write (names and what each asserts); how you will check each acceptance criterion; risks and open questions. Wait for approval.
3. Implement. Write the tests in the same change. Run the commands in `CLAUDE.md` for every part you touched until they pass.
4. Update `docs/PROGRESS.md` (task status, decisions, open questions, test counts) and any doc whose content changed.
5. Reply with: what was built, tests added, commands run with results, anything left open. Suggest the commit message.

If this was the last task of a phase, also run the `test-writer` agent, then the `code-reviewer` agent, fix what they report, and say the phase is ready to tag.

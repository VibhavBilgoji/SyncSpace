# SyncSpace: product requirements (v5)

v5 replaces the scope of the v4 technical design. Changes from v4: document version history is now in scope, and the scope notes from the Figma wireframes v4.1 are adopted.

## 1. Problem

Teams that share files by email or chat overwrite each other's edits, keep documents and tasks in separate tools, cannot see who changed what, and cannot go back to an earlier state of a document. SyncSpace gives a team one database-backed workspace where concurrent edits merge automatically, access is controlled by roles, important actions are recorded, and earlier versions of a document can be previewed and restored.

## 2. Users and roles

Each user has one role per workspace. The workspace owner is always a member with the ADMIN role, and the owner's membership cannot be changed or removed ("Owner access is locked").

| Action | ADMIN | EDITOR | VIEWER |
|---|---|---|---|
| Read documents and boards, see cursors | yes | yes | yes |
| Send document edits | yes | yes | no |
| Create documents | yes | yes | no |
| Rename a document, change its icon | yes | yes | no |
| Delete documents | yes | no | no |
| Create boards | yes | yes | no |
| Create, edit, move and delete tasks | yes | yes | no |
| View version history and preview versions | yes | yes | yes |
| Save a named version, restore a version | yes | yes | no |
| Manage members and roles, edit workspace settings | yes | no | no |
| View and export the audit trail | yes | no | no |

## 3. Functional requirements

| ID | Requirement |
|---|---|
| FR1 | A user can register with name, email and password, and log in. A session lasts 24 hours. |
| FR2 | A logged-in user can create a workspace (name, unique slug) and becomes its owner and an ADMIN member. |
| FR3 | An ADMIN can add an existing registered user by email with a role, change a member's role, and remove a member. The owner cannot be changed or removed. |
| FR4 | ADMINs and EDITORs can create documents (title, icon). |
| FR5 | ADMINs and EDITORs can edit documents made of blocks: PARAGRAPH, HEADING_1, HEADING_2, CODE, CHECKLIST, CALLOUT. Simultaneous edits, including edits made while disconnected, merge without conflicts. |
| FR6 | Everyone in a document sees the other people present and their cursors in real time. |
| FR7 | The server saves each open document to MongoDB at most every 5 seconds while it changes, and when the last client leaves. The document shows its revision number and save state. |
| FR8 | Only ADMINs can delete documents. |
| FR9 | ADMINs and EDITORs can create boards and create, edit and delete tasks (title, description, status, priority, assignees, tags). |
| FR10 | ADMINs and EDITORs can move tasks between columns (Backlog, To do, In progress, Review, Done) and reorder them. Every member sees moves live. |
| FR11 | VIEWERs have read-only access. The server rejects their write attempts. A role change or removal takes effect on open sessions immediately. **Partly future work (ADR-013): for now it takes effect when the session's socket reconnects.** |
| FR12 | The system writes one audit row for each of these actions: DOC_CREATE, DOC_DELETE, ROLE_CHANGE, MEMBER_ADD, MEMBER_REMOVE, TASK_CREATE, TASK_MOVE, TASK_DELETE, VERSION_SAVE, VERSION_RESTORE. |
| FR13 | Only ADMINs can read the audit trail, filter it by action, actor and entity type, inspect an entry's JSON, and export the loaded entries as CSV or JSON. |
| FR14 | ADMINs and EDITORs can save a named version (manual save point with a label). |
| FR15 | The server saves an automatic version at most every 10 minutes while a document changes, and when the last editor leaves if the content changed since the last version. It keeps the newest 50 automatic versions per document. Manual and restore versions are kept. **Future work: not built for the submission (ADR-012).** |
| FR16 | Every member can list a document's versions (number, type, label, time, author) and preview any version read-only. |
| FR17 | ADMINs and EDITORs can restore a version. The current content is saved first as a RESTORE version, then the old content is applied as a new edit that everyone connected sees live. No version is deleted by a restore. |
| FR18 | A command palette (Ctrl/Cmd + K) jumps to documents, boards, settings and (for ADMINs) the audit trail. |
| FR19 | A document can be shared with a workspace-scoped link that only workspace members can open. |

## 4. Non-functional requirements

| Area | Requirement |
|---|---|
| Latency | Update-to-broadcast p95 under 100 ms on one node with 20 editors in one document |
| Database load | MongoDB writes per second at least 20 times lower than edits per second under the load test |
| Durability | No acknowledged edit is lost if the server restarts while at least one client stays open |
| Security | argon2id password hashes; httpOnly session cookie; WebSocket tickets valid 30 s; Origin check; per-connection rate limits; 1 MiB frame limit |
| Storage | Fits the Atlas free cluster (0.5 GB) for a class demo: version snapshots are compressed and automatic versions are capped at 50 per document |
| Accessibility | Keyboard navigation for editor, palette, dialogs and board; visible focus states; contrast per the design tokens |
| Browsers | Latest Chrome, Firefox, Edge and Safari on desktop at 1280 px width and above |

## 5. Out of scope

From the Figma scope notes and the v4 design:
- inline bold and italic formatting
- public (non-member) sharing links
- deleting a workspace
- deleting individual versions, and side-by-side comparison of versions
- task due dates and nested documents (the wireframes show them as design references only; the backend does not model them, so the UI shows the task creation date and a flat document list)
- a server-side audit export endpoint (export covers the loaded entries only)
- media hosting, video calls, billing, offline-first PWA caching, refresh tokens

Optional last phase: running several server nodes with Redis Pub/Sub.

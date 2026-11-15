# UI specification

Source: Figma "SyncSpace · Wireframes v4.1" (https://www.figma.com/design/SjNEVcjUYzfuuD9AklAcHM/SyncSpace-%C2%B7-Wireframes-v4.1). Style: "Technical Minimalism", dark theme by default. Frames are 1440 px wide.

| Figma page | Node id | Frames |
|---|---|---|
| 00 · Foundations & states | 0-1 | design foundations, component states, scope notes, components |
| 01 · Access & workspace | 2-129 | 01 to 08 |
| 02 · Documents | 2-130 | 09 to 17, 41, 42 |
| 03 · Version history | 2-131 | 18 to 24b |
| 04 · Kanban | 2-132 | 25 to 32 |
| 05 · Settings & audit | 2-133 | 33 to 40 |
| 06 · Full HD | | larger-screen variants; use the same layout rules |

Pixel sizes and type sizes below are working values read from the frames. If a Figma MCP server is available, check them against the frames and update this file.

## 1. Design tokens

Define these as CSS variables in `apps/web/src/styles/tokens.css` and map them in the Tailwind theme. Do not use raw hex values in components.

| Token | Value | Use |
|---|---|---|
| `--canvas` | `#090A0F` | page background |
| `--surface` | `#12141A` | sidebar, cards, panels |
| `--elevated` | `#1A1D26` | dialogs, menus, popovers, code blocks |
| `--hover` | `#222634` | hover rows and items |
| `--border` | `#232733` | dividers, inputs, card outlines |
| `--indigo` | `#6366F1` | primary actions, focus ring, selection, MANUAL badge |
| `--success` | `#10B981` | connected state, saved state, ADMIN text badge |
| `--warning` | `#F59E0B` | reconnecting, read-only archive banner, HIGH priority |
| `--danger` | `#EF4444` | destructive actions, URGENT priority, errors |
| `--text` | `#E6E8EF` | primary text |
| `--text-muted` | `#8A90A2` | secondary text, metadata |
| `--text-faint` | `#5B6172` | placeholders, disabled |

Priority badge colours: URGENT danger, HIGH warning, MEDIUM indigo, LOW text-muted. Version badges: MANUAL indigo, RESTORE violet `#A855F7` on a dark violet tint, AUTOMATIC muted. Audit action pills: one tint per action (see frame 39).

Radius: 6 px controls, 8 px cards, 10 px dialogs. Spacing: 4 px grid. Focus: 2 px indigo ring, offset 2 px.

## 2. Typography

Inter for the UI, JetBrains Mono for code, revision labels, ids and the audit JSON.

| Style | Size / line height / weight |
|---|---|
| display-xl | 32 / 40 / 700 (document title) |
| heading-lg | 22 / 30 / 600 (HEADING_1 block, page titles) |
| heading-md | 17 / 24 / 600 (HEADING_2 block, dialog titles) |
| body-md | 15 / 24 / 400 (paragraph blocks) |
| ui-base | 14 / 20 / 500 (buttons, nav) |
| ui-sm | 13 / 18 / 400 (tables, metadata) |
| micro | 11 / 14 / 600, uppercase, letter-spacing 0.06em (section labels like DOCUMENTS, column headers) |
| mono-code | 13 / 20 / 400 |

## 3. Application shell

**Sidebar** (240 px, `--surface`): workspace switcher (initial tile, workspace name, the user's role in micro text, chevron opens the picker); sections DOCUMENTS (document list), BOARDS (board list), SYSTEM (Audit Trail with an ADMIN badge, shown to ADMINs only; Settings & Members); at the bottom a connection pill ("Connected (WebSocket) 5ms", "Reconnecting...", "Connecting...") and the user row with sign-out.

**Header** (52 px): breadcrumb `Workspace / Section / Item`; search field "Search or jump to... Ctrl K" which opens the command palette; on document pages also presence avatars (max 3 plus "+N"), "Version History" button and a primary "Share" button.

Nested documents in the sidebar are a design reference only: render a flat list.

## 4. Routes and frames

| Route | Frames | Notes |
|---|---|---|
| `/login` | 01 Sign in, 03 Authentication error | error shown inline under the form |
| `/register` | 02 Create account | |
| `/onboarding` | 04 Create first workspace | shown when the user has no workspaces |
| `/w` | 05 Workspace picker | cards with role and counts |
| `/w/[slug]` | redirects to `/w/[slug]/docs` | |
| `/w/[slug]/docs` | 06 Documents list, 07 Empty workspace, 08 Create document | create is a dialog |
| `/w/[slug]/docs/[docId]` | 09 to 16, 41, 42 | the editor; share link uses this URL |
| same, `?history=1` | 18 to 24b | history panel on the right (360 px) |
| `/w/[slug]/boards/[boardId]` | 25 to 32 | |
| `/w/[slug]/settings` | 33 General | tabs: General Settings, Members & Access, Danger Zone |
| `/w/[slug]/settings/members` | 34 to 37 | |
| `/w/[slug]/settings/danger` | 38 | |
| `/w/[slug]/audit` | 39, 40 | ADMIN only; others get a 403 page |
| any page | 17 Command palette | Ctrl/Cmd + K |

## 5. Screen notes

### Access and workspace (01 to 08)
- Sign in and create account use the Input and Button components; disabled submit until valid; loading state on submit.
- Create first workspace: name and slug (slug auto-filled from the name, editable, live uniqueness error from 409).
- Empty workspace (07): illustration-free empty state with a primary "Create document" action (hidden for VIEWERs).

### Documents (09 to 17, 41, 42)
- **09 Collaborative document.** Document icon, title (display-xl), metadata line in mono: `Revision: rev_42 · Saved to MongoDB · 5s interval · Active Sockets: 3`. "Saved to MongoDB" turns to "Waiting to sync" (warning) while offline. Blocks render in a 680 px column. Remote cursors show a coloured caret with a name tag; remote selection is highlighted in the user's colour.
- **10 Slash command menu.** Typing `/` at the start of an empty block opens a filterable menu: Heading 1, Heading 2, Paragraph, Code, Checklist, Callout, with arrow keys, Enter to select, Esc to close.
- **11 Block hover and focus.** Hover shows `+` (insert block below) and a drag handle (`⋮⋮`) to the left; focus shows a caret and a faint left marker. States per the "Block canvas item" component: Idle, Hover, Focused, RemoteEdit.
- **Block types.** CODE has a language label (RUST) and a copy button; CHECKLIST has a checkbox and strikes through checked items; CALLOUT has an indigo left border on `--elevated`.
- **12 Viewer document.** Banner: "VIEWER · Read-only access. You can follow edits and preview saved versions." No `+`, no handles, inputs read-only, sidebar hides create actions.
- **13 Reconnecting.** Warning banner "Reconnecting... Your edits are kept in this tab and will merge when connected." Connection pill turns warning.
- **14 Role demotion.** Toast (danger) "Your role was changed to VIEWER. Write capabilities have been revoked." and the page switches to the viewer state without reload.
- **15 Document sharing.** Dialog "Share <title>": "Only workspace members can open this document.", WORKSPACE LINK field with the URL, "Copy link", member count line, "Admins and editors can edit. Viewers can read and preview versions."
- **16 Delete document.** ADMIN only. "This removes the document from <workspace>. This action cannot be undone." Danger button.
- **17 Command palette.** Groups DOCUMENTS and WORKSPACE (boards, Settings & Members, Audit Trail with ADMIN badge for admins). Footer hints: navigate, open, close.
- **41 Session expired.** Dialog: "Sign in again to reconnect to <workspace>. Keep this tab open until your document is synced." Warning "Edits waiting to sync are kept in this open tab." Buttons Cancel, Sign in again (opens login in a dialog or new tab and returns).
- **42 Workspace access removed.** Blocking dialog; the only action returns to `/w`.

### Version history (18 to 24b)
- **18 Panel.** Title "Version History", button "Create Named Version", then version cards newest first: badge `v4 · MANUAL` / `v3 · RESTORE` / `v2 · AUTOMATIC`, label, `time · author` ("Server Snapshot" for automatic), buttons Preview and Restore. Footer: "Newest 50 automatic versions retained. Manual and restore versions are kept."
- **19 Create named version.** Dialog with a label field (required, 80 chars).
- **20 Archived version preview.** The document area shows the archived blocks read-only, with a warning banner "Viewing Version 4 · Read-only archive. Live editing is suspended." and metadata `Version 4 · MANUAL · Oct 12, 4:15 PM`. The previewed card shows "Previewing" (disabled) and Restore. Live updates keep arriving in the background; leaving preview shows the live doc.
- **21 Restore confirmation.** "Restore Version 4?" "The archived blocks will replace the current content for everyone in this document. A copy of the current content is saved first, so you can undo the restore." Warning line with the live collaborator count. Buttons Cancel, Confirm & Restore.
- **22 Restore success.** Toast and the new RESTORE card at the top.
- **23 Viewer version history.** Same list with Preview only; no create or restore buttons.
- **24a Empty history.** "No versions yet" with the create action for editors.
- **24b Restore source no longer kept.** RESTORE card reads "Restored from Version 1 · no longer kept" when `restored_from_kept` is false.

### Kanban (25 to 32)
- **25 Board.** Title, filter buttons "Assignee: All" and "Priority: All", primary "+ New Task". Five fixed-width columns (BACKLOG, TODO, IN PROGRESS, REVIEW, DONE) with a coloured dot, count and `+`. Empty column shows "Drop tasks here" and "+ Add task".
- **Task card** (component states Idle, Hover, Dragging, DropGhost): tags as `#tag`, priority badge, title, created date (the wireframe's due date slot shows `created_at`, since due dates are not modeled), assignee avatars.
- **26 Filters.** Dropdown menus; filtering is client-side over the loaded tasks and reflected in the URL query.
- **27a Create board, 27b Empty board.**
- **28 Create task, 29 Task details and edit.** Fields: title, description, status, priority, assignees (multiple, members only), tags (comma separated). Details has Delete for ADMIN and EDITOR.
- **30 Drag and drop.** dnd-kit with a drop ghost; keyboard dragging supported.
- **31 Viewer board.** No create, no drag, read-only details.
- **32 Delete task** confirmation.

### Settings and audit (33 to 40)
- **33 General.** Workspace name and slug; Save. ADMIN only (others see read-only values).
- **34 Members & access.** Table: member, email, role (dropdown), joined, action. Owner row shows `ADMIN (Owner)` with a lock icon instead of delete. Note: "Only admins can manage members. Owner access is locked." "+ Invite Member" opens 35.
- **35 Add workspace member.** Email (must be a registered account) and role. Error "No SyncSpace account uses this email" on 404.
- **36 Change member role.** Role menu with descriptions; warning "Role changes apply to open sessions immediately."
- **37 Remove workspace member.** Confirmation.
- **38 Danger zone.** No workspace deletion. Card "Delete workspace content": links to delete documents (Admin only) and tasks (Admin and Editor); "Deletions are recorded in the audit trail."
- **39 Audit trail.** Title "System Audit Trail" with an "IMMUTABLE APPEND-ONLY" badge; filter bar (action, actor, entity); columns TIMESTAMP (UTC, mono), ACTOR, ACTION (pill), ENTITY TYPE, ENTITY ID (short id), STATE DELTA METADATA (one-line summary); footer "N events loaded" and "Load older events"; button "Export CSV / JSON" (loaded events only).
- **40 JSON inspector.** Clicking a row expands its JSON (event_id, actor_id, action, entity_id, state_delta, timestamp) in mono.

## 6. Component inventory

Build these first (phase 1 of the web app) with stories or a `/dev/components` page and unit tests:

`Button` (primary, secondary, danger, ghost; default, hover, focused, disabled, loading), `Input`, `Select`, `Dialog`, `Toast`, `Badge`, `Avatar` and `AvatarStack`, `NavItem`, `AppSidebar`, `AppHeader`, `ConnectionPill`, `CommandPalette`, `BlockItem` (Idle, Hover, Focused, RemoteEdit), `SlashMenu`, `RemoteCursor`, `VersionCard`, `TaskCard` (Idle, Hover, Dragging, DropGhost), `KanbanColumn`, `MemberRow`, `AuditRow`, `JsonInspector`, `EmptyState`, `Banner` (info, warning, danger).

## 7. Behaviour rules

- Every write control is hidden or disabled for roles that cannot use it, and the server still enforces it.
- Optimistic updates for task moves and edits, rolled back with a toast on error.
- All dialogs trap focus, close on Esc, and return focus to the trigger.
- Times show in the user's local time zone, except the audit trail which shows UTC as in the design.
- Loading states use skeletons shaped like the content; no spinners over the whole page.

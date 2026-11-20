# Demo script (7 minutes)

Live app: **https://syncspace-amber.vercel.app**. Three browser windows side by side, one per role, all signed in to the same workspace:

| Window | Account | Role |
|---|---|---|
| **A, Admin** | `vibhav@syncspace.demo` | ADMIN, owns the workspace |
| **E, Editor** | `sparsh@syncspace.demo` | EDITOR |
| **V, Viewer** | `suyash@syncspace.demo` | VIEWER |

The password is `DEMO_PASSWORD` in your local, gitignored `.env.production.local` (it is not in the repository and not printed anywhere). Use three separate sessions: a normal window, a private window and a second browser (or three browser profiles), because the session cookie is shared inside one browser profile.

## Before you present (5 minutes before)

1. Open `https://syncspace-api-latest.onrender.com/healthz` once. The free API instance sleeps after 15 minutes; the first request takes about a minute. It must say `{"status":"ok","mongo":"ok"}`.
2. In each window open `https://syncspace-amber.vercel.app` (it goes to the login page), sign in with that window's account, and click the workspace card **Engineering Team**.
3. Arrange the three windows so all three are visible. Zoom each to about 90 %.
4. Leave every window on **All documents**.

If something looks wrong, see "If something goes wrong" at the end.

## Rehearse once

The automated live smoke test (`pnpm --dir e2e test:live`) covers sign-in for the three accounts, live editing, the read-only viewer, save and restore, a Kanban move seen in a second browser, members and the audit table. It does **not** cover the slash menu, drag and drop, the role-change dialog or the two-people-typing-at-once step; run through those yourself once before presenting.

## 0:00 to 0:45. What it is and who is who

*Say:* "SyncSpace is a collaborative workspace: documents several people edit at the same time, Kanban boards, three roles, version history and an audit trail. The same Rust code that runs on the server also runs in the browser as WebAssembly, which is how edits merge without conflicts."

1. In **A**, point at the sidebar: **Documents**, **Boards**, and under **System**, **Audit Trail** with the green **Admin** badge and **Settings & Members**.
2. In **E** and **V**, point out that the sidebar has **no Audit Trail**. Look at the bottom left of each window: the user name and the connection pill.

## 0:45 to 2:15. Live editing and presence

1. In all three windows click the document **Product Requirements**.
2. Look at the top right of the header: the presence avatars, and under the title the line `Revision: rev_N · Connected · Active Sockets: 3` (it changes to `Saved to MongoDB` a few seconds after an edit).
3. In **A** click at the end of the last paragraph ("Do we keep automatic versions for the next release?") and type ` Yes, as future work.` It appears in **E** and **V** while you type.
4. In **E** click at the end of the first heading, press **Enter**, then type `/` and choose **Heading 2** from the menu. The new block appears everywhere; type `Demo notes`.
5. In **E** press **Enter** again and type a sentence at the same time as someone types in **A** (two people typing in the same block): both texts are kept. *Say:* "This is a CRDT: both edits merge, nobody's text is lost."
6. In **V** point at the banner **VIEWER · Read-only access**. Click into a paragraph and type: nothing changes, and there are no insert buttons. *Say:* "The server refuses a viewer's edits as well, not only the interface."

## 2:15 to 3:45. Version history and restore

1. In **A** click **Version History** (top right of the header). The panel lists two versions: **v2 · MANUAL** "Ready for review" and **v1 · MANUAL** "Outline", saved by Sparsh Pai Bir.
2. In **A** click **Create Named Version**, type `Demo checkpoint`, click **Save version**. A new **v3 · MANUAL** card appears. In **E** click **Version History** too: the new card appears there without a reload.
3. In **A** click **Preview** on **v1**: the banner **Viewing Version 1 · Read-only archive** appears and the document shows the outline. Click **Back to live document**.
4. In **A** type a few extra words in the document. Then click **Restore** on **v1**. The dialog says "A copy of the current content is saved first, so you can undo the restore" and (with the other windows open) how many other people have the document open. Click **Confirm & Restore**.
5. All three windows show the outline at once. The panel shows a new **v4 · RESTORE** card.
6. Undo it: in **A** click **Restore** on the **v4 · RESTORE** card and confirm. The words you typed are back everywhere.

## 3:45 to 5:15. Kanban with live updates

1. In all three windows click **All boards**, then **Sprint 12**. Five columns: Backlog, To do, In progress, Review, Done.
2. In **E** drag the card **Build the board page** from **In progress** to **Review** with the mouse. It moves in **A** and **V** at the same moment. *Say:* "Moves travel over a workspace WebSocket; the server writes the audit row first."
3. In **E** show the keyboard way: Tab to the handle `⠿` of **Write the API documentation**, press **Space**, **Arrow right**, **Space**. *Say:* "Drag and drop works from the keyboard, and the arrow buttons and the status dropdown on each card do the same."
4. In **A** click **+ New Task**, type `Record the demo video`, choose priority **High**, click **Create task**. It appears in **E** and **V**.
5. In **V** point out that there are no handles, no **+ New Task** and no arrow buttons, and that clicking a card opens read-only details.

## 5:15 to 6:15. Members and roles

1. In **A** click **Settings & Members**. The table shows the three members; the owner row says **ADMIN (Owner)** with a lock and has no controls.
2. In **A** change the role of **Suyash Berde** (the menu "Role of Suyash Berde") from **VIEWER** to **EDITOR**. The dialog explains each role and warns "Role changes apply to open sessions immediately". Click **Change role**.
3. In **V**: the open document connection is closed by the server and reconnects with the new role. Open **Product Requirements**: the **VIEWER** banner is gone and typing works. *Say:* "A role change reaches an open session; nothing waits for a log out."
4. Put it back: in **A** change **Suyash Berde** to **VIEWER** again and confirm. Reload **V** (or open a document) to see the read-only page again.

## 6:15 to 7:00. The audit trail

1. In **A** click **Audit Trail**. The title **System Audit Trail** with the badge **IMMUTABLE APPEND-ONLY**. Newest first: your own `ROLE_CHANGE`, `TASK_CREATE`, `TASK_MOVE`, `VERSION_RESTORE` and `VERSION_SAVE` rows on top, then the seeded rows. (Rows named `Smoke ...` come from the automated smoke test of the live site; they are real rows and cannot be deleted, which is the point.)
2. In the **Action** menu choose **TASK_MOVE**: only moves remain. Choose **All actions** again.
3. *Say:* "Each row is written by the server in the same code path as the action. There is no route that edits or deletes a row; a test checks that only one module may touch the collection, and that it only inserts." Point at the **State delta** column: `from_column: IN_PROGRESS, to_column: REVIEW`.
4. In **E** open `https://syncspace-amber.vercel.app/w/engineering-team/audit` directly: "Only admins can read the audit trail."

*Close:* "Rust and WebAssembly for the CRDT, MongoDB for the data, three roles enforced on the server, and about 800 automated tests including browser tests with several users."

## If something goes wrong

| Symptom | What to do |
|---|---|
| The first page takes about a minute | The free API instance was asleep. Wait, and open `/healthz` beforehand next time. |
| Login says the email or password is wrong | Use the exact emails above; the password is case sensitive. |
| A window shows "Reconnecting..." | Wait a few seconds (it retries with backoff); if it persists, reload that window. |
| A card or text does not appear in another window | Reload that window. The pages load the current state when they connect. |
| The workspace picker is empty | The seed was not run, or you are signed in as someone else. Run the seed (see README) and sign in again. |
| The sidebar pill says "Connecting..." on the members, audit and documents pages | Known cosmetic issue: only pages with a live socket (a document, a board) show the live state. |
| You changed something during the demo and want it clean | Everything here can be reverted from the interface; there is no need to reseed. |

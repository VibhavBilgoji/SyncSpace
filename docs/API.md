# API and real-time protocol

Base path for REST: `/api/v1`. JSON bodies use `snake_case`. Ids are 24-character hex strings. The browser calls REST through the Next.js rewrite (`/api/*` on the web domain), so the session cookie is first-party. WebSockets connect directly to the API host with a short-lived ticket.

## 1. Conventions

**Auth.** `POST /auth/register` and `POST /auth/login` set the cookie `ss_session`: a signed HS256 JWT valid for 24 hours, sent as `ss_session=<jwt>; Max-Age=86400; Path=/; HttpOnly; SameSite=Lax`, plus `Secure` whenever `COOKIE_SECURE` is true (always in production). Every endpoint except register, login and `/healthz` requires it; a missing, expired, forged or malformed session is always the same 401 `not signed in`. The token carries the user id (`sub`), `iat` and `exp` and is stateless: logout clears the cookie but cannot revoke a token before it expires.

**Errors.** Always `{"error": {"code": "...", "message": "...", "details": {...}?}}`.

| HTTP | code | When |
|---|---|---|
| 400 | `BAD_REQUEST` | malformed JSON or id |
| 401 | `UNAUTHORIZED` | missing or expired session |
| 403 | `FORBIDDEN` | the caller is a member but their role is too low |
| 404 | `NOT_FOUND` | unknown resource, or a resource in a workspace the caller is not a member of (the same answer for both, so existence does not leak) |
| 409 | `CONFLICT` | duplicate email or slug, member already present, owner change attempt |
| 422 | `VALIDATION` | field rules failed; `details` maps field to message |
| 405 | `METHOD_NOT_ALLOWED` | the path exists but not for this HTTP method |
| 413 | `PAYLOAD_TOO_LARGE` | request body over 256 KiB (also when it is streamed without a Content-Length) |
| 429 | `RATE_LIMITED` | too many requests; also on register and login while the server is already hashing as many passwords as it allows (a request waits up to 5 s for its turn) |
| 500 | `INTERNAL` | a server fault. The message is always "internal server error"; the cause is only in the server log |

Every response the server produces follows this format, including unknown paths (404), wrong methods (405) and panics caught by the server (500). A body that is not JSON, is truncated JSON, or is sent without `Content-Type: application/json` gives 400. Valid JSON of the wrong shape (a missing field, a wrong type) gives 422 with the reason in `details.body`; field rules checked by a handler map each field to its message in `details`.

**Pagination.** Cursor based: `?limit=50&cursor=<opaque>` and response `{items: [...], next_cursor: string | null}`.

## 2. REST endpoints

Role column: minimum role needed in the workspace the resource belongs to.

### Auth and health
| Method and path | Role | Body | Response |
|---|---|---|---|
| `GET /healthz` | public | | 200 `{status: "ok", mongo: "ok"}`; 503 `{status: "degraded", mongo: "error"}` when MongoDB cannot be reached or a ping takes more than 2 s. The cause is never in the answer. **Served at the server root, not under `/api/v1`** (Render's health check and the deploy checklist call `/healthz`) |
| `POST /auth/register` | public | `{name, email, password}` (rules below) | 201 `User` + session cookie; 409 when the email is taken; 422 with `details` for every field that breaks a rule |
| `POST /auth/login` | public | `{email, password}` | 200 `User` + session cookie; 401 `invalid email or password` for any failure, byte for byte the same for an unknown email, a wrong password and over-long input |
| `POST /auth/logout` | user | | 204 and a `Set-Cookie` that expires `ss_session` at once; 401 without a valid session |
| `GET /auth/me` | user | | `User`; 401 when the session is not valid or the account no longer exists |
| `POST /auth/ws-ticket` | member | `{scope: "doc" | "workspace", id}` | `{ticket, expires_at}` valid 30 s, single use |

**Account rules.** `name`: trimmed, 1 to 80 characters, no control characters. `email`: trimmed and stored lower-cased (so `Ada@Example.com` and `ada@example.com` are one account), ASCII only, at most 254 characters, a plain `local@domain.tld` address. `password`: 8 to 128 characters counted as characters and not bytes, never trimmed or changed, stored only as an argon2id hash (19 MiB, 2 iterations, 1 lane) and never returned. A taken email is a 409; two simultaneous registrations of one email give one 201 and one 409. A login for an unknown email still verifies a dummy hash, so it takes as long as a wrong password. Registration, login and logout are not audited (they are not in the audit table of `DATA_MODEL.md`).

### Workspaces and members
| Method and path | Role | Body | Response |
|---|---|---|---|
| `GET /workspaces` | user | | `[WorkspaceSummary]` with the caller's role and counts, by name |
| `POST /workspaces` | user | `{name, slug}` | 201 `Workspace` (the caller is the owner and an ADMIN member); 409 for a taken slug; 422 for every field that breaks a rule |
| `GET /workspaces/:wid` | VIEWER | | `Workspace` |
| `PATCH /workspaces/:wid` | ADMIN | `{name?, slug?}` (at least one) | `Workspace`; 409 for a taken slug |
| `GET /workspaces/:wid/members` | VIEWER | | `[Member]` with name, email, role, joined_at, is_owner; the owner first, then in join order |
| `POST /workspaces/:wid/members` | ADMIN | `{email, role}` | 201 `Member`; 404 if no account has that email; 409 if already a member; audit MEMBER_ADD |
| `PATCH /workspaces/:wid/members/:uid` | ADMIN | `{role}` | `Member`; 409 for the owner; 404 if not a member; audit ROLE_CHANGE (none when the role is already that one); the user's open document sockets are closed with 1000 "role changed" and the reconnect applies the new role (ADR-014) |
| `DELETE /workspaces/:wid/members/:uid` | ADMIN | | 204; 409 for the owner; 404 if not a member; audit MEMBER_REMOVE; the user leaves the assignees of the workspace's tasks; the user's open document sockets are closed with 4003 (ADR-014) |

**Workspace rules.** `name`: trimmed, 1 to 60 characters, no control characters. `slug`: exactly `^[a-z0-9](?:[a-z0-9-]{1,38}[a-z0-9])$` (3 to 40 characters); it is never changed for the caller, so `Team` is rejected, not turned into `team`. A path id that is not 24 hex characters is 400. A caller who is not a member of `:wid` gets the same 404 as for an id that does not exist; a member below the route's role gets 403, and the role is checked before the body, so a refused caller learns nothing about their body. The owner is an ADMIN member who cannot be demoted or removed, by anyone. Any other ADMIN may demote or remove themselves. A member email is matched like a login email (trimmed, case-insensitive). Creating or editing a workspace is not audited; adding, removing and changing members is, one row each, only when something changed.

**Live roles (ADR-014).** There is no `ROLE_CHANGED` frame: a role change or removal closes the user's open document sockets (1000 or 4003) and the reconnect applies the new role. REST re-reads membership on every request.

### Documents
| Method and path | Role | Body | Response |
|---|---|---|---|
| `GET /workspaces/:wid/documents` | VIEWER | | `[DocumentSummary]` (id, title, icon, revision, updated_at) |
| `POST /workspaces/:wid/documents` | EDITOR | `{title, icon?}` | 201 `Document`; audit DOC_CREATE |
| `GET /documents/:id` | VIEWER | | `Document` with `blocks` (no snapshot) |
| `PATCH /documents/:id` | EDITOR | `{title?, icon?}` | `Document` |
| `DELETE /documents/:id` | ADMIN | | 204; open sockets closed with 4004; versions deleted; audit DOC_DELETE |

**Document rules.** `title`: trimmed, 1 to 120 characters, no control characters. `icon`: optional, trimmed, 1 to 16 characters with no control characters (one emoji or a short icon key); a document created without one gets `📄`. A new document holds **one empty PARAGRAPH block**, so the editor can be used at once. `blocks` in a `Document` are the blocks as of the last snapshot (written at most every 5 s, task 2.4), never the CRDT itself. A document that is in a workspace the caller is not a member of, and one that does not exist, give the same 404 with the same message. The role is checked before the body is read. Renaming (`PATCH`) is not audited; creating and deleting are (DOC_CREATE, DOC_DELETE with the title). Deleting a document also deletes its versions. Closing the open sockets of a deleted document with 4004 is done by the hub (task 2.3).

### Versions
| Method and path | Role | Body | Response |
|---|---|---|---|
| `GET /documents/:id/versions` | VIEWER | `?limit&cursor` | page of `VersionSummary` (number, change_type, label, restored_from, restored_from_kept, created_by {id, name} or null, created_at) newest first |
| `POST /documents/:id/versions` | EDITOR | `{label}` | 201 `VersionSummary` (MANUAL); audit VERSION_SAVE |
| `GET /documents/:id/versions/:n` | VIEWER | | `{version: VersionSummary, blocks: [Block]}` built from the stored snapshot |
| `POST /documents/:id/versions/:n/restore` | EDITOR | | 200 `{restore_version: VersionSummary, revision}`; audit VERSION_RESTORE |

`restored_from_kept` is false when the restored version was an automatic version that has since been pruned (frame 24b shows "no longer kept").

### Boards and tasks
| Method and path | Role | Body | Response |
|---|---|---|---|
| `GET /workspaces/:wid/boards` | VIEWER | | `[Board]` with task counts per status |
| `POST /workspaces/:wid/boards` | EDITOR | `{name}` | 201 `Board` |
| `GET /boards/:bid/tasks` | VIEWER | `?assignee=&priority=` | `[Task]` sorted by status, order, _id |
| `POST /boards/:bid/tasks` | EDITOR | `{title, description?, status?, priority?, assignee_ids?, tags?}` | 201 `Task` placed at the end of its column; audit TASK_CREATE |
| `PATCH /tasks/:id` | EDITOR | any of `{title, description, priority, assignee_ids, tags}` | `Task` |
| `PATCH /tasks/:id/move` | EDITOR | `{to_status, before_id?, after_id?}` | `Task` with the server-computed `order`; audit TASK_MOVE |
| `DELETE /tasks/:id` | EDITOR | | 204; audit TASK_DELETE |

Move rule: `order` = midpoint of the neighbours, `first / 2` at the head, `last + 1.0` at the tail, `1.0` in an empty column. If the gap falls below `1e-9`, renumber that column (1.0, 2.0, ...) with one bulk write and broadcast `COLUMN_RENUMBERED`.

**Board and task rules (task 5.1).** A board's `name` is 1 to 60 characters. A task's `title` is 1 to 200 characters, `description` up to 5000 (several lines allowed), `tags` at most 10 of 1 to 24 characters (lower-cased, duplicates dropped), `assignee_ids` must be current members of the workspace (duplicates dropped); every broken rule is a 422 with one entry per field. A task created without `status` goes to `BACKLOG`, without `priority` it is `MEDIUM`. `GET /boards/:bid/tasks` sorts by column (BACKLOG first), then `order`, then id; `assignee` and `priority` are optional filters (an invalid value is a 422). A board or task of another workspace and one that does not exist give the same 404 with the same message. `PATCH /tasks/:id` needs at least one field and never changes the column or the position (only a move does); it is not audited. **Move:** `before_id` is the task that will be directly above the moved one and `after_id` the one directly below, both in the target column and never the moved task itself, else 422 on that field. Giving only `before_id` places the task just after it (before whatever follows it, or at the tail); giving only `after_id` places it just before it (the head when it is the first); giving neither puts it at the end of the target column, which also works inside the task's own column (the end is measured without the task itself). `before_id` must come before `after_id` in the column (422 otherwise). When the neighbours are closer than 1e-9 the column is renumbered 1.0, 2.0, ... first (one update per task) and the task is then placed. Live updates of an open board come from the workspace socket (section 3).

### Audit
| Method and path | Role | Body | Response |
|---|---|---|---|
| `GET /workspaces/:wid/audit` | ADMIN | `?limit&cursor&action&actor_id&entity_type` | page of `AuditEntry` (with actor name), newest first |

There is no server export endpoint. The UI exports the loaded entries as CSV or JSON.

## 3. WebSocket protocol

### Connecting
1. `POST /api/v1/auth/ws-ticket {scope, id}` through the web domain. The caller must be a member of the workspace the id belongs to (anyone else gets the same 404 as for an unknown id). The ticket is an HS256 token signed with `TICKET_SECRET` (never the session key) that names the user, the scope and the id, and expires after 30 seconds.
2. Open `wss://<API_HOST>/ws/v1/docs/{doc_id}?ticket=...` or `wss://<API_HOST>/ws/v1/workspaces/{wid}?ticket=...`.
3. The `Origin` header must be present and one of `ALLOWED_ORIGINS`: otherwise the handshake is refused with HTTP 403, before anything else. After the upgrade the server checks the ticket (genuine, not expired, for this scope and id, **never used before**: it is burnt at once), that the document exists and that the user is still a member. It stores the role for the life of the socket. Failures close the socket: 4001 for any bad ticket, 4004 for a document that does not exist, 4003 when the user is no longer a member.

The role is read when the socket connects and checked on every frame. When the role changes or the member is removed, the server closes that user's open sockets (ADR-014) and the new role applies at the reconnect.

### Document socket
Binary frames follow the y-sync protocol (`yrs::sync`), first varint is the message type: `0` sync (SyncStep1, SyncStep2, Update) and `1` awareness.

| Frame | ADMIN / EDITOR | VIEWER |
|---|---|---|
| SyncStep1 | allow | allow |
| SyncStep2, Update | allow | reject: JSON `ERROR {code: "READ_ONLY"}`, update dropped |
| Awareness | allow | allow |

**Awareness state (JSON, at most 1024 bytes).** `{"name": string, "cursor"?: {"block": block id, "anchor": int, "head": int}}`, or `null` when the person leaves. `anchor` and `head` are offsets into that block's text in UTF-16 units; the head is the end the person moves, so a backwards selection has `head < anchor`. A client re-announces every 10 s, and on a change of cursor at most every 100 ms (the awareness limit is 20 per second). The server only relays; receivers ignore a cursor that is not a block id and two non-negative integers up to 1 000 000.

Text frames are JSON `{"type": ..., ...}`. The server answers a viewer's `SyncStep2` or `Update` with `ERROR READ_ONLY` and drops the frame (the document is not changed and nobody else receives it); frames over the rate limit get `ERROR RATE_LIMITED` and are dropped, and the socket stays open; a frame or document over its size limit closes with 1009. A text frame the server does not understand, and a binary frame it cannot decode or that breaks the document schema, close with 1008.

An update that builds on changes the server does not have (it arrived before the update it depends on) is not applied; the server answers with its own SyncStep1 and keeps the socket open, and the client replies with the diff. Any other frame or update the server cannot handle closes the socket with 1008.

| Type | Direction | Payload | Purpose |
|---|---|---|---|
| `PING` / `PONG` | client -> server / reply | `{t}` | every 20 s; the client shows round-trip latency ("5ms") |
| `SNAPSHOT_SAVED` | server -> clients | `{revision, saved_at}` | updates "rev_N · Saved to MongoDB" |
| `VERSION_CREATED` | server -> clients | `VersionSummary` | refreshes an open history panel |
| `DOC_RESTORED` | server -> clients | `{restored_from, by: {id, name}, restore_version}` | toast "Restored Version N" |
| `ROLE_CHANGED` | server -> one client | `{role}` | downgrade to read-only or upgrade, without reconnecting |
| `PRESENCE_COUNT` | server -> clients | `{active_sockets}` | "Active Sockets: N" |
| `ERROR` | server -> client | `{code, message}` | `READ_ONLY`, `RATE_LIMITED`, `TOO_LARGE` |

### Workspace socket
Opened by the board pages (`/w/<slug>/boards` and `/boards/<id>`). Server to client JSON only; the one frame a client may send is `PING` (answered with `PONG`, at most the awareness rate, 20 per second); anything else closes with 1008, a text frame over 1 KiB with 1009. The handshake is the document socket's with a ticket of scope `workspace` and the workspace id: a bad `Origin` is 403 before the upgrade; a missing, bad, reused or wrong-scope ticket closes with 4001; a user who is not (or no longer) a member closes with 4003. Everyone who may read the workspace (VIEWER and up) hears its events. `Hub::close_user` closes a user's workspace sockets together with their document sockets when their role changes (1000) or they are removed (4003), as in ADR-014. A connection that falls 512 messages behind is closed with 1008 and reconnects. **Events are not stored:** a page that was not connected loads the current state over REST, and a page that reconnects loads it again.

`BOARD_EVENT` is sent after the change and its audit row succeeded, to every open socket of the workspace, the one that made the change included: `TASK_CREATED`, `TASK_UPDATED` and `TASK_MOVED` carry the `task` as it is now, `TASK_DELETED` the `task` as it was, `BOARD_CREATED` the `board`, `COLUMN_RENUMBERED` the `status` of the column (its tasks have new `order` values: reload them; it is sent before the `TASK_MOVED` of the move that caused it). `DOC_LIST_EVENT` and `MEMBER_EVENT` are **not built** (task 3.5, SHOULD).

| Type | Payload |
|---|---|
| `BOARD_EVENT` | `{board_id, kind: TASK_CREATED | TASK_UPDATED | TASK_MOVED | TASK_DELETED | BOARD_CREATED | COLUMN_RENUMBERED, task? , board?, status?}` |
| `DOC_LIST_EVENT` | `{kind: CREATED | UPDATED | DELETED, document: DocumentSummary}` |
| `MEMBER_EVENT` | `{kind: ADDED | ROLE_CHANGED | REMOVED, member}` |
| `PING` / `PONG` | `{t}` |

### Close codes
| Code | Meaning | UI |
|---|---|---|
| 1008 | policy violation (bad frame, decode error) | reconnect with backoff |
| 1009 | frame over 1 MiB | toast, reconnect |
| 4001 | unauthorized (session expired) | frame 41 "Your session has expired" |
| 4003 | forbidden (removed from workspace) | frame 42 "Workspace access removed" |
| 4004 | document deleted | redirect to documents list with a toast |

### Limits
- Max frame 1 MiB. Enforce it in application code below a higher library cap (for example 4 MiB): the WebSocket library drops the TCP connection without a close frame above its own limit, so a clean `1009` is only possible for frames between the two.
- The 1 MiB limit applies to client update frames. Full-state sync can exceed it for a large document, and a client that enforces the same limit on what it receives then cannot load the document. The 0.3 spike relay sidesteps this with a document size cap (768 KiB, checked before applying an update, close `1009` past it). The hub needs a real answer before documents can grow that large: chunked state sync or a separate state-sync limit, and a per-room size policy.
- Token bucket per connection: document updates 60/s burst 120; awareness 20/s burst 40 (separate buckets).
- Reconnect backoff: 0.5 s, 1 s, 2 s, 4 s, then 8 s max, with jitter.

## 4. Shared types (TypeScript view)

```ts
type Role = "ADMIN" | "EDITOR" | "VIEWER";
type BlockType = "PARAGRAPH" | "HEADING_1" | "HEADING_2" | "CODE" | "CHECKLIST" | "CALLOUT";
type Status = "BACKLOG" | "TODO" | "IN_PROGRESS" | "REVIEW" | "DONE";
type Priority = "LOW" | "MEDIUM" | "HIGH" | "URGENT";
type ChangeType = "AUTOMATIC" | "MANUAL" | "RESTORE";

interface User { id: string; name: string; email: string }
interface Workspace { id: string; name: string; slug: string; owner_id: string; role: Role /* the caller's */; created_at: string; updated_at: string }
interface WorkspaceSummary { id: string; name: string; slug: string; role: Role; is_owner: boolean; member_count: number; document_count: number; board_count: number }
interface UserRef { id: string; name: string }
interface DocumentSummary { id: string; title: string; icon: string; revision: number; updated_at: string }
interface Document { id: string; workspace_id: string; title: string; icon: string; revision: number; blocks: Block[];
                     created_by: string; created_at: string; updated_at: string }
interface StatusCounts { backlog: number; todo: number; in_progress: number; review: number; done: number }
interface Board { id: string; workspace_id: string; name: string; task_counts: StatusCounts; created_at: string }
interface Page<T> { items: T[]; next_cursor: string | null }
type ErrorCode = "BAD_REQUEST" | "UNAUTHORIZED" | "FORBIDDEN" | "NOT_FOUND" | "CONFLICT" | "VALIDATION"
               | "RATE_LIMITED" | "PAYLOAD_TOO_LARGE" | "METHOD_NOT_ALLOWED" | "INTERNAL";
interface ApiErrorBody { code: ErrorCode; message: string; details?: Record<string, string> }
interface ApiErrorResponse { error: ApiErrorBody }
type Scope = "doc" | "workspace";
interface WsTicketResponse { ticket: string; expires_at: string }
type SocketErrorCode = "READ_ONLY" | "RATE_LIMITED" | "TOO_LARGE"; // TOO_LARGE is reserved: oversized frames close with 1009
type ServerFrame = { type: "PONG"; t: number }
                 | { type: "ERROR"; code: SocketErrorCode; message: string }
                 | { type: "PRESENCE_COUNT"; active_sockets: number }
                 | { type: "SNAPSHOT_SAVED"; revision: number; saved_at: string }
                 | { type: "BOARD_EVENT"; board_id: string; kind: BoardEventKind; task?: Task; board?: Board; status?: Status };
type BoardEventKind = "TASK_CREATED" | "TASK_UPDATED" | "TASK_MOVED" | "TASK_DELETED" | "BOARD_CREATED" | "COLUMN_RENUMBERED";
type ClientFrame = { type: "PING"; t: number };
interface Member { user_id: string; name: string; email: string; role: Role; joined_at: string; is_owner: boolean }
interface Block { block_id: string; type: BlockType; content: string; properties: Record<string, unknown>; order: number }
interface Task { id: string; board_id: string; title: string; description: string; status: Status; priority: Priority;
                 assignee_ids: string[]; tags: string[]; order: number; created_at: string; updated_at: string }
interface VersionSummary { version_number: number; change_type: ChangeType; label: string | null; restored_from: number | null;
                           restored_from_kept: boolean | null; created_by: { id: string; name: string } | null; created_at: string }
interface VersionDetail { version: VersionSummary; blocks: Block[] }
interface RestoreResponse { restore_version: VersionSummary; revision: number }
interface AuditEntry { event_id: string; actor: { id: string; name: string }; action: string; entity_type: string;
                       entity_id: string; metadata: Record<string, unknown>; timestamp: string }
```

The TypeScript types are generated from the Rust structs (`ts-rs`) so the two never drift: `cargo run -p server --bin gen-types` writes `apps/web/src/types/generated/` (one file per type plus `index.ts`), the files are committed and never edited by hand, and CI regenerates them and fails when anything differs. The block above is the human-readable view and a test checks that it lists exactly the generated types; the request bodies (`RegisterRequest`, `LoginRequest`, `CreateWorkspaceRequest`, `UpdateWorkspaceRequest`, `AddMemberRequest`, `ChangeRoleRequest`) are generated from the bodies in the tables of section 2. `Document`, `DocumentSummary`, `Board`, `StatusCounts`, `UserRef` and `Page` are provisional until tasks 2.2 and 5.1 build their endpoints (the tables of section 2 name only some of their fields); the task that builds an endpoint may change them and regenerates. The WebSocket frame types are added with the hub (task 2.3).

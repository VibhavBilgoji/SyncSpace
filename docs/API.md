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
| `PATCH /workspaces/:wid/members/:uid` | ADMIN | `{role}` | `Member`; 409 for the owner; 404 if not a member; audit ROLE_CHANGE (none when the role is already that one); live sessions updated |
| `DELETE /workspaces/:wid/members/:uid` | ADMIN | | 204; 409 for the owner; 404 if not a member; audit MEMBER_REMOVE; the user leaves the assignees of the workspace's tasks; sessions closed with 4003 |

**Workspace rules.** `name`: trimmed, 1 to 60 characters, no control characters. `slug`: exactly `^[a-z0-9](?:[a-z0-9-]{1,38}[a-z0-9])$` (3 to 40 characters); it is never changed for the caller, so `Team` is rejected, not turned into `team`. A path id that is not 24 hex characters is 400. A caller who is not a member of `:wid` gets the same 404 as for an id that does not exist; a member below the route's role gets 403, and the role is checked before the body, so a refused caller learns nothing about their body. The owner is an ADMIN member who cannot be demoted or removed, by anyone. Any other ADMIN may demote or remove themselves. A member email is matched like a login email (trimmed, case-insensitive). Creating or editing a workspace is not audited; adding, removing and changing members is, one row each, only when something changed.

**Not yet live.** `ROLE_CHANGED` and the 4003 close need the WebSocket hub (task 2.3). Until then nothing is open to update, and REST re-reads membership on every request, so a removed or demoted user is refused at once.

### Documents
| Method and path | Role | Body | Response |
|---|---|---|---|
| `GET /workspaces/:wid/documents` | VIEWER | | `[DocumentSummary]` (id, title, icon, revision, updated_at) |
| `POST /workspaces/:wid/documents` | EDITOR | `{title, icon?}` | 201 `Document`; audit DOC_CREATE |
| `GET /documents/:id` | VIEWER | | `Document` with `blocks` (no snapshot) |
| `PATCH /documents/:id` | EDITOR | `{title?, icon?}` | `Document` |
| `DELETE /documents/:id` | ADMIN | | 204; open sockets closed with 4004; versions deleted; audit DOC_DELETE |

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

### Audit
| Method and path | Role | Body | Response |
|---|---|---|---|
| `GET /workspaces/:wid/audit` | ADMIN | `?limit&cursor&action&actor_id&entity_type` | page of `AuditEntry` (with actor name), newest first |

There is no server export endpoint. The UI exports the loaded entries as CSV or JSON.

## 3. WebSocket protocol

### Connecting
1. `POST /api/v1/auth/ws-ticket {scope, id}` through the web domain.
2. Open `wss://<API_HOST>/ws/v1/docs/{doc_id}?ticket=...` or `wss://<API_HOST>/ws/v1/workspaces/{wid}?ticket=...`.
3. The server checks the ticket signature, expiry and single use, the `Origin` header against `ALLOWED_ORIGINS`, and membership. It stores the role in the session.

### Document socket
Binary frames follow the y-sync protocol (`yrs::sync`), first varint is the message type: `0` sync (SyncStep1, SyncStep2, Update) and `1` awareness.

| Frame | ADMIN / EDITOR | VIEWER |
|---|---|---|
| SyncStep1 | allow | allow |
| SyncStep2, Update | allow | reject: JSON `ERROR {code: "READ_ONLY"}`, update dropped |
| Awareness | allow | allow |

Text frames are JSON `{"type": ..., ...}`.

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
Opened by every page inside a workspace. Server to client JSON only.

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
interface Member { user_id: string; name: string; email: string; role: Role; joined_at: string; is_owner: boolean }
interface Block { block_id: string; type: BlockType; content: string; properties: Record<string, unknown>; order: number }
interface Task { id: string; board_id: string; title: string; description: string; status: Status; priority: Priority;
                 assignee_ids: string[]; tags: string[]; order: number; created_at: string; updated_at: string }
interface VersionSummary { version_number: number; change_type: ChangeType; label: string | null; restored_from: number | null;
                           restored_from_kept: boolean | null; created_by: { id: string; name: string } | null; created_at: string }
interface AuditEntry { event_id: string; actor: { id: string; name: string }; action: string; entity_type: string;
                       entity_id: string; metadata: Record<string, unknown>; timestamp: string }
```

The TypeScript types are generated from the Rust structs (`ts-rs`) so the two never drift: `cargo run -p server --bin gen-types` writes `apps/web/src/types/generated/` (one file per type plus `index.ts`), the files are committed and never edited by hand, and CI regenerates them and fails when anything differs. The block above is the human-readable view and a test checks that it lists exactly the generated types; the request bodies (`RegisterRequest`, `LoginRequest`, `CreateWorkspaceRequest`, `UpdateWorkspaceRequest`, `AddMemberRequest`, `ChangeRoleRequest`) are generated from the bodies in the tables of section 2. `Document`, `DocumentSummary`, `Board`, `StatusCounts`, `UserRef` and `Page` are provisional until tasks 2.2 and 5.1 build their endpoints (the tables of section 2 name only some of their fields); the task that builds an endpoint may change them and regenerates. The WebSocket frame types are added with the hub (task 2.3).

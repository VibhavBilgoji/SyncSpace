# Data model

MongoDB is the system of record. The Rust server is the only service that writes to it. All timestamps are UTC `DateTime`. All `_id` values are `ObjectId` unless stated.

## 1. ER summary

Entities: User, Workspace, Document, Block (weak, owned by Document), Document version (weak, owned by Document), Board, Task, Audit log.

| Relationship | Entities | Cardinality | Stored as |
|---|---|---|---|
| Owns | User - Workspace | 1 : N | `workspaces.owner_id` |
| Member of (role, joined_at) | User - Workspace | M : N | `workspaces.members[]` |
| Contains | Workspace - Document | 1 : N | `documents.workspace_id` |
| Has block | Document - Block | 1 : N | `documents.blocks[]` (derived) |
| Has version | Document - Document version | 1 : N | `document_versions.document_id` |
| Saved by | User - Document version | 1 : N, optional | `document_versions.created_by` (null for automatic) |
| Has | Workspace - Board | 1 : N | `boards.workspace_id` |
| Groups | Board - Task | 1 : N | `tasks.board_id` |
| Assigned to | Task - User | M : N | `tasks.assignee_ids[]` |
| Performs | User - Audit log | 1 : N | `audit_logs.actor_id` |
| Records | Workspace - Audit log | 1 : N | `audit_logs.workspace_id` |

"Saved by" is new in v5: the Figma version history shows who saved each version. Add it to the ER diagram.

## 2. Collections

### users
| Field | Type | Notes |
|---|---|---|
| `_id` | ObjectId | |
| `email` | string | lower-cased, unique |
| `name` | string | 1 to 80 chars |
| `password_hash` | string | argon2id PHC string |
| `created_at` | DateTime | |

### workspaces
| Field | Type | Notes |
|---|---|---|
| `_id` | ObjectId | |
| `name` | string | 1 to 60 chars |
| `slug` | string | unique, `^[a-z0-9](?:[a-z0-9-]{1,38}[a-z0-9])$` |
| `owner_id` | ObjectId -> users | owner is also in `members` with `ADMIN` |
| `members` | array of `{user_id, role, joined_at}` | role is `ADMIN`, `EDITOR` or `VIEWER`; one entry per user |
| `created_at`, `updated_at` | DateTime | |

### documents
| Field | Type | Notes |
|---|---|---|
| `_id` | ObjectId | |
| `workspace_id` | ObjectId -> workspaces | |
| `title` | string | 1 to 120 chars |
| `icon` | string | one emoji or a short icon key |
| `revision` | i64 | +1 on every snapshot write; shown as `rev_N` |
| `crdt_snapshot` | binData | Yrs `encode_state_as_update_v1`; source of truth for content |
| `blocks` | array of `{block_id, type, content, properties, order}` | derived from the snapshot at every write, read-only, used for previews and search |
| `last_version_at` | DateTime or null | time of the newest version of any type |
| `next_version` | i32 | counter for the next `version_number`, starts at 1 |
| `changed_since_version` | bool | set by snapshot writes, cleared when a version is saved |
| `created_by` | ObjectId -> users | |
| `created_at`, `updated_at` | DateTime | |

Block fields: `block_id` (string, unique within the document), `type` (`PARAGRAPH`, `HEADING_1`, `HEADING_2`, `CODE`, `CHECKLIST`, `CALLOUT`), `content` (plain text), `properties` (object: `language` for CODE, `checked` for CHECKLIST, `tone` for CALLOUT), `order` (f64).

### document_versions (new in v5)
| Field | Type | Notes |
|---|---|---|
| `_id` | ObjectId | |
| `document_id` | ObjectId -> documents | |
| `workspace_id` | ObjectId -> workspaces | denormalised for permission checks |
| `version_number` | i32 | 1, 2, 3 ... per document; partial key |
| `change_type` | string | `AUTOMATIC`, `MANUAL` or `RESTORE` |
| `label` | string or null | required for MANUAL (1 to 80 chars); generated for the others: AUTOMATIC "Periodic Snapshot", RESTORE "Restored from Version N" |
| `restored_from` | i32 or null | version number that was restored; only for RESTORE |
| `created_by` | ObjectId or null | null for AUTOMATIC ("Server Snapshot") |
| `created_at` | DateTime | |
| `crdt_snapshot` | binData | zstd-compressed `encode_state_as_update_v1` |
| `size_bytes` | i32 | compressed size, for the storage budget |

A RESTORE row holds the content as it was **before** the restore, so a restore can be undone by restoring that row.

### boards
`_id`, `workspace_id`, `name` (1 to 60 chars), `created_by`, `created_at`.

### tasks
| Field | Type | Notes |
|---|---|---|
| `_id` | ObjectId | |
| `workspace_id` | ObjectId | duplicates the path through the board, for queries |
| `board_id` | ObjectId -> boards | |
| `title` | string | 1 to 200 chars |
| `description` | string | up to 5000 chars |
| `status` | string | `BACKLOG`, `TODO`, `IN_PROGRESS`, `REVIEW`, `DONE` |
| `priority` | string | `LOW`, `MEDIUM`, `HIGH`, `URGENT` |
| `assignee_ids` | array of ObjectId | each must be a current member |
| `tags` | array of string | up to 10, each 1 to 24 chars, lower-case |
| `order` | f64 | position within its (board, status) column |
| `created_by` | ObjectId | |
| `created_at`, `updated_at` | DateTime | |

### audit_logs
| Field | Type | Notes |
|---|---|---|
| `_id` | ObjectId | shown as `event_id` |
| `workspace_id` | ObjectId | |
| `actor_id` | ObjectId | |
| `action` | string | one of the 10 actions below |
| `entity_type` | string | `DOCUMENT`, `TASK`, `WORKSPACE`, `MEMBER` |
| `entity_id` | string | id of the item acted on |
| `metadata` | object | action-specific details (the "state delta") |
| `timestamp` | DateTime | |

## 3. Audit action mapping

This mapping follows the Figma audit trail frames (39, 40). Every row stores the actor and workspace.

| Action | entity_type | entity_id | metadata |
|---|---|---|---|
| DOC_CREATE | DOCUMENT | document id | `{title}` |
| DOC_DELETE | DOCUMENT | document id | `{title}` (kept because the document is gone) |
| VERSION_SAVE | DOCUMENT | document id | `{version_number, label}` |
| VERSION_RESTORE | DOCUMENT | document id | `{restored_from, version_number}` (the new RESTORE row's number) |
| TASK_CREATE | TASK | task id | `{title, status, priority}` |
| TASK_MOVE | TASK | task id | `{from_column, to_column, calculated_order}` |
| TASK_DELETE | TASK | task id | `{title}` |
| MEMBER_ADD | WORKSPACE | workspace id | `{user_id, assigned}` (role) |
| MEMBER_REMOVE | WORKSPACE | workspace id | `{user_id, role}` |
| ROLE_CHANGE | MEMBER | user id | `{from, to}` |

Not logged: snapshot writes, automatic versions, task edits other than moves, logins.

## 4. Indexes

```
users:              { email: 1 } unique
workspaces:         { slug: 1 } unique;  { "members.user_id": 1 }
documents:          { workspace_id: 1, updated_at: -1 }
document_versions:  { document_id: 1, version_number: -1 } unique
                    { document_id: 1, change_type: 1, version_number: -1 }
boards:             { workspace_id: 1 }
tasks:              { board_id: 1, status: 1, order: 1 };  { workspace_id: 1, assignee_ids: 1 }
audit_logs:         { workspace_id: 1, timestamp: -1, _id: -1 }
```

Create indexes at server start (idempotent `create_indexes`).

## 5. Invariants (each needs a test)

1. A workspace has exactly one owner, and the owner has an ADMIN entry in `members`.
2. `documents.blocks` always equals the projection of `crdt_snapshot`.
3. `revision` only increases.
4. `version_number` is unique per document and increases by 1; allocate it with `findOneAndUpdate` on a per-document counter (`documents.next_version`, default 1) to avoid races.
5. Automatic versions per document never exceed 50 after a version write.
6. A task's `assignee_ids` only contains current members. When a member is removed, remove them from that workspace's tasks.
7. `audit_logs` rows are never updated or deleted.
8. Deleting a document deletes its `document_versions` rows.

## 6. CRDT document structure (inside the Y.Doc)

```
root Y.Map "blocks": block_id -> Y.Map {
  type:  string            // one of the six block types
  order: f64               // fractional position
  text:  Y.Text            // plain text content
  props: Y.Map             // language | checked | tone
}
root Y.Map "meta": { schema: 1 }
```

Blocks render sorted by `(order, block_id)`. Moving a block changes its `order` (midpoint of neighbours). Deleting a block removes its key. New block ids are random 12-character base62 strings created by the client.

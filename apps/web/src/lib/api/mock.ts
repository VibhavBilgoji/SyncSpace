import { isValidSlug } from "@/lib/slug";
import type {
  Board,
  Document,
  DocumentSummary,
  Task,
  User,
  VersionSummary,
  Workspace,
  WorkspaceSummary,
} from "@/types/generated";
import { ApiError } from "./errors";
import type { Api } from "./types";

/**
 * In-memory implementation of the contract, typed with the same interfaces as the real one.
 * Behaviour follows docs/API.md (statuses, messages, 422 details).
 */
export function createMockApi(): Api {
  const users: Array<User & { password: string }> = [];
  const workspaces: WorkspaceSummary[] = [];
  const documents: Array<Document & { updated: number }> = [];
  let sessionUserId: string | null = null;
  let nextId = 1;
  const id = () => (nextId++).toString(16).padStart(24, "0");
  const boardStore: Board[] = [];
  const taskStore: Task[] = [];
  const STATUSES = ["BACKLOG", "TODO", "IN_PROGRESS", "REVIEW", "DONE"] as const;
  const countsOf = (boardId: string): Board["task_counts"] => {
    const n = (status: Task["status"]) =>
      taskStore.filter((t) => t.board_id === boardId && t.status === status).length;
    return {
      backlog: n("BACKLOG"),
      todo: n("TODO"),
      in_progress: n("IN_PROGRESS"),
      review: n("REVIEW"),
      done: n("DONE"),
    };
  };
  const column = (boardId: string, status: Task["status"], except?: string) =>
    taskStore
      .filter((t) => t.board_id === boardId && t.status === status && t.id !== except)
      .sort((a, b) => a.order - b.order);
  const versionStore = new Map<string, Array<{ summary: VersionSummary }>>();
  /** Newest first. */
  const versionsOf = (docId: string) => versionStore.get(docId) ?? [];
  function addVersion(
    docId: string,
    change_type: VersionSummary["change_type"],
    label: string | null,
    restored_from: number | null,
  ): VersionSummary {
    const existing = versionsOf(docId);
    const user = users.find((u) => u.id === sessionUserId);
    const summary: VersionSummary = {
      version_number: (existing[0]?.summary.version_number ?? 0) + 1,
      change_type,
      label: label ?? (restored_from === null ? null : `Restored from Version ${restored_from}`),
      restored_from,
      restored_from_kept: restored_from === null ? null : true,
      created_by: user ? { id: user.id, name: user.name } : null,
      created_at: new Date().toISOString(),
    };
    versionStore.set(docId, [{ summary }, ...existing]);
    return summary;
  }

  return {
    auth: {
      async register({ name, email, password }) {
        const details: Record<string, string> = {};
        if (name.trim().length < 1 || name.trim().length > 80)
          details.name = "name must be 1 to 80 characters";
        if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(email.trim()))
          details.email = "enter a valid email address";
        if ([...password].length < 8 || [...password].length > 128) {
          details.password = "password must be 8 to 128 characters";
        }
        if (Object.keys(details).length) {
          throw new ApiError(422, "VALIDATION", "validation failed", details);
        }
        const normalized = email.trim().toLowerCase();
        if (users.some((u) => u.email === normalized)) {
          throw new ApiError(409, "CONFLICT", "that email is already registered");
        }
        const user = { id: id(), name: name.trim(), email: normalized, password };
        users.push(user);
        sessionUserId = user.id;
        return { id: user.id, name: user.name, email: user.email };
      },
      async login({ email, password }) {
        const user = users.find((u) => u.email === email.trim().toLowerCase());
        if (!user || user.password !== password) {
          throw new ApiError(401, "UNAUTHORIZED", "invalid email or password");
        }
        sessionUserId = user.id;
        return { id: user.id, name: user.name, email: user.email };
      },
      async logout() {
        sessionUserId = null;
      },
      async wsTicket() {
        if (!sessionUserId) throw new ApiError(401, "UNAUTHORIZED", "not signed in");
        return { ticket: "mock-ticket", expires_at: new Date(Date.now() + 30_000).toISOString() };
      },
      async me() {
        const user = users.find((u) => u.id === sessionUserId);
        if (!user) throw new ApiError(401, "UNAUTHORIZED", "not signed in");
        return { id: user.id, name: user.name, email: user.email };
      },
    },
    workspaces: {
      async list() {
        if (!sessionUserId) throw new ApiError(401, "UNAUTHORIZED", "not signed in");
        return [...workspaces].sort((a, b) => a.name.localeCompare(b.name));
      },
      async members() {
        const user = users.find((u) => u.id === sessionUserId);
        if (!user) throw new ApiError(401, "UNAUTHORIZED", "not signed in");
        return [
          {
            user_id: user.id,
            name: user.name,
            email: user.email,
            role: "ADMIN" as const,
            joined_at: new Date().toISOString(),
            is_owner: true,
          },
        ];
      },
      async create({ name, slug }) {
        if (!sessionUserId) throw new ApiError(401, "UNAUTHORIZED", "not signed in");
        const details: Record<string, string> = {};
        if (name.trim().length < 1 || name.trim().length > 60)
          details.name = "name must be 1 to 60 characters";
        if (!isValidSlug(slug))
          details.slug = "slug must be 3 to 40 lowercase letters, digits or dashes";
        if (Object.keys(details).length) {
          throw new ApiError(422, "VALIDATION", "validation failed", details);
        }
        if (workspaces.some((w) => w.slug === slug)) {
          throw new ApiError(409, "CONFLICT", "that slug is already taken");
        }
        const wid = id();
        workspaces.push({
          id: wid,
          name: name.trim(),
          slug,
          role: "ADMIN",
          is_owner: true,
          member_count: 1,
          document_count: 0,
          board_count: 0,
        });
        const now = new Date().toISOString();
        const created: Workspace = {
          id: wid,
          name: name.trim(),
          slug,
          owner_id: sessionUserId,
          role: "ADMIN",
          created_at: now,
          updated_at: now,
        };
        return created;
      },
    },
    documents: {
      async list(workspaceId) {
        if (!sessionUserId) throw new ApiError(401, "UNAUTHORIZED", "not signed in");
        return documents
          .filter((d) => d.workspace_id === workspaceId)
          .sort((a, b) => b.updated - a.updated)
          .map<DocumentSummary>((d) => ({
            id: d.id,
            title: d.title,
            icon: d.icon,
            revision: d.revision,
            updated_at: d.updated_at,
          }));
      },
      async create(workspaceId, { title, icon }) {
        if (!sessionUserId) throw new ApiError(401, "UNAUTHORIZED", "not signed in");
        const workspace = workspaces.find((w) => w.id === workspaceId);
        if (!workspace) throw new ApiError(404, "NOT_FOUND", "not found");
        if (workspace.role === "VIEWER") throw new ApiError(403, "FORBIDDEN", "forbidden");
        const trimmed = title.trim();
        const details: Record<string, string> = {};
        if ([...trimmed].length < 1 || [...trimmed].length > 120)
          details.title = "title must be 1 to 120 characters";
        const chosen = icon?.trim() || "📄";
        if ([...chosen].length > 16) details.icon = "icon must be at most 16 characters";
        if (Object.keys(details).length) {
          throw new ApiError(422, "VALIDATION", "validation failed", details);
        }
        const now = new Date().toISOString();
        const doc = {
          id: id(),
          workspace_id: workspaceId,
          title: trimmed,
          icon: chosen,
          revision: 0,
          blocks: [],
          created_by: sessionUserId,
          created_at: now,
          updated_at: now,
          updated: Date.now(),
        };
        documents.push(doc);
        workspace.document_count += 1;
        return doc;
      },
      async get(docId) {
        const doc = documents.find((d) => d.id === docId);
        if (!sessionUserId) throw new ApiError(401, "UNAUTHORIZED", "not signed in");
        if (!doc) throw new ApiError(404, "NOT_FOUND", "not found");
        return doc;
      },
      async update(docId, { title, icon }) {
        const doc = await this.get(docId);
        if (title !== undefined) doc.title = title.trim();
        if (icon !== undefined) doc.icon = icon.trim();
        return doc;
      },
      async remove(docId) {
        const index = documents.findIndex((d) => d.id === docId);
        if (index < 0) throw new ApiError(404, "NOT_FOUND", "not found");
        documents.splice(index, 1);
      },
    },
    boards: {
      async list(workspaceId) {
        if (!sessionUserId) throw new ApiError(401, "UNAUTHORIZED", "not signed in");
        return boardStore
          .filter((b) => b.workspace_id === workspaceId)
          .map((b) => ({ ...b, task_counts: countsOf(b.id) }));
      },
      async create(workspaceId, { name }) {
        if (!sessionUserId) throw new ApiError(401, "UNAUTHORIZED", "not signed in");
        const trimmed = name.trim();
        if ([...trimmed].length < 1 || [...trimmed].length > 60) {
          throw new ApiError(422, "VALIDATION", "validation failed", {
            name: "name must be 1 to 60 characters",
          });
        }
        const board: Board = {
          id: id(),
          workspace_id: workspaceId,
          name: trimmed,
          task_counts: countsOf(""),
          created_at: new Date().toISOString(),
        };
        boardStore.push(board);
        return board;
      },
      async tasks(boardId) {
        if (!sessionUserId) throw new ApiError(401, "UNAUTHORIZED", "not signed in");
        if (!boardStore.some((b) => b.id === boardId))
          throw new ApiError(404, "NOT_FOUND", "board not found");
        return taskStore
          .filter((t) => t.board_id === boardId)
          .sort(
            (a, b) => STATUSES.indexOf(a.status) - STATUSES.indexOf(b.status) || a.order - b.order,
          );
      },
      async createTask(boardId, input) {
        if (!sessionUserId) throw new ApiError(401, "UNAUTHORIZED", "not signed in");
        if (!boardStore.some((b) => b.id === boardId))
          throw new ApiError(404, "NOT_FOUND", "board not found");
        const title = input.title.trim();
        if ([...title].length < 1 || [...title].length > 200) {
          throw new ApiError(422, "VALIDATION", "validation failed", {
            title: "title must be 1 to 200 characters",
          });
        }
        const status = input.status ?? "BACKLOG";
        const last = column(boardId, status).at(-1);
        const now = new Date().toISOString();
        const task: Task = {
          id: id(),
          board_id: boardId,
          title,
          description: input.description?.trim() ?? "",
          status,
          priority: input.priority ?? "MEDIUM",
          assignee_ids: input.assignee_ids ?? [],
          tags: (input.tags ?? []).map((t) => t.trim().toLowerCase()),
          order: (last?.order ?? 0) + 1,
          created_at: now,
          updated_at: now,
        };
        taskStore.push(task);
        return task;
      },
      async updateTask(taskId, input) {
        if (!sessionUserId) throw new ApiError(401, "UNAUTHORIZED", "not signed in");
        const task = taskStore.find((t) => t.id === taskId);
        if (!task) throw new ApiError(404, "NOT_FOUND", "task not found");
        if (input.title !== undefined) task.title = input.title.trim();
        if (input.description !== undefined) task.description = input.description.trim();
        if (input.priority !== undefined) task.priority = input.priority;
        if (input.assignee_ids !== undefined) task.assignee_ids = input.assignee_ids;
        if (input.tags !== undefined) task.tags = input.tags.map((t) => t.trim().toLowerCase());
        task.updated_at = new Date().toISOString();
        return task;
      },
      async moveTask(taskId, { to_status }) {
        if (!sessionUserId) throw new ApiError(401, "UNAUTHORIZED", "not signed in");
        const task = taskStore.find((t) => t.id === taskId);
        if (!task) throw new ApiError(404, "NOT_FOUND", "task not found");
        const last = column(task.board_id, to_status, task.id).at(-1);
        task.status = to_status;
        task.order = (last?.order ?? 0) + 1;
        task.updated_at = new Date().toISOString();
        return task;
      },
      async removeTask(taskId) {
        const index = taskStore.findIndex((t) => t.id === taskId);
        if (index < 0) throw new ApiError(404, "NOT_FOUND", "task not found");
        taskStore.splice(index, 1);
      },
    },
    versions: {
      async list(docId, { limit = 50 } = {}) {
        if (!sessionUserId) throw new ApiError(401, "UNAUTHORIZED", "not signed in");
        const all = versionsOf(docId);
        return { items: all.slice(0, limit).map((v) => v.summary), next_cursor: null };
      },
      async create(docId, { label }) {
        if (!sessionUserId) throw new ApiError(401, "UNAUTHORIZED", "not signed in");
        const trimmed = label.trim();
        if ([...trimmed].length < 1 || [...trimmed].length > 80) {
          throw new ApiError(422, "VALIDATION", "validation failed", {
            label: "label must be 1 to 80 characters",
          });
        }
        return addVersion(docId, "MANUAL", trimmed, null);
      },
      async get(docId, number) {
        if (!sessionUserId) throw new ApiError(401, "UNAUTHORIZED", "not signed in");
        const found = versionsOf(docId).find((v) => v.summary.version_number === number);
        if (!found) throw new ApiError(404, "NOT_FOUND", "not found");
        return { version: found.summary, blocks: [] };
      },
      async restore(docId, number) {
        if (!sessionUserId) throw new ApiError(401, "UNAUTHORIZED", "not signed in");
        if (!versionsOf(docId).some((v) => v.summary.version_number === number)) {
          throw new ApiError(404, "NOT_FOUND", "not found");
        }
        const restore_version = addVersion(docId, "RESTORE", null, number);
        return { restore_version, revision: restore_version.version_number };
      },
    },
  };
}

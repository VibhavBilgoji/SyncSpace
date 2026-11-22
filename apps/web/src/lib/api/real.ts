import { request } from "./http";
import type {
  AuditApi,
  AuthApi,
  BoardsApi,
  DocumentsApi,
  VersionsApi,
  WorkspacesApi,
} from "./types";
import type {
  AuditEntry,
  AuthProviders,
  Board,
  CreateBoardRequest,
  CreateTaskRequest,
  Document,
  DocumentSummary,
  Member,
  MoveTaskRequest,
  Page,
  RestoreResponse,
  SaveVersionRequest,
  Task,
  UpdateTaskRequest,
  VersionDetail,
  VersionSummary,
  User,
  Workspace,
  WorkspaceSummary,
  WsTicketResponse,
} from "@/types/generated";

export const realAuth: AuthApi = {
  register: (input) => request<User>("POST", "/auth/register", input),
  login: (input) => request<User>("POST", "/auth/login", input),
  logout: () => request<void>("POST", "/auth/logout"),
  me: () => request<User>("GET", "/auth/me"),
  providers: () => request<AuthProviders>("GET", "/auth/providers"),
  wsTicket: (input) => request<WsTicketResponse>("POST", "/auth/ws-ticket", input),
};

export const realWorkspaces: WorkspacesApi = {
  list: () => request<WorkspaceSummary[]>("GET", "/workspaces"),
  create: (input) => request<Workspace>("POST", "/workspaces", input),
  members: (workspaceId) => request<Member[]>("GET", `/workspaces/${workspaceId}/members`),
  addMember: (workspaceId, input) =>
    request<Member>("POST", `/workspaces/${workspaceId}/members`, input),
  changeRole: (workspaceId, userId, input) =>
    request<Member>("PATCH", `/workspaces/${workspaceId}/members/${userId}`, input),
  removeMember: (workspaceId, userId) =>
    request<void>("DELETE", `/workspaces/${workspaceId}/members/${userId}`),
};

export const realDocuments: DocumentsApi = {
  list: (workspaceId) => request<DocumentSummary[]>("GET", `/workspaces/${workspaceId}/documents`),
  create: (workspaceId, input) =>
    request<Document>("POST", `/workspaces/${workspaceId}/documents`, input),
  get: (id) => request<Document>("GET", `/documents/${id}`),
  update: (id, input) => request<Document>("PATCH", `/documents/${id}`, input),
  remove: (id) => request<void>("DELETE", `/documents/${id}`),
};

export const realVersions: VersionsApi = {
  list: (docId, { limit, cursor } = {}) => {
    const query = new URLSearchParams();
    if (limit !== undefined) query.set("limit", String(limit));
    if (cursor !== undefined) query.set("cursor", cursor);
    const suffix = query.size > 0 ? `?${query.toString()}` : "";
    return request<Page<VersionSummary>>("GET", `/documents/${docId}/versions${suffix}`);
  },
  create: (docId, input: SaveVersionRequest) =>
    request<VersionSummary>("POST", `/documents/${docId}/versions`, input),
  get: (docId, number) => request<VersionDetail>("GET", `/documents/${docId}/versions/${number}`),
  restore: (docId, number) =>
    request<RestoreResponse>("POST", `/documents/${docId}/versions/${number}/restore`),
};

export const realBoards: BoardsApi = {
  list: (workspaceId) => request<Board[]>("GET", `/workspaces/${workspaceId}/boards`),
  create: (workspaceId, input: CreateBoardRequest) =>
    request<Board>("POST", `/workspaces/${workspaceId}/boards`, input),
  tasks: (boardId) => request<Task[]>("GET", `/boards/${boardId}/tasks`),
  createTask: (boardId, input: CreateTaskRequest) =>
    request<Task>("POST", `/boards/${boardId}/tasks`, input),
  updateTask: (id, input: UpdateTaskRequest) => request<Task>("PATCH", `/tasks/${id}`, input),
  moveTask: (id, input: MoveTaskRequest) => request<Task>("PATCH", `/tasks/${id}/move`, input),
  removeTask: (id) => request<void>("DELETE", `/tasks/${id}`),
};

export const realAudit: AuditApi = {
  list: (workspaceId, { limit, cursor, action } = {}) => {
    const query = new URLSearchParams();
    if (limit !== undefined) query.set("limit", String(limit));
    if (cursor !== undefined) query.set("cursor", cursor);
    if (action !== undefined && action !== "") query.set("action", action);
    const suffix = query.size > 0 ? `?${query.toString()}` : "";
    return request<Page<AuditEntry>>("GET", `/workspaces/${workspaceId}/audit${suffix}`);
  },
};

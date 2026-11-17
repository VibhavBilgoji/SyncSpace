import { request } from "./http";
import type { AuthApi, DocumentsApi, VersionsApi, WorkspacesApi } from "./types";
import type {
  Document,
  DocumentSummary,
  Page,
  RestoreResponse,
  SaveVersionRequest,
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
  wsTicket: (input) => request<WsTicketResponse>("POST", "/auth/ws-ticket", input),
};

export const realWorkspaces: WorkspacesApi = {
  list: () => request<WorkspaceSummary[]>("GET", "/workspaces"),
  create: (input) => request<Workspace>("POST", "/workspaces", input),
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

import { request } from "./http";
import type { AuthApi, DocumentsApi, WorkspacesApi } from "./types";
import type {
  Document,
  DocumentSummary,
  User,
  Workspace,
  WorkspaceSummary,
} from "@/types/generated";

export const realAuth: AuthApi = {
  register: (input) => request<User>("POST", "/auth/register", input),
  login: (input) => request<User>("POST", "/auth/login", input),
  logout: () => request<void>("POST", "/auth/logout"),
  me: () => request<User>("GET", "/auth/me"),
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

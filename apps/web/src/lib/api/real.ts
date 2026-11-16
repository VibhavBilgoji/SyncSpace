import { request } from "./http";
import type { AuthApi, WorkspacesApi } from "./types";
import type { User, Workspace, WorkspaceSummary } from "@/types/generated";

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

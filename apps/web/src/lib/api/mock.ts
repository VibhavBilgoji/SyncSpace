import { isValidSlug } from "@/lib/slug";
import type { User, Workspace, WorkspaceSummary } from "@/types/generated";
import { ApiError } from "./errors";
import type { Api } from "./types";

/**
 * In-memory implementation of the contract, typed with the same interfaces as the real one.
 * Behaviour follows docs/API.md (statuses, messages, 422 details).
 */
export function createMockApi(): Api {
  const users: Array<User & { password: string }> = [];
  const workspaces: WorkspaceSummary[] = [];
  let sessionUserId: string | null = null;
  let nextId = 1;
  const id = () => (nextId++).toString(16).padStart(24, "0");

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
  };
}

import { render } from "@testing-library/react";
import type { ReactElement } from "react";
import { vi } from "vitest";
import { ApiError, api } from "@/lib/api";
import { SessionProvider } from "@/lib/session";
import type { User, WorkspaceSummary } from "@/types/generated";

export const ada: User = {
  id: "a".repeat(24),
  name: "Ada Lovelace",
  email: "ada@example.com",
  github_login: null,
};

export function workspaceSummary(overrides: Partial<WorkspaceSummary> = {}): WorkspaceSummary {
  return {
    id: "b".repeat(24),
    name: "Engineering Team",
    slug: "eng",
    role: "ADMIN",
    is_owner: true,
    member_count: 3,
    document_count: 5,
    board_count: 1,
    ...overrides,
  };
}

/** Renders inside a SessionProvider whose `/auth/me` answers with `user`, or 401 for null. */
export function renderWithSession(ui: ReactElement, user: User | null = null) {
  vi.spyOn(api.auth, "me").mockImplementation(() =>
    user
      ? Promise.resolve(user)
      : Promise.reject(new ApiError(401, "UNAUTHORIZED", "not signed in")),
  );
  return render(<SessionProvider>{ui}</SessionProvider>);
}

import { createMockApi } from "./mock";
import { realAuth, realBoards, realDocuments, realVersions, realWorkspaces } from "./real";
import type { Api } from "./types";

/**
 * Picks the implementation per endpoint group. `NEXT_PUBLIC_API_MOCKS` is a comma list of
 * groups to mock (for example `documents` until its endpoints merge); the default is all real.
 * A group is swapped to the real call the day its endpoint merges, by removing it from the list.
 */
export function buildApi(mocks: string): Api {
  const mocked = new Set(
    mocks
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean),
  );
  const mock = mocked.size > 0 ? createMockApi() : null;
  return {
    auth: mock && mocked.has("auth") ? mock.auth : realAuth,
    workspaces: mock && mocked.has("workspaces") ? mock.workspaces : realWorkspaces,
    documents: mock && mocked.has("documents") ? mock.documents : realDocuments,
    versions: mock && mocked.has("versions") ? mock.versions : realVersions,
    boards: mock && mocked.has("boards") ? mock.boards : realBoards,
  };
}

export const api: Api = buildApi(process.env.NEXT_PUBLIC_API_MOCKS ?? "");

export { ApiError, isApiError } from "./errors";
export type { Api, AuthApi, BoardsApi, DocumentsApi, VersionsApi, WorkspacesApi } from "./types";

import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "@/lib/api";
import { RepoError } from "@/lib/github";
import { ada, renderWithSession, workspaceSummary } from "@/test/render";
import { RepositoriesPage } from "./RepositoriesPage";

vi.mock("@/components/shell/WorkspaceShell", () => ({
  useWorkspace: () => workspaceSummary({ slug: "eng" }),
}));

const github = vi.hoisted(() => ({ fetchPublicRepos: vi.fn() }));
vi.mock("@/lib/github", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/github")>()),
  fetchPublicRepos: github.fetchPublicRepos,
}));

const linked = { ...ada, github_login: "octocat" };

const repo = (n: number, extra: Record<string, unknown> = {}) => ({
  id: n,
  name: `repo-${n}`,
  url: `https://github.com/octocat/repo-${n}`,
  description: `Description ${n}`,
  language: n % 2 ? "Rust" : "TypeScript",
  stars: n * 2,
  forks: n,
  updatedAt: "2026-11-20T10:00:00Z",
  fork: false,
  archived: false,
  ...extra,
});

beforeEach(() => {
  github.fetchPublicRepos.mockReset();
  vi.spyOn(api.auth, "providers").mockResolvedValue({ github: true });
});
afterEach(() => vi.restoreAllMocks());

describe("RepositoriesPage when GitHub is not linked", () => {
  it("offers to connect GitHub and returns to this page afterwards", async () => {
    renderWithSession(<RepositoriesPage />, ada);
    const connect = await screen.findByRole("link", { name: "Connect GitHub" });
    expect(connect).toHaveAttribute("href", "/api/v1/auth/github/start?next=%2Fw%2Feng%2Frepos");
    expect(github.fetchPublicRepos).not.toHaveBeenCalled();
  });

  it("says so, with no button, when this server has no GitHub sign-in", async () => {
    vi.spyOn(api.auth, "providers").mockResolvedValue({ github: false });
    renderWithSession(<RepositoriesPage />, ada);
    expect(await screen.findByText(/not set up on this server/)).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Connect GitHub" })).not.toBeInTheDocument();
  });

  it("shows no button when the providers cannot be read", async () => {
    vi.spyOn(api.auth, "providers").mockRejectedValue(new Error("down"));
    renderWithSession(<RepositoriesPage />, ada);
    expect(await screen.findByText(/not set up on this server/)).toBeInTheDocument();
  });
});

describe("RepositoriesPage when GitHub is linked", () => {
  it("shows a loading state and then every repository with its details", async () => {
    let resolve!: (value: unknown) => void;
    github.fetchPublicRepos.mockReturnValue(new Promise((r) => (resolve = r)));
    renderWithSession(<RepositoriesPage />, linked);
    expect(await screen.findByLabelText("Loading repositories")).toBeInTheDocument();

    await act(async () => {
      resolve({
        repos: [
          repo(1, { fork: true }),
          repo(2, { archived: true, description: null, language: null }),
        ],
        truncated: false,
      });
    });
    const list = await screen.findByRole("list", { name: "Repositories" });
    const items = within(list).getAllByRole("listitem");
    expect(items).toHaveLength(2);
    const first = items[0] as HTMLElement;
    const link = within(first).getByRole("link", { name: "repo-1" });
    expect(link).toHaveAttribute("href", "https://github.com/octocat/repo-1");
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noopener noreferrer");
    expect(first).toHaveTextContent("Fork");
    expect(first).toHaveTextContent("Description 1");
    expect(first).toHaveTextContent("Rust");
    expect(within(first).getByLabelText("2 stars")).toBeInTheDocument();
    expect(within(first).getByLabelText("1 forks")).toBeInTheDocument();
    expect(first).toHaveTextContent("Updated Nov 20, 2026");
    expect(items[1]).toHaveTextContent("Archived");
    expect(screen.getByText(/2 found/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "@octocat" })).toHaveAttribute(
      "href",
      "https://github.com/octocat",
    );
    expect(github.fetchPublicRepos).toHaveBeenCalledWith("octocat", expect.any(AbortSignal));
  });

  it("filters by name, description and language, and says when nothing matches", async () => {
    github.fetchPublicRepos.mockResolvedValue({
      repos: [repo(1), repo(2), repo(3, { name: "special-tool", description: "Does X" })],
      truncated: false,
    });
    renderWithSession(<RepositoriesPage />, linked);
    const filter = await screen.findByLabelText("Filter");
    const names = () =>
      within(screen.getByRole("list", { name: "Repositories" }))
        .getAllByRole("listitem")
        .map((li) => within(li).getAllByRole("link")[0]?.textContent);

    fireEvent.change(filter, { target: { value: "SPECIAL" } });
    expect(names()).toEqual(["special-tool"]);
    fireEvent.change(filter, { target: { value: "does x" } });
    expect(names()).toEqual(["special-tool"]);
    fireEvent.change(filter, { target: { value: "typescript" } });
    expect(names()).toEqual(["repo-2"]);
    fireEvent.change(filter, { target: { value: "zzz" } });
    expect(screen.queryByRole("list", { name: "Repositories" })).not.toBeInTheDocument();
    expect(screen.getByText(/No repository matches/)).toBeInTheDocument();
    fireEvent.change(filter, { target: { value: "" } });
    expect(names()).toHaveLength(3);
  });

  it("explains an account with no public repositories", async () => {
    github.fetchPublicRepos.mockResolvedValue({ repos: [], truncated: false });
    renderWithSession(<RepositoriesPage />, linked);
    expect(await screen.findByText("No public repositories")).toBeInTheDocument();
    expect(screen.queryByLabelText("Filter")).not.toBeInTheDocument();
  });

  it("says when there are more repositories than it fetched", async () => {
    github.fetchPublicRepos.mockResolvedValue({ repos: [repo(1)], truncated: true });
    renderWithSession(<RepositoriesPage />, linked);
    expect(await screen.findByText(/Showing the first 500 repositories/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "See all on GitHub" })).toHaveAttribute(
      "href",
      "https://github.com/octocat?tab=repositories",
    );
  });

  it.each([
    ["rate_limited", /limits how often/],
    ["failed", /Could not load the repositories/],
  ] as const)("a %s error is explained and can be retried", async (kind, message) => {
    github.fetchPublicRepos
      .mockRejectedValueOnce(new RepoError(kind))
      .mockResolvedValueOnce({ repos: [repo(1)], truncated: false });
    renderWithSession(<RepositoriesPage />, linked);
    expect(await screen.findByRole("alert")).toHaveTextContent(message);
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("link", { name: "repo-1" })).toBeInTheDocument();
    expect(github.fetchPublicRepos).toHaveBeenCalledTimes(2);
  });

  it("an account GitHub no longer knows is explained without a pointless retry", async () => {
    github.fetchPublicRepos.mockRejectedValue(new RepoError("not_found"));
    renderWithSession(<RepositoriesPage />, linked);
    expect(await screen.findByRole("alert")).toHaveTextContent(/does not know this account/);
    expect(screen.queryByRole("button", { name: "Try again" })).not.toBeInTheDocument();
  });

  it("an unexpected error is a failure, not a crash", async () => {
    github.fetchPublicRepos.mockRejectedValue(new Error("boom"));
    renderWithSession(<RepositoriesPage />, linked);
    expect(await screen.findByRole("alert")).toHaveTextContent(/Could not load/);
  });

  it("does not show a result that arrives after the page was left", async () => {
    let resolve!: (value: unknown) => void;
    github.fetchPublicRepos.mockImplementation(
      (_login: string, signal: AbortSignal) =>
        new Promise((r, reject) => {
          resolve = r;
          signal.addEventListener("abort", () => reject(new DOMException("a", "AbortError")));
        }),
    );
    const view = renderWithSession(<RepositoriesPage />, linked);
    await screen.findByLabelText("Loading repositories");
    view.unmount();
    await act(async () => resolve({ repos: [repo(1)], truncated: false }));
    await waitFor(() => expect(screen.queryByRole("link", { name: "repo-1" })).toBeNull());
  });
});

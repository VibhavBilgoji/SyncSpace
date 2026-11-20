import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "@/lib/api";
import { ada, renderWithSession, workspaceSummary } from "@/test/render";
import { WorkspaceShell } from "./WorkspaceShell";

const nav = vi.hoisted(() => ({
  router: { replace: vi.fn(), push: vi.fn() },
  params: { slug: "eng" },
  pathname: "/w/eng/docs",
}));
vi.mock("next/navigation", () => ({
  useRouter: () => nav.router,
  useParams: () => nav.params,
  usePathname: () => nav.pathname,
}));

const docs = [
  {
    id: "d1",
    title: "Product Requirements",
    icon: "📋",
    revision: 3,
    updated_at: "2026-11-20T00:00:00Z",
  },
  {
    id: "d2",
    title: "Sprint 12 Planning",
    icon: "🗓️",
    revision: 1,
    updated_at: "2026-11-20T00:00:00Z",
  },
];
const boards = [
  {
    id: "b1",
    workspace_id: "w",
    name: "Sprint 12",
    task_counts: { backlog: 0, todo: 0, in_progress: 0, review: 0, done: 0 },
    created_at: "2026-11-20T00:00:00Z",
  },
];

const press = (key: string) => fireEvent.keyDown(screen.getByRole("combobox"), { key });
const type = (value: string) =>
  fireEvent.change(screen.getByRole("combobox"), { target: { value } });

beforeEach(() => {
  nav.router.push.mockClear();
  vi.restoreAllMocks();
});

async function openShell(role: "ADMIN" | "EDITOR" | "VIEWER" = "ADMIN") {
  vi.spyOn(api.workspaces, "list").mockResolvedValue([workspaceSummary({ role })]);
  vi.spyOn(api.documents, "list").mockResolvedValue(docs);
  vi.spyOn(api.boards, "list").mockResolvedValue(boards);
  renderWithSession(
    <WorkspaceShell title="Documents">
      <p>page content</p>
    </WorkspaceShell>,
    ada,
  );
  return screen.findByRole("button", { name: "Search or jump to" });
}

describe("CommandPalette", () => {
  it("opens from the search field and lists documents, boards and pages in groups", async () => {
    const field = await openShell();
    fireEvent.click(field);
    const list = await screen.findByRole("listbox", { name: "Results" });
    await waitFor(() => expect(within(list).getAllByRole("option")).toHaveLength(7));
    expect(within(list).getByRole("option", { name: /Product Requirements/ })).toBeInTheDocument();
    expect(within(list).getByRole("option", { name: "Board: Sprint 12" })).toBeInTheDocument();
    expect(within(list).getByRole("option", { name: /Audit Trail/ })).toHaveTextContent("Admin");
    expect(screen.getByText("DOCUMENTS")).toBeInTheDocument();
    expect(screen.getByText("WORKSPACE")).toBeInTheDocument();
  });

  it("opens with Ctrl+K and with Cmd+K, and closes with Esc", async () => {
    await openShell();
    fireEvent.keyDown(document, { key: "k", ctrlKey: true });
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    fireEvent.keyDown(document, { key: "K", metaKey: true });
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
  });

  it("filters while typing, ignoring case, and says so when nothing matches", async () => {
    await openShell();
    fireEvent.keyDown(document, { key: "k", ctrlKey: true });
    const input = await screen.findByRole("combobox");
    await waitFor(() => expect(screen.getAllByRole("option").length).toBeGreaterThan(4));
    type("SPRINT");
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual([
      "🗓️ Sprint 12 Planning",
      "Board: Sprint 12",
    ]);
    type("zzz");
    expect(screen.queryAllByRole("option")).toHaveLength(0);
    expect(screen.getByText(/No results for/)).toBeInTheDocument();
  });

  it("moves with the arrow keys and opens the chosen result with Enter", async () => {
    await openShell();
    fireEvent.keyDown(document, { key: "k", ctrlKey: true });
    const input = await screen.findByRole("combobox");
    await waitFor(() => expect(screen.getAllByRole("option")).toHaveLength(7));
    expect(screen.getAllByRole("option")[0]).toHaveAttribute("aria-selected", "true");
    press("ArrowDown");
    expect(screen.getAllByRole("option")[1]).toHaveAttribute("aria-selected", "true");
    press("ArrowUp");
    press("ArrowUp");
    expect(screen.getAllByRole("option")[6]).toHaveAttribute("aria-selected", "true");
    press("ArrowDown");
    press("ArrowDown");
    expect(input).toHaveAttribute("aria-activedescendant", expect.stringContaining("d-d2"));
    press("Enter");
    expect(nav.router.push).toHaveBeenCalledWith("/w/eng/docs/d2");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("opens a result by clicking it", async () => {
    await openShell();
    fireEvent.keyDown(document, { key: "k", ctrlKey: true });
    fireEvent.click(await screen.findByRole("option", { name: "Board: Sprint 12" }));
    expect(nav.router.push).toHaveBeenCalledWith("/w/eng/boards/b1");
  });

  it("links the workspace pages, and shows no Audit Trail to a non-admin", async () => {
    await openShell("EDITOR");
    fireEvent.keyDown(document, { key: "k", ctrlKey: true });
    const input = await screen.findByRole("combobox");
    expect(screen.queryByRole("option", { name: /Audit Trail/ })).not.toBeInTheDocument();
    type("members");
    press("Enter");
    expect(nav.router.push).toHaveBeenCalledWith("/w/eng/settings/members");
  });

  it("still offers the pages when documents and boards cannot be loaded", async () => {
    vi.spyOn(api.workspaces, "list").mockResolvedValue([workspaceSummary()]);
    vi.spyOn(api.documents, "list").mockRejectedValue(new Error("down"));
    vi.spyOn(api.boards, "list").mockResolvedValue([]);
    renderWithSession(
      <WorkspaceShell title="Documents">
        <p>page content</p>
      </WorkspaceShell>,
      ada,
    );
    fireEvent.keyDown(await screen.findByText("page content"), { key: "k", ctrlKey: true });
    expect(await screen.findByRole("alert")).toHaveTextContent("could not be loaded");
    expect(screen.getByRole("option", { name: /All documents/ })).toBeInTheDocument();
  });

  it("loads the lists again each time it opens, so a new document shows up", async () => {
    await openShell();
    fireEvent.keyDown(document, { key: "k", ctrlKey: true });
    await waitFor(() => expect(screen.getAllByRole("option")).toHaveLength(7));
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    vi.mocked(api.documents.list).mockResolvedValue([
      ...docs,
      {
        id: "d3",
        title: "Brand new doc",
        icon: "📄",
        revision: 0,
        updated_at: docs[0]!.updated_at,
      },
    ]);
    fireEvent.keyDown(document, { key: "k", ctrlKey: true });
    expect(await screen.findByRole("option", { name: /Brand new doc/ })).toBeInTheDocument();
  });
});

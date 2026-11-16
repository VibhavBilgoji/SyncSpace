import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError, api } from "@/lib/api";
import { WorkspaceShell } from "@/components/shell/WorkspaceShell";
import { ada, renderWithSession, workspaceSummary } from "@/test/render";
import type { Document, DocumentSummary } from "@/types/generated";
import { DocumentsPage } from "./DocumentsPage";

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

beforeEach(() => {
  nav.router.push.mockClear();
  vi.restoreAllMocks();
});

const summary = (overrides: Partial<DocumentSummary> = {}): DocumentSummary => ({
  id: "d".repeat(24),
  title: "Sprint notes",
  icon: "📝",
  revision: 7,
  updated_at: "2026-11-17T10:00:00.000Z",
  ...overrides,
});

const created = (overrides: Partial<Document> = {}): Document => ({
  id: "e".repeat(24),
  workspace_id: "b".repeat(24),
  title: "Fresh",
  icon: "📄",
  revision: 0,
  blocks: [],
  created_by: ada.id,
  created_at: "2026-11-17T10:00:00.000Z",
  updated_at: "2026-11-17T10:00:00.000Z",
  ...overrides,
});

function open(role: "ADMIN" | "EDITOR" | "VIEWER", documents: () => Promise<DocumentSummary[]>) {
  vi.spyOn(api.workspaces, "list").mockResolvedValue([workspaceSummary({ role })]);
  const list = vi.spyOn(api.documents, "list").mockImplementation(documents);
  renderWithSession(
    <WorkspaceShell title="Documents">
      <DocumentsPage />
    </WorkspaceShell>,
    ada,
  );
  return list;
}

describe("DocumentsPage", () => {
  it("lists the documents with icon, title, revision and a link to the editor", async () => {
    const list = open("EDITOR", () =>
      Promise.resolve([summary(), summary({ id: "f".repeat(24), title: "Plan" })]),
    );
    const link = await screen.findByRole("link", { name: /Sprint notes/ });
    expect(link).toHaveAttribute("href", `/w/eng/docs/${"d".repeat(24)}`);
    expect(link).toHaveTextContent("📝");
    expect(link).toHaveTextContent("rev_7");
    expect(screen.getByRole("link", { name: /Plan/ })).toBeInTheDocument();
    expect(list).toHaveBeenCalledWith("b".repeat(24));
  });

  it("shows a skeleton while loading", async () => {
    open("EDITOR", () => new Promise(() => {}));
    expect(await screen.findByLabelText("Loading documents")).toBeInTheDocument();
  });

  it("shows the empty state with a create action for an editor", async () => {
    open("EDITOR", () => Promise.resolve([]));
    expect(await screen.findByText("No documents yet")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create document" })).toBeInTheDocument();
  });

  it("hides the create action from a viewer, in the empty state and in the list", async () => {
    open("VIEWER", () => Promise.resolve([]));
    expect(await screen.findByText("No documents yet")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Create document" })).not.toBeInTheDocument();
  });

  it("hides the create action from a viewer when there are documents", async () => {
    open("VIEWER", () => Promise.resolve([summary()]));
    await screen.findByRole("link", { name: /Sprint notes/ });
    expect(screen.queryByRole("button", { name: "Create document" })).not.toBeInTheDocument();
  });

  it("shows an error with a working retry", async () => {
    const list = open("ADMIN", () => Promise.reject(new ApiError(0, "NETWORK", "offline")));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load the documents");
    list.mockImplementation(() => Promise.resolve([summary()]));
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("link", { name: /Sprint notes/ })).toBeInTheDocument();
  });

  it("creates a document from the dialog and opens it", async () => {
    open("EDITOR", () => Promise.resolve([]));
    const create = vi.spyOn(api.documents, "create").mockResolvedValue(created());
    fireEvent.click(await screen.findByRole("button", { name: "Create document" }));
    const dialog = await screen.findByRole("dialog");
    const submit = within(dialog).getByRole("button", { name: "Create document" });
    expect(submit).toBeDisabled();
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "  Fresh  " } });
    fireEvent.click(submit);
    await waitFor(() => expect(create).toHaveBeenCalledWith("b".repeat(24), { title: "Fresh" }));
    await waitFor(() =>
      expect(nav.router.push).toHaveBeenCalledWith(`/w/eng/docs/${"e".repeat(24)}`),
    );
  });

  it("sends the icon when one is typed", async () => {
    open("ADMIN", () => Promise.resolve([]));
    const create = vi.spyOn(api.documents, "create").mockResolvedValue(created());
    fireEvent.click(await screen.findByRole("button", { name: "Create document" }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "Fresh" } });
    fireEvent.change(within(dialog).getByLabelText("Icon (optional)"), { target: { value: "🚀" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Create document" }));
    await waitFor(() =>
      expect(create).toHaveBeenCalledWith("b".repeat(24), { title: "Fresh", icon: "🚀" }),
    );
  });

  it("shows the server's field errors and keeps the dialog open", async () => {
    open("ADMIN", () => Promise.resolve([]));
    vi.spyOn(api.documents, "create").mockRejectedValue(
      new ApiError(422, "VALIDATION", "validation failed", {
        title: "title must be 1 to 120 characters",
      }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Create document" }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "x" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Create document" }));
    expect(
      await within(dialog).findByText("title must be 1 to 120 characters"),
    ).toBeInTheDocument();
    expect(nav.router.push).not.toHaveBeenCalled();
  });

  it("explains a network failure and closes with Cancel", async () => {
    open("ADMIN", () => Promise.resolve([]));
    vi.spyOn(api.documents, "create").mockRejectedValue(new ApiError(0, "NETWORK", "offline"));
    fireEvent.click(await screen.findByRole("button", { name: "Create document" }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "Fresh" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Create document" }));
    expect(await within(dialog).findByText(/Could not create the document/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });
});

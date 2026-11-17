import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui/Toast";
import type { DocumentSession } from "@/lib/doc/useDocumentSession";
import type { VersionDetail } from "@/types/generated";
import { DocumentPage } from "./DocumentPage";

const nav = vi.hoisted(() => ({
  replace: vi.fn(),
  search: new URLSearchParams(),
  role: "EDITOR" as "ADMIN" | "EDITOR" | "VIEWER",
}));
const api = vi.hoisted(() => ({
  documents: { get: vi.fn() },
  versions: { list: vi.fn(), get: vi.fn(), create: vi.fn(), restore: vi.fn() },
}));
const live = vi.hoisted(() => ({ status: "live" as string }));

vi.mock("next/navigation", () => ({
  useParams: () => ({ docId: "doc1", slug: "eng" }),
  usePathname: () => "/w/eng/docs/doc1",
  useRouter: () => ({ replace: nav.replace }),
  useSearchParams: () => nav.search,
}));
vi.mock("@/components/shell/WorkspaceShell", () => ({
  useWorkspace: () => ({ role: nav.role, slug: "eng" }),
}));
vi.mock("@/lib/api", () => ({ api }));
vi.mock("@/lib/session", () => ({
  useSession: () => ({ session: { status: "authed", user: { id: "me", name: "Ada" } } }),
}));
vi.mock("@/lib/doc/useDocumentSession", () => ({
  useDocumentSession: (): DocumentSession => ({
    status: live.status as DocumentSession["status"],
    blocks: [
      {
        block_id: "live00000001",
        type: "PARAGRAPH",
        content: "Live text",
        properties: {},
        order: 1,
      },
    ],
    activeSockets: 2,
    peers: [],
    saved: null,
    versionEvent: null,
    notice: null,
    insertText: vi.fn(),
    deleteText: vi.fn(),
    createBlockAfter: vi.fn(),
    deleteBlock: vi.fn(),
    setType: vi.fn(),
    setProps: vi.fn(),
  }),
}));

const manual = {
  version_number: 4,
  change_type: "MANUAL" as const,
  label: "Before the demo",
  restored_from: null,
  restored_from_kept: null,
  created_by: { id: "me", name: "Ada" },
  created_at: "2026-11-17T10:15:00Z",
};
const detail: VersionDetail = {
  version: manual,
  blocks: [
    {
      block_id: "arch00000001",
      type: "PARAGRAPH",
      content: "Archived text",
      properties: {},
      order: 1,
    },
  ],
};

function renderPage() {
  return render(
    <ToastProvider>
      <DocumentPage />
    </ToastProvider>,
  );
}

beforeEach(() => {
  nav.replace.mockReset();
  nav.search = new URLSearchParams();
  nav.role = "EDITOR";
  live.status = "live";
  api.documents.get.mockResolvedValue({ id: "doc1", title: "Notes", icon: "📝" });
  api.versions.list.mockResolvedValue({ items: [manual], next_cursor: null });
  api.versions.get.mockReset();
  api.versions.get.mockResolvedValue(detail);
});

describe("DocumentPage version history", () => {
  it("has the history closed by default and opens it through the URL", async () => {
    renderPage();
    expect(await screen.findByDisplayValue("Live text")).toBeInTheDocument();
    expect(screen.queryByLabelText("Version History")).not.toBeInTheDocument();
    const toggle = screen.getByRole("button", { name: "Version history" });
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(toggle);
    expect(nav.replace).toHaveBeenCalledWith("/w/eng/docs/doc1?history=1");
  });

  it("shows the panel next to the editor when the URL says history=1, and closes it through the URL", async () => {
    nav.search = new URLSearchParams("history=1");
    renderPage();
    expect(await screen.findByLabelText("Version History")).toBeInTheDocument();
    expect(await screen.findByText("Before the demo")).toBeInTheDocument();
    expect(screen.getByDisplayValue("Live text")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Version history" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(nav.replace).toHaveBeenCalledWith("/w/eng/docs/doc1");
  });

  it("previews a version read-only in the editor and goes back to the live blocks", async () => {
    nav.search = new URLSearchParams("history=1");
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Preview" }));
    expect(await screen.findByDisplayValue("Archived text")).toBeInTheDocument();
    expect(api.versions.get).toHaveBeenCalledWith("doc1", 4);
    expect(screen.queryByDisplayValue("Live text")).not.toBeInTheDocument();
    expect(screen.getByText("Viewing Version 4 · Read-only archive.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Previewing" })).toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: "Back to live document" }));
    expect(await screen.findByDisplayValue("Live text")).toBeInTheDocument();
    expect(screen.queryByText(/Read-only archive/)).not.toBeInTheDocument();
  });

  it("says so and stays on the live document when a version cannot be opened", async () => {
    nav.search = new URLSearchParams("history=1");
    api.versions.get.mockRejectedValue(new Error("gone"));
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Preview" }));
    expect(await screen.findByText("Could not open Version 4")).toBeInTheDocument();
    expect(screen.getByDisplayValue("Live text")).toBeInTheDocument();
  });

  it("leaves the preview when the history is closed", async () => {
    nav.search = new URLSearchParams("history=1");
    const view = renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Preview" }));
    await screen.findByDisplayValue("Archived text");
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(nav.replace).toHaveBeenCalledWith("/w/eng/docs/doc1");
    // The URL change closes the panel: render again without ?history=1.
    nav.search = new URLSearchParams();
    view.rerender(
      <ToastProvider>
        <DocumentPage />
      </ToastProvider>,
    );
    await waitFor(() => expect(screen.getByDisplayValue("Live text")).toBeInTheDocument());
    expect(screen.queryByText(/Read-only archive/)).not.toBeInTheDocument();
  });

  it("gives a viewer the panel with Preview only", async () => {
    nav.search = new URLSearchParams("history=1");
    nav.role = "VIEWER";
    renderPage();
    await screen.findByText("Before the demo");
    expect(screen.getByRole("button", { name: "Preview" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Restore" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Create Named Version" })).not.toBeInTheDocument();
  });

  it("offers no history button and no panel for a deleted document", async () => {
    nav.search = new URLSearchParams("history=1");
    live.status = "gone";
    renderPage();
    expect(await screen.findByText("This document was deleted.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Version history" })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Version History")).not.toBeInTheDocument();
  });

  it("says when the document does not exist", async () => {
    api.documents.get.mockRejectedValue(new Error("404"));
    renderPage();
    expect(await screen.findByText("Document not found.")).toBeInTheDocument();
  });
});

import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui/Toast";
import { HeaderSlot, HeaderSlotProvider } from "@/lib/headerSlot";
import type { DocumentSession } from "@/lib/doc/useDocumentSession";
import type { VersionDetail } from "@/types/generated";
import { DocumentPage } from "./DocumentPage";

const nav = vi.hoisted(() => ({
  replace: vi.fn(),
  search: new URLSearchParams(),
  role: "EDITOR" as "ADMIN" | "EDITOR" | "VIEWER",
  members: 3,
}));
const api = vi.hoisted(() => ({
  documents: { get: vi.fn() },
  versions: { list: vi.fn(), get: vi.fn(), create: vi.fn(), restore: vi.fn() },
}));
const live = vi.hoisted(() => ({ status: "live" as string, peers: [] as string[] }));

vi.mock("next/navigation", () => ({
  useParams: () => ({ docId: "doc1", slug: "eng" }),
  usePathname: () => "/w/eng/docs/doc1",
  useRouter: () => ({ replace: nav.replace }),
  useSearchParams: () => nav.search,
}));
vi.mock("@/components/shell/WorkspaceShell", () => ({
  useWorkspace: () => ({
    role: nav.role,
    slug: "eng",
    name: "Engineering",
    member_count: nav.members,
  }),
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
    peers: live.peers,
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

const tree = (
  <ToastProvider>
    <HeaderSlotProvider>
      <HeaderSlot />
      <DocumentPage />
    </HeaderSlotProvider>
  </ToastProvider>
);

function renderPage() {
  return render(tree);
}

beforeEach(() => {
  nav.replace.mockReset();
  nav.search = new URLSearchParams();
  nav.role = "EDITOR";
  nav.members = 3;
  live.status = "live";
  live.peers = [];
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
    const toggle = screen.getByRole("button", { name: "Version History" });
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
    expect(screen.getByRole("button", { name: "Version History" })).toHaveAttribute(
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
    view.rerender(tree);
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
    expect(screen.queryByRole("button", { name: "Version History" })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Version History")).not.toBeInTheDocument();
  });

  it("says when the document does not exist", async () => {
    api.documents.get.mockRejectedValue(new Error("404"));
    renderPage();
    expect(await screen.findByText("Document not found.")).toBeInTheDocument();
  });
});

describe("DocumentPage header (UI_SPEC section 3)", () => {
  it("puts Version History and a primary Share in the header slot, not in the page body", async () => {
    const view = renderPage();
    await screen.findByDisplayValue("Live text");
    const slot = view.container.querySelector("[data-header-slot]") as HTMLElement;
    expect(within(slot).getByRole("button", { name: "Version History" })).toBeInTheDocument();
    expect(within(slot).getByRole("button", { name: "Share" })).toBeInTheDocument();
    const buttons = within(slot)
      .getAllByRole("button")
      .map((b) => b.textContent);
    expect(buttons).toEqual(["Version History", "Share"]); // Version History, then Share on the right
  });

  it("shows the people here as avatars before the buttons: three, then +N", async () => {
    live.peers = [
      "Grace Hopper",
      "Alan Turing",
      "Edsger Dijkstra",
      "Barbara Liskov",
      "Ken Thompson",
    ];
    const view = renderPage();
    await screen.findByDisplayValue("Live text");
    const slot = view.container.querySelector("[data-header-slot]") as HTMLElement;
    const people = within(slot).getByLabelText("People here now");
    expect(within(people).getAllByRole("img")).toHaveLength(4); // 3 avatars and "+2"
    expect(within(people).getByText("+2")).toBeInTheDocument();
    const order = Array.from(slot.children).map(
      (c) => c.getAttribute("aria-label") ?? c.textContent,
    );
    expect(order).toEqual(["People here now", "Version History", "Share"]);
  });

  it("shows no avatar strip when nobody else is here", async () => {
    renderPage();
    await screen.findByDisplayValue("Live text");
    expect(screen.queryByLabelText("People here now")).not.toBeInTheDocument();
  });

  it("opens the Share dialog with the document link and the member count", async () => {
    renderPage();
    await screen.findByDisplayValue("Live text");
    fireEvent.click(screen.getByRole("button", { name: "Share" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Share Notes")).toBeInTheDocument();
    expect(
      within(dialog).getByText("Only workspace members can open this document."),
    ).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Workspace link")).toHaveValue(
      `${window.location.origin}/w/eng/docs/doc1`,
    );
    expect(within(dialog).getByText("3 members in Engineering.")).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        "Admins and editors can edit. Viewers can read and preview versions.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("copies the link and says so, or says it could not", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    renderPage();
    await screen.findByDisplayValue("Live text");
    fireEvent.click(screen.getByRole("button", { name: "Share" }));
    fireEvent.click(await screen.findByRole("button", { name: "Copy link" }));
    expect(writeText).toHaveBeenCalledWith(`${window.location.origin}/w/eng/docs/doc1`);
    expect(await screen.findByText("Link copied")).toBeInTheDocument();

    writeText.mockRejectedValueOnce(new Error("denied"));
    fireEvent.click(screen.getByRole("button", { name: "Copy link" }));
    expect(await screen.findByText("Could not copy")).toBeInTheDocument();
  });

  it("says member in the singular for a workspace of one", async () => {
    nav.members = 1;
    renderPage();
    await screen.findByDisplayValue("Live text");
    fireEvent.click(screen.getByRole("button", { name: "Share" }));
    expect(await screen.findByText("1 member in Engineering.")).toBeInTheDocument();
  });

  it("has no header buttons for a deleted document", async () => {
    live.status = "gone";
    renderPage();
    await screen.findByText("This document was deleted.");
    expect(screen.queryByRole("button", { name: "Share" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Version History" })).not.toBeInTheDocument();
  });
});

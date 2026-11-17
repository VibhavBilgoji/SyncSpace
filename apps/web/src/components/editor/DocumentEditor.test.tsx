import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DocumentSession } from "@/lib/doc/useDocumentSession";
import type { Block } from "@/types/generated";
import { DocumentEditor } from "./DocumentEditor";
import { filterSlashItems } from "./SlashMenu";

const block = (id: string, overrides: Partial<Block> = {}): Block => ({
  block_id: id,
  type: "PARAGRAPH",
  content: "",
  properties: {},
  order: 1,
  ...overrides,
});

function fakeSession(overrides: Partial<DocumentSession> = {}): DocumentSession {
  return {
    status: "live",
    blocks: [block("blk000000001", { content: "Hello" })],
    activeSockets: 2,
    peers: [],
    saved: { revision: 4, savedAt: "2026-11-17T10:00:00Z" },
    notice: null,
    versionEvent: null,
    insertText: vi.fn(),
    deleteText: vi.fn(),
    createBlockAfter: vi.fn(() => "blk000000002"),
    deleteBlock: vi.fn(),
    setType: vi.fn(),
    setProps: vi.fn(),
    ...overrides,
  };
}

function renderEditor(session: DocumentSession, role: "ADMIN" | "EDITOR" | "VIEWER" = "EDITOR") {
  return render(<DocumentEditor title="Notes" icon="📝" role={role} session={session} />);
}

describe("DocumentEditor", () => {
  it("shows the title and the metadata line", () => {
    renderEditor(fakeSession());
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Notes");
    expect(
      screen.getByText("Revision: rev_4 · Saved to MongoDB · Active Sockets: 2", { exact: false }),
    ).toBeInTheDocument();
  });

  it("says Waiting to sync and shows the reconnect banner while offline", () => {
    renderEditor(fakeSession({ status: "reconnecting" }));
    expect(screen.getByText(/Waiting to sync/)).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Reconnecting...");
  });

  it("renders every block type with its own look", () => {
    renderEditor(
      fakeSession({
        blocks: [
          block("blk000000001", { type: "HEADING_1", content: "Title", order: 1 }),
          block("blk000000002", {
            type: "CODE",
            content: "fn main() {}",
            order: 2,
            properties: { language: "rust" },
          }),
          block("blk000000003", {
            type: "CHECKLIST",
            content: "Ship it",
            order: 3,
            properties: { checked: true },
          }),
          block("blk000000004", { type: "CALLOUT", content: "Careful", order: 4 }),
        ],
      }),
    );
    expect(screen.getByLabelText("heading 1 block")).toHaveValue("Title");
    expect(screen.getByText("rust")).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Done" })).toBeChecked();
    expect(screen.getByLabelText("callout block")).toHaveValue("Careful");
  });

  it("turns typing into insert and delete operations at the right index", () => {
    const session = fakeSession();
    renderEditor(session);
    const box = screen.getByLabelText("paragraph block");
    fireEvent.change(box, { target: { value: "Hello world" } });
    expect(session.insertText).toHaveBeenCalledWith("blk000000001", 5, " world");
    fireEvent.change(box, { target: { value: "Held world" } });
    expect(session.deleteText).toHaveBeenCalledWith("blk000000001", 3, 2);
  });

  it("opens the slash menu on / and filters, then changes the type with Enter", () => {
    const session = fakeSession({ blocks: [block("blk000000001")] });
    renderEditor(session);
    const box = screen.getByLabelText("paragraph block");
    fireEvent.change(box, { target: { value: "/" } });
    expect(screen.getByRole("listbox", { name: "Block types" })).toBeInTheDocument();
    expect(screen.getAllByRole("option")).toHaveLength(6);
    fireEvent.change(box, { target: { value: "/head" } });
    const options = screen.getAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual([
      "Heading 1Large section title",
      "Heading 2Medium section title",
    ]);
    fireEvent.keyDown(box, { key: "ArrowDown" });
    fireEvent.keyDown(box, { key: "Enter" });
    expect(session.setType).toHaveBeenCalledWith("blk000000001", "HEADING_2");
  });

  it("closes the slash menu with Escape and leaves the text alone", () => {
    const session = fakeSession({ blocks: [block("blk000000001")] });
    renderEditor(session);
    const box = screen.getByLabelText("paragraph block");
    fireEvent.change(box, { target: { value: "/" } });
    fireEvent.keyDown(box, { key: "Escape" });
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(session.setType).not.toHaveBeenCalled();
  });

  it("picks an item with the mouse", () => {
    const session = fakeSession({ blocks: [block("blk000000001")] });
    renderEditor(session);
    fireEvent.change(screen.getByLabelText("paragraph block"), { target: { value: "/" } });
    fireEvent.mouseDown(within(screen.getByRole("listbox")).getByText("Callout"));
    expect(session.setType).toHaveBeenCalledWith("blk000000001", "CALLOUT");
  });

  it("splits the block at the caret on Enter", () => {
    const session = fakeSession();
    renderEditor(session);
    const box = screen.getByLabelText("paragraph block") as HTMLTextAreaElement;
    box.setSelectionRange(2, 2);
    fireEvent.keyDown(box, { key: "Enter" });
    expect(session.deleteText).toHaveBeenCalledWith("blk000000001", 2, 3);
    expect(session.createBlockAfter).toHaveBeenCalledWith("blk000000001", "PARAGRAPH", "llo");
  });

  it("deletes an empty block with Backspace but never the first one", () => {
    const session = fakeSession({
      blocks: [block("blk000000001", { content: "a" }), block("blk000000002", { order: 2 })],
    });
    renderEditor(session);
    const boxes = screen.getAllByLabelText("paragraph block");
    fireEvent.keyDown(boxes[1] as HTMLElement, { key: "Backspace" });
    expect(session.deleteBlock).toHaveBeenCalledWith("blk000000002");
    fireEvent.keyDown(boxes[0] as HTMLElement, { key: "Backspace" });
    expect(session.deleteBlock).toHaveBeenCalledTimes(1);
  });

  it("toggles a checklist item through the block properties", () => {
    const session = fakeSession({
      blocks: [
        block("blk000000001", { type: "CHECKLIST", content: "Do", properties: { checked: false } }),
      ],
    });
    renderEditor(session);
    fireEvent.click(screen.getByRole("checkbox", { name: "Done" }));
    expect(session.setProps).toHaveBeenCalledWith("blk000000001", { checked: true });
  });

  it("gives a viewer a read-only page: banner, no insert buttons, no operations", () => {
    const session = fakeSession();
    renderEditor(session, "VIEWER");
    expect(screen.getByText(/VIEWER · Read-only access/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Insert block below" })).not.toBeInTheDocument();
    const box = screen.getByLabelText("paragraph block");
    expect(box).toHaveAttribute("readonly");
    expect(box).not.toHaveAttribute("placeholder");
    fireEvent.change(box, { target: { value: "x" } });
    fireEvent.keyDown(box, { key: "Enter" });
    expect(session.insertText).not.toHaveBeenCalled();
    expect(session.createBlockAfter).not.toHaveBeenCalled();
  });

  it("shows an editor who lost write access what happened", () => {
    renderEditor(fakeSession({ notice: "READ_ONLY" }), "EDITOR");
    expect(screen.getByText("Your role no longer allows editing.")).toBeInTheDocument();
  });

  it("explains a deleted document and lost access instead of an empty page", () => {
    const { rerender } = renderEditor(fakeSession({ status: "gone" }));
    expect(screen.getByRole("alert")).toHaveTextContent("This document was deleted.");
    rerender(
      <DocumentEditor
        title="Notes"
        icon="📝"
        role="EDITOR"
        session={fakeSession({ status: "forbidden" })}
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("no longer have access");
  });

  it("shows a skeleton until the first sync", () => {
    renderEditor(fakeSession({ status: "connecting", blocks: [] }));
    expect(screen.getByLabelText("Loading document")).toBeInTheDocument();
  });
});

describe("filterSlashItems", () => {
  it("matches case-insensitively on the label and returns everything for an empty query", () => {
    expect(filterSlashItems("")).toHaveLength(6);
    expect(filterSlashItems("CODE").map((i) => i.type)).toEqual(["CODE"]);
    expect(filterSlashItems("xyz")).toEqual([]);
  });
});

describe("DocumentEditor preview (frame 20)", () => {
  const archived = {
    version: {
      version_number: 4,
      change_type: "MANUAL" as const,
      label: "Before the demo",
      restored_from: null,
      restored_from_kept: null,
      created_by: { id: "u1", name: "Ada Lovelace" },
      created_at: "2026-11-17T10:15:00Z",
    },
    blocks: [block("arch00000001", { content: "Old text" })],
  };

  function renderPreview(role: "ADMIN" | "EDITOR" | "VIEWER", onLeave = vi.fn()) {
    const session = fakeSession({
      blocks: [block("blk000000001", { content: "Live text" })],
    });
    render(
      <DocumentEditor
        title="Notes"
        icon="📝"
        role={role}
        session={session}
        preview={archived}
        onLeavePreview={onLeave}
      />,
    );
    return { session, onLeave };
  }

  it("shows the archived blocks instead of the live ones, with the warning and the metadata", () => {
    renderPreview("EDITOR");
    expect(screen.getByText("Viewing Version 4 · Read-only archive.")).toBeInTheDocument();
    expect(screen.getByText("Live editing is suspended.")).toBeInTheDocument();
    expect(screen.getByDisplayValue("Old text")).toBeInTheDocument();
    expect(screen.queryByDisplayValue("Live text")).not.toBeInTheDocument();
    expect(screen.getByText(/^Version 4 · MANUAL · /)).toBeInTheDocument();
    expect(screen.queryByText(/Active Sockets/)).not.toBeInTheDocument();
  });

  it("makes the archive read-only for an editor too: no insert buttons, no operations", () => {
    const { session } = renderPreview("EDITOR");
    const field = screen.getByDisplayValue("Old text");
    expect(field).toHaveAttribute("readonly");
    fireEvent.change(field, { target: { value: "Changed" } });
    expect(session.insertText).not.toHaveBeenCalled();
    expect(session.deleteText).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: /Add block|Insert/i })).not.toBeInTheDocument();
  });

  it("tells a viewer who previews both things: the archive and the read-only role", () => {
    renderPreview("VIEWER");
    expect(screen.getAllByText(/Read-only/).length).toBe(2); // the archive banner and the viewer one
    expect(screen.getByText("Viewing Version 4 · Read-only archive.")).toBeInTheDocument();
  });

  it("goes back to the live document from the banner", () => {
    const { onLeave } = renderPreview("EDITOR");
    fireEvent.click(screen.getByRole("button", { name: "Back to live document" }));
    expect(onLeave).toHaveBeenCalled();
  });

  it("shows the live blocks and the revision line when there is no preview", () => {
    renderEditor(fakeSession());
    expect(screen.queryByText(/Read-only archive/)).not.toBeInTheDocument();
    expect(screen.getByDisplayValue("Hello")).toBeInTheDocument();
  });
});

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

  it("shows who else is here and hides the strip when nobody is", () => {
    const { rerender } = renderEditor(fakeSession());
    expect(screen.queryByLabelText("People here now")).not.toBeInTheDocument();
    rerender(
      <DocumentEditor
        title="Notes"
        icon="📝"
        role="EDITOR"
        session={fakeSession({ peers: ["Grace Hopper", "Alan Turing"] })}
      />,
    );
    const strip = screen.getByLabelText("People here now");
    expect(within(strip).getByRole("img", { name: "Grace Hopper" })).toBeInTheDocument();
    expect(within(strip).getByRole("img", { name: "Alan Turing" })).toBeInTheDocument();
    expect(strip).toHaveTextContent("2 here");
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

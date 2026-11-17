import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui/Toast";
import { ApiError } from "@/lib/api/errors";
import type { VersionEvent } from "@/lib/doc/useDocumentSession";
import type { VersionSummary } from "@/types/generated";
import { VersionHistoryPanel } from "./VersionHistoryPanel";

const api = vi.hoisted(() => ({
  versions: { list: vi.fn(), create: vi.fn(), get: vi.fn(), restore: vi.fn() },
}));
vi.mock("@/lib/api", () => ({ api }));
vi.mock("@/lib/session", () => ({
  useSession: () => ({
    session: { status: "authed", user: { id: "me", name: "Ada Lovelace", email: "ada@x.dev" } },
  }),
}));

const ada = { id: "me", name: "Ada Lovelace" };
const grace = { id: "u2", name: "Grace Hopper" };

function version(n: number, overrides: Partial<VersionSummary> = {}): VersionSummary {
  return {
    version_number: n,
    change_type: "MANUAL",
    label: `Label ${n}`,
    restored_from: null,
    restored_from_kept: null,
    created_by: ada,
    created_at: "2026-11-17T10:15:00Z",
    ...overrides,
  };
}

const history = [
  version(4, { label: "Before the demo" }),
  version(3, {
    change_type: "RESTORE",
    label: "Restored from Version 1",
    restored_from: 1,
    restored_from_kept: true,
    created_by: grace,
  }),
  version(2, { change_type: "AUTOMATIC", label: "Periodic Snapshot", created_by: null }),
];

type Role = "ADMIN" | "EDITOR" | "VIEWER";

function Providers({ children }: { children: ReactNode }) {
  return <ToastProvider>{children}</ToastProvider>;
}

function renderPanel(
  props: Partial<{
    role: Role;
    previewing: number | null;
    event: VersionEvent | null;
    collaborators: number;
  }> = {},
) {
  const handlers = { onPreview: vi.fn(), onLeavePreview: vi.fn(), onClose: vi.fn() };
  const view = render(
    <VersionHistoryPanel
      docId="doc1"
      role={props.role ?? "EDITOR"}
      previewing={props.previewing ?? null}
      event={props.event ?? null}
      collaborators={props.collaborators ?? 1}
      {...handlers}
    />,
    { wrapper: Providers },
  );
  const again = (next: Partial<{ event: VersionEvent | null }>) =>
    view.rerender(
      <VersionHistoryPanel
        docId="doc1"
        role={props.role ?? "EDITOR"}
        previewing={props.previewing ?? null}
        event={next.event ?? null}
        collaborators={props.collaborators ?? 1}
        {...handlers}
      />,
    );
  return { ...handlers, again };
}

beforeEach(() => {
  for (const fn of Object.values(api.versions)) fn.mockReset();
  api.versions.list.mockResolvedValue({ items: history, next_cursor: null });
});

describe("the list (frame 18)", () => {
  it("shows a skeleton, then the versions newest first with badge, label, time and author", async () => {
    renderPanel();
    expect(screen.getByLabelText("Loading versions")).toBeInTheDocument();
    const cards = await screen.findAllByRole("listitem");
    expect(cards.map((c) => c.getAttribute("aria-label"))).toEqual([
      "Version 4",
      "Version 3",
      "Version 2",
    ]);
    expect(within(cards[0]!).getByText("v4 · MANUAL")).toBeInTheDocument();
    expect(within(cards[0]!).getByText("Before the demo")).toBeInTheDocument();
    expect(within(cards[0]!).getByText(/Ada Lovelace/)).toBeInTheDocument();
    expect(within(cards[1]!).getByText("v3 · RESTORE")).toBeInTheDocument();
    expect(within(cards[1]!).getByText("Restored from Version 1")).toBeInTheDocument();
    expect(within(cards[2]!).getByText("v2 · AUTOMATIC")).toBeInTheDocument();
    expect(within(cards[2]!).getByText(/Server Snapshot/)).toBeInTheDocument();
    expect(
      screen.getByText(
        "Newest 50 automatic versions retained. Manual and restore versions are kept.",
      ),
    ).toBeInTheDocument();
    expect(api.versions.list).toHaveBeenCalledWith("doc1", { limit: 50 });
  });

  it.each<Role>(["EDITOR", "ADMIN"])("%s gets create, preview and restore", async (role) => {
    renderPanel({ role });
    const cards = await screen.findAllByRole("listitem");
    expect(screen.getByRole("button", { name: "Create Named Version" })).toBeInTheDocument();
    for (const card of cards) {
      expect(within(card).getByRole("button", { name: "Preview" })).toBeInTheDocument();
      expect(within(card).getByRole("button", { name: "Restore" })).toBeInTheDocument();
    }
  });

  it("gives a viewer Preview only, with no create or restore (frame 23)", async () => {
    renderPanel({ role: "VIEWER" });
    const cards = await screen.findAllByRole("listitem");
    expect(screen.queryByRole("button", { name: "Create Named Version" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Restore" })).not.toBeInTheDocument();
    for (const card of cards) {
      expect(within(card).getByRole("button", { name: "Preview" })).toBeInTheDocument();
    }
  });

  it("says when the restore source is no longer kept (frame 24b)", async () => {
    api.versions.list.mockResolvedValue({
      items: [
        version(5, {
          change_type: "RESTORE",
          label: "Restored from Version 1",
          restored_from: 1,
          restored_from_kept: false,
        }),
      ],
      next_cursor: null,
    });
    renderPanel();
    expect(await screen.findByText("Restored from Version 1 · no longer kept")).toBeInTheDocument();
  });

  it("marks the previewed card and disables its Preview button", async () => {
    renderPanel({ previewing: 3 });
    const cards = await screen.findAllByRole("listitem");
    expect(within(cards[1]!).getByRole("button", { name: "Previewing" })).toBeDisabled();
    expect(cards[1]).toHaveAttribute("aria-current", "true");
    expect(within(cards[1]!).getByRole("button", { name: "Restore" })).toBeEnabled();
    expect(within(cards[0]!).getByRole("button", { name: "Preview" })).toBeEnabled();
  });

  it("asks the page to preview a version", async () => {
    const { onPreview } = renderPanel();
    const cards = await screen.findAllByRole("listitem");
    fireEvent.click(within(cards[2]!).getByRole("button", { name: "Preview" }));
    expect(onPreview).toHaveBeenCalledWith(2);
  });

  it("closes from its own button", async () => {
    const { onClose } = renderPanel();
    await screen.findAllByRole("listitem");
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalled();
  });

  it("offers a retry when the list cannot be loaded", async () => {
    api.versions.list.mockRejectedValueOnce(new Error("offline"));
    renderPanel();
    expect(await screen.findByText("Could not load the version history.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findAllByRole("listitem")).toHaveLength(3);
    expect(api.versions.list).toHaveBeenCalledTimes(2);
  });

  it("loads older versions with the cursor and keeps the list sorted", async () => {
    api.versions.list.mockResolvedValueOnce({ items: history, next_cursor: "2" });
    api.versions.list.mockResolvedValueOnce({ items: [version(1)], next_cursor: null });
    renderPanel();
    await screen.findAllByRole("listitem");
    fireEvent.click(screen.getByRole("button", { name: "Load older versions" }));
    await waitFor(() => expect(screen.getAllByRole("listitem")).toHaveLength(4));
    expect(api.versions.list).toHaveBeenLastCalledWith("doc1", { limit: 50, cursor: "2" });
    expect(screen.getAllByRole("listitem").at(-1)).toHaveAttribute("aria-label", "Version 1");
    expect(screen.queryByRole("button", { name: "Load older versions" })).not.toBeInTheDocument();
  });
});

describe("the empty history (frame 24a)", () => {
  beforeEach(() => api.versions.list.mockResolvedValue({ items: [], next_cursor: null }));

  it("invites an editor to create the first version", async () => {
    renderPanel();
    expect(await screen.findByText("No versions yet")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create Named Version" })).toBeInTheDocument();
  });

  it("offers a viewer no action", async () => {
    renderPanel({ role: "VIEWER" });
    expect(await screen.findByText("No versions yet")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Create Named Version" })).not.toBeInTheDocument();
  });
});

describe("Create Named Version (frame 19)", () => {
  async function openDialog() {
    renderPanel();
    await screen.findAllByRole("listitem");
    fireEvent.click(screen.getByRole("button", { name: "Create Named Version" }));
    return screen.getByRole("dialog");
  }

  it("refuses an empty or blank label without calling the server", async () => {
    const dialog = await openDialog();
    fireEvent.click(within(dialog).getByRole("button", { name: "Save version" }));
    expect(await within(dialog).findByText("Enter a label for this version.")).toBeInTheDocument();
    fireEvent.change(within(dialog).getByLabelText("Label"), { target: { value: "   " } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save version" }));
    expect(api.versions.create).not.toHaveBeenCalled();
  });

  it("refuses a label over 80 characters", async () => {
    const dialog = await openDialog();
    fireEvent.change(within(dialog).getByLabelText("Label"), { target: { value: "x".repeat(81) } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save version" }));
    expect(await within(dialog).findByText("Use at most 80 characters.")).toBeInTheDocument();
    expect(api.versions.create).not.toHaveBeenCalled();
  });

  it("saves the trimmed label, shows the new card first and confirms with a toast", async () => {
    api.versions.create.mockResolvedValue(version(5, { label: "Draft one" }));
    const dialog = await openDialog();
    fireEvent.change(within(dialog).getByLabelText("Label"), {
      target: { value: "  Draft one  " },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save version" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(api.versions.create).toHaveBeenCalledWith("doc1", { label: "Draft one" });
    const cards = screen.getAllByRole("listitem");
    expect(cards[0]).toHaveAttribute("aria-label", "Version 5");
    expect(screen.getByText("Saved Version 5")).toBeInTheDocument();
  });

  it("shows the server's message for the label when it answers 422", async () => {
    api.versions.create.mockRejectedValue(
      new ApiError(422, "VALIDATION", "validation failed", {
        label: "label must be 1 to 80 characters",
      }),
    );
    const dialog = await openDialog();
    fireEvent.change(within(dialog).getByLabelText("Label"), { target: { value: "ok" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save version" }));
    expect(await within(dialog).findByText("label must be 1 to 80 characters")).toBeInTheDocument();
    expect(screen.getAllByRole("listitem")).toHaveLength(3);
  });

  it("keeps the dialog open and says so when the save fails for another reason", async () => {
    api.versions.create.mockRejectedValue(new ApiError(500, "INTERNAL", "internal error"));
    const dialog = await openDialog();
    fireEvent.change(within(dialog).getByLabelText("Label"), { target: { value: "ok" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save version" }));
    expect(await screen.findByText("Could not save the version")).toBeInTheDocument();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("starts empty every time it is opened", async () => {
    const dialog = await openDialog();
    fireEvent.change(within(dialog).getByLabelText("Label"), { target: { value: "left over" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Create Named Version" }));
    expect(within(screen.getByRole("dialog")).getByLabelText("Label")).toHaveValue("");
  });
});

describe("Restore (frames 21 and 22)", () => {
  async function openRestore(collaborators = 1) {
    const handlers = renderPanel({ collaborators, previewing: 4 });
    const cards = await screen.findAllByRole("listitem");
    fireEvent.click(within(cards[0]!).getByRole("button", { name: "Restore" }));
    return { ...handlers, dialog: screen.getByRole("dialog") };
  }

  it("explains what will happen and that a copy is saved first", async () => {
    const { dialog } = await openRestore();
    expect(within(dialog).getByText("Restore Version 4?")).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        /A copy of the current content is saved first, so you can undo the restore\./,
      ),
    ).toBeInTheDocument();
    expect(
      within(dialog).queryByText(/have this document open|has this document open/),
    ).not.toBeInTheDocument();
  });

  it("warns how many other people have the document open", async () => {
    const { dialog } = await openRestore(3);
    expect(within(dialog).getByText("2 other people have this document open.")).toBeInTheDocument();
  });

  it("uses the singular for one other person", async () => {
    const { dialog } = await openRestore(2);
    expect(within(dialog).getByText("1 other person has this document open.")).toBeInTheDocument();
  });

  it("restores, shows the new RESTORE card first, leaves the preview and toasts", async () => {
    api.versions.restore.mockResolvedValue({
      restore_version: version(5, {
        change_type: "RESTORE",
        label: "Restored from Version 4",
        restored_from: 4,
        restored_from_kept: true,
      }),
      revision: 9,
    });
    const { dialog, onLeavePreview } = await openRestore();
    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm & Restore" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(api.versions.restore).toHaveBeenCalledWith("doc1", 4);
    expect(screen.getAllByRole("listitem")[0]).toHaveAttribute("aria-label", "Version 5");
    expect(onLeavePreview).toHaveBeenCalled();
    expect(screen.getByText("Restored Version 4")).toBeInTheDocument();
  });

  it("changes nothing when cancelled", async () => {
    const { dialog, onLeavePreview } = await openRestore();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(api.versions.restore).not.toHaveBeenCalled();
    expect(onLeavePreview).not.toHaveBeenCalled();
  });

  it("says the restore failed, keeps the dialog and leaves the list and the preview alone", async () => {
    api.versions.restore.mockRejectedValue(new ApiError(500, "INTERNAL", "internal error"));
    const { dialog, onLeavePreview } = await openRestore();
    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm & Restore" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Could not restore this version. Nothing was changed.",
    );
    expect(screen.getAllByRole("listitem")).toHaveLength(3);
    expect(onLeavePreview).not.toHaveBeenCalled();
  });
});

describe("version frames from the socket", () => {
  const created: VersionEvent = {
    seq: 1,
    kind: "created",
    version: version(5, { label: "From Grace", created_by: grace }),
  };
  const restoredByGrace: VersionEvent = {
    seq: 2,
    kind: "restored",
    by: grace,
    restoredFrom: 2,
    version: version(6, {
      change_type: "RESTORE",
      label: "Restored from Version 2",
      restored_from: 2,
      restored_from_kept: true,
      created_by: grace,
    }),
  };

  it("adds a version someone else saved", async () => {
    const { again } = renderPanel();
    await screen.findAllByRole("listitem");
    again({ event: created });
    await waitFor(() => expect(screen.getAllByRole("listitem")).toHaveLength(4));
    expect(screen.getAllByRole("listitem")[0]).toHaveAttribute("aria-label", "Version 5");
  });

  it("does not list the same version twice when its frame and its REST answer both arrive", async () => {
    api.versions.create.mockResolvedValue(version(5, { label: "From Grace" }));
    const { again } = renderPanel();
    await screen.findAllByRole("listitem");
    again({ event: created });
    await waitFor(() => expect(screen.getAllByRole("listitem")).toHaveLength(4));
    again({ event: { ...created, seq: 3 } });
    await waitFor(() => expect(screen.getAllByRole("listitem")).toHaveLength(4));
  });

  it("adds a restore by someone else and tells you about it", async () => {
    const { again } = renderPanel();
    await screen.findAllByRole("listitem");
    again({ event: restoredByGrace });
    expect(await screen.findByText("Version 2 restored")).toBeInTheDocument();
    expect(
      screen.getByText("Grace Hopper restored it for everyone in this document."),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("listitem")[0]).toHaveAttribute("aria-label", "Version 6");
  });

  it("adds your own restore without a second toast (the restore answer already toasted)", async () => {
    const { again } = renderPanel();
    await screen.findAllByRole("listitem");
    again({ event: { ...restoredByGrace, by: ada } });
    await waitFor(() => expect(screen.getAllByRole("listitem")).toHaveLength(4));
    expect(screen.queryByText("Version 2 restored")).not.toBeInTheDocument();
  });

  it("ignores a frame that was already there when the panel opened (no stale toast)", async () => {
    renderPanel({ event: restoredByGrace });
    await screen.findAllByRole("listitem");
    expect(screen.getAllByRole("listitem")).toHaveLength(3);
    expect(screen.queryByText("Version 2 restored")).not.toBeInTheDocument();
  });
});

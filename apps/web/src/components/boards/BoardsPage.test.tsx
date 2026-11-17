import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "@/lib/api/errors";
import type { Board } from "@/types/generated";
import { BoardsPage } from "./BoardsPage";

const ctx = vi.hoisted(() => ({
  role: "EDITOR" as "ADMIN" | "EDITOR" | "VIEWER",
  push: vi.fn(),
}));
const api = vi.hoisted(() => ({ boards: { list: vi.fn(), create: vi.fn() } }));

const socket = vi.hoisted(() => ({ onFrame: null as null | ((frame: unknown) => void) }));
vi.mock("@/lib/workspaceSocket", () => ({
  useWorkspaceSocket: (_id: string, handlers: { onFrame(frame: unknown): void }) => {
    socket.onFrame = handlers.onFrame;
    return "live";
  },
}));
vi.mock("next/navigation", () => ({ useRouter: () => ({ push: ctx.push }) }));
vi.mock("@/components/shell/WorkspaceShell", () => ({
  useWorkspace: () => ({ id: "ws1", slug: "eng", name: "Engineering", role: ctx.role }),
}));
vi.mock("@/lib/api", async () => {
  const errors = await import("@/lib/api/errors");
  return { api, isApiError: errors.isApiError };
});

function board(id: string, name: string, counts: Partial<Board["task_counts"]> = {}): Board {
  return {
    id,
    workspace_id: "ws1",
    name,
    task_counts: { backlog: 0, todo: 0, in_progress: 0, review: 0, done: 0, ...counts },
    created_at: "2026-11-10T00:00:00Z",
  };
}

beforeEach(() => {
  ctx.role = "EDITOR";
  ctx.push.mockReset();
  api.boards.list.mockReset();
  api.boards.create.mockReset();
  api.boards.list.mockResolvedValue([
    board("b1", "Sprint 12", { todo: 2, done: 1 }),
    board("b2", "Roadmap", { backlog: 1 }),
    board("b3", "Empty one"),
  ]);
});

describe("BoardsPage", () => {
  it("shows a skeleton, then the boards as links with their task counts", async () => {
    render(<BoardsPage />);
    expect(screen.getByLabelText("Loading boards")).toBeInTheDocument();
    const sprint = await screen.findByRole("link", { name: /Sprint 12/ });
    expect(sprint).toHaveAttribute("href", "/w/eng/boards/b1");
    expect(sprint).toHaveTextContent("3 tasks");
    expect(screen.getByRole("link", { name: /Roadmap/ })).toHaveTextContent("1 task");
    expect(screen.getByRole("link", { name: /Empty one/ })).toHaveTextContent("0 tasks");
    expect(api.boards.list).toHaveBeenCalledWith("ws1");
  });

  it("gives an editor the create button, with and without boards", async () => {
    render(<BoardsPage />);
    expect(await screen.findByRole("button", { name: "Create board" })).toBeInTheDocument();
  });

  it("says there are no boards yet and offers to create one (frame 27b)", async () => {
    api.boards.list.mockResolvedValue([]);
    render(<BoardsPage />);
    expect(await screen.findByText("No boards yet")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create board" })).toBeInTheDocument();
  });

  it("gives a viewer no way to create: not with boards, not without", async () => {
    ctx.role = "VIEWER";
    const view = render(<BoardsPage />);
    await screen.findByRole("link", { name: /Sprint 12/ });
    expect(screen.queryByRole("button", { name: "Create board" })).not.toBeInTheDocument();
    view.unmount();

    api.boards.list.mockResolvedValue([]);
    render(<BoardsPage />);
    expect(await screen.findByText("No boards yet")).toBeInTheDocument();
    expect(screen.getByText(/Boards created by editors will appear here/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Create board" })).not.toBeInTheDocument();
  });

  it("offers a retry when the boards cannot be loaded", async () => {
    api.boards.list.mockRejectedValueOnce(new Error("offline"));
    render(<BoardsPage />);
    expect(await screen.findByText("Could not load the boards.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("link", { name: /Sprint 12/ })).toBeInTheDocument();
  });
});

describe("Create board (frame 27a)", () => {
  async function open() {
    render(<BoardsPage />);
    fireEvent.click(await screen.findByRole("button", { name: "Create board" }));
    return screen.getByRole("dialog");
  }

  it("needs a name before it can be submitted", async () => {
    const dialog = await open();
    const submit = within(dialog).getByRole("button", { name: "Create board" });
    expect(submit).toBeDisabled();
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "   " } });
    expect(submit).toBeDisabled();
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "Launch" } });
    expect(submit).toBeEnabled();
  });

  it("creates the board with the trimmed name and opens it", async () => {
    api.boards.create.mockResolvedValue(board("new1", "Launch"));
    const dialog = await open();
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "  Launch  " } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Create board" }));
    await waitFor(() => expect(ctx.push).toHaveBeenCalledWith("/w/eng/boards/new1"));
    expect(api.boards.create).toHaveBeenCalledWith("ws1", { name: "Launch" });
  });

  it("shows the server's message for the name and keeps the dialog", async () => {
    api.boards.create.mockRejectedValue(
      new ApiError(422, "VALIDATION", "validation failed", {
        name: "name must be 1 to 60 characters",
      }),
    );
    const dialog = await open();
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "x" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Create board" }));
    expect(await within(dialog).findByText("name must be 1 to 60 characters")).toBeInTheDocument();
    expect(ctx.push).not.toHaveBeenCalled();
  });

  it("explains a refused role and an unreachable server", async () => {
    api.boards.create.mockRejectedValueOnce(new ApiError(403, "FORBIDDEN", "forbidden"));
    const dialog = await open();
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "x" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Create board" }));
    expect(await within(dialog).findByText(/does not allow creating boards/)).toBeInTheDocument();

    api.boards.create.mockRejectedValueOnce(
      new ApiError(0, "NETWORK", "Could not reach the server"),
    );
    fireEvent.click(within(dialog).getByRole("button", { name: "Create board" }));
    expect(await within(dialog).findByText(/Could not create the board/)).toBeInTheDocument();
  });

  it("cancels without creating anything", async () => {
    const dialog = await open();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(api.boards.create).not.toHaveBeenCalled();
  });
});

describe("live changes", () => {
  it("shows a board someone else created, and refreshed task counts", async () => {
    render(<BoardsPage />);
    await screen.findByRole("link", { name: /Sprint 12/ });
    api.boards.list.mockResolvedValue([
      board("b1", "Sprint 12", { todo: 5 }),
      board("b4", "Brand new board"),
    ]);
    act(() => socket.onFrame?.({ type: "BOARD_EVENT", board_id: "b4", kind: "BOARD_CREATED" }));
    expect(await screen.findByRole("link", { name: /Brand new board/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Sprint 12/ })).toHaveTextContent("5 tasks");
  });

  it("ignores frames that are not board events", async () => {
    render(<BoardsPage />);
    await screen.findByRole("link", { name: /Sprint 12/ });
    act(() => socket.onFrame?.({ type: "PRESENCE_COUNT", active_sockets: 3 }));
    expect(api.boards.list).toHaveBeenCalledTimes(1);
  });
});

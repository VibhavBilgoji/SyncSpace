import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui/Toast";
import { ApiError } from "@/lib/api/errors";
import type { Member, Task } from "@/types/generated";
import { BoardPage } from "./BoardPage";

const ctx = vi.hoisted(() => ({ role: "EDITOR" as "ADMIN" | "EDITOR" | "VIEWER" }));
const api = vi.hoisted(() => ({
  boards: {
    list: vi.fn(),
    tasks: vi.fn(),
    createTask: vi.fn(),
    updateTask: vi.fn(),
    moveTask: vi.fn(),
    removeTask: vi.fn(),
  },
  workspaces: { members: vi.fn() },
}));

const socket = vi.hoisted(() => ({
  handlers: null as null | {
    onFrame(frame: unknown): void;
    onConnected?(again: boolean): void;
  },
}));
vi.mock("@/lib/workspaceSocket", () => ({
  useWorkspaceSocket: (_id: string, handlers: NonNullable<typeof socket.handlers>) => {
    socket.handlers = handlers;
    return "live";
  },
}));
vi.mock("next/navigation", () => ({ useParams: () => ({ boardId: "board1", slug: "eng" }) }));
vi.mock("@/components/shell/WorkspaceShell", () => ({
  useWorkspace: () => ({ id: "ws1", slug: "eng", name: "Engineering", role: ctx.role }),
}));
vi.mock("@/lib/api", async () => {
  const errors = await import("@/lib/api/errors");
  return { api, isApiError: errors.isApiError };
});

const members: Member[] = [
  {
    user_id: "u1",
    name: "Ada Lovelace",
    email: "ada@x.dev",
    role: "ADMIN",
    joined_at: "2026-11-01T00:00:00Z",
    is_owner: true,
  },
  {
    user_id: "u2",
    name: "Grace Hopper",
    email: "grace@x.dev",
    role: "EDITOR",
    joined_at: "2026-11-02T00:00:00Z",
    is_owner: false,
  },
];

function task(id: string, overrides: Partial<Task> = {}): Task {
  return {
    id,
    board_id: "board1",
    title: `Task ${id}`,
    description: "",
    status: "TODO",
    priority: "MEDIUM",
    assignee_ids: [],
    tags: [],
    order: 1,
    created_at: "2026-11-17T10:00:00Z",
    updated_at: "2026-11-17T10:00:00Z",
    ...overrides,
  };
}

const board = {
  id: "board1",
  workspace_id: "ws1",
  name: "Sprint 12",
  task_counts: { backlog: 0, todo: 0, in_progress: 0, review: 0, done: 0 },
  created_at: "2026-11-10T00:00:00Z",
};

const tasks: Task[] = [
  task("a", { title: "Write the plan", status: "BACKLOG", priority: "LOW", order: 1 }),
  task("b", {
    title: "Build the API",
    status: "TODO",
    priority: "URGENT",
    order: 1,
    tags: ["backend", "api"],
    assignee_ids: ["u1", "u2"],
  }),
  task("c", { title: "Review the API", status: "TODO", priority: "HIGH", order: 2 }),
  task("d", { title: "Ship it", status: "DONE", order: 1 }),
];

function renderBoard() {
  return render(
    <ToastProvider>
      <BoardPage />
    </ToastProvider>,
  );
}

const column = (label: string) => screen.getByRole("region", { name: label });
const cardsIn = (label: string) =>
  within(column(label))
    .queryAllByRole("button", { name: /^Open task / })
    .map((b) => b.getAttribute("aria-label")?.replace("Open task ", ""));

beforeEach(() => {
  socket.handlers = null;
  ctx.role = "EDITOR";
  for (const fn of Object.values(api.boards)) fn.mockReset();
  api.workspaces.members.mockReset();
  api.boards.list.mockResolvedValue([board]);
  api.boards.tasks.mockResolvedValue(tasks);
  api.workspaces.members.mockResolvedValue(members);
});

describe("the board (frame 25)", () => {
  it("shows a skeleton, then the five columns with their tasks in order and the counts", async () => {
    renderBoard();
    expect(screen.getByLabelText("Loading board")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Sprint 12" })).toBeInTheDocument();
    for (const label of ["Backlog", "To do", "In progress", "Review", "Done"]) {
      expect(column(label)).toBeInTheDocument();
    }
    expect(cardsIn("Backlog")).toEqual(["Write the plan"]);
    expect(cardsIn("To do")).toEqual(["Build the API", "Review the API"]);
    expect(cardsIn("Done")).toEqual(["Ship it"]);
    expect(within(column("To do")).getByLabelText("2 tasks")).toBeInTheDocument();
    expect(within(column("Review")).getByLabelText("0 tasks")).toBeInTheDocument();
    expect(api.boards.list).toHaveBeenCalledWith("ws1");
    expect(api.boards.tasks).toHaveBeenCalledWith("board1");
    expect(api.workspaces.members).toHaveBeenCalledWith("ws1");
  });

  it("shows tags, priority, the creation date and the assignees on a card", async () => {
    renderBoard();
    const card = (await screen.findByRole("button", { name: "Open task Build the API" })).closest(
      "li",
    ) as HTMLElement;
    expect(within(card).getByText("#backend")).toBeInTheDocument();
    expect(within(card).getByText("#api")).toBeInTheDocument();
    expect(within(card).getByText("Urgent")).toBeInTheDocument();
    expect(within(card).getByRole("img", { name: "Ada Lovelace" })).toBeInTheDocument();
    expect(within(card).getByRole("img", { name: "Grace Hopper" })).toBeInTheDocument();
    expect(card.querySelector("time")).toHaveAttribute("datetime", "2026-11-17T10:00:00Z");
  });

  it("says an empty column is empty and offers to add a task", async () => {
    renderBoard();
    await screen.findByRole("heading", { name: "Sprint 12" });
    const review = column("Review");
    expect(within(review).getByText("No tasks")).toBeInTheDocument();
    expect(within(review).getByRole("button", { name: "+ Add task" })).toBeInTheDocument();
    expect(within(review).getByRole("button", { name: "Add task to Review" })).toBeInTheDocument();
  });

  it("says the board does not exist, and offers a retry when loading fails", async () => {
    api.boards.list.mockResolvedValue([{ ...board, id: "other" }]);
    const { unmount } = renderBoard();
    expect(await screen.findByText("Board not found.")).toBeInTheDocument();
    unmount();

    api.boards.list.mockRejectedValueOnce(new Error("offline"));
    api.boards.list.mockResolvedValue([board]);
    renderBoard();
    expect(await screen.findByText("Could not load the board.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("heading", { name: "Sprint 12" })).toBeInTheDocument();
  });

  it("reloads the tasks when the window gets focus, to pick up other people's changes", async () => {
    renderBoard();
    await screen.findByRole("heading", { name: "Sprint 12" });
    expect(api.boards.tasks).toHaveBeenCalledTimes(1);
    api.boards.tasks.mockResolvedValue([
      ...tasks,
      task("e", { title: "Added by someone", status: "REVIEW" }),
    ]);
    fireEvent.focus(window);
    expect(
      await screen.findByRole("button", { name: "Open task Added by someone" }),
    ).toBeInTheDocument();
    expect(api.boards.tasks).toHaveBeenCalledTimes(2);
  });
});

describe("moving a task with the arrow buttons (no drag and drop)", () => {
  it("moves a card one column right and shows it at the end of that column", async () => {
    api.boards.moveTask.mockResolvedValue({ ...tasks[1], status: "IN_PROGRESS", order: 1 });
    renderBoard();
    fireEvent.click(
      await screen.findByRole("button", { name: "Move Build the API to In progress" }),
    );
    await waitFor(() => expect(cardsIn("In progress")).toEqual(["Build the API"]));
    expect(api.boards.moveTask).toHaveBeenCalledWith("b", { to_status: "IN_PROGRESS" });
    expect(cardsIn("To do")).toEqual(["Review the API"]);
    expect(within(column("To do")).getByLabelText("1 tasks")).toBeInTheDocument();
  });

  it("moves a card one column left", async () => {
    api.boards.moveTask.mockResolvedValue({ ...tasks[3], status: "REVIEW", order: 1 });
    renderBoard();
    fireEvent.click(await screen.findByRole("button", { name: "Move Ship it to Review" }));
    await waitFor(() => expect(cardsIn("Review")).toEqual(["Ship it"]));
    expect(api.boards.moveTask).toHaveBeenCalledWith("d", { to_status: "REVIEW" });
    expect(cardsIn("Done")).toEqual([]);
  });

  it("puts the move to the previous column on the left (←) and to the next on the right (→)", async () => {
    renderBoard();
    const card = (await screen.findByRole("button", { name: "Open task Build the API" })).closest(
      "li",
    ) as HTMLElement;
    const moves = within(card)
      .getAllByRole("button")
      .filter((b) => b.getAttribute("aria-label")?.startsWith("Move "));
    expect(moves.map((b) => b.textContent)).toEqual(["←", "→"]);
    expect(moves.map((b) => b.getAttribute("aria-label"))).toEqual([
      "Move Build the API to Backlog",
      "Move Build the API to In progress",
    ]);
  });

  it("has no button past the edges: nothing left of Backlog, nothing right of Done", async () => {
    renderBoard();
    await screen.findByRole("heading", { name: "Sprint 12" });
    const backlog = within(column("Backlog")).getByRole("listitem");
    expect(
      within(backlog).queryByRole("button", { name: /Move .* to Backlog/ }),
    ).not.toBeInTheDocument();
    expect(
      within(backlog).getByRole("button", { name: "Move Write the plan to To do" }),
    ).toBeInTheDocument();
    const done = within(column("Done")).getByRole("listitem");
    expect(
      within(done).getByRole("button", { name: "Move Ship it to Review" }),
    ).toBeInTheDocument();
    expect(
      within(done).queryByRole("button", { name: /Move .* to (Done|Archive)/ }),
    ).not.toBeInTheDocument();
  });

  it("says so, and reloads, when the move fails", async () => {
    api.boards.moveTask.mockRejectedValue(new ApiError(500, "INTERNAL", "internal error"));
    renderBoard();
    fireEvent.click(
      await screen.findByRole("button", { name: "Move Build the API to In progress" }),
    );
    expect(await screen.findByText("Could not move the task")).toBeInTheDocument();
    expect(cardsIn("To do")).toEqual(["Build the API", "Review the API"]);
    await waitFor(() => expect(api.boards.tasks).toHaveBeenCalledTimes(2));
  });
});

describe("a viewer (frame 31)", () => {
  beforeEach(() => {
    ctx.role = "VIEWER";
  });

  it("sees the board without New Task, add buttons or move buttons", async () => {
    renderBoard();
    await screen.findByRole("heading", { name: "Sprint 12" });
    expect(screen.queryByRole("button", { name: "+ New Task" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Add task/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Move / })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "+ Add task" })).not.toBeInTheDocument();
    expect(cardsIn("To do")).toEqual(["Build the API", "Review the API"]);
  });

  it("opens read-only details with no Save and no Delete", async () => {
    renderBoard();
    fireEvent.click(await screen.findByRole("button", { name: "Open task Build the API" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Task details")).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Title")).toHaveAttribute("readonly");
    expect(within(dialog).getByLabelText("Status")).toBeDisabled();
    expect(within(dialog).getByLabelText("Priority")).toBeDisabled();
    expect(within(dialog).getByLabelText("Ada Lovelace")).toBeDisabled();
    expect(within(dialog).queryByRole("button", { name: "Save" })).not.toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: "Delete" })).not.toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});

describe("creating a task (frame 28)", () => {
  it("opens from New Task in Backlog and from a column's + in that column", async () => {
    renderBoard();
    fireEvent.click(await screen.findByRole("button", { name: "+ New Task" }));
    expect(within(screen.getByRole("dialog")).getByLabelText("Status")).toHaveValue("BACKLOG");
    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Cancel" }));
    fireEvent.click(screen.getByRole("button", { name: "Add task to Review" }));
    expect(within(screen.getByRole("dialog")).getByLabelText("Status")).toHaveValue("REVIEW");
  });

  it("needs a title, then creates with every field and puts the card in its column", async () => {
    api.boards.createTask.mockResolvedValue(
      task("n", {
        title: "New one",
        status: "REVIEW",
        priority: "HIGH",
        tags: ["ui", "polish"],
        assignee_ids: ["u2"],
      }),
    );
    renderBoard();
    fireEvent.click(await screen.findByRole("button", { name: "Add task to Review" }));
    const dialog = screen.getByRole("dialog");
    const create = within(dialog).getByRole("button", { name: "Create task" });
    expect(create).toBeDisabled();
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "  New one " } });
    fireEvent.change(within(dialog).getByLabelText("Description"), {
      target: { value: "Some words" },
    });
    fireEvent.change(within(dialog).getByLabelText("Priority"), { target: { value: "HIGH" } });
    fireEvent.click(within(dialog).getByLabelText("Grace Hopper"));
    fireEvent.change(within(dialog).getByLabelText("Tags"), {
      target: { value: " ui, polish ,, " },
    });
    expect(create).toBeEnabled();
    fireEvent.click(create);
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(api.boards.createTask).toHaveBeenCalledWith("board1", {
      title: "New one",
      description: "Some words",
      status: "REVIEW",
      priority: "HIGH",
      assignee_ids: ["u2"],
      tags: ["ui", "polish"],
    });
    expect(cardsIn("Review")).toEqual(["New one"]);
  });

  it("shows the server's message under the field it belongs to and keeps the dialog", async () => {
    api.boards.createTask.mockRejectedValue(
      new ApiError(422, "VALIDATION", "validation failed", {
        title: "must be 1 to 200 characters",
        tags: "must be at most 10 tags",
      }),
    );
    renderBoard();
    fireEvent.click(await screen.findByRole("button", { name: "+ New Task" }));
    const dialog = screen.getByRole("dialog");
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "x" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Create task" }));
    expect(await within(dialog).findByText("must be 1 to 200 characters")).toBeInTheDocument();
    expect(within(dialog).getByText("must be at most 10 tags")).toBeInTheDocument();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("explains a failure to reach the server and a refused role", async () => {
    api.boards.createTask.mockRejectedValueOnce(
      new ApiError(0, "NETWORK", "Could not reach the server"),
    );
    renderBoard();
    fireEvent.click(await screen.findByRole("button", { name: "+ New Task" }));
    const dialog = screen.getByRole("dialog");
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "x" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Create task" }));
    expect(await within(dialog).findByText(/Could not save the task/)).toBeInTheDocument();

    api.boards.createTask.mockRejectedValueOnce(new ApiError(403, "FORBIDDEN", "forbidden"));
    fireEvent.click(within(dialog).getByRole("button", { name: "Create task" }));
    expect(await within(dialog).findByText(/does not allow changing tasks/)).toBeInTheDocument();
  });
});

describe("editing a task (frame 29)", () => {
  async function openB() {
    renderBoard();
    fireEvent.click(await screen.findByRole("button", { name: "Open task Build the API" }));
    return screen.getByRole("dialog");
  }

  it("shows the task's fields", async () => {
    const dialog = await openB();
    expect(within(dialog).getByText("Edit task")).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Title")).toHaveValue("Build the API");
    expect(within(dialog).getByLabelText("Status")).toHaveValue("TODO");
    expect(within(dialog).getByLabelText("Priority")).toHaveValue("URGENT");
    expect(within(dialog).getByLabelText("Tags")).toHaveValue("backend, api");
    expect(within(dialog).getByLabelText("Ada Lovelace")).toBeChecked();
    expect(within(dialog).getByLabelText("Grace Hopper")).toBeChecked();
  });

  it("saves only through the update call when the column is unchanged", async () => {
    api.boards.updateTask.mockResolvedValue({
      ...tasks[1],
      title: "Build the API v2",
      priority: "LOW",
    });
    const dialog = await openB();
    fireEvent.change(within(dialog).getByLabelText("Title"), {
      target: { value: "Build the API v2" },
    });
    fireEvent.change(within(dialog).getByLabelText("Priority"), { target: { value: "LOW" } });
    fireEvent.click(within(dialog).getByLabelText("Grace Hopper"));
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(api.boards.updateTask).toHaveBeenCalledWith("b", {
      title: "Build the API v2",
      description: "",
      priority: "LOW",
      assignee_ids: ["u1"],
      tags: ["backend", "api"],
    });
    expect(api.boards.moveTask).not.toHaveBeenCalled();
    expect(
      await screen.findByRole("button", { name: "Open task Build the API v2" }),
    ).toBeInTheDocument();
  });

  it("moves the task when the status dropdown changed, after saving the fields", async () => {
    api.boards.updateTask.mockResolvedValue(tasks[1]);
    api.boards.moveTask.mockResolvedValue({ ...tasks[1], status: "REVIEW", order: 1 });
    const dialog = await openB();
    fireEvent.change(within(dialog).getByLabelText("Status"), { target: { value: "REVIEW" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(api.boards.updateTask).toHaveBeenCalledTimes(1);
    expect(api.boards.moveTask).toHaveBeenCalledWith("b", { to_status: "REVIEW" });
    expect(cardsIn("Review")).toEqual(["Build the API"]);
    expect(cardsIn("To do")).toEqual(["Review the API"]);
  });

  it("does not move when saving fails, and says the task is gone on a 404", async () => {
    api.boards.updateTask.mockRejectedValue(new ApiError(404, "NOT_FOUND", "task not found"));
    const dialog = await openB();
    fireEvent.change(within(dialog).getByLabelText("Status"), { target: { value: "DONE" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(await within(dialog).findByText(/no longer exists/)).toBeInTheDocument();
    expect(api.boards.moveTask).not.toHaveBeenCalled();
  });
});

describe("deleting a task (frame 32)", () => {
  async function openDelete() {
    renderBoard();
    fireEvent.click(await screen.findByRole("button", { name: "Open task Ship it" }));
    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Delete" }));
    return screen.getByRole("dialog");
  }

  it("asks first, and cancelling changes nothing", async () => {
    const dialog = await openDelete();
    expect(within(dialog).getByText("Delete task?")).toBeInTheDocument();
    expect(within(dialog).getByText(/"Ship it" will be deleted for everyone/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(api.boards.removeTask).not.toHaveBeenCalled();
    expect(within(screen.getByRole("dialog")).getByText("Edit task")).toBeInTheDocument();
  });

  it("deletes after confirmation and removes the card", async () => {
    api.boards.removeTask.mockResolvedValue(undefined);
    const dialog = await openDelete();
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete task" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(api.boards.removeTask).toHaveBeenCalledWith("d");
    expect(cardsIn("Done")).toEqual([]);
    expect(within(column("Done")).getByText("No tasks")).toBeInTheDocument();
  });

  it("keeps the card and says so when the delete fails", async () => {
    api.boards.removeTask.mockRejectedValue(new ApiError(500, "INTERNAL", "internal error"));
    const dialog = await openDelete();
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete task" }));
    expect(await screen.findByText(/Could not delete the task/)).toBeInTheDocument();
    expect(cardsIn("Done")).toEqual(["Ship it"]);
  });
});

describe("live changes from the workspace socket", () => {
  const event = (kind: string, extra: Record<string, unknown> = {}) => ({
    type: "BOARD_EVENT",
    board_id: "board1",
    kind,
    ...extra,
  });
  const emit = (frame: unknown) => act(() => socket.handlers?.onFrame(frame));

  it("shows a task someone else created, in its column and at the right place", async () => {
    renderBoard();
    await screen.findByRole("heading", { name: "Sprint 12" });
    emit(
      event("TASK_CREATED", {
        task: task("n", { title: "From a colleague", status: "TODO", order: 1.5 }),
      }),
    );
    expect(cardsIn("To do")).toEqual(["Build the API", "From a colleague", "Review the API"]);
    expect(within(column("To do")).getByLabelText("3 tasks")).toBeInTheDocument();
  });

  it("applies an edit to a task that is on the board", async () => {
    renderBoard();
    await screen.findByRole("heading", { name: "Sprint 12" });
    emit(
      event("TASK_UPDATED", { task: { ...tasks[1], title: "Build the API v2", priority: "LOW" } }),
    );
    expect(cardsIn("To do")).toEqual(["Build the API v2", "Review the API"]);
    const card = screen
      .getByRole("button", { name: "Open task Build the API v2" })
      .closest("li") as HTMLElement;
    expect(within(card).getByText("Low")).toBeInTheDocument();
  });

  it("moves a card to the column and place another user moved it to, without a reload", async () => {
    renderBoard();
    await screen.findByRole("heading", { name: "Sprint 12" });
    emit(event("TASK_MOVED", { task: { ...tasks[1], status: "REVIEW", order: 1 } }));
    expect(cardsIn("To do")).toEqual(["Review the API"]);
    expect(cardsIn("Review")).toEqual(["Build the API"]);
    expect(api.boards.tasks).toHaveBeenCalledTimes(1);
  });

  it("removes a task someone else deleted", async () => {
    renderBoard();
    await screen.findByRole("heading", { name: "Sprint 12" });
    emit(event("TASK_DELETED", { task: tasks[3] }));
    expect(cardsIn("Done")).toEqual([]);
    expect(within(column("Done")).getByText("No tasks")).toBeInTheDocument();
  });

  it("ignores events of other boards, board creations and malformed events", async () => {
    renderBoard();
    await screen.findByRole("heading", { name: "Sprint 12" });
    emit({
      ...event("TASK_CREATED", { task: task("x", { title: "Elsewhere" }) }),
      board_id: "other",
    });
    emit(event("BOARD_CREATED", { board: { ...board, id: "new" } }));
    emit(event("TASK_CREATED"));
    emit(event("TASK_DELETED"));
    emit({ type: "PRESENCE_COUNT", active_sockets: 2 });
    expect(cardsIn("To do")).toEqual(["Build the API", "Review the API"]);
    expect(screen.queryByText("Elsewhere")).not.toBeInTheDocument();
  });

  it("loads the tasks again after a renumbered column, and whenever the socket connects", async () => {
    renderBoard();
    await screen.findByRole("heading", { name: "Sprint 12" });
    api.boards.tasks.mockResolvedValue([
      ...tasks,
      task("late", { title: "Missed event", status: "REVIEW" }),
    ]);
    emit(event("COLUMN_RENUMBERED", { status: "TODO" }));
    expect(
      await screen.findByRole("button", { name: "Open task Missed event" }),
    ).toBeInTheDocument();
    const calls = api.boards.tasks.mock.calls.length;
    act(() => socket.handlers?.onConnected?.(true));
    await waitFor(() => expect(api.boards.tasks.mock.calls.length).toBe(calls + 1));
  });

  it("applies the same event twice without duplicating the card (our own change comes back)", async () => {
    renderBoard();
    await screen.findByRole("heading", { name: "Sprint 12" });
    const created = task("n", { title: "Once", status: "BACKLOG", order: 2 });
    emit(event("TASK_CREATED", { task: created }));
    emit(event("TASK_CREATED", { task: created }));
    expect(cardsIn("Backlog")).toEqual(["Write the plan", "Once"]);
  });
});

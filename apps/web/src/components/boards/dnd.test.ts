import { describe, expect, it } from "vitest";
import type { Status, Task } from "@/types/generated";
import {
  announcements,
  applyPlan,
  columnId,
  columnTasks,
  moveBody,
  optimisticOrder,
  planMove,
  statusOfColumnId,
} from "./dnd";

function task(id: string, status: Status, order: number, title = `Task ${id}`): Task {
  return {
    id,
    board_id: "b",
    title,
    description: "",
    status,
    priority: "MEDIUM",
    assignee_ids: [],
    tags: [],
    order,
    created_at: "2026-11-18T10:00:00Z",
    updated_at: "2026-11-18T10:00:00Z",
  };
}

// TODO: a, b, c (in that order); REVIEW: x, y; DONE: empty; BACKLOG: z.
const tasks = [
  task("a", "TODO", 1),
  task("b", "TODO", 2),
  task("c", "TODO", 3),
  task("x", "REVIEW", 1),
  task("y", "REVIEW", 2),
  task("z", "BACKLOG", 1),
];

describe("column ids", () => {
  it("round-trip, and refuse anything that is not a column", () => {
    for (const status of ["BACKLOG", "TODO", "IN_PROGRESS", "REVIEW", "DONE"] as const) {
      expect(statusOfColumnId(columnId(status))).toBe(status);
    }
    expect(statusOfColumnId("a")).toBeNull();
    expect(statusOfColumnId("column:NOWHERE")).toBeNull();
    expect(statusOfColumnId("")).toBeNull();
  });

  it("lists a column by position, then id", () => {
    const tied = [task("q", "TODO", 1), task("p", "TODO", 1), task("r", "TODO", 0.5)];
    expect(columnTasks(tied, "TODO").map((t) => t.id)).toEqual(["r", "p", "q"]);
    expect(columnTasks(tied, "DONE")).toEqual([]);
  });
});

describe("planMove inside a column", () => {
  it("moves down: it takes the place of the task it is dropped on", () => {
    expect(planMove(tasks, "a", "c")).toEqual({
      status: "TODO",
      beforeId: "c",
      afterId: null,
      index: 2,
      size: 3,
    });
  });

  it("moves up", () => {
    expect(planMove(tasks, "c", "a")).toEqual({
      status: "TODO",
      beforeId: null,
      afterId: "a",
      index: 0,
      size: 3,
    });
  });

  it("swaps two neighbours, with both neighbours named", () => {
    expect(planMove(tasks, "a", "b")).toEqual({
      status: "TODO",
      beforeId: "b",
      afterId: "c",
      index: 1,
      size: 3,
    });
  });

  it("does nothing when dropped on itself, in its own place or on something unknown", () => {
    expect(planMove(tasks, "b", "b")).toBeNull();
    expect(planMove(tasks, "c", columnId("TODO"))).toBeNull(); // already last
    expect(planMove(tasks, "nope", "a")).toBeNull();
    expect(planMove(tasks, "a", "nope")).toBeNull();
  });
});

describe("planMove between columns", () => {
  it("lands where it is dropped: before the task it is over", () => {
    expect(planMove(tasks, "a", "y")).toEqual({
      status: "REVIEW",
      beforeId: "x",
      afterId: "y",
      index: 1,
      size: 3,
    });
    expect(planMove(tasks, "a", "x")).toEqual({
      status: "REVIEW",
      beforeId: null,
      afterId: "x",
      index: 0,
      size: 3,
    });
  });

  it("goes to the end when dropped on the column itself", () => {
    expect(planMove(tasks, "a", columnId("REVIEW"))).toEqual({
      status: "REVIEW",
      beforeId: "y",
      afterId: null,
      index: 2,
      size: 3,
    });
    expect(planMove(tasks, "b", columnId("TODO"))).toEqual({
      status: "TODO",
      beforeId: "c",
      afterId: null,
      index: 2,
      size: 3,
    });
  });

  it("fills an empty column", () => {
    expect(planMove(tasks, "a", columnId("DONE"))).toEqual({
      status: "DONE",
      beforeId: null,
      afterId: null,
      index: 0,
      size: 1,
    });
  });

  it("never names the moved task as its own neighbour", () => {
    for (const over of ["a", "b", "c", "x", "y", "z", columnId("TODO"), columnId("DONE")]) {
      for (const active of ["a", "b", "c", "x"]) {
        const plan = planMove(tasks, active, over);
        if (plan) {
          expect(plan.beforeId).not.toBe(active);
          expect(plan.afterId).not.toBe(active);
        }
      }
    }
  });
});

describe("what is sent to the server", () => {
  it("leaves out a neighbour that does not exist", () => {
    expect(moveBody({ status: "DONE", beforeId: null, afterId: null, index: 0, size: 1 })).toEqual({
      to_status: "DONE",
    });
    expect(moveBody({ status: "DONE", beforeId: "p", afterId: null, index: 1, size: 2 })).toEqual({
      to_status: "DONE",
      before_id: "p",
    });
    expect(moveBody({ status: "TODO", beforeId: "p", afterId: "q", index: 1, size: 3 })).toEqual({
      to_status: "TODO",
      before_id: "p",
      after_id: "q",
    });
  });
});

describe("the position shown before the server answers", () => {
  const plan = (beforeId: string | null, afterId: string | null) => ({
    status: "TODO" as const,
    beforeId,
    afterId,
    index: 0,
    size: 1,
  });

  it("follows the rule of API.md: midpoint, last plus one, half of the first, one", () => {
    expect(optimisticOrder(tasks, plan("a", "b"))).toBe(1.5);
    expect(optimisticOrder(tasks, plan("c", null))).toBe(4);
    expect(optimisticOrder(tasks, plan(null, "a"))).toBe(0.5);
    expect(optimisticOrder(tasks, plan(null, null))).toBe(1);
  });

  it("changes only the moved task, and puts it where the plan says", () => {
    const planned = planMove(tasks, "a", "y")!;
    const moved = applyPlan(tasks, "a", planned);
    expect(moved.find((t) => t.id === "a")).toMatchObject({ status: "REVIEW", order: 1.5 });
    expect(moved.filter((t) => t.id !== "a")).toEqual(tasks.filter((t) => t.id !== "a"));
    expect(columnTasks(moved, "REVIEW").map((t) => t.id)).toEqual(["x", "a", "y"]);
    expect(columnTasks(moved, "TODO").map((t) => t.id)).toEqual(["b", "c"]);
  });
});

describe("what a screen reader says while dragging", () => {
  const say = announcements([
    task("a", "TODO", 1, "Write docs"),
    task("b", "TODO", 2, "Build API"),
    task("x", "REVIEW", 1, "Review it"),
  ]);
  const ref = (id: string) => ({ id }) as never;

  it("says where the task was picked up", () => {
    expect(say.onDragStart?.({ active: ref("a") } as never)).toBe(
      "Picked up Write docs from To do.",
    );
  });

  it("says which column and place it is over, and where it was dropped", () => {
    expect(say.onDragOver?.({ active: ref("a"), over: ref("x") } as never)).toBe(
      "Write docs is over Review, place 1 of 2.",
    );
    expect(say.onDragEnd?.({ active: ref("a"), over: ref("b") } as never)).toBe(
      "Dropped Write docs in To do, place 2 of 2.",
    );
    expect(say.onDragOver?.({ active: ref("a"), over: ref("column:DONE") } as never)).toBe(
      "Write docs is over Done, place 1 of 1.",
    );
  });

  it("says where it still is when it is over its own place, and what happens on cancel or no target", () => {
    expect(say.onDragOver?.({ active: ref("a"), over: ref("a") } as never)).toBe(
      "Write docs is over To do, place 1 of 2.",
    );
    expect(say.onDragEnd?.({ active: ref("a"), over: null } as never)).toBe("Dropped Write docs.");
    expect(say.onDragOver?.({ active: ref("a"), over: null } as never)).toBeUndefined();
    expect(say.onDragCancel?.({ active: ref("a"), over: null } as never)).toBe(
      "Cancelled. Write docs is where it was.",
    );
  });
});

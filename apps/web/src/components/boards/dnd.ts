import type { Announcements } from "@dnd-kit/core";
import { arrayMove } from "@dnd-kit/sortable";
import type { Status, Task } from "@/types/generated";
import { COLUMNS, STATUS_LABEL } from "./columns";

/** Ids of the column drop areas, so an empty column or the space below a column can be dropped on. */
export const COLUMN_PREFIX = "column:";
export const columnId = (status: Status): string => `${COLUMN_PREFIX}${status}`;

export function statusOfColumnId(id: string): Status | null {
  if (!id.startsWith(COLUMN_PREFIX)) return null;
  const status = id.slice(COLUMN_PREFIX.length);
  return COLUMNS.find((c) => c.status === status)?.status ?? null;
}

/** The tasks of one column in display order. */
export function columnTasks(tasks: Task[], status: Status): Task[] {
  return tasks
    .filter((t) => t.status === status)
    .sort((a, b) => a.order - b.order || a.id.localeCompare(b.id));
}

/** Where a dragged task lands: the column, and the tasks directly above and below it there. */
export interface MovePlan {
  status: Status;
  beforeId: string | null;
  afterId: string | null;
  /** Position in the target column, counting the moved task. */
  index: number;
  /** Tasks in the target column once the move is done. */
  size: number;
}

/**
 * What dropping `activeId` on `overId` means. `overId` is a task (the dragged task takes that
 * task's place, as in a sortable list) or a column (the end of that column). `null` when nothing
 * would change: dropped on itself, or in the place it already has.
 */
export function planMove(tasks: Task[], activeId: string, overId: string): MovePlan | null {
  const active = tasks.find((t) => t.id === activeId);
  if (!active) return null;

  let status: Status;
  let after: Task[]; // the target column as it will be, the moved task included
  const column = statusOfColumnId(overId);
  if (column) {
    status = column;
    after = [...columnTasks(tasks, status).filter((t) => t.id !== activeId), active];
  } else {
    const over = tasks.find((t) => t.id === overId);
    if (!over || over.id === active.id) return null;
    status = over.status;
    const list = columnTasks(tasks, status);
    const overIndex = list.findIndex((t) => t.id === over.id);
    if (status === active.status) {
      after = arrayMove(
        list,
        list.findIndex((t) => t.id === active.id),
        overIndex,
      );
    } else {
      after = [...list];
      after.splice(overIndex, 0, active);
    }
  }

  const index = after.findIndex((t) => t.id === activeId);
  if (status === active.status) {
    const before = columnTasks(tasks, status);
    if (before.findIndex((t) => t.id === activeId) === index) return null;
  }
  return {
    status,
    beforeId: after[index - 1]?.id ?? null,
    afterId: after[index + 1]?.id ?? null,
    index,
    size: after.length,
  };
}

/** The request body of the move endpoint for a plan. */
export function moveBody(plan: MovePlan): {
  to_status: Status;
  before_id?: string;
  after_id?: string;
} {
  return {
    to_status: plan.status,
    ...(plan.beforeId ? { before_id: plan.beforeId } : {}),
    ...(plan.afterId ? { after_id: plan.afterId } : {}),
  };
}

/**
 * A position for the dragged task while the server has not answered yet, by the rule of
 * `docs/API.md` (midpoint, half of the first, last plus one). The server's answer replaces it.
 */
export function optimisticOrder(tasks: Task[], plan: MovePlan): number {
  const order = (id: string | null) => tasks.find((t) => t.id === id)?.order ?? null;
  const before = order(plan.beforeId);
  const after = order(plan.afterId);
  if (before !== null && after !== null) return before + (after - before) / 2;
  if (before !== null) return before + 1;
  if (after !== null) return after / 2;
  return 1;
}

/** The board with the task moved as planned, in the order the page shows it. */
export function applyPlan(tasks: Task[], activeId: string, plan: MovePlan): Task[] {
  const order = optimisticOrder(tasks, plan);
  return tasks.map((t) => (t.id === activeId ? { ...t, status: plan.status, order } : t));
}

/** What a screen reader says while a task is dragged with the keyboard. */
export const SCREEN_READER_INSTRUCTIONS =
  "To pick up a task, press space or enter on its drag handle. Use the arrow keys to move it between columns and places, press space or enter to drop it, or escape to cancel.";

export function announcements(tasks: Task[]): Announcements {
  const title = (id: string | number) => tasks.find((t) => t.id === id)?.title ?? "task";
  const where = (activeId: string | number, overId: string | number | undefined) => {
    if (overId === undefined) return null;
    const plan = planMove(tasks, String(activeId), String(overId));
    if (plan) return `${STATUS_LABEL[plan.status]}, place ${plan.index + 1} of ${plan.size}`;
    // Over itself or its own place: where it is now.
    const current = tasks.find((t) => t.id === activeId);
    if (!current) return null;
    const list = columnTasks(tasks, current.status);
    return `${STATUS_LABEL[current.status]}, place ${list.findIndex((t) => t.id === activeId) + 1} of ${list.length}`;
  };
  return {
    onDragStart({ active }) {
      const task = tasks.find((t) => t.id === active.id);
      return `Picked up ${title(active.id)}${task ? ` from ${STATUS_LABEL[task.status]}` : ""}.`;
    },
    onDragOver({ active, over }) {
      const at = where(active.id, over?.id);
      return at ? `${title(active.id)} is over ${at}.` : undefined;
    },
    onDragEnd({ active, over }) {
      const at = where(active.id, over?.id);
      return at ? `Dropped ${title(active.id)} in ${at}.` : `Dropped ${title(active.id)}.`;
    },
    onDragCancel({ active }) {
      return `Cancelled. ${title(active.id)} is where it was.`;
    },
  };
}

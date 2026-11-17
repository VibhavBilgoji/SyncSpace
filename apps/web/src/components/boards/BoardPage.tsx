"use client";

import {
  DndContext,
  DragOverlay,
  KeyboardSensor,
  PointerSensor,
  closestCorners,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import { sortableKeyboardCoordinates } from "@dnd-kit/sortable";
import { useParams } from "next/navigation";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useWorkspace } from "@/components/shell/WorkspaceShell";
import { Banner } from "@/components/ui/Banner";
import { Button } from "@/components/ui/Button";
import { useToast } from "@/components/ui/Toast";
import { api } from "@/lib/api";
import { useWorkspaceSocket } from "@/lib/workspaceSocket";
import type { Board, Member, Status, Task } from "@/types/generated";
import { COLUMNS } from "./columns";
import { SCREEN_READER_INSTRUCTIONS, announcements, applyPlan, moveBody, planMove } from "./dnd";
import { DroppableColumn, SortableTaskCard } from "./Sortable";
import { TaskCard, TaskCardGhost } from "./TaskCard";
import { TaskDialog, type TaskDialogMode } from "./TaskDialog";

type Load =
  | { status: "loading" }
  | { status: "error" }
  | { status: "missing" }
  | { status: "ready"; board: Board; tasks: Task[]; members: Member[] };

const rank = (status: Status) => COLUMNS.findIndex((c) => c.status === status);

/** The order the server sends: column, then position. */
function sorted(tasks: Task[]): Task[] {
  return [...tasks].sort((a, b) => rank(a.status) - rank(b.status) || a.order - b.order);
}

/**
 * Frames 25 to 32: columns and cards. A task moves by dragging it (pointer, or keyboard from the
 * handle on the card), with the arrow buttons or with the status dropdown of its dialog; the
 * last two work for people who cannot or do not want to drag.
 */
export function BoardPage() {
  const { boardId } = useParams<{ boardId: string }>();
  const workspace = useWorkspace();
  const { toast } = useToast();
  const canEdit = workspace.role !== "VIEWER";
  const [state, setState] = useState<Load>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);
  const [dialog, setDialog] = useState<TaskDialogMode | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  // A few pixels before a drag starts, so that a click on a card still opens it.
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  useEffect(() => {
    let cancelled = false;
    Promise.all([
      api.boards.list(workspace.id),
      api.boards.tasks(boardId),
      api.workspaces.members(workspace.id),
    ])
      .then(([boards, tasks, members]) => {
        if (cancelled) return;
        const board = boards.find((b) => b.id === boardId);
        setState(board ? { status: "ready", board, tasks, members } : { status: "missing" });
      })
      .catch(() => !cancelled && setState({ status: "error" }));
    return () => {
      cancelled = true;
    };
  }, [workspace.id, boardId, attempt]);

  // Safety net next to the workspace socket: load again when the window comes back.
  const refresh = useCallback(() => {
    api.boards
      .tasks(boardId)
      .then((tasks) =>
        setState((current) => (current.status === "ready" ? { ...current, tasks } : current)),
      )
      .catch(() => undefined);
  }, [boardId]);
  useEffect(() => {
    const onVisible = () => {
      if (document.visibilityState === "visible") refresh();
    };
    window.addEventListener("focus", refresh);
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      window.removeEventListener("focus", refresh);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [refresh]);

  const replace = useCallback((task: Task) => {
    setState((current) =>
      current.status === "ready"
        ? {
            ...current,
            tasks: sorted(
              current.tasks.some((t) => t.id === task.id)
                ? current.tasks.map((t) => (t.id === task.id ? task : t))
                : [...current.tasks, task],
            ),
          }
        : current,
    );
  }, []);

  const remove = useCallback((id: string) => {
    setState((current) =>
      current.status === "ready"
        ? { ...current, tasks: current.tasks.filter((t) => t.id !== id) }
        : current,
    );
  }, []);

  // Live changes of this board, from the workspace socket. Our own changes come back through it
  // too, which is harmless: applying the same task twice changes nothing.
  useWorkspaceSocket(workspace.id, {
    onFrame: (frame) => {
      if (frame.type !== "BOARD_EVENT" || frame.board_id !== boardId) return;
      if (frame.kind === "COLUMN_RENUMBERED") refresh();
      else if (frame.kind === "TASK_DELETED") {
        if (frame.task) remove(frame.task.id);
      } else if (frame.task) replace(frame.task);
    },
    // Events sent while the socket was down (or before it opened) were missed: load again.
    onConnected: () => refresh(),
  });

  const names = useMemo(
    () =>
      new Map(
        state.status === "ready" ? state.members.map((m) => [m.user_id, m.name] as const) : [],
      ),
    [state],
  );

  async function move(task: Task, to: Status) {
    try {
      replace(await api.boards.moveTask(task.id, { to_status: to }));
    } catch {
      toast({ title: "Could not move the task", tone: "danger" });
      refresh();
    }
  }

  /** A drop: show the task in its new place at once, ask the server, and undo on failure. */
  async function onDragEnd(event: DragEndEvent) {
    setDragging(null);
    if (state.status !== "ready" || !event.over) return;
    const id = String(event.active.id);
    const plan = planMove(state.tasks, id, String(event.over.id));
    if (!plan) return;
    setState((current) =>
      current.status === "ready"
        ? { ...current, tasks: sorted(applyPlan(current.tasks, id, plan)) }
        : current,
    );
    try {
      replace(await api.boards.moveTask(id, moveBody(plan)));
    } catch {
      toast({ title: "Could not move the task", tone: "danger" });
      refresh();
    }
  }

  if (state.status === "loading") {
    return (
      <div aria-busy="true" aria-label="Loading board" className="flex gap-4">
        {COLUMNS.map((c) => (
          <div key={c.status} className="h-64 w-64 animate-pulse rounded-card bg-surface" />
        ))}
      </div>
    );
  }
  if (state.status === "missing") {
    return <Banner tone="danger" title="Board not found." />;
  }
  if (state.status === "error") {
    return (
      <Banner
        tone="danger"
        title="Could not load the board."
        action={
          <Button
            variant="secondary"
            onClick={() => {
              setState({ status: "loading" });
              setAttempt((n) => n + 1);
            }}
          >
            Retry
          </Button>
        }
      />
    );
  }

  const { board, tasks, members } = state;
  const draggedTask = tasks.find((t) => t.id === dragging) ?? null;
  const assigneesOf = (task: Task) => task.assignee_ids.flatMap((id) => names.get(id) ?? []);
  const columns = (
    <div className="flex gap-4 overflow-x-auto pb-4">
      {COLUMNS.map((column) => {
        const inColumn = tasks.filter((t) => t.status === column.status);
        const body = (
          <>
            {inColumn.length === 0 ? (
              <div className="rounded-card border border-dashed border-border px-3 py-6 text-center text-ui-sm text-text-faint">
                <p>No tasks</p>
                {canEdit && (
                  <button
                    type="button"
                    onClick={() => setDialog({ kind: "create", status: column.status })}
                    className="mt-1 text-text-muted hover:text-text"
                  >
                    + Add task
                  </button>
                )}
              </div>
            ) : (
              <ul className="flex flex-col gap-2">
                {inColumn.map((task) =>
                  canEdit ? (
                    <SortableTaskCard
                      key={task.id}
                      task={task}
                      assignees={assigneesOf(task)}
                      canEdit={canEdit}
                      onOpen={(t) => setDialog({ kind: "edit", task: t })}
                      onMove={(t, to) => void move(t, to)}
                    />
                  ) : (
                    <TaskCard
                      key={task.id}
                      task={task}
                      assignees={assigneesOf(task)}
                      canEdit={canEdit}
                      onOpen={(t) => setDialog({ kind: "edit", task: t })}
                      onMove={(t, to) => void move(t, to)}
                    />
                  ),
                )}
              </ul>
            )}
          </>
        );
        return (
          <section
            key={column.status}
            aria-label={column.label}
            className="flex w-64 shrink-0 flex-col gap-3"
          >
            <header className="flex items-center gap-2">
              <span aria-hidden="true" className={`size-2 rounded-full ${column.dot}`} />
              <h2 className="flex-1 text-micro text-text-muted uppercase">{column.label}</h2>
              <span aria-label={`${inColumn.length} tasks`} className="text-micro text-text-muted">
                {inColumn.length}
              </span>
              {canEdit && (
                <button
                  type="button"
                  aria-label={`Add task to ${column.label}`}
                  onClick={() => setDialog({ kind: "create", status: column.status })}
                  className="rounded-control px-1.5 text-ui-base text-text-muted hover:bg-hover hover:text-text"
                >
                  +
                </button>
              )}
            </header>
            {canEdit ? (
              <DroppableColumn status={column.status} taskIds={inColumn.map((t) => t.id)}>
                {body}
              </DroppableColumn>
            ) : (
              body
            )}
          </section>
        );
      })}
    </div>
  );
  return (
    <div>
      <div className="mb-6 flex items-center justify-between gap-4">
        <h1 className="text-heading-lg">{board.name}</h1>
        {canEdit && (
          <Button onClick={() => setDialog({ kind: "create", status: "BACKLOG" })}>
            + New Task
          </Button>
        )}
      </div>

      {canEdit ? (
        <DndContext
          sensors={sensors}
          collisionDetection={closestCorners}
          accessibility={{
            announcements: announcements(tasks),
            screenReaderInstructions: { draggable: SCREEN_READER_INSTRUCTIONS },
          }}
          onDragStart={(event: DragStartEvent) => setDragging(String(event.active.id))}
          onDragEnd={(event) => void onDragEnd(event)}
          onDragCancel={() => setDragging(null)}
        >
          {columns}
          <DragOverlay>
            {draggedTask ? (
              <ul className="w-64">
                <TaskCardGhost task={draggedTask} assignees={assigneesOf(draggedTask)} />
              </ul>
            ) : null}
          </DragOverlay>
        </DndContext>
      ) : (
        columns
      )}

      {dialog && (
        <TaskDialog
          key={dialog.kind === "edit" ? dialog.task.id : `new-${dialog.status}`}
          mode={dialog}
          boardId={board.id}
          members={members}
          canEdit={canEdit}
          onClose={() => {
            setDialog(null);
            // The task may have changed under the dialog (a 404 asks the user to close it).
            refresh();
          }}
          onSaved={(task) => {
            replace(task);
            setDialog(null);
          }}
          onDeleted={(id) => {
            remove(id);
            setDialog(null);
          }}
        />
      )}
    </div>
  );
}

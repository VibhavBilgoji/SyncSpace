"use client";

import type { ReactNode } from "react";
import { AvatarStack } from "@/components/ui/Avatar";
import { Badge } from "@/components/ui/Badge";
import type { Status, Task } from "@/types/generated";
import { PRIORITY_LABEL, PRIORITY_TONE, STATUS_LABEL, neighbourColumn } from "./columns";

export interface TaskCardProps {
  task: Task;
  /** Names of the assignees that are still members, for the avatars. */
  assignees: string[];
  /** Editors and admins get the move buttons; viewers only open the details. */
  canEdit: boolean;
  onOpen(task: Task): void;
  onMove(task: Task, to: Status): void;
}

/** The card's face: tags, priority, title, created date and assignee avatars. */
function Face({ task, assignees }: Pick<TaskCardProps, "task" | "assignees">) {
  return (
    <>
      {task.tags.length > 0 && (
        <span className="flex flex-wrap gap-1 text-micro text-text-muted">
          {task.tags.map((tag) => (
            <span key={tag}>#{tag}</span>
          ))}
        </span>
      )}
      <span className="flex items-start justify-between gap-2">
        <span className="min-w-0 break-words text-ui-base text-text">{task.title}</span>
        <Badge tone={PRIORITY_TONE[task.priority]}>{PRIORITY_LABEL[task.priority]}</Badge>
      </span>
      <span className="flex items-center justify-between gap-2 text-micro text-text-muted">
        <time dateTime={task.created_at}>
          {new Date(task.created_at).toLocaleDateString(undefined, {
            month: "short",
            day: "numeric",
          })}
        </time>
        {assignees.length > 0 && <AvatarStack names={assignees} max={3} />}
      </span>
    </>
  );
}

/**
 * What is inside a card: an optional drag handle, the face (which opens the details) and, for
 * editors, the buttons that move the task to the neighbouring columns.
 */
export function TaskCardBody({
  task,
  assignees,
  canEdit,
  onOpen,
  onMove,
  handle,
}: TaskCardProps & { handle?: ReactNode }) {
  const left = neighbourColumn(task.status, -1);
  const right = neighbourColumn(task.status, 1);
  return (
    <>
      <div className="flex items-start gap-2">
        {handle}
        <button
          type="button"
          onClick={() => onOpen(task)}
          aria-label={`Open task ${task.title}`}
          className="flex min-w-0 flex-1 flex-col gap-2 rounded-control text-left"
        >
          <Face task={task} assignees={assignees} />
        </button>
      </div>
      {canEdit && (
        <div className="mt-2 flex justify-between border-t border-border pt-2">
          <button
            type="button"
            disabled={left === null}
            onClick={() => left && onMove(task, left)}
            aria-label={left ? `Move ${task.title} to ${STATUS_LABEL[left]}` : undefined}
            className="rounded-control px-2 py-0.5 text-ui-sm text-text-muted hover:bg-hover hover:text-text disabled:invisible"
          >
            <span aria-hidden="true">←</span>
          </button>
          <button
            type="button"
            disabled={right === null}
            onClick={() => right && onMove(task, right)}
            aria-label={right ? `Move ${task.title} to ${STATUS_LABEL[right]}` : undefined}
            className="rounded-control px-2 py-0.5 text-ui-sm text-text-muted hover:bg-hover hover:text-text disabled:invisible"
          >
            <span aria-hidden="true">→</span>
          </button>
        </div>
      )}
    </>
  );
}

/** A card that cannot be dragged (viewers). */
export function TaskCard(props: TaskCardProps) {
  return (
    <li className="rounded-card border border-border bg-surface p-3">
      <TaskCardBody {...props} />
    </li>
  );
}

/** The copy that follows the pointer while dragging: no buttons, nothing to click. */
export function TaskCardGhost({ task, assignees }: Pick<TaskCardProps, "task" | "assignees">) {
  return (
    <li
      aria-hidden="true"
      className="flex cursor-grabbing flex-col gap-2 rounded-card border border-indigo bg-elevated p-3 shadow-xl"
    >
      <Face task={task} assignees={assignees} />
    </li>
  );
}

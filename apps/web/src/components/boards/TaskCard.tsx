"use client";

import { AvatarStack } from "@/components/ui/Avatar";
import { Badge } from "@/components/ui/Badge";
import type { Status, Task } from "@/types/generated";
import { PRIORITY_LABEL, PRIORITY_TONE, STATUS_LABEL, neighbourColumn } from "./columns";

interface Props {
  task: Task;
  /** Names of the assignees that are still members, for the avatars. */
  assignees: string[];
  /** Editors and admins get the move buttons; viewers only open the details. */
  canEdit: boolean;
  onOpen(task: Task): void;
  onMove(task: Task, to: Status): void;
}

/** The task card of frame 25: tags, priority, title, created date and assignee avatars. */
export function TaskCard({ task, assignees, canEdit, onOpen, onMove }: Props) {
  const left = neighbourColumn(task.status, -1);
  const right = neighbourColumn(task.status, 1);
  return (
    <li className="rounded-card border border-border bg-surface p-3">
      <button
        type="button"
        onClick={() => onOpen(task)}
        aria-label={`Open task ${task.title}`}
        className="flex w-full flex-col gap-2 rounded-control text-left"
      >
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
      </button>
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
    </li>
  );
}

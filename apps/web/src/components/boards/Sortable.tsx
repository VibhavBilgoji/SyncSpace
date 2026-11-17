"use client";

import { useDroppable } from "@dnd-kit/core";
import { SortableContext, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import type { KeyboardEventHandler, PointerEventHandler, ReactNode } from "react";
import { cn } from "@/lib/cn";
import type { Status } from "@/types/generated";
import { columnId } from "./dnd";
import { TaskCardBody, type TaskCardProps } from "./TaskCard";

/**
 * A task card that can be dragged: by pointer from anywhere on it (after a few pixels, so a click
 * still opens it), by keyboard from its handle (space or enter to lift, arrows to move, space or
 * enter to drop, escape to cancel).
 */
export function SortableTaskCard(props: TaskCardProps) {
  const {
    attributes,
    listeners,
    setNodeRef,
    setActivatorNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: props.task.id });
  return (
    <li
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      onPointerDown={listeners?.onPointerDown as PointerEventHandler<HTMLLIElement> | undefined}
      data-dragging={isDragging ? "true" : undefined}
      className={cn(
        "rounded-card border bg-surface p-3",
        // The drop ghost: the place the card is leaving, dashed and dim.
        isDragging ? "border-dashed border-indigo opacity-40" : "border-border",
      )}
    >
      <TaskCardBody
        {...props}
        handle={
          <button
            type="button"
            ref={setActivatorNodeRef}
            {...attributes}
            onKeyDown={listeners?.onKeyDown as KeyboardEventHandler<HTMLButtonElement> | undefined}
            aria-label={`Drag ${props.task.title}`}
            className="mt-0.5 cursor-grab rounded-control px-1 text-ui-sm text-text-faint hover:bg-hover hover:text-text active:cursor-grabbing"
          >
            <span aria-hidden="true">⠿</span>
          </button>
        }
      />
    </li>
  );
}

/**
 * The body of a column as a place to drop: its tasks are one sortable list, and the whole area,
 * including an empty column, accepts a drop (at the end).
 */
export function DroppableColumn({
  status,
  taskIds,
  children,
}: {
  status: Status;
  taskIds: string[];
  children: ReactNode;
}) {
  const { setNodeRef, isOver } = useDroppable({ id: columnId(status) });
  return (
    <SortableContext items={taskIds} strategy={verticalListSortingStrategy}>
      <div
        ref={setNodeRef}
        data-column-drop={status}
        className={cn(
          "flex min-h-24 flex-col gap-2 rounded-card transition-colors",
          isOver && "bg-hover/40 ring-1 ring-indigo",
        )}
      >
        {children}
      </div>
    </SortableContext>
  );
}

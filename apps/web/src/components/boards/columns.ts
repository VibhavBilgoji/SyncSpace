import type { BadgeTone } from "@/components/ui/Badge";
import type { Priority, Status } from "@/types/generated";

/** The five fixed columns of a board, left to right (frame 25). */
export const COLUMNS: Array<{ status: Status; label: string; dot: string }> = [
  { status: "BACKLOG", label: "Backlog", dot: "bg-text-faint" },
  { status: "TODO", label: "To do", dot: "bg-indigo" },
  { status: "IN_PROGRESS", label: "In progress", dot: "bg-warning" },
  { status: "REVIEW", label: "Review", dot: "bg-violet" },
  { status: "DONE", label: "Done", dot: "bg-success" },
];

export const STATUS_LABEL: Record<Status, string> = Object.fromEntries(
  COLUMNS.map((c) => [c.status, c.label]),
) as Record<Status, string>;

export const PRIORITIES: Priority[] = ["LOW", "MEDIUM", "HIGH", "URGENT"];

/** UI_SPEC section 1: URGENT danger, HIGH warning, MEDIUM indigo, LOW muted. */
export const PRIORITY_TONE: Record<Priority, BadgeTone> = {
  URGENT: "danger",
  HIGH: "warning",
  MEDIUM: "indigo",
  LOW: "muted",
};

export const PRIORITY_LABEL: Record<Priority, string> = {
  LOW: "Low",
  MEDIUM: "Medium",
  HIGH: "High",
  URGENT: "Urgent",
};

/** The column to the left or right of `status`, or null at the edge. */
export function neighbourColumn(status: Status, direction: -1 | 1): Status | null {
  const index = COLUMNS.findIndex((c) => c.status === status);
  return COLUMNS[index + direction]?.status ?? null;
}

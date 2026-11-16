import type { ReactNode } from "react";

/** Illustration-free empty state (frame 07). Pass `action` only for roles that may use it. */
export function EmptyState({
  title,
  description,
  action,
}: {
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center gap-3 rounded-card border border-dashed border-border px-6 py-12 text-center">
      <h2 className="text-heading-md text-text">{title}</h2>
      {description && <p className="max-w-sm text-ui-sm text-text-muted">{description}</p>}
      {action}
    </div>
  );
}

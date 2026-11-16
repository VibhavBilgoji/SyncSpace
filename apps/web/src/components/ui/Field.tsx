import type { ReactNode } from "react";

interface FieldProps {
  id: string;
  label: string;
  hint?: string;
  error?: string;
  children: ReactNode;
}

/** Label, control slot, then the error (or the hint). Shared by Input and Select. */
export function Field({ id, label, hint, error, children }: FieldProps) {
  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={id} className="text-ui-sm text-text-muted">
        {label}
      </label>
      {children}
      {error ? (
        <p id={`${id}-message`} role="alert" className="text-ui-sm text-danger">
          {error}
        </p>
      ) : hint ? (
        <p id={`${id}-message`} className="text-ui-sm text-text-faint">
          {hint}
        </p>
      ) : null}
    </div>
  );
}

export const controlClass =
  "h-9 w-full rounded-control border bg-surface px-3 text-ui-base text-text placeholder:text-text-faint " +
  "disabled:cursor-not-allowed disabled:opacity-50";

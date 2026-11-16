import { useId, type SelectHTMLAttributes } from "react";
import { cn } from "@/lib/cn";
import { Field, controlClass } from "./Field";

export interface SelectOption {
  value: string;
  label: string;
}

export interface SelectProps extends Omit<SelectHTMLAttributes<HTMLSelectElement>, "id"> {
  label: string;
  options: SelectOption[];
  hint?: string;
  error?: string;
  id?: string;
}

/** A native select, so keyboard and screen reader behaviour come for free. */
export function Select({ label, options, hint, error, id, className, ...rest }: SelectProps) {
  const generated = useId();
  const selectId = id ?? generated;
  return (
    <Field id={selectId} label={label} hint={hint} error={error}>
      <select
        id={selectId}
        aria-invalid={error ? true : undefined}
        aria-describedby={error || hint ? `${selectId}-message` : undefined}
        className={cn(controlClass, error ? "border-danger" : "border-border", className)}
        {...rest}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </Field>
  );
}

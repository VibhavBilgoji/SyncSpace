import { useId, type InputHTMLAttributes } from "react";
import { cn } from "@/lib/cn";
import { Field, controlClass } from "./Field";

export interface InputProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "id"> {
  label: string;
  hint?: string;
  /** Message shown under the field; also marks it invalid. */
  error?: string;
  id?: string;
}

export function Input({ label, hint, error, id, className, ...rest }: InputProps) {
  const generated = useId();
  const inputId = id ?? generated;
  return (
    <Field id={inputId} label={label} hint={hint} error={error}>
      <input
        id={inputId}
        aria-invalid={error ? true : undefined}
        aria-describedby={error || hint ? `${inputId}-message` : undefined}
        className={cn(controlClass, error ? "border-danger" : "border-border", className)}
        {...rest}
      />
    </Field>
  );
}

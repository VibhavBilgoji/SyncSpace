import type { ButtonHTMLAttributes } from "react";
import { cn } from "@/lib/cn";

export type ButtonVariant = "primary" | "secondary" | "danger" | "ghost";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  /** Shows a spinner inside the button and blocks clicks. */
  loading?: boolean;
}

const variants: Record<ButtonVariant, string> = {
  primary: "bg-indigo text-text hover:brightness-110",
  secondary: "border border-border bg-elevated text-text hover:bg-hover",
  danger: "bg-danger text-text hover:brightness-110",
  ghost: "bg-transparent text-text-muted hover:bg-hover hover:text-text",
};

export function Button({
  variant = "primary",
  loading = false,
  disabled,
  type = "button",
  className,
  children,
  ...rest
}: ButtonProps) {
  return (
    <button
      type={type}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      className={cn(
        "inline-flex h-9 items-center justify-center gap-2 rounded-control px-4 text-ui-base transition",
        "disabled:cursor-not-allowed disabled:opacity-50",
        variants[variant],
        className,
      )}
      {...rest}
    >
      {loading && (
        <span
          aria-hidden="true"
          className="size-3.5 animate-spin rounded-full border-2 border-current border-t-transparent"
        />
      )}
      {children}
    </button>
  );
}

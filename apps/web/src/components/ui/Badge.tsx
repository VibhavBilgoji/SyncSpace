import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

/**
 * Tones map to UI_SPEC section 1: priority URGENT=danger, HIGH=warning, MEDIUM=indigo, LOW=muted;
 * version MANUAL=indigo, RESTORE=violet, AUTOMATIC=muted; the ADMIN badge is success.
 */
export type BadgeTone = "indigo" | "success" | "warning" | "danger" | "violet" | "muted";

const tones: Record<BadgeTone, string> = {
  indigo: "bg-indigo/15 text-indigo",
  success: "bg-success/15 text-success",
  warning: "bg-warning/15 text-warning",
  danger: "bg-danger/15 text-danger",
  violet: "bg-violet/15 text-violet",
  muted: "bg-text-muted/15 text-text-muted",
};

export function Badge({
  tone = "muted",
  className,
  children,
}: {
  tone?: BadgeTone;
  className?: string;
  children: ReactNode;
}) {
  return (
    <span
      data-tone={tone}
      className={cn(
        "inline-flex items-center rounded-control px-1.5 py-0.5 text-micro uppercase",
        tones[tone],
        className,
      )}
    >
      {children}
    </span>
  );
}

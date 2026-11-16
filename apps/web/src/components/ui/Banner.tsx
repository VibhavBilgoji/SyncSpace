import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

export type BannerTone = "info" | "warning" | "danger";

const tones: Record<BannerTone, string> = {
  info: "border-indigo/40 bg-indigo/10",
  warning: "border-warning/40 bg-warning/10",
  danger: "border-danger/40 bg-danger/10",
};

export function Banner({
  tone = "info",
  title,
  children,
  action,
}: {
  tone?: BannerTone;
  title?: string;
  children?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div
      role={tone === "danger" ? "alert" : "status"}
      className={cn("flex items-center gap-3 rounded-card border px-4 py-3", tones[tone])}
    >
      <div className="min-w-0 flex-1 text-ui-sm text-text">
        {title && <strong className="mr-2 text-ui-base">{title}</strong>}
        {children}
      </div>
      {action}
    </div>
  );
}

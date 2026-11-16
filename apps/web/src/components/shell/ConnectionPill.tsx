import { cn } from "@/lib/cn";

export type ConnectionState = "connected" | "reconnecting" | "connecting";

const label: Record<ConnectionState, string> = {
  connected: "Connected (WebSocket)",
  reconnecting: "Reconnecting...",
  connecting: "Connecting...",
};

const dot: Record<ConnectionState, string> = {
  connected: "bg-success",
  reconnecting: "bg-warning",
  connecting: "bg-text-faint",
};

/** Placeholder until the socket client exists (tasks 2.6 and 3.2); defaults to "connecting". */
export function ConnectionPill({
  state = "connecting",
  latencyMs,
}: {
  state?: ConnectionState;
  latencyMs?: number;
}) {
  return (
    <div
      role="status"
      data-state={state}
      className="flex items-center gap-2 rounded-control border border-border px-2 py-1 text-ui-sm text-text-muted"
    >
      <span aria-hidden="true" className={cn("size-2 rounded-full", dot[state])} />
      <span>
        {label[state]}
        {state === "connected" && latencyMs !== undefined ? ` ${latencyMs}ms` : ""}
      </span>
    </div>
  );
}

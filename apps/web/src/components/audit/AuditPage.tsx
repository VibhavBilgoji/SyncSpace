"use client";

import { useCallback, useEffect, useState } from "react";
import { useWorkspace } from "@/components/shell/WorkspaceShell";
import { Badge } from "@/components/ui/Badge";
import { Banner } from "@/components/ui/Banner";
import { Button } from "@/components/ui/Button";
import { EmptyState } from "@/components/ui/EmptyState";
import { Select } from "@/components/ui/Select";
import { api, isApiError } from "@/lib/api";
import type { AuditEntry } from "@/types/generated";

/** The audited actions of `docs/DATA_MODEL.md`, for the filter. */
export const ACTIONS = [
  "MEMBER_ADD",
  "MEMBER_REMOVE",
  "ROLE_CHANGE",
  "DOC_CREATE",
  "DOC_DELETE",
  "VERSION_SAVE",
  "VERSION_RESTORE",
  "TASK_CREATE",
  "TASK_MOVE",
  "TASK_DELETE",
] as const;

const PAGE = 50;

type State =
  | { status: "loading" }
  | { status: "error" }
  | { status: "forbidden" }
  | {
      status: "ready";
      items: AuditEntry[];
      next: string | null;
      loadingMore: boolean;
    };

/** `2026-11-17 10:15:00` in UTC, as the server stored it. */
export function utc(iso: string): string {
  return iso
    .replace("T", " ")
    .replace(/\.\d+Z$|Z$/, "")
    .slice(0, 19);
}

/** The metadata on one line: `title: Plan, status: TODO`. */
export function summary(metadata: Record<string, unknown>): string {
  const parts = Object.entries(metadata).map(
    ([key, value]) =>
      `${key}: ${typeof value === "object" && value !== null ? JSON.stringify(value) : String(value)}`,
  );
  return parts.length > 0 ? parts.join(", ") : "none";
}

/** Frame 39, reduced: the audit trail as a plain table with a filter on the action. */
export function AuditPage() {
  const workspace = useWorkspace();
  const [action, setAction] = useState("");
  const [state, setState] = useState<State>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);

  const failure = useCallback((e: unknown): State => {
    return isApiError(e) && e.status === 403 ? { status: "forbidden" } : { status: "error" };
  }, []);

  useEffect(() => {
    let cancelled = false;
    api.audit
      .list(workspace.id, { limit: PAGE, ...(action ? { action } : {}) })
      .then(
        (page) =>
          !cancelled &&
          setState({
            status: "ready",
            items: page.items,
            next: page.next_cursor,
            loadingMore: false,
          }),
      )
      .catch((e: unknown) => !cancelled && setState(failure(e)));
    return () => {
      cancelled = true;
    };
  }, [workspace.id, action, attempt, failure]);

  async function loadMore() {
    if (state.status !== "ready" || state.next === null || state.loadingMore) return;
    const cursor = state.next;
    setState({ ...state, loadingMore: true });
    try {
      const page = await api.audit.list(workspace.id, {
        limit: PAGE,
        cursor,
        ...(action ? { action } : {}),
      });
      setState((current) =>
        current.status === "ready"
          ? {
              status: "ready",
              items: [...current.items, ...page.items],
              next: page.next_cursor,
              loadingMore: false,
            }
          : current,
      );
    } catch {
      setState((current) =>
        current.status === "ready" ? { ...current, loadingMore: false } : current,
      );
    }
  }

  return (
    <div className="mx-auto max-w-5xl">
      <div className="mb-6 flex flex-wrap items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <h1 className="text-heading-lg">System Audit Trail</h1>
          <Badge tone="success">IMMUTABLE APPEND-ONLY</Badge>
        </div>
        {state.status !== "forbidden" && (
          <div className="w-56">
            <Select
              label="Action"
              value={action}
              options={[
                { value: "", label: "All actions" },
                ...ACTIONS.map((a) => ({ value: a, label: a })),
              ]}
              onChange={(e) => {
                setState({ status: "loading" });
                setAction(e.target.value);
              }}
            />
          </div>
        )}
      </div>

      {state.status === "loading" && (
        <div aria-busy="true" aria-label="Loading audit trail" className="flex flex-col gap-2">
          {[0, 1, 2, 3].map((n) => (
            <div key={n} className="h-10 animate-pulse rounded-control bg-surface" />
          ))}
        </div>
      )}

      {state.status === "forbidden" && (
        <Banner tone="warning" title="Only admins can read the audit trail." />
      )}

      {state.status === "error" && (
        <Banner
          tone="danger"
          title="Could not load the audit trail."
          action={
            <Button
              variant="secondary"
              onClick={() => {
                setState({ status: "loading" });
                setAttempt((n) => n + 1);
              }}
            >
              Retry
            </Button>
          }
        />
      )}

      {state.status === "ready" && state.items.length === 0 && (
        <EmptyState
          title="No events"
          description={
            action
              ? "No event of this action has been recorded yet."
              : "Nothing has been recorded in this workspace yet."
          }
        />
      )}

      {state.status === "ready" && state.items.length > 0 && (
        <>
          <table className="w-full border-collapse text-left text-ui-sm">
            <thead>
              <tr className="border-b border-border text-micro text-text-muted uppercase">
                <th className="py-2 pr-3 font-semibold">Timestamp (UTC)</th>
                <th className="py-2 pr-3 font-semibold">Actor</th>
                <th className="py-2 pr-3 font-semibold">Action</th>
                <th className="py-2 pr-3 font-semibold">Entity type</th>
                <th className="py-2 pr-3 font-semibold">Entity id</th>
                <th className="py-2 font-semibold">State delta</th>
              </tr>
            </thead>
            <tbody>
              {state.items.map((entry) => (
                <tr key={entry.event_id} className="border-b border-border align-top">
                  <td className="py-2 pr-3 font-mono text-mono-code whitespace-nowrap">
                    <time dateTime={entry.timestamp}>{utc(entry.timestamp)}</time>
                  </td>
                  <td className="py-2 pr-3">{entry.actor.name}</td>
                  <td className="py-2 pr-3">
                    <Badge tone="indigo">{entry.action}</Badge>
                  </td>
                  <td className="py-2 pr-3">{entry.entity_type}</td>
                  <td className="py-2 pr-3 font-mono text-mono-code" title={entry.entity_id}>
                    {entry.entity_id.slice(0, 8)}
                  </td>
                  <td className="py-2 font-mono text-mono-code text-text-muted">
                    {summary(entry.metadata)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="mt-4 flex items-center justify-between text-ui-sm text-text-muted">
            <span>
              {state.items.length} {state.items.length === 1 ? "event" : "events"} loaded
            </span>
            {state.next !== null && (
              <Button
                variant="secondary"
                loading={state.loadingMore}
                onClick={() => void loadMore()}
              >
                Load older events
              </Button>
            )}
          </div>
        </>
      )}
    </div>
  );
}

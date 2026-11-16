"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import { Badge } from "@/components/ui/Badge";
import { Banner } from "@/components/ui/Banner";
import { Button } from "@/components/ui/Button";
import { api } from "@/lib/api";
import { RequireAuth } from "@/lib/session";
import type { Role, WorkspaceSummary } from "@/types/generated";

export const roleTone = {
  ADMIN: "success",
  EDITOR: "indigo",
  VIEWER: "muted",
} as const satisfies Record<Role, "success" | "indigo" | "muted">;

/** Frame 05. A user with no workspace is sent to onboarding. */
export function WorkspacePicker() {
  return (
    <RequireAuth>
      <PickerInner />
    </RequireAuth>
  );
}

type State =
  { status: "loading" } | { status: "error" } | { status: "ready"; items: WorkspaceSummary[] };

function PickerInner() {
  const router = useRouter();
  const [state, setState] = useState<State>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let cancelled = false;
    api.workspaces
      .list()
      .then((items) => {
        if (cancelled) return;
        if (items.length === 0) router.replace("/onboarding");
        else setState({ status: "ready", items });
      })
      .catch(() => !cancelled && setState({ status: "error" }));
    return () => {
      cancelled = true;
    };
  }, [router, attempt]);

  return (
    <main className="mx-auto flex max-w-3xl flex-col gap-6 p-8">
      <div className="flex items-center justify-between">
        <h1 className="text-heading-lg">Choose a workspace</h1>
        <Link href="/onboarding" className="text-ui-base text-indigo hover:underline">
          New workspace
        </Link>
      </div>

      {state.status === "loading" && (
        <div aria-busy="true" aria-label="Loading workspaces" className="grid gap-3 sm:grid-cols-2">
          {[0, 1].map((i) => (
            <div
              key={i}
              className="h-28 animate-pulse rounded-card border border-border bg-surface"
            />
          ))}
        </div>
      )}

      {state.status === "error" && (
        <Banner
          tone="danger"
          title="Could not load your workspaces."
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

      {state.status === "ready" && (
        <ul className="grid gap-3 sm:grid-cols-2">
          {state.items.map((w) => (
            <li key={w.id}>
              <Link
                href={`/w/${w.slug}/docs`}
                className="flex flex-col gap-3 rounded-card border border-border bg-surface p-4 hover:bg-hover"
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="truncate text-heading-md">{w.name}</span>
                  <Badge tone={roleTone[w.role]}>{w.role}</Badge>
                </div>
                <p className="text-ui-sm text-text-muted">
                  {w.member_count} members · {w.document_count} documents · {w.board_count} boards
                </p>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </main>
  );
}

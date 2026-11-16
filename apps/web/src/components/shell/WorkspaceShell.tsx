"use client";

import Link from "next/link";
import { useParams } from "next/navigation";
import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { Banner } from "@/components/ui/Banner";
import { Button } from "@/components/ui/Button";
import { api } from "@/lib/api";
import { RequireAuth } from "@/lib/session";
import type { WorkspaceSummary } from "@/types/generated";
import { AppHeader } from "./AppHeader";
import { AppSidebar } from "./AppSidebar";

const WorkspaceContext = createContext<WorkspaceSummary | null>(null);

/** The workspace of the current `/w/[slug]` route, with the caller's role. */
export function useWorkspace(): WorkspaceSummary {
  const workspace = useContext(WorkspaceContext);
  if (!workspace) throw new Error("useWorkspace must be used inside <WorkspaceShell>");
  return workspace;
}

type State =
  | { status: "loading" }
  | { status: "error" }
  | { status: "missing" }
  | { status: "ready"; workspace: WorkspaceSummary };

/** Auth guard, workspace lookup by slug, sidebar and header around the page content. */
export function WorkspaceShell({ children, title }: { children: ReactNode; title?: string }) {
  return (
    <RequireAuth>
      <ShellInner title={title}>{children}</ShellInner>
    </RequireAuth>
  );
}

function ShellInner({ children, title }: { children: ReactNode; title?: string }) {
  const { slug } = useParams<{ slug: string }>();
  const [state, setState] = useState<State>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let cancelled = false;
    api.workspaces
      .list()
      .then((items) => {
        if (cancelled) return;
        const workspace = items.find((w) => w.slug === slug);
        setState(workspace ? { status: "ready", workspace } : { status: "missing" });
      })
      .catch(() => !cancelled && setState({ status: "error" }));
    return () => {
      cancelled = true;
    };
  }, [slug, attempt]);

  if (state.status === "loading") {
    return (
      <div aria-busy="true" aria-label="Loading workspace" className="flex min-h-screen">
        <div className="w-60 animate-pulse border-r border-border bg-surface" />
        <div className="flex-1" />
      </div>
    );
  }

  if (state.status === "missing") {
    // Same answer for "does not exist" and "not a member" (docs/API.md section 1).
    return (
      <main className="mx-auto max-w-md p-8 text-center">
        <h1 className="text-heading-lg">Workspace not found</h1>
        <p className="mt-2 text-ui-sm text-text-muted">
          It may not exist, or you may not be a member.
        </p>
        <Link href="/w" className="mt-4 inline-block text-indigo hover:underline">
          Back to your workspaces
        </Link>
      </main>
    );
  }

  if (state.status === "error") {
    return (
      <main className="mx-auto max-w-md p-8">
        <Banner
          tone="danger"
          title="Could not load the workspace."
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
      </main>
    );
  }

  return (
    <WorkspaceContext.Provider value={state.workspace}>
      <div className="flex min-h-screen">
        <AppSidebar />
        <div className="flex min-w-0 flex-1 flex-col">
          <AppHeader section={title} />
          <div className="flex-1 p-8">{children}</div>
        </div>
      </div>
    </WorkspaceContext.Provider>
  );
}

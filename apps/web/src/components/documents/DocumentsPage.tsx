"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import { useWorkspace } from "@/components/shell/WorkspaceShell";
import { Banner } from "@/components/ui/Banner";
import { Button } from "@/components/ui/Button";
import { EmptyState } from "@/components/ui/EmptyState";
import { api } from "@/lib/api";
import type { DocumentSummary } from "@/types/generated";
import { CreateDocumentDialog } from "./CreateDocumentDialog";

type State =
  { status: "loading" } | { status: "error" } | { status: "ready"; documents: DocumentSummary[] };

/** Frames 06 (list), 07 (empty workspace) and 08 (create dialog). */
export function DocumentsPage() {
  const workspace = useWorkspace();
  const router = useRouter();
  const [state, setState] = useState<State>({ status: "loading" });
  const [creating, setCreating] = useState(false);
  const canCreate = workspace.role !== "VIEWER";

  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let cancelled = false;
    api.documents
      .list(workspace.id)
      .then((documents) => !cancelled && setState({ status: "ready", documents }))
      .catch(() => !cancelled && setState({ status: "error" }));
    return () => {
      cancelled = true;
    };
  }, [workspace.id, attempt]);

  const createButton = canCreate ? (
    <Button onClick={() => setCreating(true)}>Create document</Button>
  ) : undefined;

  return (
    <div className="mx-auto max-w-3xl">
      <div className="mb-6 flex items-center justify-between gap-4">
        <h1 className="text-heading-lg">Documents</h1>
        {state.status === "ready" && state.documents.length > 0 && createButton}
      </div>

      {state.status === "loading" && (
        <div aria-busy="true" aria-label="Loading documents" className="flex flex-col gap-2">
          {[0, 1, 2].map((n) => (
            <div key={n} className="h-14 animate-pulse rounded-card bg-surface" />
          ))}
        </div>
      )}

      {state.status === "error" && (
        <Banner
          tone="danger"
          title="Could not load the documents."
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

      {state.status === "ready" && state.documents.length === 0 && (
        <EmptyState
          title="No documents yet"
          description={
            canCreate
              ? "Create the first document of this workspace."
              : "Documents created by editors will appear here."
          }
          action={createButton}
        />
      )}

      {state.status === "ready" && state.documents.length > 0 && (
        <ul className="flex flex-col gap-2">
          {state.documents.map((document) => (
            <li key={document.id}>
              <Link
                href={`/w/${workspace.slug}/docs/${document.id}`}
                className="flex items-center gap-3 rounded-card border border-border bg-surface px-4 py-3 hover:bg-hover"
              >
                <span aria-hidden="true" className="text-heading-md">
                  {document.icon}
                </span>
                <span className="min-w-0 flex-1 truncate text-ui-base">{document.title}</span>
                <span className="text-micro text-text-muted">rev_{document.revision}</span>
                <time dateTime={document.updated_at} className="text-micro text-text-muted">
                  {new Date(document.updated_at).toLocaleString(undefined, {
                    month: "short",
                    day: "numeric",
                    hour: "numeric",
                    minute: "2-digit",
                  })}
                </time>
              </Link>
            </li>
          ))}
        </ul>
      )}

      <CreateDocumentDialog
        open={creating}
        workspaceId={workspace.id}
        onClose={() => setCreating(false)}
        onCreated={(document) => {
          setCreating(false);
          router.push(`/w/${workspace.slug}/docs/${document.id}`);
        }}
      />
    </div>
  );
}

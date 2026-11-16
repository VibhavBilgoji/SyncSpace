"use client";

import { useCallback, useState } from "react";
import { Banner } from "@/components/ui/Banner";
import type { DocumentSession } from "@/lib/doc/useDocumentSession";
import { BlockRow } from "./BlockRow";

interface Props {
  title: string;
  icon: string;
  /** The caller's role in the workspace; VIEWER gets the read-only page (frame 12). */
  role: "ADMIN" | "EDITOR" | "VIEWER";
  session: DocumentSession;
}

/** The status text of the metadata line (frame 09), "Waiting to sync" while offline. */
function saveLabel(session: DocumentSession): string {
  if (session.status !== "live") return "Waiting to sync";
  return session.saved ? "Saved to MongoDB" : "Connected";
}

/** Frames 09 to 12: icon, title, metadata line and the block column. */
export function DocumentEditor({ title, icon, role, session }: Props) {
  const readOnly = role === "VIEWER";
  const [focusId, setFocusId] = useState<string | null>(null);
  const clearFocus = useCallback(() => setFocusId(null), []);

  if (session.status === "gone") {
    return <Banner tone="danger" title="This document was deleted." />;
  }
  if (session.status === "forbidden" || session.status === "unauthorized") {
    return (
      <Banner tone="danger" title="You no longer have access to this document.">
        Go back to your workspaces and sign in again if needed.
      </Banner>
    );
  }
  if (session.status === "failed") {
    return <Banner tone="danger" title="Could not open the document. Reload the page to retry." />;
  }

  return (
    <div className="mx-auto w-full max-w-[680px]">
      {readOnly && (
        <div className="mb-4">
          <Banner tone="info" title="VIEWER · Read-only access.">
            You can follow edits and preview saved versions.
          </Banner>
        </div>
      )}
      {session.status === "reconnecting" && (
        <div className="mb-4">
          <Banner tone="warning" title="Reconnecting...">
            Your edits are kept in this tab and will merge when connected.
          </Banner>
        </div>
      )}
      {session.notice === "READ_ONLY" && !readOnly && (
        <div className="mb-4">
          <Banner tone="warning" title="Your role no longer allows editing.">
            Reload the page to continue as a viewer.
          </Banner>
        </div>
      )}

      <header className="mb-6">
        <h1 className="text-display-xl">
          <span aria-hidden="true">{icon} </span>
          {title}
        </h1>
        <p className="mt-2 font-mono text-mono-code text-text-muted">
          Revision: rev_{session.saved?.revision ?? 0} · {saveLabel(session)} · Active Sockets:{" "}
          {session.activeSockets}
        </p>
      </header>

      {session.status === "loading" || session.status === "connecting" ? (
        <div aria-busy="true" aria-label="Loading document" className="flex flex-col gap-3">
          {[0, 1, 2].map((n) => (
            <div key={n} className="h-6 animate-pulse rounded-control bg-surface" />
          ))}
        </div>
      ) : (
        <div className="flex flex-col gap-1">
          {session.blocks.map((block, index) => (
            <BlockRow
              key={block.block_id}
              block={block}
              previousId={session.blocks[index - 1]?.block_id ?? null}
              readOnly={readOnly}
              focusRequested={focusId === block.block_id}
              onFocused={clearFocus}
              onFocusBlock={setFocusId}
              actions={session}
            />
          ))}
        </div>
      )}
    </div>
  );
}

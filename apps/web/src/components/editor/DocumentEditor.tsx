"use client";

import { useCallback, useState } from "react";
import { Banner } from "@/components/ui/Banner";
import { Button } from "@/components/ui/Button";
import { formatWhen } from "@/components/versions/format";
import type { DocumentSession } from "@/lib/doc/useDocumentSession";
import type { Block, VersionSummary } from "@/types/generated";
import { BlockRow } from "./BlockRow";

interface Props {
  title: string;
  icon: string;
  /** The caller's role in the workspace; VIEWER gets the read-only page (frame 12). */
  role: "ADMIN" | "EDITOR" | "VIEWER";
  session: DocumentSession;
  /** An archived version shown read-only instead of the live blocks (frame 20). */
  preview?: { version: VersionSummary; blocks: Block[] } | null;
  onLeavePreview?: () => void;
}

const ignoreCursor = () => {};

/** The status text of the metadata line (frame 09), "Waiting to sync" while offline. */
function saveLabel(session: DocumentSession): string {
  if (session.status !== "live") return "Waiting to sync";
  return session.saved ? "Saved to MongoDB" : "Connected";
}

/** Frames 09 to 12: icon, title, metadata line and the block column. */
export function DocumentEditor({
  title,
  icon,
  role,
  session,
  preview = null,
  onLeavePreview,
}: Props) {
  const viewer = role === "VIEWER";
  // Previewing an archive suspends live editing for everyone, viewers included.
  const readOnly = viewer || preview !== null;
  const blocks = preview ? preview.blocks : session.blocks;
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
      {preview && (
        <div className="mb-4">
          <Banner
            tone="warning"
            title={`Viewing Version ${preview.version.version_number} · Read-only archive.`}
            action={
              onLeavePreview && (
                <Button variant="secondary" onClick={onLeavePreview}>
                  Back to live document
                </Button>
              )
            }
          >
            Live editing is suspended.
          </Banner>
        </div>
      )}
      {viewer && (
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
      {session.notice === "READ_ONLY" && !viewer && (
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
          {preview
            ? `Version ${preview.version.version_number} · ${preview.version.change_type} · ${formatWhen(preview.version.created_at)}`
            : `Revision: rev_${session.saved?.revision ?? 0} · ${saveLabel(session)} · Active Sockets: ${session.activeSockets}`}
        </p>
      </header>

      {!preview && (session.status === "loading" || session.status === "connecting") ? (
        <div aria-busy="true" aria-label="Loading document" className="flex flex-col gap-3">
          {[0, 1, 2].map((n) => (
            <div key={n} className="h-6 animate-pulse rounded-control bg-surface" />
          ))}
        </div>
      ) : (
        <div className="flex flex-col gap-1">
          {blocks.map((block, index) => (
            <BlockRow
              key={block.block_id}
              block={block}
              previousId={blocks[index - 1]?.block_id ?? null}
              readOnly={readOnly}
              focusRequested={focusId === block.block_id}
              onFocused={clearFocus}
              onFocusBlock={setFocusId}
              actions={session}
              cursors={preview ? [] : session.cursors.filter((c) => c.blockId === block.block_id)}
              onCursor={preview ? ignoreCursor : session.setCursor}
            />
          ))}
        </div>
      )}
    </div>
  );
}

"use client";

import { useParams, usePathname, useRouter, useSearchParams } from "next/navigation";
import { useCallback, useEffect, useState } from "react";
import { useWorkspace } from "@/components/shell/WorkspaceShell";
import { AvatarStack } from "@/components/ui/Avatar";
import { Banner } from "@/components/ui/Banner";
import { Button } from "@/components/ui/Button";
import { useToast } from "@/components/ui/Toast";
import { VersionHistoryPanel } from "@/components/versions/VersionHistoryPanel";
import { api } from "@/lib/api";
import { useDocumentSession } from "@/lib/doc/useDocumentSession";
import { HeaderActions } from "@/lib/headerSlot";
import type { VersionDetail } from "@/types/generated";
import { DocumentEditor } from "./DocumentEditor";
import { ShareDialog } from "./ShareDialog";

type Meta =
  { status: "loading" } | { status: "missing" } | { status: "ready"; title: string; icon: string };

/**
 * The route `/w/[slug]/docs/[docId]`: title and icon over REST, the blocks over the socket. The
 * header carries the presence avatars, "Version History" and "Share" (UI_SPEC section 3); with
 * `?history=1` the history panel is on the right (frames 18 to 24b).
 */
export function DocumentPage() {
  const { docId } = useParams<{ docId: string }>();
  const workspace = useWorkspace();
  const session = useDocumentSession(docId);
  const [meta, setMeta] = useState<Meta>({ status: "loading" });
  const router = useRouter();
  const pathname = usePathname();
  const historyOpen = useSearchParams().get("history") === "1";
  const [preview, setPreview] = useState<VersionDetail | null>(null);
  const [sharing, setSharing] = useState(false);
  const { toast } = useToast();

  useEffect(() => {
    let cancelled = false;
    api.documents
      .get(docId)
      .then((d) => !cancelled && setMeta({ status: "ready", title: d.title, icon: d.icon }))
      .catch(() => !cancelled && setMeta({ status: "missing" }));
    return () => {
      cancelled = true;
    };
  }, [docId]);

  const setHistory = useCallback(
    (open: boolean) => {
      if (!open) setPreview(null);
      router.replace(open ? `${pathname}?history=1` : pathname);
    },
    [router, pathname],
  );

  const openPreview = useCallback(
    (versionNumber: number) => {
      api.versions
        .get(docId, versionNumber)
        .then(setPreview)
        .catch(() => toast({ title: `Could not open Version ${versionNumber}`, tone: "danger" }));
    },
    [docId, toast],
  );

  if (meta.status === "missing") {
    return <Banner tone="danger" title="Document not found." />;
  }
  const gone = session.status === "gone";
  const showPanel = historyOpen && !gone;
  const title = meta.status === "ready" ? meta.title : "";
  return (
    <div className="flex gap-6">
      {!gone && (
        <HeaderActions>
          {session.peers.length > 0 && (
            <div aria-label="People here now" className="flex items-center gap-2">
              <AvatarStack names={session.peers} max={3} />
            </div>
          )}
          <Button
            variant="secondary"
            aria-pressed={historyOpen}
            onClick={() => setHistory(!historyOpen)}
          >
            Version History
          </Button>
          <Button onClick={() => setSharing(true)}>Share</Button>
        </HeaderActions>
      )}
      <div className="min-w-0 flex-1">
        <DocumentEditor
          title={title}
          icon={meta.status === "ready" ? meta.icon : ""}
          role={workspace.role}
          session={session}
          preview={showPanel ? preview : null}
          onLeavePreview={() => setPreview(null)}
        />
      </div>
      {showPanel && (
        <VersionHistoryPanel
          docId={docId}
          role={workspace.role}
          previewing={preview?.version.version_number ?? null}
          onPreview={openPreview}
          onLeavePreview={() => setPreview(null)}
          event={session.versionEvent}
          collaborators={session.activeSockets}
          onClose={() => setHistory(false)}
        />
      )}
      {sharing && (
        <ShareDialog
          open
          onClose={() => setSharing(false)}
          title={title || "this document"}
          link={`${window.location.origin}${pathname}`}
          workspaceName={workspace.name}
          memberCount={workspace.member_count}
        />
      )}
    </div>
  );
}

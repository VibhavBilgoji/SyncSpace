"use client";

import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { Badge } from "@/components/ui/Badge";
import { Banner } from "@/components/ui/Banner";
import { Button } from "@/components/ui/Button";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { useToast } from "@/components/ui/Toast";
import { api } from "@/lib/api";
import { isApiError } from "@/lib/api/errors";
import type { VersionEvent } from "@/lib/doc/useDocumentSession";
import { useSession } from "@/lib/session";
import type { VersionSummary } from "@/types/generated";
import { BADGE_TONE, upsertVersion, versionBadge, versionMeta, versionTitle } from "./format";

export const LABEL_MAX = 80;
const PAGE_SIZE = 50;

interface Props {
  docId: string;
  /** The caller's role: only ADMIN and EDITOR may create and restore (frame 23 for VIEWER). */
  role: "ADMIN" | "EDITOR" | "VIEWER";
  /** The version being previewed, if any (frame 20). */
  previewing: number | null;
  onPreview(versionNumber: number): void;
  /** Called after a restore: the live document is shown again. */
  onLeavePreview(): void;
  /** The last version frame from the socket. */
  event: VersionEvent | null;
  /** People with the document open, for the restore confirmation (frame 21). */
  collaborators: number;
  onClose(): void;
}

type Load =
  | { status: "loading" }
  | { status: "error" }
  | { status: "ready"; items: VersionSummary[]; next: string | null; loadingMore: boolean };

/** Frames 18 to 24b: the version history panel on the right of the editor. */
export function VersionHistoryPanel({
  docId,
  role,
  previewing,
  onPreview,
  onLeavePreview,
  event,
  collaborators,
  onClose,
}: Props) {
  const canWrite = role !== "VIEWER";
  const { toast } = useToast();
  const { session } = useSession();
  const myId = session.status === "authed" ? session.user.id : null;
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [creating, setCreating] = useState(false);
  const [restoring, setRestoring] = useState<VersionSummary | null>(null);

  const fetchFirstPage = useCallback(() => {
    setLoad({ status: "loading" });
    api.versions
      .list(docId, { limit: PAGE_SIZE })
      .then((page) =>
        setLoad({ status: "ready", items: page.items, next: page.next_cursor, loadingMore: false }),
      )
      .catch(() => setLoad({ status: "error" }));
  }, [docId]);

  useEffect(() => {
    let cancelled = false;
    api.versions
      .list(docId, { limit: PAGE_SIZE })
      .then((page) => {
        if (!cancelled) {
          setLoad({
            status: "ready",
            items: page.items,
            next: page.next_cursor,
            loadingMore: false,
          });
        }
      })
      .catch(() => !cancelled && setLoad({ status: "error" }));
    return () => {
      cancelled = true;
    };
  }, [docId]);

  const put = useCallback((version: VersionSummary) => {
    setLoad((current) =>
      current.status === "ready"
        ? { ...current, items: upsertVersion(current.items, version) }
        : current,
    );
  }, []);

  // Version frames that arrive while the panel is open: someone saved or restored. Frames that
  // were already there when the panel opened are not replayed.
  const handled = useRef(event?.seq ?? 0);
  useEffect(() => {
    if (!event || event.seq <= handled.current) return;
    handled.current = event.seq;
    put(event.version);
    if (event.kind === "restored" && event.by.id !== myId) {
      toast({
        title: `Version ${event.restoredFrom} restored`,
        description: `${event.by.name} restored it for everyone in this document.`,
        tone: "info",
      });
    }
  }, [event, put, toast, myId]);

  async function loadMore() {
    if (load.status !== "ready" || load.next === null || load.loadingMore) return;
    const cursor = load.next;
    setLoad({ ...load, loadingMore: true });
    try {
      const page = await api.versions.list(docId, { limit: PAGE_SIZE, cursor });
      setLoad((current) =>
        current.status === "ready"
          ? {
              status: "ready",
              items: page.items.reduce(upsertVersion, current.items),
              next: page.next_cursor,
              loadingMore: false,
            }
          : current,
      );
    } catch {
      setLoad((current) =>
        current.status === "ready" ? { ...current, loadingMore: false } : current,
      );
      toast({ title: "Could not load more versions", tone: "danger" });
    }
  }

  const items = load.status === "ready" ? load.items : [];

  return (
    <aside
      aria-label="Version History"
      className="flex w-[360px] shrink-0 flex-col gap-4 border-l border-border pl-6"
    >
      <div className="flex items-center justify-between gap-2">
        <h2 className="text-heading-md">Version History</h2>
        <button
          type="button"
          onClick={onClose}
          className="rounded-control px-2 py-1 text-ui-sm text-text-muted hover:bg-hover hover:text-text"
        >
          Close
        </button>
      </div>

      {canWrite && load.status === "ready" && items.length > 0 && (
        <Button variant="secondary" onClick={() => setCreating(true)}>
          Create Named Version
        </Button>
      )}

      {load.status === "loading" && (
        <div aria-busy="true" aria-label="Loading versions" className="flex flex-col gap-3">
          {[0, 1, 2].map((n) => (
            <div key={n} className="h-20 animate-pulse rounded-card bg-surface" />
          ))}
        </div>
      )}

      {load.status === "error" && (
        <Banner
          tone="danger"
          title="Could not load the version history."
          action={
            <Button variant="secondary" onClick={fetchFirstPage}>
              Retry
            </Button>
          }
        />
      )}

      {load.status === "ready" && items.length === 0 && (
        <EmptyState
          title="No versions yet"
          description={
            canWrite
              ? "Save a named version to keep a copy you can come back to."
              : "Editors can save named versions of this document."
          }
          action={
            canWrite ? (
              <Button onClick={() => setCreating(true)}>Create Named Version</Button>
            ) : null
          }
        />
      )}

      {items.length > 0 && (
        <ul className="flex flex-col gap-3">
          {items.map((v) => {
            const isPreviewing = previewing === v.version_number;
            return (
              <li
                key={v.version_number}
                aria-label={`Version ${v.version_number}`}
                aria-current={isPreviewing || undefined}
                className="flex flex-col gap-2 rounded-card border border-border bg-surface p-3"
              >
                <div className="flex items-center gap-2">
                  <Badge tone={BADGE_TONE[v.change_type]}>{versionBadge(v)}</Badge>
                </div>
                <p className="text-ui-base text-text">{versionTitle(v)}</p>
                <p className="text-ui-sm text-text-muted">{versionMeta(v)}</p>
                <div className="flex gap-2">
                  {isPreviewing ? (
                    <Button variant="secondary" disabled>
                      Previewing
                    </Button>
                  ) : (
                    <Button variant="secondary" onClick={() => onPreview(v.version_number)}>
                      Preview
                    </Button>
                  )}
                  {canWrite && (
                    <Button variant="ghost" onClick={() => setRestoring(v)}>
                      Restore
                    </Button>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
      )}

      {load.status === "ready" && load.next !== null && (
        <Button variant="ghost" loading={load.loadingMore} onClick={() => void loadMore()}>
          Load older versions
        </Button>
      )}

      {load.status === "ready" && items.length > 0 && (
        <p className="text-ui-sm text-text-faint">
          Newest 50 automatic versions retained. Manual and restore versions are kept.
        </p>
      )}

      {/* Mounted only while open, so each opening starts with fresh state. */}
      {creating && (
        <CreateVersionDialog
          docId={docId}
          onClose={() => setCreating(false)}
          onCreated={(version) => {
            put(version);
            setCreating(false);
            toast({ title: `Saved Version ${version.version_number}`, tone: "success" });
          }}
        />
      )}
      {restoring && (
        <RestoreDialog
          version={restoring}
          docId={docId}
          collaborators={collaborators}
          onClose={() => setRestoring(null)}
          onRestored={(restored, from) => {
            put(restored);
            setRestoring(null);
            onLeavePreview();
            toast({ title: `Restored Version ${from.version_number}`, tone: "success" });
          }}
        />
      )}
    </aside>
  );
}

/** Frame 19: a label of 1 to 80 characters. */
function CreateVersionDialog({
  docId,
  onClose,
  onCreated,
}: {
  docId: string;
  onClose(): void;
  onCreated(version: VersionSummary): void;
}) {
  const [label, setLabel] = useState("");
  const [error, setError] = useState<string | undefined>();
  const [busy, setBusy] = useState(false);
  const { toast } = useToast();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const trimmed = label.trim();
    if (trimmed === "") return setError("Enter a label for this version.");
    if ([...trimmed].length > LABEL_MAX) return setError(`Use at most ${LABEL_MAX} characters.`);
    setBusy(true);
    setError(undefined);
    try {
      onCreated(await api.versions.create(docId, { label: trimmed }));
    } catch (e) {
      if (isApiError(e) && e.status === 422)
        setError(e.details.label ?? "That label is not valid.");
      else toast({ title: "Could not save the version", tone: "danger" });
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog
      open
      onClose={onClose}
      title="Create Named Version"
      description="Saves a copy of the document as it is now."
    >
      <form onSubmit={(e) => void submit(e)} className="flex flex-col gap-4">
        <Input
          label="Label"
          value={label}
          maxLength={LABEL_MAX * 2}
          onChange={(e) => setLabel(e.target.value)}
          error={error}
          hint={`${[...label].length} of ${LABEL_MAX}`}
          autoComplete="off"
        />
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" loading={busy}>
            Save version
          </Button>
        </div>
      </form>
    </Dialog>
  );
}

/** Frame 21: restoring replaces the content for everyone, after saving a copy of it. */
function RestoreDialog({
  version,
  docId,
  collaborators,
  onClose,
  onRestored,
}: {
  version: VersionSummary;
  docId: string;
  collaborators: number;
  onClose(): void;
  onRestored(restore: VersionSummary, from: VersionSummary): void;
}) {
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);

  async function confirm() {
    setBusy(true);
    setFailed(false);
    try {
      const result = await api.versions.restore(docId, version.version_number);
      onRestored(result.restore_version, version);
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  }

  const others = Math.max(0, collaborators - 1);
  return (
    <Dialog
      open
      onClose={onClose}
      title={`Restore Version ${version.version_number}?`}
      description="The archived blocks will replace the current content for everyone in this document. A copy of the current content is saved first, so you can undo the restore."
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button loading={busy} onClick={() => void confirm()}>
            Confirm &amp; Restore
          </Button>
        </>
      }
    >
      {others > 0 && (
        <Banner
          tone="warning"
          title={`${others} other ${others === 1 ? "person has" : "people have"} this document open.`}
        >
          They will see the restored content at once.
        </Banner>
      )}
      {failed && (
        <p role="alert" className="mt-3 text-ui-sm text-danger">
          Could not restore this version. Nothing was changed.
        </p>
      )}
    </Dialog>
  );
}

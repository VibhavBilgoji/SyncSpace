import type { BadgeTone } from "@/components/ui/Badge";
import type { VersionSummary } from "@/types/generated";

export const BADGE_TONE: Record<VersionSummary["change_type"], BadgeTone> = {
  MANUAL: "indigo",
  RESTORE: "violet",
  AUTOMATIC: "muted",
};

/** `v4 · MANUAL` (frame 18). */
export function versionBadge(v: VersionSummary): string {
  return `v${v.version_number} · ${v.change_type}`;
}

/** The card's headline: the label, with the restore source flagged when it was pruned (frame 24b). */
export function versionTitle(v: VersionSummary): string {
  const base =
    v.label ??
    (v.change_type === "RESTORE" && v.restored_from !== null
      ? `Restored from Version ${v.restored_from}`
      : v.change_type === "AUTOMATIC"
        ? "Periodic Snapshot"
        : "Untitled version");
  return v.change_type === "RESTORE" && v.restored_from_kept === false
    ? `${base} · no longer kept`
    : base;
}

/** `Oct 12, 4:15 PM` in the viewer's time zone. */
export function formatWhen(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return new Intl.DateTimeFormat("en-US", {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(date);
}

/** `time · author`; automatic versions have no author ("Server Snapshot"). */
export function versionMeta(v: VersionSummary): string {
  return `${formatWhen(v.created_at)} · ${v.created_by?.name ?? "Server Snapshot"}`;
}

/** Puts `incoming` into a newest-first list, replacing a version with the same number. */
export function upsertVersion(list: VersionSummary[], incoming: VersionSummary): VersionSummary[] {
  const rest = list.filter((v) => v.version_number !== incoming.version_number);
  return [incoming, ...rest].sort((a, b) => b.version_number - a.version_number);
}

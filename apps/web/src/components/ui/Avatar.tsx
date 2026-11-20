import { cn } from "@/lib/cn";

const palette = [
  "bg-indigo/25 text-indigo",
  "bg-success/25 text-success",
  "bg-warning/25 text-warning",
  "bg-danger/25 text-danger",
  "bg-violet/25 text-violet",
];

function initialOf(name: string): string {
  const first = Array.from(name.trim())[0];
  return first ? first.toUpperCase() : "?";
}

function hashOf(name: string): number {
  let hash = 0;
  for (const ch of name) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  return hash;
}

/** The same name always gets the same colour. */
function colorOf(name: string): string {
  return palette[hashOf(name) % palette.length] ?? "";
}

const cursorColors = [
  "var(--indigo)",
  "var(--success)",
  "var(--warning)",
  "var(--danger)",
  "var(--violet)",
];

/** The design token behind a name's avatar colour, for a remote cursor of the same person. */
export function cursorColorOf(name: string): string {
  return cursorColors[hashOf(name) % cursorColors.length] ?? "var(--indigo)";
}

export type AvatarSize = "sm" | "md";

export function Avatar({
  name,
  size = "md",
  className,
}: {
  name: string;
  size?: AvatarSize;
  className?: string;
}) {
  return (
    <span
      role="img"
      aria-label={name}
      title={name}
      className={cn(
        "inline-flex shrink-0 items-center justify-center rounded-full border border-surface font-semibold",
        size === "sm" ? "size-6 text-micro" : "size-8 text-ui-sm",
        colorOf(name),
        className,
      )}
    >
      {initialOf(name)}
    </span>
  );
}

/** Up to `max` avatars (default 3, as in the header), then "+N". */
export function AvatarStack({ names, max = 3 }: { names: string[]; max?: number }) {
  const shown = names.slice(0, max);
  const extra = names.length - shown.length;
  return (
    <div className="flex items-center -space-x-2">
      {shown.map((name, i) => (
        <Avatar key={`${name}-${i}`} name={name} size="sm" />
      ))}
      {extra > 0 && (
        <span
          role="img"
          aria-label={`${extra} more`}
          className="z-10 inline-flex size-6 items-center justify-center rounded-full border border-surface bg-hover text-micro text-text-muted"
        >
          +{extra}
        </span>
      )}
    </div>
  );
}

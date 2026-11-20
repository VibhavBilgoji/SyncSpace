"use client";

import { useRouter } from "next/navigation";
import { useEffect, useId, useMemo, useState, type KeyboardEvent } from "react";
import { Badge } from "@/components/ui/Badge";
import { Dialog } from "@/components/ui/Dialog";
import { api } from "@/lib/api";
import type { WorkspaceSummary } from "@/types/generated";

interface Item {
  id: string;
  group: "DOCUMENTS" | "WORKSPACE";
  label: string;
  href: string;
  icon?: string;
  admin?: boolean;
}

/** The fixed entries of the WORKSPACE group (`docs/UI_SPEC.md`, screen 17). */
function workspaceItems(workspace: WorkspaceSummary): Item[] {
  const base = `/w/${workspace.slug}`;
  const items: Item[] = [
    { id: "w-docs", group: "WORKSPACE", label: "All documents", href: `${base}/docs` },
    { id: "w-boards", group: "WORKSPACE", label: "All boards", href: `${base}/boards` },
  ];
  if (workspace.role === "ADMIN") {
    items.push({
      id: "w-audit",
      group: "WORKSPACE",
      label: "Audit Trail",
      href: `${base}/audit`,
      admin: true,
    });
  }
  items.push({
    id: "w-members",
    group: "WORKSPACE",
    label: "Settings & Members",
    href: `${base}/settings/members`,
  });
  return items;
}

/**
 * Command palette (FR18, screen 17): filter documents, boards, Settings & Members and, for
 * admins, the Audit Trail, and jump to one. Arrow keys move, Enter opens, Esc closes. Documents
 * and boards are loaded each time it opens, so a new one shows up at once.
 */
export function CommandPalette({
  open,
  onClose,
  workspace,
}: {
  open: boolean;
  onClose: () => void;
  workspace: WorkspaceSummary;
}) {
  return (
    <Dialog open={open} onClose={onClose} title="Search">
      {/* Mounted only while open, so every opening starts with an empty query and a fresh load. */}
      <PaletteBody onClose={onClose} workspace={workspace} />
    </Dialog>
  );
}

function PaletteBody({ onClose, workspace }: { onClose: () => void; workspace: WorkspaceSummary }) {
  const router = useRouter();
  const listId = useId();
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const [loaded, setLoaded] = useState<Item[]>([]);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    Promise.all([api.documents.list(workspace.id), api.boards.list(workspace.id)])
      .then(([documents, boards]) => {
        if (cancelled) return;
        setLoaded([
          ...documents.map((d) => ({
            id: `d-${d.id}`,
            group: "DOCUMENTS" as const,
            label: d.title,
            icon: d.icon,
            href: `/w/${workspace.slug}/docs/${d.id}`,
          })),
          ...boards.map((b) => ({
            id: `b-${b.id}`,
            group: "WORKSPACE" as const,
            label: `Board: ${b.name}`,
            href: `/w/${workspace.slug}/boards/${b.id}`,
          })),
        ]);
      })
      .catch(() => !cancelled && setFailed(true));
    return () => {
      cancelled = true;
    };
  }, [workspace.id, workspace.slug]);

  const results = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const all = [...loaded, ...workspaceItems(workspace)];
    const matching = all.filter((item) => item.label.toLowerCase().includes(needle));
    // DOCUMENTS first, then WORKSPACE, each in its own order.
    return [
      ...matching.filter((i) => i.group === "DOCUMENTS"),
      ...matching.filter((i) => i.group === "WORKSPACE"),
    ];
  }, [loaded, query, workspace]);

  const current = Math.min(active, Math.max(results.length - 1, 0));

  function go(item: Item | undefined) {
    if (!item) return;
    onClose();
    router.push(item.href);
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActive(results.length === 0 ? 0 : (current + 1) % results.length);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActive(results.length === 0 ? 0 : (current - 1 + results.length) % results.length);
    } else if (event.key === "Enter") {
      event.preventDefault();
      go(results[current]);
    }
  }

  let lastGroup = "";
  return (
    <>
      <input
        autoFocus
        type="text"
        role="combobox"
        aria-expanded="true"
        aria-controls={listId}
        aria-activedescendant={results[current] ? `${listId}-${results[current].id}` : undefined}
        aria-label="Search documents, boards and pages"
        placeholder="Search documents, boards and pages..."
        value={query}
        onChange={(event) => {
          setQuery(event.target.value);
          setActive(0);
        }}
        onKeyDown={onKeyDown}
        className="h-9 w-full rounded-control border border-border bg-surface px-3 text-ui-sm placeholder:text-text-faint"
      />
      <ul id={listId} role="listbox" aria-label="Results" className="mt-3 max-h-80 overflow-y-auto">
        {results.map((item, index) => {
          const heading = item.group !== lastGroup ? item.group : null;
          lastGroup = item.group;
          return (
            <li key={item.id} role="presentation">
              {heading && (
                <p className="px-2 pt-2 pb-1 text-label uppercase text-text-faint">{heading}</p>
              )}
              <div
                id={`${listId}-${item.id}`}
                role="option"
                aria-selected={index === current}
                onMouseMove={() => setActive(index)}
                onClick={() => go(item)}
                className={`flex cursor-pointer items-center justify-between gap-2 rounded-control px-2 py-2 text-ui-sm ${
                  index === current ? "bg-hover text-text" : "text-text-muted"
                }`}
              >
                <span className="truncate">
                  {item.icon ? `${item.icon} ` : ""}
                  {item.label}
                </span>
                {item.admin && <Badge tone="success">Admin</Badge>}
              </div>
            </li>
          );
        })}
      </ul>
      {results.length === 0 && (
        <p className="px-2 py-4 text-ui-sm text-text-muted">
          No results for &ldquo;{query}&rdquo;.
        </p>
      )}
      {failed && (
        <p role="alert" className="px-2 pt-2 text-ui-sm text-text-muted">
          Documents and boards could not be loaded; pages are still available.
        </p>
      )}
      <p className="mt-3 border-t border-border pt-2 text-ui-sm text-text-faint">
        ↑↓ navigate · Enter open · Esc close
      </p>
    </>
  );
}

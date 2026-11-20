"use client";

import { useEffect, useState } from "react";
import { HeaderSlot } from "@/lib/headerSlot";
import { CommandPalette } from "./CommandPalette";
import { useWorkspace } from "./WorkspaceShell";

/** 52 px header: breadcrumb and the search field, which opens the command palette (Ctrl/Cmd + K). */
export function AppHeader({ section }: { section?: string }) {
  const workspace = useWorkspace();
  const [paletteOpen, setPaletteOpen] = useState(false);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen(true);
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, []);

  return (
    <header className="flex h-[52px] items-center gap-4 border-b border-border px-6">
      <nav aria-label="Breadcrumb" className="min-w-0 flex-1 truncate text-ui-sm text-text-muted">
        <span>{workspace.name}</span>
        {section && (
          <>
            {" / "}
            <span className="text-text">{section}</span>
          </>
        )}
      </nav>
      <button
        type="button"
        aria-label="Search or jump to"
        aria-haspopup="dialog"
        onClick={() => setPaletteOpen(true)}
        className="flex h-8 w-64 items-center rounded-control border border-border bg-surface px-3 text-left text-ui-sm text-text-faint hover:bg-hover"
      >
        Search or jump to...&nbsp; Ctrl K
      </button>
      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        workspace={workspace}
      />
      <HeaderSlot />
    </header>
  );
}

"use client";

import { useWorkspace } from "./WorkspaceShell";

/** 52 px header: breadcrumb and the search field (the command palette is task 3.5). */
export function AppHeader({ section }: { section?: string }) {
  const workspace = useWorkspace();
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
      <input
        type="search"
        disabled
        aria-label="Search or jump to"
        placeholder="Search or jump to...  Ctrl K"
        className="h-8 w-64 rounded-control border border-border bg-surface px-3 text-ui-sm placeholder:text-text-faint"
      />
    </header>
  );
}

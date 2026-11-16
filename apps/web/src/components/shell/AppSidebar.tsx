"use client";

import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { Avatar } from "@/components/ui/Avatar";
import { Badge } from "@/components/ui/Badge";
import { NavItem } from "@/components/ui/NavItem";
import { useSession } from "@/lib/session";
import { ConnectionPill } from "./ConnectionPill";
import { useWorkspace } from "./WorkspaceShell";

function SectionLabel({ children }: { children: string }) {
  return <h2 className="px-2 pt-4 pb-1 text-micro text-text-faint uppercase">{children}</h2>;
}

/** Frame 05 onwards: 240 px sidebar. Boards and the document list are filled by later tasks. */
export function AppSidebar() {
  const workspace = useWorkspace();
  const { session, signOut } = useSession();
  const router = useRouter();
  const pathname = usePathname();
  const base = `/w/${workspace.slug}`;
  const isAdmin = workspace.role === "ADMIN";

  async function onSignOut() {
    await signOut();
    router.replace("/login");
  }

  return (
    <aside className="flex w-60 shrink-0 flex-col border-r border-border bg-surface p-3">
      <Link
        href="/w"
        aria-label="Switch workspace"
        className="flex items-center gap-2 rounded-control p-2 hover:bg-hover"
      >
        <span
          aria-hidden="true"
          className="flex size-8 items-center justify-center rounded-control bg-indigo text-ui-base"
        >
          {Array.from(workspace.name)[0]?.toUpperCase()}
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate text-ui-base">{workspace.name}</span>
          <span className="block text-micro text-text-muted uppercase">{workspace.role}</span>
        </span>
        <span aria-hidden="true" className="text-text-muted">
          ⌄
        </span>
      </Link>

      <nav aria-label="Workspace" className="flex-1">
        <SectionLabel>Documents</SectionLabel>
        <NavItem href={`${base}/docs`} active={pathname.startsWith(`${base}/docs`)}>
          All documents
        </NavItem>

        <SectionLabel>Boards</SectionLabel>
        <p className="px-2 text-ui-sm text-text-faint">No boards yet</p>

        <SectionLabel>System</SectionLabel>
        {isAdmin && (
          <NavItem
            href={`${base}/audit`}
            active={pathname.startsWith(`${base}/audit`)}
            trailing={<Badge tone="success">Admin</Badge>}
          >
            Audit Trail
          </NavItem>
        )}
        <NavItem href={`${base}/settings`} active={pathname.startsWith(`${base}/settings`)}>
          Settings &amp; Members
        </NavItem>
      </nav>

      <div className="flex flex-col gap-3">
        <ConnectionPill />
        {session.status === "authed" && (
          <div className="flex items-center gap-2">
            <Avatar name={session.user.name} size="sm" />
            <span className="min-w-0 flex-1 truncate text-ui-sm">{session.user.name}</span>
            <button
              type="button"
              onClick={onSignOut}
              className="rounded-control px-2 py-1 text-ui-sm text-text-muted hover:bg-hover hover:text-text"
            >
              Sign out
            </button>
          </div>
        )}
      </div>
    </aside>
  );
}

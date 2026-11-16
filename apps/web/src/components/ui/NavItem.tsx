import Link from "next/link";
import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

export interface NavItemProps {
  href: string;
  children: ReactNode;
  icon?: ReactNode;
  /** Right-hand slot, for example an ADMIN badge. */
  trailing?: ReactNode;
  active?: boolean;
}

export function NavItem({ href, children, icon, trailing, active = false }: NavItemProps) {
  return (
    <Link
      href={href}
      aria-current={active ? "page" : undefined}
      className={cn(
        "flex h-8 items-center gap-2 rounded-control px-2 text-ui-base",
        active ? "bg-hover text-text" : "text-text-muted hover:bg-hover hover:text-text",
      )}
    >
      {icon && <span aria-hidden="true">{icon}</span>}
      <span className="min-w-0 flex-1 truncate">{children}</span>
      {trailing}
    </Link>
  );
}

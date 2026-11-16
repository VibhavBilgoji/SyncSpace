import type { ReactNode } from "react";

/** Centered card used by sign in, create account and onboarding (frames 01 to 04). */
export function AuthCard({
  title,
  subtitle,
  children,
  footer,
}: {
  title: string;
  subtitle?: string;
  children: ReactNode;
  footer?: ReactNode;
}) {
  return (
    <main className="flex min-h-screen items-center justify-center p-4">
      <div className="w-full max-w-sm rounded-dialog border border-border bg-surface p-8">
        <h1 className="text-heading-lg text-text">{title}</h1>
        {subtitle && <p className="mt-1 text-ui-sm text-text-muted">{subtitle}</p>}
        <div className="mt-6">{children}</div>
        {footer && <div className="mt-6 text-center text-ui-sm text-text-muted">{footer}</div>}
      </div>
    </main>
  );
}

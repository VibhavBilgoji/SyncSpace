"use client";

import { useEffect, useState } from "react";
import { api } from "@/lib/api";

/** Where the browser goes to start signing in with GitHub (through the web domain). */
export function githubStartUrl(next?: string): string {
  const base = "/api/v1/auth/github/start";
  return next ? `${base}?next=${encodeURIComponent(next)}` : base;
}

/**
 * "Continue with GitHub", shown only when the server says it is set up (`GET /auth/providers`).
 * It is a plain link, because the sign-in is a full-page trip to GitHub and back.
 */
export function GithubButton({ next }: { next?: string }) {
  const [available, setAvailable] = useState(false);

  useEffect(() => {
    let cancelled = false;
    api.auth
      .providers()
      .then((providers) => !cancelled && setAvailable(providers.github))
      .catch(() => undefined); // No button is better than a broken one.
    return () => {
      cancelled = true;
    };
  }, []);

  if (!available) return null;
  return (
    <>
      <a
        href={githubStartUrl(next)}
        className="inline-flex h-9 items-center justify-center gap-2 rounded-control border border-border bg-elevated px-4 text-ui-base text-text transition hover:bg-hover"
      >
        <svg aria-hidden="true" viewBox="0 0 16 16" className="size-4 fill-current">
          <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27s1.36.09 2 .27c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z" />
        </svg>
        Continue with GitHub
      </a>
      <p aria-hidden="true" className="text-center text-ui-sm text-text-faint">
        or use your email
      </p>
    </>
  );
}

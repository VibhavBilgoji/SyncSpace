"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { githubStartUrl } from "@/components/auth/GithubButton";
import { useWorkspace } from "@/components/shell/WorkspaceShell";
import { Badge } from "@/components/ui/Badge";
import { Banner } from "@/components/ui/Banner";
import { Button } from "@/components/ui/Button";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { api } from "@/lib/api";
import { MAX_PAGES, RepoError, fetchPublicRepos, type RepoList } from "@/lib/github";
import { useSession } from "@/lib/session";

type State =
  | { status: "loading" }
  | { status: "ready"; list: RepoList }
  | { status: "error"; kind: RepoError["kind"] };

const ERRORS: Record<RepoError["kind"], string> = {
  not_found: "GitHub does not know this account any more. It may have been renamed or deleted.",
  rate_limited:
    "GitHub limits how often repositories can be listed without signing in. Try again in a few minutes.",
  failed: "Could not load the repositories from GitHub. Check your connection and try again.",
};

function formatDate(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return new Intl.DateTimeFormat("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
  }).format(date);
}

/** The public repositories of the signed-in person's linked GitHub account (read only). */
export function RepositoriesPage() {
  const workspace = useWorkspace();
  const { session } = useSession();
  const login = session.status === "authed" ? session.user.github_login : null;

  return (
    <div className="mx-auto w-full max-w-[760px] py-6">
      <h1 className="text-display-xl">Repositories</h1>
      {login ? <Linked login={login} /> : <NotLinked next={`/w/${workspace.slug}/repos`} />}
    </div>
  );
}

function NotLinked({ next }: { next: string }) {
  const [available, setAvailable] = useState<boolean | null>(null);
  useEffect(() => {
    let cancelled = false;
    api.auth
      .providers()
      .then((p) => !cancelled && setAvailable(p.github))
      .catch(() => !cancelled && setAvailable(false));
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="mt-6">
      <EmptyState
        title="Connect your GitHub account"
        description={
          available === false
            ? "Sign in with GitHub is not set up on this server yet."
            : "Link GitHub to list your public repositories here. SyncSpace asks only for your identity and verified email, and never stores a GitHub token."
        }
        action={
          available ? (
            <a
              href={githubStartUrl(next)}
              className="inline-flex h-9 items-center justify-center rounded-control bg-indigo px-4 text-ui-base text-text transition hover:brightness-110"
            >
              Connect GitHub
            </a>
          ) : undefined
        }
      />
    </div>
  );
}

function Linked({ login }: { login: string }) {
  const [state, setState] = useState<State>({ status: "loading" });
  const [filter, setFilter] = useState("");
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    const controller = new AbortController();
    fetchPublicRepos(login, controller.signal)
      .then((list) => setState({ status: "ready", list }))
      .catch((error: unknown) => {
        if (controller.signal.aborted) return;
        setState({ status: "error", kind: error instanceof RepoError ? error.kind : "failed" });
      });
    return () => controller.abort();
  }, [login, attempt]);

  const retry = useCallback(() => {
    setState({ status: "loading" });
    setAttempt((n) => n + 1);
  }, []);

  const shown = useMemo(() => {
    if (state.status !== "ready") return [];
    const needle = filter.trim().toLowerCase();
    return state.list.repos.filter(
      (r) =>
        needle === "" ||
        r.name.toLowerCase().includes(needle) ||
        (r.description ?? "").toLowerCase().includes(needle) ||
        (r.language ?? "").toLowerCase().includes(needle),
    );
  }, [state, filter]);

  const total = state.status === "ready" ? state.list.repos.length : 0;

  return (
    <>
      <p className="mt-2 text-ui-sm text-text-muted">
        Public repositories of{" "}
        <a
          href={`https://github.com/${login}`}
          target="_blank"
          rel="noopener noreferrer"
          className="text-indigo hover:underline"
        >
          @{login}
        </a>
        {state.status === "ready" && ` · ${total} found`}
      </p>

      {state.status === "loading" && (
        <ul aria-busy="true" aria-label="Loading repositories" className="mt-6 flex flex-col gap-3">
          {[0, 1, 2].map((n) => (
            <li key={n} className="h-20 animate-pulse rounded-card bg-surface" />
          ))}
        </ul>
      )}

      {state.status === "error" && (
        <div className="mt-6">
          <Banner
            tone="danger"
            title="Repositories not loaded"
            action={
              state.kind === "not_found" ? undefined : (
                <Button variant="secondary" onClick={retry}>
                  Try again
                </Button>
              )
            }
          >
            {ERRORS[state.kind]}
          </Banner>
        </div>
      )}

      {state.status === "ready" && (
        <>
          {total > 0 && (
            <div className="mt-6 max-w-sm">
              <Input
                label="Filter"
                type="search"
                placeholder="Name, description or language"
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
              />
            </div>
          )}
          {total === 0 ? (
            <div className="mt-6">
              <EmptyState
                title="No public repositories"
                description="This GitHub account has no public repositories yet."
              />
            </div>
          ) : shown.length === 0 ? (
            <p className="mt-6 text-ui-sm text-text-muted">No repository matches “{filter}”.</p>
          ) : (
            <ul aria-label="Repositories" className="mt-4 flex flex-col gap-3">
              {shown.map((repo) => (
                <li
                  key={repo.id}
                  className="rounded-card border border-border bg-surface px-4 py-3"
                >
                  <div className="flex flex-wrap items-center gap-2">
                    <a
                      href={repo.url}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-heading-md text-indigo hover:underline"
                    >
                      {repo.name}
                    </a>
                    {repo.fork && <Badge tone="muted">Fork</Badge>}
                    {repo.archived && <Badge tone="warning">Archived</Badge>}
                  </div>
                  {repo.description && (
                    <p className="mt-1 text-ui-sm text-text-muted">{repo.description}</p>
                  )}
                  <p className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-ui-sm text-text-faint">
                    {repo.language && <span>{repo.language}</span>}
                    <span aria-label={`${repo.stars} stars`}>★ {repo.stars}</span>
                    <span aria-label={`${repo.forks} forks`}>⑂ {repo.forks}</span>
                    {repo.updatedAt && <span>Updated {formatDate(repo.updatedAt)}</span>}
                  </p>
                </li>
              ))}
            </ul>
          )}
          {state.list.truncated && (
            <p className="mt-4 text-ui-sm text-text-muted">
              Showing the first {MAX_PAGES * 100} repositories.{" "}
              <a
                href={`https://github.com/${login}?tab=repositories`}
                target="_blank"
                rel="noopener noreferrer"
                className="text-indigo hover:underline"
              >
                See all on GitHub
              </a>
            </p>
          )}
        </>
      )}
    </>
  );
}

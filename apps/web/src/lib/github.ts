/**
 * Public repositories of a GitHub user, read straight from GitHub's REST API by the browser.
 * No token is needed for public data and none is ever sent: SyncSpace does not keep one (ADR-015).
 * The login comes from our own API, and everything GitHub returns is treated as untrusted.
 */

/** Overridable at build time for tests; GitHub's API in production. */
export const GITHUB_API = (
  process.env.NEXT_PUBLIC_GITHUB_API_URL ?? "https://api.github.com"
).replace(/\/+$/, "");

const PER_PAGE = 100;
/** At most 500 repositories (five requests); the page says when there are more. */
export const MAX_PAGES = 5;

export interface Repo {
  id: number;
  name: string;
  url: string;
  description: string | null;
  language: string | null;
  stars: number;
  forks: number;
  updatedAt: string;
  fork: boolean;
  archived: boolean;
}

export interface RepoList {
  repos: Repo[];
  /** True when GitHub has more than the pages fetched. */
  truncated: boolean;
}

export type RepoErrorKind = "not_found" | "rate_limited" | "failed";

export class RepoError extends Error {
  constructor(readonly kind: RepoErrorKind) {
    super(kind);
  }
}

/** A GitHub username: letters, digits and single hyphens, at most 39 (also checked by the server). */
export function isGithubLogin(value: string): boolean {
  return /^[A-Za-z0-9-]{1,39}$/.test(value);
}

function text(value: unknown, max: number): string | null {
  return typeof value === "string" && value.trim() !== "" ? value.slice(0, max) : null;
}

function count(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? Math.floor(value) : 0;
}

/** Only web links are followed from the list: never `javascript:` or `data:` from a response. */
function webUrl(value: unknown): string | null {
  if (typeof value !== "string") return null;
  try {
    const url = new URL(value);
    return url.protocol === "https:" || url.protocol === "http:" ? url.href : null;
  } catch {
    return null;
  }
}

/** One repository from GitHub's JSON, or null when it does not have what the list needs. */
export function parseRepo(raw: unknown): Repo | null {
  if (typeof raw !== "object" || raw === null) return null;
  const r = raw as Record<string, unknown>;
  const name = text(r.name, 100);
  const url = webUrl(r.html_url);
  const id = typeof r.id === "number" && Number.isFinite(r.id) ? r.id : null;
  if (name === null || url === null || id === null) return null;
  return {
    id,
    name,
    url,
    description: text(r.description, 300),
    language: text(r.language, 40),
    stars: count(r.stargazers_count),
    forks: count(r.forks_count),
    updatedAt: text(r.updated_at, 40) ?? "",
    fork: r.fork === true,
    archived: r.archived === true,
  };
}

/** Every public repository of `login`, newest activity first. Throws [`RepoError`]. */
export async function fetchPublicRepos(login: string, signal?: AbortSignal): Promise<RepoList> {
  if (!isGithubLogin(login)) throw new RepoError("not_found");
  const repos: Repo[] = [];
  let truncated = false;
  for (let page = 1; page <= MAX_PAGES; page += 1) {
    let response: Response;
    try {
      response = await fetch(
        `${GITHUB_API}/users/${login}/repos?per_page=${PER_PAGE}&sort=updated&page=${page}`,
        { signal, headers: { Accept: "application/vnd.github+json" } },
      );
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") throw error;
      throw new RepoError("failed");
    }
    if (response.status === 404) throw new RepoError("not_found");
    if (
      response.status === 429 ||
      (response.status === 403 && response.headers.get("x-ratelimit-remaining") === "0")
    ) {
      throw new RepoError("rate_limited");
    }
    if (!response.ok) throw new RepoError("failed");
    let body: unknown;
    try {
      body = await response.json();
    } catch {
      throw new RepoError("failed");
    }
    if (!Array.isArray(body)) throw new RepoError("failed");
    for (const item of body) {
      const repo = parseRepo(item);
      if (repo) repos.push(repo);
    }
    if (body.length < PER_PAGE) return { repos, truncated: false };
    truncated = page === MAX_PAGES;
  }
  return { repos, truncated };
}

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  GITHUB_API,
  MAX_PAGES,
  RepoError,
  fetchPublicRepos,
  isGithubLogin,
  parseRepo,
} from "./github";

const raw = (n: number, extra: Record<string, unknown> = {}) => ({
  id: n,
  name: `repo-${n}`,
  html_url: `https://github.com/octocat/repo-${n}`,
  description: `Repo number ${n}`,
  language: "Rust",
  stargazers_count: n,
  forks_count: 1,
  updated_at: "2026-11-20T10:00:00Z",
  fork: false,
  archived: false,
  ...extra,
});

function respond(body: unknown, init: { status?: number; headers?: Record<string, string> } = {}) {
  return Promise.resolve(
    new Response(JSON.stringify(body), {
      status: init.status ?? 200,
      headers: init.headers,
    }),
  );
}

const fetchMock = vi.fn();
beforeEach(() => {
  fetchMock.mockReset();
  vi.stubGlobal("fetch", fetchMock);
});
afterEach(() => vi.unstubAllGlobals());

describe("isGithubLogin", () => {
  it("accepts GitHub usernames and nothing that could change a URL", () => {
    for (const good of ["octocat", "a", "Some-One-1", "a".repeat(39)]) {
      expect(isGithubLogin(good)).toBe(true);
    }
    for (const bad of ["", "a b", "a/b", "../x", "a?b=1", "a#b", "é", "a_b", "a".repeat(40)]) {
      expect(isGithubLogin(bad)).toBe(false);
    }
  });
});

describe("parseRepo", () => {
  it("reads the fields the list shows", () => {
    expect(parseRepo(raw(3, { fork: true, archived: true }))).toEqual({
      id: 3,
      name: "repo-3",
      url: "https://github.com/octocat/repo-3",
      description: "Repo number 3",
      language: "Rust",
      stars: 3,
      forks: 1,
      updatedAt: "2026-11-20T10:00:00Z",
      fork: true,
      archived: true,
    });
  });

  it("copes with missing or null optional fields", () => {
    const repo = parseRepo({ id: 1, name: "bare", html_url: "https://github.com/o/bare" });
    expect(repo).toMatchObject({
      description: null,
      language: null,
      stars: 0,
      forks: 0,
      updatedAt: "",
      fork: false,
      archived: false,
    });
  });

  it("drops anything that is not a repository with a web link", () => {
    for (const bad of [
      null,
      "x",
      42,
      [],
      {},
      raw(1, { name: "" }),
      raw(1, { name: 5 }),
      raw(1, { id: "1" }),
      raw(1, { html_url: "javascript:alert(1)" }),
      raw(1, { html_url: "data:text/html,<script>1</script>" }),
      raw(1, { html_url: "not a url" }),
      raw(1, { html_url: null }),
    ]) {
      expect(parseRepo(bad), JSON.stringify(bad)).toBeNull();
    }
  });

  it("clamps counts and lengths", () => {
    const repo = parseRepo(
      raw(1, {
        stargazers_count: -5,
        forks_count: "many",
        description: "d".repeat(1000),
        name: "n".repeat(500),
      }),
    )!;
    expect(repo.stars).toBe(0);
    expect(repo.forks).toBe(0);
    expect(repo.description).toHaveLength(300);
    expect(repo.name).toHaveLength(100);
  });
});

describe("fetchPublicRepos", () => {
  it("asks GitHub for the user's public repositories, newest first, without any credential", async () => {
    fetchMock.mockReturnValueOnce(respond([raw(1), raw(2)]));
    const list = await fetchPublicRepos("octocat");
    expect(list.repos.map((r) => r.name)).toEqual(["repo-1", "repo-2"]);
    expect(list.truncated).toBe(false);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, options] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`${GITHUB_API}/users/octocat/repos?per_page=100&sort=updated&page=1`);
    const headers = new Headers(options.headers);
    expect(headers.get("authorization")).toBeNull();
    expect(options.credentials).toBeUndefined();
  });

  it("follows full pages and stops at the first short one", async () => {
    const full = Array.from({ length: 100 }, (_, i) => raw(i + 1));
    fetchMock.mockReturnValueOnce(respond(full)).mockReturnValueOnce(respond([raw(101)]));
    const list = await fetchPublicRepos("octocat");
    expect(list.repos).toHaveLength(101);
    expect(list.truncated).toBe(false);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(fetchMock.mock.calls[1]?.[0]).toContain("page=2");
  });

  it("stops after the page limit and says there may be more", async () => {
    fetchMock.mockImplementation(() => respond(Array.from({ length: 100 }, (_, i) => raw(i + 1))));
    const list = await fetchPublicRepos("octocat");
    expect(fetchMock).toHaveBeenCalledTimes(MAX_PAGES);
    expect(list.truncated).toBe(true);
  });

  it("skips entries it cannot use and keeps the rest", async () => {
    fetchMock.mockReturnValueOnce(
      respond([raw(1), { id: 2, name: "x", html_url: "javascript:1" }, null, raw(4)]),
    );
    const list = await fetchPublicRepos("octocat");
    expect(list.repos.map((r) => r.id)).toEqual([1, 4]);
  });

  it("never calls GitHub for something that is not a username", async () => {
    await expect(fetchPublicRepos("../orgs/evil")).rejects.toMatchObject({ kind: "not_found" });
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it.each([
    [404, {}, "not_found"],
    [429, {}, "rate_limited"],
    [403, { "x-ratelimit-remaining": "0" }, "rate_limited"],
    [403, {}, "failed"],
    [500, {}, "failed"],
    [503, {}, "failed"],
  ])("a %s answer (%j) is %s", async (status, headers, kind) => {
    fetchMock.mockReturnValueOnce(respond({ message: "x" }, { status, headers }));
    await expect(fetchPublicRepos("octocat")).rejects.toMatchObject({ kind });
  });

  it("treats a network error, a non-JSON body and a non-list as a failure", async () => {
    fetchMock.mockRejectedValueOnce(new TypeError("offline"));
    await expect(fetchPublicRepos("octocat")).rejects.toBeInstanceOf(RepoError);
    fetchMock.mockReturnValueOnce(Promise.resolve(new Response("<html>", { status: 200 })));
    await expect(fetchPublicRepos("octocat")).rejects.toMatchObject({ kind: "failed" });
    fetchMock.mockReturnValueOnce(respond({ message: "not a list" }));
    await expect(fetchPublicRepos("octocat")).rejects.toMatchObject({ kind: "failed" });
  });

  it("lets an abort through unchanged, so a left page is not shown an error", async () => {
    const controller = new AbortController();
    fetchMock.mockRejectedValueOnce(new DOMException("aborted", "AbortError"));
    controller.abort();
    await expect(fetchPublicRepos("octocat", controller.signal)).rejects.toMatchObject({
      name: "AbortError",
    });
  });
});

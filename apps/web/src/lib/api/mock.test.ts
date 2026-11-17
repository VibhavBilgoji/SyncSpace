import { afterEach, describe, expect, it, vi } from "vitest";
import { buildApi } from "./index";
import { createMockApi } from "./mock";
import { realAuth, realVersions, realWorkspaces } from "./real";

describe("mock auth", () => {
  it("registers, normalizes the email and starts a session", async () => {
    const { auth } = createMockApi();
    await expect(auth.me()).rejects.toMatchObject({ status: 401 });
    const user = await auth.register({
      name: " Ada ",
      email: "Ada@Example.com",
      password: "long enough",
    });
    expect(user).toMatchObject({ name: "Ada", email: "ada@example.com" });
    expect(user).not.toHaveProperty("password");
    await expect(auth.me()).resolves.toEqual(user);
  });

  it("rejects a taken email with 409 and bad fields with 422 details", async () => {
    const { auth } = createMockApi();
    await auth.register({ name: "Ada", email: "ada@example.com", password: "long enough" });
    await expect(
      auth.register({ name: "Ada 2", email: "ADA@example.com", password: "long enough" }),
    ).rejects.toMatchObject({ status: 409 });
    await expect(
      auth.register({ name: "", email: "nope", password: "short" }),
    ).rejects.toMatchObject({
      status: 422,
      details: expect.objectContaining({
        name: expect.any(String),
        email: expect.any(String),
        password: expect.any(String),
      }),
    });
  });

  it("logs in with the same error for a wrong password and an unknown email, and logs out", async () => {
    const { auth } = createMockApi();
    await auth.register({ name: "Ada", email: "ada@example.com", password: "long enough" });
    await auth.logout();
    const wrong = await auth
      .login({ email: "ada@example.com", password: "nope nope" })
      .catch((e: Error) => e);
    const unknown = await auth
      .login({ email: "who@example.com", password: "nope nope" })
      .catch((e: Error) => e);
    expect(wrong).toMatchObject({ status: 401, message: "invalid email or password" });
    expect(unknown).toMatchObject({ status: 401, message: "invalid email or password" });
    await expect(
      auth.login({ email: "ada@example.com", password: "long enough" }),
    ).resolves.toBeDefined();
    await auth.logout();
    await expect(auth.me()).rejects.toMatchObject({ status: 401 });
  });
});

describe("mock workspaces", () => {
  it("needs a session", async () => {
    const { workspaces } = createMockApi();
    await expect(workspaces.list()).rejects.toMatchObject({ status: 401 });
    await expect(workspaces.create({ name: "Team", slug: "team" })).rejects.toMatchObject({
      status: 401,
    });
  });

  it("creates an owned ADMIN workspace and lists it by name", async () => {
    const { auth, workspaces } = createMockApi();
    const user = await auth.register({
      name: "Ada",
      email: "ada@example.com",
      password: "long enough",
    });
    const zeta = await workspaces.create({ name: "Zeta", slug: "zeta" });
    await workspaces.create({ name: "Alpha", slug: "alpha" });
    expect(zeta).toMatchObject({ owner_id: user.id, role: "ADMIN", slug: "zeta" });
    const list = await workspaces.list();
    expect(list.map((w) => w.name)).toEqual(["Alpha", "Zeta"]);
    expect(list[0]).toMatchObject({ role: "ADMIN", is_owner: true, member_count: 1 });
  });

  it("gives 409 for a taken slug and 422 for a slug that breaks the rule", async () => {
    const { auth, workspaces } = createMockApi();
    await auth.register({ name: "Ada", email: "ada@example.com", password: "long enough" });
    await workspaces.create({ name: "Team", slug: "team" });
    await expect(workspaces.create({ name: "Other", slug: "team" })).rejects.toMatchObject({
      status: 409,
    });
    await expect(workspaces.create({ name: "Team", slug: "Team" })).rejects.toMatchObject({
      status: 422,
      details: { slug: expect.any(String) },
    });
  });
});

describe("buildApi", () => {
  it("uses the real implementation for every group by default", () => {
    const api = buildApi("");
    expect(api.auth).toBe(realAuth);
    expect(api.workspaces).toBe(realWorkspaces);
  });

  it("swaps only the listed groups to the mock", () => {
    const api = buildApi("workspaces");
    expect(api.auth).toBe(realAuth);
    expect(api.workspaces).not.toBe(realWorkspaces);
  });
});

describe("mock documents", () => {
  async function ready() {
    const api = createMockApi();
    await api.auth.register({ name: "Ada", email: "ada@example.com", password: "password1" });
    const workspace = await api.workspaces.create({ name: "Eng", slug: "eng" });
    return { api, workspace };
  }

  it("creates, lists newest first, gets and removes", async () => {
    const { api, workspace } = await ready();
    const first = await api.documents.create(workspace.id, { title: " One " });
    await new Promise((r) => setTimeout(r, 2));
    const second = await api.documents.create(workspace.id, { title: "Two", icon: "🚀" });
    expect(first.title).toBe("One");
    expect(first.icon).toBe("📄");
    expect((await api.documents.list(workspace.id)).map((d) => d.title)).toEqual(["Two", "One"]);
    expect((await api.documents.get(second.id)).icon).toBe("🚀");
    await api.documents.remove(first.id);
    expect(await api.documents.list(workspace.id)).toHaveLength(1);
  });

  it("validates like the server", async () => {
    const { api, workspace } = await ready();
    await expect(api.documents.create(workspace.id, { title: "  " })).rejects.toMatchObject({
      status: 422,
    });
    await expect(
      api.documents.create(workspace.id, { title: "x", icon: "y".repeat(17) }),
    ).rejects.toMatchObject({ status: 422 });
    await expect(api.documents.create("0".repeat(24), { title: "x" })).rejects.toMatchObject({
      status: 404,
    });
  });
});

describe("mock versions", () => {
  async function ready() {
    const api = createMockApi();
    await api.auth.register({ name: "Ada", email: "ada@example.com", password: "password1" });
    return api;
  }

  it("needs a session", async () => {
    const api = createMockApi();
    await expect(api.versions.list("d")).rejects.toMatchObject({ status: 401 });
    await expect(api.versions.create("d", { label: "x" })).rejects.toMatchObject({ status: 401 });
  });

  it("saves named versions, lists them newest first and numbers them from 1", async () => {
    const api = await ready();
    const one = await api.versions.create("d", { label: "  First  " });
    const two = await api.versions.create("d", { label: "Second" });
    expect([one.version_number, two.version_number]).toEqual([1, 2]);
    expect(one).toMatchObject({ change_type: "MANUAL", label: "First" });
    const page = await api.versions.list("d");
    expect(page.items.map((v) => v.version_number)).toEqual([2, 1]);
    expect(page.next_cursor).toBeNull();
    expect((await api.versions.list("other")).items).toEqual([]);
  });

  it("rejects a blank or over-long label with 422 details", async () => {
    const api = await ready();
    await expect(api.versions.create("d", { label: "   " })).rejects.toMatchObject({
      status: 422,
      details: { label: expect.any(String) },
    });
    await expect(api.versions.create("d", { label: "x".repeat(81) })).rejects.toMatchObject({
      status: 422,
    });
  });

  it("restores into a new RESTORE version and refuses an unknown number", async () => {
    const api = await ready();
    await api.versions.create("d", { label: "First" });
    const { restore_version } = await api.versions.restore("d", 1);
    expect(restore_version).toMatchObject({
      version_number: 2,
      change_type: "RESTORE",
      restored_from: 1,
      label: "Restored from Version 1",
    });
    await expect(api.versions.restore("d", 9)).rejects.toMatchObject({ status: 404 });
    await expect(api.versions.get("d", 9)).rejects.toMatchObject({ status: 404 });
    expect((await api.versions.get("d", 1)).version.version_number).toBe(1);
  });

  it("is picked per group by buildApi", () => {
    expect(buildApi("").versions).toBe(realVersions);
    expect(buildApi("versions").versions).not.toBe(realVersions);
  });
});

describe("real versions", () => {
  afterEach(() => vi.unstubAllGlobals());

  function stub(body: unknown, status = 200) {
    const fn = vi.fn(() => Promise.resolve(Response.json(body, { status })));
    vi.stubGlobal("fetch", fn);
    return fn;
  }

  it("lists with the limit and cursor in the query, and without a query by default", async () => {
    const fn = stub({ items: [], next_cursor: null });
    await realVersions.list("d1");
    await realVersions.list("d1", { limit: 50, cursor: "7" });
    expect(fn.mock.calls.map((c) => (c as unknown[])[0])).toEqual([
      "/api/v1/documents/d1/versions",
      "/api/v1/documents/d1/versions?limit=50&cursor=7",
    ]);
  });

  it("posts the label, reads one version and restores one", async () => {
    const fn = stub({});
    await realVersions.create("d1", { label: "Draft" });
    await realVersions.get("d1", 3);
    await realVersions.restore("d1", 3);
    const calls = fn.mock.calls as unknown as Array<[string, RequestInit]>;
    expect(calls[0]![0]).toBe("/api/v1/documents/d1/versions");
    expect(calls[0]![1]).toMatchObject({
      method: "POST",
      body: JSON.stringify({ label: "Draft" }),
    });
    expect(calls[1]![0]).toBe("/api/v1/documents/d1/versions/3");
    expect(calls[1]![1]).toMatchObject({ method: "GET" });
    expect(calls[2]![0]).toBe("/api/v1/documents/d1/versions/3/restore");
    expect(calls[2]![1]).toMatchObject({ method: "POST" });
  });
});

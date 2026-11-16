import { describe, expect, it } from "vitest";
import { buildApi } from "./index";
import { createMockApi } from "./mock";
import { realAuth, realWorkspaces } from "./real";

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

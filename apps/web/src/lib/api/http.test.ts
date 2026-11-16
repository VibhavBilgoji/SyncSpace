import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./errors";
import { request } from "./http";

function stubFetch(response: Response | Error) {
  const fn = vi.fn(() =>
    response instanceof Error ? Promise.reject(response) : Promise.resolve(response),
  );
  vi.stubGlobal("fetch", fn);
  return fn;
}

afterEach(() => vi.unstubAllGlobals());

describe("request", () => {
  it("sends JSON to /api/v1 with the first-party cookie and returns the body", async () => {
    const fetchMock = stubFetch(Response.json({ id: "1" }, { status: 201 }));
    const result = await request<{ id: string }>("POST", "/auth/login", { email: "a@b.co" });
    expect(result).toEqual({ id: "1" });
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/auth/login",
      expect.objectContaining({
        method: "POST",
        credentials: "same-origin",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email: "a@b.co" }),
      }),
    );
  });

  it("sends no body or content type on a GET", async () => {
    const fetchMock = stubFetch(Response.json([]));
    await request("GET", "/workspaces");
    const init = (fetchMock.mock.calls[0] as unknown as [string, RequestInit])[1];
    expect(init.body).toBeUndefined();
    expect(init.headers).toBeUndefined();
  });

  it("returns undefined for 204", async () => {
    stubFetch(new Response(null, { status: 204 }));
    await expect(request<void>("POST", "/auth/logout")).resolves.toBeUndefined();
  });

  it("turns the error envelope into an ApiError with details", async () => {
    stubFetch(
      Response.json(
        { error: { code: "VALIDATION", message: "bad", details: { email: "invalid" } } },
        { status: 422 },
      ),
    );
    const error = await request("POST", "/auth/register", {}).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ApiError);
    expect(error).toMatchObject({
      status: 422,
      code: "VALIDATION",
      message: "bad",
      details: { email: "invalid" },
    });
  });

  it("copes with an error body that is not JSON", async () => {
    stubFetch(new Response("<html>bad gateway</html>", { status: 502 }));
    await expect(request("GET", "/workspaces")).rejects.toMatchObject({
      status: 502,
      code: "UNKNOWN",
      details: {},
    });
  });

  it("reports an unreachable server as status 0", async () => {
    stubFetch(new TypeError("failed to fetch"));
    await expect(request("GET", "/workspaces")).rejects.toMatchObject({
      status: 0,
      code: "NETWORK",
    });
  });
});

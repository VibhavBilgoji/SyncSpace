import { NextRequest } from "next/server";
import { describe, expect, it } from "vitest";
import { config, proxy } from "./proxy";

function visit(cookie?: string) {
  const headers = new Headers();
  if (cookie !== undefined) headers.set("cookie", cookie);
  return proxy(new NextRequest("https://syncspace.example/", { headers }));
}

describe("proxy for /", () => {
  it("sends a signed-in browser to the workspace picker", () => {
    const response = visit("ss_session=abc.def.ghi");
    expect(response.status).toBe(307);
    expect(response.headers.get("location")).toBe("https://syncspace.example/w");
  });

  it("sends a browser without a session to the login page", () => {
    for (const cookie of [undefined, "", "theme=dark", "ss_session="]) {
      const response = visit(cookie);
      expect(response.status).toBe(307);
      expect(response.headers.get("location")).toBe("https://syncspace.example/login");
    }
  });

  it("keeps the host of the request, so it works on every deployment URL", () => {
    const response = proxy(new NextRequest("https://other.vercel.app/", {}));
    expect(response.headers.get("location")).toBe("https://other.vercel.app/login");
  });

  it("runs only on the home page, so no other route and no asset is touched", () => {
    expect(config.matcher).toBe("/");
  });
});

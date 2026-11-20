import { NextResponse, type NextRequest } from "next/server";

/** The session cookie the API sets (`docs/API.md`, auth): HttpOnly, so only the server can see it. */
const SESSION_COOKIE = "ss_session";

/**
 * `/` has nothing of its own: it sends a signed-in browser to the workspace picker and everyone
 * else to the login page, before anything is rendered (no flash of a placeholder). The cookie is
 * only checked for being there; the API verifies it, and `/w` sends a browser whose session has
 * expired on to `/login`.
 */
export function proxy(request: NextRequest) {
  const target = request.cookies.get(SESSION_COOKIE)?.value ? "/w" : "/login";
  return NextResponse.redirect(new URL(target, request.url));
}

export const config = {
  matcher: "/",
};

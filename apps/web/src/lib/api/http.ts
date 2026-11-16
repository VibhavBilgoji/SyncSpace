import { ApiError } from "./errors";

const BASE = "/api/v1";

/**
 * JSON request through the Next.js rewrite, so the session cookie is first-party.
 * Throws `ApiError` for every non-2xx answer; returns undefined for 204.
 */
export async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  let response: Response;
  try {
    response = await fetch(`${BASE}${path}`, {
      method,
      credentials: "same-origin",
      headers: body === undefined ? undefined : { "Content-Type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    throw new ApiError(0, "NETWORK", "Could not reach the server");
  }

  if (response.status === 204) return undefined as T;

  let payload: unknown = null;
  try {
    payload = await response.json();
  } catch {
    // A body that is not JSON is handled below.
  }

  if (!response.ok) {
    const error = (payload as { error?: Record<string, unknown> } | null)?.error;
    const details = error?.details;
    throw new ApiError(
      response.status,
      typeof error?.code === "string" ? error.code : "UNKNOWN",
      typeof error?.message === "string" ? error.message : `Request failed (${response.status})`,
      details && typeof details === "object" ? (details as Record<string, string>) : {},
    );
  }
  return payload as T;
}

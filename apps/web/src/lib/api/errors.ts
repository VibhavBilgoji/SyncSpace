/** A non-2xx answer from the API, in the shape of docs/API.md section 1. */
export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  /** Field to message, present on 422. */
  readonly details: Record<string, string>;

  constructor(status: number, code: string, message: string, details: Record<string, string> = {}) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.details = details;
  }
}

export function isApiError(value: unknown): value is ApiError {
  return value instanceof ApiError;
}

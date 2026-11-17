/** Reconnect backoff of `docs/API.md` (Limits): 0.5 s, 1 s, 2 s, 4 s, then 8 s at most, with jitter. */
const BASE_MS = 500;
const MAX_MS = 8000;
/** The wait is scaled by a factor between 1 - JITTER and 1 + JITTER, so a restart does not bring every tab back in the same instant. */
const JITTER = 0.2;

/** The wait before reconnect number `attempt` (0 is the first retry). `random` is injectable for tests. */
export function retryDelay(attempt: number, random: () => number = Math.random): number {
  const steps = Math.max(0, Math.floor(attempt));
  const base = Math.min(BASE_MS * 2 ** Math.min(steps, 10), MAX_MS);
  return Math.round(base * (1 - JITTER + 2 * JITTER * random()));
}

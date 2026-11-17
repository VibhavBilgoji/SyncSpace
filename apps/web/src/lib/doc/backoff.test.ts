import { describe, expect, it } from "vitest";
import { retryDelay } from "./backoff";

describe("retryDelay", () => {
  it("follows 0.5 s, 1 s, 2 s, 4 s, then 8 s at most (no jitter at the middle of the range)", () => {
    const middle = () => 0.5;
    expect([0, 1, 2, 3, 4, 5, 6, 40].map((n) => retryDelay(n, middle))).toEqual([
      500, 1000, 2000, 4000, 8000, 8000, 8000, 8000,
    ]);
  });

  it("spreads each wait by 20 percent either way", () => {
    expect(retryDelay(2, () => 0)).toBe(1600);
    expect(retryDelay(2, () => 1)).toBe(2400);
    expect(retryDelay(5, () => 1)).toBe(9600);
  });

  it("treats a negative or fractional attempt as the first or the floor of it", () => {
    expect(retryDelay(-3, () => 0.5)).toBe(500);
    expect(retryDelay(1.9, () => 0.5)).toBe(1000);
  });

  it("is different from one call to the next with the real random source", () => {
    const waits = new Set(Array.from({ length: 30 }, () => retryDelay(3)));
    expect(waits.size).toBeGreaterThan(1);
    for (const wait of waits) {
      expect(wait).toBeGreaterThanOrEqual(3200);
      expect(wait).toBeLessThanOrEqual(4800);
    }
  });
});

import { describe, expect, it } from "vitest";
import { isValidSlug, slugify } from "./slug";

describe("slugify", () => {
  it("lowercases and joins words with dashes", () => {
    expect(slugify("Engineering Team")).toBe("engineering-team");
  });

  it("drops accents, punctuation and edge dashes", () => {
    expect(slugify("  Café — R&D!  ")).toBe("cafe-r-d");
  });

  it("cuts to 40 characters without a trailing dash", () => {
    const slug = slugify("a".repeat(39) + " b");
    expect(slug.length).toBeLessThanOrEqual(40);
    expect(slug.endsWith("-")).toBe(false);
  });

  it("is empty when nothing usable is left", () => {
    expect(slugify("!!!")).toBe("");
  });
});

describe("isValidSlug", () => {
  it.each(["abc", "eng-team", "a1b", "a".repeat(40)])("accepts %s", (slug) => {
    expect(isValidSlug(slug)).toBe(true);
  });

  it.each(["ab", "Team", "-abc", "abc-", "a b c", "a".repeat(41), ""])("rejects %j", (slug) => {
    expect(isValidSlug(slug)).toBe(false);
  });
});

import { describe, expect, it } from "vitest";
import { applyDiff, diffText, shiftOffset } from "@/lib/textDiff";

describe("diffText", () => {
  it("returns null when nothing changed", () => {
    expect(diffText("same", "same")).toBeNull();
    expect(diffText("", "")).toBeNull();
  });

  it("describes an insert in the middle", () => {
    expect(diffText("helo", "hello")).toEqual({ index: 3, deleteCount: 0, insert: "l" });
  });

  it("describes an insert at the start and at the end", () => {
    expect(diffText("bc", "abc")).toEqual({ index: 0, deleteCount: 0, insert: "a" });
    expect(diffText("ab", "abc")).toEqual({ index: 2, deleteCount: 0, insert: "c" });
  });

  it("describes a delete", () => {
    expect(diffText("hello", "hlo")).toEqual({ index: 1, deleteCount: 2, insert: "" });
  });

  it("describes a replace", () => {
    expect(diffText("cat", "cut")).toEqual({ index: 1, deleteCount: 1, insert: "u" });
  });

  it("handles a full replace and an empty text", () => {
    expect(diffText("abc", "xyz")).toEqual({ index: 0, deleteCount: 3, insert: "xyz" });
    expect(diffText("", "abc")).toEqual({ index: 0, deleteCount: 0, insert: "abc" });
    expect(diffText("abc", "")).toEqual({ index: 0, deleteCount: 3, insert: "" });
  });

  it("handles repeated characters without double counting", () => {
    expect(diffText("aaa", "aaaa")).toEqual({ index: 3, deleteCount: 0, insert: "a" });
    expect(diffText("aaaa", "aaa")).toEqual({ index: 3, deleteCount: 1, insert: "" });
    expect(diffText("abab", "ababab")).toEqual({ index: 4, deleteCount: 0, insert: "ab" });
  });

  it("inserts and deletes an emoji as a whole (two UTF-16 units)", () => {
    expect(diffText("ab", "a🙂b")).toEqual({ index: 1, deleteCount: 0, insert: "🙂" });
    expect(diffText("a🙂b", "ab")).toEqual({ index: 1, deleteCount: 2, insert: "" });
  });

  it("replaces one emoji with another without splitting the pair", () => {
    // 🙂 is D83D DE42 and 😀 is D83D DE00: they share the high surrogate.
    expect(diffText("a🙂b", "a😀b")).toEqual({ index: 1, deleteCount: 2, insert: "😀" });
  });

  it("does not split a pair when the low halves match", () => {
    // 😀 is D83D DE00 and 😠 is D83D DE20: only the low unit differs.
    const diff = diffText("😀", "😠");
    expect(diff).toEqual({ index: 0, deleteCount: 2, insert: "😠" });
  });

  it("handles multi-byte text such as Devanagari", () => {
    const prev = "नमस्ते";
    const next = "नमस्ते दुनिया";
    const diff = diffText(prev, next);
    expect(diff).toEqual({ index: prev.length, deleteCount: 0, insert: " दुनिया" });
  });

  it("round-trips random edits and never cuts a surrogate pair", () => {
    const pieces = ["a", "b", "é", "🙂", "😀", "नमस्ते", " ", "\n"];
    let seed = 12345;
    const rand = (n: number): number => {
      seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
      return seed % n;
    };
    const randomText = (): string =>
      Array.from({ length: rand(8) }, () => pieces[rand(pieces.length)] ?? "").join("");

    for (let i = 0; i < 500; i++) {
      const prev = randomText();
      const next = randomText();
      const diff = diffText(prev, next);
      if (diff === null) {
        expect(prev).toBe(next);
        continue;
      }
      expect(applyDiff(prev, diff)).toBe(next);
      for (const cut of [diff.index, diff.index + diff.deleteCount]) {
        const before = prev.charCodeAt(cut - 1);
        const after = prev.charCodeAt(cut);
        const splitsPair =
          before >= 0xd800 && before <= 0xdbff && after >= 0xdc00 && after <= 0xdfff;
        expect(splitsPair).toBe(false);
      }
    }
  });
});

describe("shiftOffset", () => {
  const insertAtTwo = { index: 2, deleteCount: 0, insert: "XYZ" };

  it("keeps offsets before the edit", () => {
    expect(shiftOffset(1, insertAtTwo)).toBe(1);
    expect(shiftOffset(2, insertAtTwo)).toBe(2);
  });

  it("moves offsets after an insert", () => {
    expect(shiftOffset(5, insertAtTwo)).toBe(8);
  });

  it("moves offsets after a delete", () => {
    expect(shiftOffset(9, { index: 2, deleteCount: 4, insert: "" })).toBe(5);
  });

  it("puts an offset that was inside a removed range after the inserted text", () => {
    expect(shiftOffset(4, { index: 2, deleteCount: 4, insert: "ab" })).toBe(4);
    expect(shiftOffset(3, { index: 2, deleteCount: 4, insert: "" })).toBe(2);
  });
});

describe("diffText surrogate and boundary edge cases", () => {
  it("does not split a pair when only the high halves differ (shared low half)", () => {
    // U+1F600 is D83D DE00 and U+1F300 is D83C DF00; build a pair sharing the low unit.
    const prev = "😀";
    const next = "🈀";
    const diff = diffText(prev, next);
    expect(diff).toEqual({ index: 0, deleteCount: 2, insert: next });
  });

  it("keeps a common suffix after an emoji replaced by plain text", () => {
    expect(diffText("x🙂end", "xYend")).toEqual({ index: 1, deleteCount: 2, insert: "Y" });
  });

  it("round-trips lone surrogates without throwing", () => {
    const prev = "a\ud83db";
    const next = "a\ude42b";
    const diff = diffText(prev, next);
    expect(diff).not.toBeNull();
    if (diff) expect(applyDiff(prev, diff)).toBe(next);
  });

  it("handles a very long text with one edit in the middle", () => {
    const prev = "a".repeat(100_000);
    const next = prev.slice(0, 50_000) + "b" + prev.slice(50_000);
    expect(diffText(prev, next)).toEqual({ index: 50_000, deleteCount: 0, insert: "b" });
  });
});

describe("shiftOffset boundaries", () => {
  it("moves an offset exactly at the end of a removed range by the net change", () => {
    expect(shiftOffset(6, { index: 2, deleteCount: 4, insert: "ab" })).toBe(4);
  });

  it("treats a pure insert at the offset as leaving the offset before it", () => {
    expect(shiftOffset(2, { index: 2, deleteCount: 0, insert: "zz" })).toBe(2);
  });

  it("keeps the offset at zero for an edit at the start", () => {
    expect(shiftOffset(0, { index: 0, deleteCount: 3, insert: "" })).toBe(0);
  });
});

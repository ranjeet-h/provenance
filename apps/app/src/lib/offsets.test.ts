import { describe, expect, it } from "vitest";
import { sliceByByteOffsets, utf8ByteToUtf16Index } from "./offsets";

describe("utf8ByteToUtf16Index", () => {
  it("is the identity for ASCII", () => {
    expect(utf8ByteToUtf16Index("hello world", 5)).toBe(5);
    expect(utf8ByteToUtf16Index("hello world", 11)).toBe(11);
  });

  it("maps past accented characters", () => {
    // "naïve": ï is 2 bytes, 1 UTF-16 unit.
    expect(utf8ByteToUtf16Index("naïve introduction", 6)).toBe(5);
    expect(utf8ByteToUtf16Index("naïve introduction", 7)).toBe(6);
  });

  it("maps past combining marks", () => {
    // "e\u0301": 3 bytes, 2 UTF-16 units.
    expect(utf8ByteToUtf16Index("e\u0301clair match", 3)).toBe(2);
  });

  it("maps past astral emoji (surrogate pairs)", () => {
    // "😀": 4 bytes, 2 UTF-16 units.
    expect(utf8ByteToUtf16Index("😀 grin match", 4)).toBe(2);
    expect(utf8ByteToUtf16Index("😀 grin match", 9)).toBe(7);
  });

  it("maps past CJK characters", () => {
    expect(utf8ByteToUtf16Index("中文测试 match here", 6)).toBe(2);
    expect(utf8ByteToUtf16Index("中文测试 match here", 18)).toBe(10);
  });

  it("rejects non-boundaries", () => {
    expect(utf8ByteToUtf16Index("naïve", 3)).toBeNull(); // mid-ï
    expect(utf8ByteToUtf16Index("😀", 2)).toBeNull(); // mid-emoji
    expect(utf8ByteToUtf16Index("hi", 99)).toBeNull();
  });
});

describe("sliceByByteOffsets", () => {
  it("slices the audit's reproduction exactly", () => {
    const text = "naïve introduction. Copied passage starts here and ends now.";
    // Byte 20..60 in Rust terms (verified against the Rust oracle test).
    const start = new TextEncoder().encode("naïve introduction. ").length;
    const end = start + new TextEncoder().encode("Copied passage starts here and ends now").length;
    expect(sliceByByteOffsets(text, start, end)).toBe(
      "Copied passage starts here and ends now",
    );
  });

  it("highlights after emoji, CJK, and combining marks", () => {
    const enc = new TextEncoder();
    const cases: Array<[string, string, string]> = [
      ["😀 grin and Copied words here", "😀 grin and ", "Copied words here"],
      ["中文测试 Copied words here", "中文测试 ", "Copied words here"],
      ["e\u0301clair Copied words here", "e\u0301clair ", "Copied words here"],
      ["caf\u00e9 Copied words here", "caf\u00e9 ", "Copied words here"],
    ];
    for (const [text, prefix, expected] of cases) {
      const start = enc.encode(prefix).length;
      const end = start + enc.encode(expected).length;
      expect(sliceByByteOffsets(text, start, end)).toBe(expected);
    }
  });

  it("returns empty text on contract violations", () => {
    expect(sliceByByteOffsets("naïve", 1, 3)).toBe("");
    expect(sliceByByteOffsets("hi", 2, 1)).toBe("");
  });
});

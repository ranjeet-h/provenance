import { afterEach, describe, expect, it, vi } from "vitest";
import { resetInvokeImpl, setInvokeImpl } from "../lib/tauri";
import { inspectText } from "../lib/inspect";

afterEach(() => {
  resetInvokeImpl();
});

describe("inspect API", () => {
  it("returns the canonical inspection", async () => {
    const mock = vi.fn(async () => ({
      original: "Hello, WORLD!",
      normalization_version: 1,
      tokens: [
        { normalized: "hello", original_start: 0, original_end: 5, ordinal: 0 },
        {
          normalized: "world",
          original_start: 7,
          original_end: 12,
          ordinal: 1,
        },
      ],
      normalized_text: "hello world",
      sentences: [
        { token_start: 0, token_end: 2, char_start: 0, char_end: 12 },
      ],
      paragraph_starts: [0],
      meaningful: [true, true],
    }));
    setInvokeImpl(mock);
    const result = await inspectText("Hello, WORLD!");
    expect(mock).toHaveBeenCalledWith("inspect_text", {
      text: "Hello, WORLD!",
    });
    expect(result.tokens.map((t) => t.normalized)).toEqual(["hello", "world"]);
    expect(result.normalized_text).toBe("hello world");
  });

  it("maps core errors and malformed payloads", async () => {
    setInvokeImpl(
      vi.fn(async () => {
        throw { code: "validation", message: "inspection text is too long" };
      }),
    );
    await expect(inspectText("x")).rejects.toMatchObject({
      code: "validation",
    });

    setInvokeImpl(vi.fn(async () => ({ wrong: "shape" })));
    await expect(inspectText("x")).rejects.toMatchObject({ code: "protocol" });
  });
});

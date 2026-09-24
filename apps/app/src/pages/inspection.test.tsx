import { afterEach, describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { resetInvokeImpl, setInvokeImpl } from "../lib/tauri";
import { inspectText } from "../lib/inspect";
import { renderRouteAt } from "../test-utils";

const PAYLOAD = {
  original: "Hello, WORLD!",
  normalization_version: 1,
  tokens: [
    { normalized: "hello", original_start: 0, original_end: 5, ordinal: 0 },
    { normalized: "world", original_start: 7, original_end: 12, ordinal: 1 },
  ],
  normalized_text: "hello world",
  sentences: [{ token_start: 0, token_end: 2, char_start: 0, char_end: 12 }],
  paragraph_starts: [0],
  meaningful: [true, true],
};

afterEach(() => {
  resetInvokeImpl();
});

describe("inspect API", () => {
  it("returns the canonical inspection", async () => {
    const mock = vi.fn(async () => PAYLOAD);
    setInvokeImpl(mock);
    const result = await inspectText("Hello, WORLD!");
    expect(mock).toHaveBeenCalledWith("inspect_text", { text: "Hello, WORLD!" });
    expect(result.tokens.map((t) => t.normalized)).toEqual(["hello", "world"]);
    expect(result.normalized_text).toBe("hello world");
  });

  it("maps core errors and malformed payloads", async () => {
    setInvokeImpl(vi.fn(async () => {
      throw { code: "validation", message: "inspection text is too long" };
    }));
    await expect(inspectText("x")).rejects.toMatchObject({ code: "validation" });

    setInvokeImpl(vi.fn(async () => ({ wrong: "shape" })));
    await expect(inspectText("x")).rejects.toMatchObject({ code: "protocol" });
  });
});

describe("Settings inspection tool", () => {
  it("blocks empty input", async () => {
    const user = userEvent.setup();
    renderRouteAt("/settings");
    await screen.findByRole("heading", { name: /settings/i });
    await user.click(screen.getByRole("button", { name: /inspect text/i }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/paste a paragraph/i);
  });

  it("shows tokens with original ranges", async () => {
    const user = userEvent.setup();
    setInvokeImpl(vi.fn(async () => PAYLOAD));
    renderRouteAt("/settings");
    await screen.findByRole("heading", { name: /settings/i });

    await user.type(screen.getByLabelText(/original text/i), "Hello, WORLD!");
    await user.click(screen.getByRole("button", { name: /inspect text/i }));

    expect(await screen.findByText(/2 tokens · 1 sentences/i)).toBeInTheDocument();
    expect(screen.getByText("hello world")).toBeInTheDocument();
    const rows = screen.getAllByRole("row");
    expect(rows.map((r) => r.textContent)).toContain("hello[0, 5)");
    expect(rows.map((r) => r.textContent)).toContain("world[7, 12)");
  });
});

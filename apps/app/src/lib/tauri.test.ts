import { afterEach, describe, expect, it, vi } from "vitest";
import { greet, resetInvokeImpl, setInvokeImpl } from "./tauri";

afterEach(() => {
  resetInvokeImpl();
});

describe("Tauri API adapter", () => {
  it("delegates greet to invoke impl", async () => {
    const mock = vi.fn(async () => {
      return "Hello, Ada! You've been greeted from Rust!";
    });
    setInvokeImpl(mock);
    await expect(greet("Ada")).resolves.toContain("Ada");
    expect(mock).toHaveBeenCalledWith("greet", { name: "Ada" });
  });

  it("rejects on unexpected response shape", async () => {
    setInvokeImpl(async () => 42);
    await expect(greet("Ada")).rejects.toThrow(/unexpected/i);
  });
});

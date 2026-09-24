import { describe, expect, it } from "vitest";
import { cn } from "./utils";

describe("cn", () => {
  it("joins class names", () => {
    expect(cn("a", "b")).toBe("a b");
  });

  it("merges conflicting tailwind classes", () => {
    expect(cn("px-2 px-4")).toBe("px-4");
  });

  it("ignores falsy values", () => {
    expect(cn("a", false, undefined, null, "b")).toBe("a b");
  });

  it("handles conditional object syntax", () => {
    expect(cn({ active: true, hidden: false })).toBe("active");
  });
});

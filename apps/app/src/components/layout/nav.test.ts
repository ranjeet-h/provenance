import { describe, expect, it } from "vitest";
import { NAV_ITEMS } from "./nav";

describe("NAV_ITEMS", () => {
  it("has exactly the four Phase 1 destinations", () => {
    expect(NAV_ITEMS.map((i) => i.to)).toEqual([
      "/",
      "/sessions",
      "/reference-libraries",
      "/settings",
    ]);
  });

  it("labels every destination", () => {
    expect(NAV_ITEMS.map((i) => i.label)).toEqual([
      "Dashboard",
      "Sessions",
      "Reference Libraries",
      "Settings",
    ]);
  });

  it("marks only the dashboard as exact-match", () => {
    expect(NAV_ITEMS.filter((i) => i.end).map((i) => i.to)).toEqual(["/"]);
  });

  it("gives every item a renderable icon component", () => {
    for (const item of NAV_ITEMS) {
      expect(item.icon).toBeTruthy();
      expect(
        "$$typeof" in (item.icon as unknown as Record<string, unknown>),
      ).toBe(true);
    }
  });
});

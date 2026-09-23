import { afterEach, describe, expect, it } from "vitest";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { resetInvokeImpl, setInvokeImpl } from "./lib/tauri";
import { createFakeSessions, renderRouteAt, sessionFixture } from "./test-utils";

function renderAt(path: string, seedSessions = false) {
  const seed = seedSessions
    ? { sessions: [sessionFixture({ id: "abc-123", name: "Demo Session" })] }
    : undefined;
  setInvokeImpl(createFakeSessions(seed).invoke);
  return renderRouteAt(path);
}

afterEach(() => {
  resetInvokeImpl();
});

describe("Phase 1 routes", () => {
  it("renders dashboard at /", async () => {
    renderAt("/");
    expect(
      await screen.findByRole("heading", { name: /^provenance$/i }),
    ).toBeInTheDocument();
  });

  it("renders sessions empty state at /sessions", async () => {
    renderAt("/sessions");
    expect(
      await screen.findByRole("heading", { name: /^sessions$/i, level: 1 }),
    ).toBeInTheDocument();
    expect(await screen.findByText(/no sessions yet/i)).toBeInTheDocument();
  });

  it("renders new session page at /sessions/new", async () => {
    renderAt("/sessions/new");
    expect(
      await screen.findByRole("heading", { name: /new session/i }),
    ).toBeInTheDocument();
  });

  it("renders session detail for /sessions/:sessionId", async () => {
    renderAt("/sessions/abc-123", true);
    expect(
      await screen.findByRole("heading", { name: /demo session/i }),
    ).toBeInTheDocument();
  });

  it("renders reference libraries empty state", async () => {
    renderAt("/reference-libraries");
    expect(
      await screen.findByRole("heading", { name: /^reference libraries$/i, level: 1 }),
    ).toBeInTheDocument();
    expect(await screen.findByText(/no reference libraries yet/i)).toBeInTheDocument();
  });

  it("renders settings diagnostics", async () => {
    renderAt("/settings");
    expect(
      await screen.findByRole("heading", { name: /settings/i }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText(/your name/i)).toBeInTheDocument();
  });

  it("renders not-found page for unknown route", async () => {
    renderAt("/does-not-exist");
    expect(await screen.findByText(/page not found/i)).toBeInTheDocument();
    expect(
      screen.getByRole("link", { name: /back to dashboard/i }),
    ).toBeInTheDocument();
  });
});

describe("Primary navigation", () => {
  it("marks the active nav item with aria-current", async () => {
    renderAt("/sessions");
    await screen.findByRole("heading", { name: /^sessions$/i, level: 1 });
    const nav = screen.getByRole("navigation", { name: /primary/i });
    const active = within(nav).getByRole("link", { name: /sessions/i });
    expect(active).toHaveAttribute("aria-current", "page");
    expect(
      within(nav).getByRole("link", { name: /settings/i }),
    ).not.toHaveAttribute("aria-current");
  });

  it("navigates by clicking sidebar links", async () => {
    const user = userEvent.setup();
    renderAt("/");
    await screen.findByRole("heading", { name: /^provenance$/i });
    const nav = screen.getByRole("navigation", { name: /primary/i });
    await user.click(within(nav).getByRole("link", { name: /sessions/i }));
    expect(
      await screen.findByText(/no sessions yet/i),
    ).toBeInTheDocument();
  });

  it("supports keyboard navigation (Tab + Enter)", async () => {
    const user = userEvent.setup();
    renderAt("/");
    await screen.findByRole("heading", { name: /^provenance$/i });
    const nav = screen.getByRole("navigation", { name: /primary/i });
    const sessionsLink = within(nav).getByRole("link", { name: /sessions/i });
    sessionsLink.focus();
    expect(sessionsLink).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(
      await screen.findByText(/no sessions yet/i),
    ).toBeInTheDocument();
  });

  it("exposes accessible names for all nav landmarks", async () => {
    renderAt("/");
    await screen.findByRole("heading", { name: /^provenance$/i });
    expect(
      screen.getByRole("navigation", { name: /primary/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /open navigation/i }),
    ).toBeInTheDocument();
  });
});

import { afterEach, describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SettingsPage } from "./SettingsPage";
import { resetInvokeImpl, setInvokeImpl } from "../lib/tauri";
import {
  createFakeSessions,
  renderRouteAt,
  sessionFixture,
  studentFixture,
} from "../test-utils";

function renderAt(path: string) {
  setInvokeImpl(
    createFakeSessions({
      sessions: [sessionFixture({ id: "demo-1", name: "Demo Session" })],
      students: [studentFixture({ id: "stud-9", session_id: "demo-1" })],
    }).invoke,
  );
  return renderRouteAt(path);
}

afterEach(() => {
  resetInvokeImpl();
});

describe("DashboardPage", () => {
  it("links each card to its destination", async () => {
    renderAt("/");
    await screen.findByRole("heading", { name: /^provenance$/i });
    const main = within(screen.getByRole("main"));
    const cards = [
      { name: /^sessions /i, href: "/sessions" },
      { name: /^reference libraries /i, href: "/reference-libraries" },
      { name: /^settings /i, href: "/settings" },
    ];
    for (const card of cards) {
      const link = main.getByRole("link", { name: card.name });
      expect(link).toHaveAttribute("href", card.href);
    }
  });

  it("opens the sessions page from its card", async () => {
    const user = userEvent.setup();
    renderAt("/");
    await screen.findByRole("heading", { name: /^provenance$/i });
    const main = within(screen.getByRole("main"));
    await user.click(main.getByRole("link", { name: /^sessions /i }));
    expect(
      await screen.findByRole("heading", { name: /^sessions$/i, level: 1 }),
    ).toBeInTheDocument();
  });
});

describe("SessionsPage", () => {
  it("offers new-session actions that route to /sessions/new", async () => {
    const user = userEvent.setup();
    renderAt("/sessions");
    await screen.findByRole("heading", { name: /^sessions$/i, level: 1 });
    const links = screen.getAllByRole("link", { name: /new session|create session/i });
    expect(links.length).toBeGreaterThan(0);
    for (const link of links) {
      expect(link).toHaveAttribute("href", "/sessions/new");
    }
    await user.click(links[0] as HTMLElement);
    expect(
      await screen.findByRole("heading", { name: /new session/i, level: 1 }),
    ).toBeInTheDocument();
  });
});

describe("NewSessionPage", () => {
  it("renders the creation form", async () => {
    renderAt("/sessions/new");
    expect(
      await screen.findByRole("heading", { name: /new session/i }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText(/assignment name/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/subject/i)).toBeInTheDocument();
  });

  it("blocks an empty name with an inline error", async () => {
    const user = userEvent.setup();
    renderAt("/sessions/new");
    await screen.findByRole("heading", { name: /new session/i });
    await user.click(screen.getByRole("button", { name: /create session/i }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/enter a session name/i);
  });
});

describe("SessionDetailPage", () => {
  it("shows the session, its students, and draft status", async () => {
    renderAt("/sessions/demo-1");
    expect(
      await screen.findByRole("heading", { name: /demo session/i }),
    ).toBeInTheDocument();
    expect(screen.getByText("Draft")).toHaveAccessibleName(/status: draft/i);
    expect(await screen.findByText("Amit")).toBeInTheDocument();
  });
});

describe("ReferenceLibrariesPage", () => {
  it("shows the historical empty state", async () => {
    renderAt("/reference-libraries");
    expect(
      await screen.findByRole("heading", { name: /^reference libraries$/i }),
    ).toBeInTheDocument();
    expect(await screen.findByText(/run an analysis in a completed session/i)).toBeInTheDocument();
  });
});

describe("SettingsPage diagnostics", () => {
  it("greets through the Tauri boundary", async () => {
    const user = userEvent.setup();
    setInvokeImpl(async (_cmd, args) => `Hello, ${String(args?.["name"])}!`);
    render(<SettingsPage />);
    await user.type(screen.getByLabelText(/your name/i), "Ada");
    await user.click(screen.getByRole("button", { name: /^greet$/i }));
    expect(await screen.findByText("Hello, Ada!")).toBeInTheDocument();
  });

  it("defaults to Provenance for a blank name", async () => {
    const user = userEvent.setup();
    const seen: Array<Record<string, unknown> | undefined> = [];
    setInvokeImpl(async (_cmd, args) => {
      seen.push(args);
      return "ok";
    });
    render(<SettingsPage />);
    await user.click(screen.getByRole("button", { name: /^greet$/i }));
    expect(seen[0]?.["name"]).toBe("Provenance");
  });

  it("shows a friendly message when the shell is unreachable", async () => {
    const user = userEvent.setup();
    setInvokeImpl(async () => {
      throw new Error("no shell");
    });
    render(<SettingsPage />);
    await user.click(screen.getByRole("button", { name: /^greet$/i }));
    expect(
      await screen.findByText(/not running in tauri shell/i),
    ).toBeInTheDocument();
  });
});

describe("NotFoundPage", () => {
  it("links back to the dashboard", async () => {
    const user = userEvent.setup();
    renderAt("/missing-page");
    expect(await screen.findByText(/page not found/i)).toBeInTheDocument();
    const back = screen.getByRole("link", { name: /back to dashboard/i });
    expect(back).toHaveAttribute("href", "/");
    await user.click(back);
    expect(
      await screen.findByRole("heading", { name: /^provenance$/i }),
    ).toBeInTheDocument();
  });
});

describe("TopBar", () => {
  it("announces offline readiness on larger screens", async () => {
    renderAt("/");
    await screen.findByRole("heading", { name: /^provenance$/i });
    const banner = screen.getByRole("banner");
    expect(within(banner).getByText(/offline ready/i)).toBeInTheDocument();
  });

  it("labels the mobile menu button from within the banner", async () => {
    renderAt("/");
    await screen.findByRole("heading", { name: /^provenance$/i });
    const banner = screen.getByRole("banner");
    expect(
      within(banner).getByRole("button", { name: /open navigation/i }),
    ).toBeInTheDocument();
  });
});

import { afterEach, describe, expect, it } from "vitest";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { resetInvokeImpl, setInvokeImpl } from "../lib/tauri";
import { createFakeSessions, renderRouteAt } from "../test-utils";

function renderWithFake(path: string) {
  setInvokeImpl(createFakeSessions().invoke);
  return renderRouteAt(path);
}

afterEach(() => {
  resetInvokeImpl();
});

describe("sessions flow", () => {
  it("creates a session from the form and lands on its detail page", async () => {
    const user = userEvent.setup();
    renderWithFake("/sessions/new");
    await screen.findByRole("heading", { name: /new session/i });

    await user.type(screen.getByLabelText(/assignment name/i), "Physics Lab 2");
    await user.type(screen.getByLabelText(/subject/i), "Physics");
    await user.click(screen.getByRole("button", { name: /create session/i }));

    expect(await screen.findByRole("heading", { name: /physics lab 2/i })).toBeInTheDocument();
    expect(screen.getByText("Draft")).toBeInTheDocument();
    expect(screen.getByText(/no students yet/i)).toBeInTheDocument();
  });

  it("shows a server-side error without navigating", async () => {
    const user = userEvent.setup();
    const { invoke: fake } = createFakeSessions();
    setInvokeImpl((cmd, args) => {
      if (cmd === "create_session") {
        return Promise.reject({ code: "validation", message: "Duplicate session name." });
      }
      return fake(cmd, args);
    });
    renderRouteAt("/sessions/new");
    await screen.findByRole("heading", { name: /new session/i });
    await user.type(screen.getByLabelText(/assignment name/i), "Valid Name");
    await user.click(screen.getByRole("button", { name: /create session/i }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/duplicate session name/i);
    expect(
      screen.getByRole("heading", { name: /new session/i, level: 1 }),
    ).toBeInTheDocument();
  });

  it("adds and removes a student with confirmation", async () => {
    const user = userEvent.setup();
    renderWithFake("/sessions/new");
    await screen.findByRole("heading", { name: /new session/i });
    await user.type(screen.getByLabelText(/assignment name/i), "Math Quiz");
    await user.click(screen.getByRole("button", { name: /create session/i }));
    await screen.findByRole("heading", { name: /math quiz/i });

    await user.type(screen.getByLabelText(/student name/i), "Rahul");
    await user.click(screen.getByRole("button", { name: /^add$/i }));
    expect(await screen.findByText("Rahul")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /remove rahul/i }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: /^remove$/i }));
    expect(screen.queryByText("Rahul")).not.toBeInTheDocument();
    expect(screen.getByText(/no students yet/i)).toBeInTheDocument();
  });

  it("renames the session inline", async () => {
    const user = userEvent.setup();
    renderWithFake("/sessions/new");
    await screen.findByRole("heading", { name: /new session/i });
    await user.type(screen.getByLabelText(/assignment name/i), "Old Title");
    await user.click(screen.getByRole("button", { name: /create session/i }));
    await screen.findByRole("heading", { name: /old title/i });

    const renameBox = screen.getByLabelText(/session name/i);
    await user.clear(renameBox);
    await user.type(renameBox, "New Title");
    await user.click(screen.getByRole("button", { name: /^save$/i }));
    expect(await screen.findByRole("heading", { name: /new title/i })).toBeInTheDocument();
  });

  it("deletes the session and returns to the list", async () => {
    const user = userEvent.setup();
    renderWithFake("/sessions/new");
    await screen.findByRole("heading", { name: /new session/i });
    await user.type(screen.getByLabelText(/assignment name/i), "Temporary");
    await user.click(screen.getByRole("button", { name: /create session/i }));
    await screen.findByRole("heading", { name: /temporary/i });

    await user.click(screen.getByRole("button", { name: /delete temporary/i }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: /^delete$/i }));
    expect(
      await screen.findByRole("heading", { name: /^sessions$/i, level: 1 }),
    ).toBeInTheDocument();
    expect(screen.getByText(/no sessions yet/i)).toBeInTheDocument();
  });

  it("lists created sessions with links to their detail pages", async () => {
    const user = userEvent.setup();
    renderWithFake("/sessions");
    await screen.findByText(/no sessions yet/i);

    await user.click(screen.getByRole("link", { name: /create session/i }));
    await screen.findByRole("heading", { name: /new session/i });
    await user.type(screen.getByLabelText(/assignment name/i), "History Essay");
    await user.click(screen.getByRole("button", { name: /create session/i }));
    await screen.findByRole("heading", { name: /history essay/i });

    await user.click(screen.getByRole("link", { name: /sessions/i }));
    const list = await screen.findByRole("list", { name: /sessions/i });
    expect(within(list).getByRole("link", { name: /history essay/i })).toBeInTheDocument();
  });
});

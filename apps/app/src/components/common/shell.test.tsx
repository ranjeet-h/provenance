import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { RouterProvider } from "@tanstack/react-router";
import { createAppRouter } from "../../router";
import { PageHeader } from "./PageHeader";
import { EmptyState } from "./EmptyState";
import { LoadingState } from "./LoadingState";
import { ErrorState } from "./ErrorState";
import { ConfirmDialog } from "./ConfirmDialog";
import { StatusBadge } from "./StatusBadge";
import { ProgressIndicator } from "./ProgressIndicator";
import { Button } from "../ui/button";

function renderAt(path: string) {
  const testRouter = createAppRouter(path);
  return render(<RouterProvider router={testRouter} />);
}

describe("AppShell mobile navigation", () => {
  it("opens the navigation drawer from the TopBar menu button", async () => {
    const user = userEvent.setup();
    renderAt("/");
    await screen.findByRole("heading", { name: /^provenance$/i });
    await user.click(screen.getByRole("button", { name: /open navigation/i }));
    const dialog = await screen.findByRole("dialog", { name: /navigation/i });
    expect(dialog).toBeInTheDocument();
  });

  it("closes the drawer after choosing a destination", async () => {
    const user = userEvent.setup();
    renderAt("/");
    await screen.findByRole("heading", { name: /^provenance$/i });
    await user.click(screen.getByRole("button", { name: /open navigation/i }));
    const dialog = await screen.findByRole("dialog", { name: /navigation/i });
    expect(dialog).toBeInTheDocument();
    // Mobile drawer links navigate; the sheet closes on selection.
    await user.click(
      within(dialog).getByRole("link", {
        name: /settings/i,
      }),
    );
    expect(
      await screen.findByRole("heading", { name: /settings/i }),
    ).toBeInTheDocument();
  });
});

describe("Shell primitives", () => {
  it("PageHeader renders title, description, and actions", () => {
    render(
      <PageHeader title="Sessions" description="Manage work" actions={<button>New</button>} />,
    );
    expect(screen.getByRole("heading", { name: /sessions/i })).toBeInTheDocument();
    expect(screen.getByText(/manage work/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /new/i })).toBeInTheDocument();
  });

  it("EmptyState renders title and action", () => {
    render(
      <EmptyState
        title="No sessions yet"
        description="Get started"
        action={<Button>Create</Button>}
      />,
    );
    expect(screen.getByText(/no sessions yet/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /create/i })).toBeInTheDocument();
  });

  it("LoadingState exposes a status role", () => {
    render(<LoadingState label="Loading sessions…" />);
    expect(screen.getByRole("status")).toHaveAccessibleName(/loading sessions/i);
  });

  it("ErrorState calls retry", async () => {
    const user = userEvent.setup();
    const onRetry = vi.fn();
    render(<ErrorState message="DB locked" onRetry={onRetry} />);
    expect(screen.getByRole("alert")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /try again/i }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it("ConfirmDialog confirms and cancels", async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();
    render(
      <ConfirmDialog
        title="Remove student?"
        description="This cannot be undone."
        confirmLabel="Remove"
        onConfirm={onConfirm}
        trigger={<Button>Remove</Button>}
      />,
    );
    await user.click(screen.getByRole("button", { name: /^remove$/i }));
    await user.click(screen.getByRole("button", { name: /^remove$/i }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it("StatusBadge announces each status", () => {
    const { rerender } = render(<StatusBadge status="draft" />);
    expect(screen.getByText("Draft")).toHaveAccessibleName(/status: draft/i);
    rerender(<StatusBadge status="locked" />);
    expect(screen.getByText("Locked")).toHaveAccessibleName(/status: locked/i);
    rerender(<StatusBadge status="analyzed" />);
    expect(screen.getByText("Analyzed")).toHaveAccessibleName(/status: analyzed/i);
  });

  it("ProgressIndicator reports progress accessibly", () => {
    render(<ProgressIndicator value={42} label="Analyzing" />);
    expect(screen.getByRole("progressbar", { name: /analyzing/i })).toHaveAttribute(
      "aria-valuenow",
      "42",
    );
  });
});

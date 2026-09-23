import { afterEach, describe, expect, it, vi } from "vitest";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { resetInvokeImpl, resetListenImpl, setInvokeImpl, setListenImpl } from "../lib/tauri";
import { analyzeSession, cellCoverage, cellCoverageByKind } from "../lib/analysis";
import {
  createFakeSessions,
  renderRouteAt,
  sessionFixture,
  studentFixture,
  submissionFixture,
} from "../test-utils";

const SHARED =
  "the rapid expansion of railway networks during the nineteenth century transformed trade";

function seedThreeStudents() {
  return {
    sessions: [sessionFixture({ id: "sess-5", name: "History Test" })],
    students: [
      studentFixture({ id: "st-a", session_id: "sess-5", display_name: "Amit" }),
      studentFixture({ id: "st-b", session_id: "sess-5", display_name: "Priya" }),
      studentFixture({ id: "st-c", session_id: "sess-5", display_name: "Chen" }),
    ],
    submissions: [
      submissionFixture({
        id: "sub-a",
        student_id: "st-a",
        session_id: "sess-5",
        original_text: `My own opening paragraph with personal wording here. ${SHARED} My own closing paragraph with independent thoughts.`,
      }),
      submissionFixture({
        id: "sub-b",
        student_id: "st-b",
        session_id: "sess-5",
        original_text: `A different opening that shares nothing at all. ${SHARED} A different ending with fresh vocabulary throughout.`,
      }),
      submissionFixture({
        id: "sub-c",
        student_id: "st-c",
        session_id: "sess-5",
        original_text:
          "Quantum tunneling lets particles cross barriers they classically cannot surmount ever.",
      }),
    ],
  };
}

afterEach(() => {
  resetInvokeImpl();
  resetListenImpl();
  setListenImpl(async () => () => {});
});

describe("analysis API", () => {
  it("calls analyze with the session id", async () => {
    const mock = vi.fn(async () => ({
      fingerprint_version: 1,
      normalization_version: 1,
      common_text_version: 1,
      modified_version: 1,
      common_text_applied: false,
      prompt_applied: false,
      pairs: [],
      per_student: [],
    }));
    setInvokeImpl(mock);
    await analyzeSession("sess-1");
    expect(mock).toHaveBeenCalledWith("analyze_session", { sessionId: "sess-1" });
  });

  it("rejects malformed reports as protocol errors", async () => {
    setInvokeImpl(vi.fn(async () => ({ pairs: [] })));
    await expect(analyzeSession("sess-1")).rejects.toMatchObject({ code: "protocol" });
  });

  it("cellCoverage picks the row student's side", () => {
    const pair = {
      a_student_id: "a",
      b_student_id: "b",
      coverage_a: 50,
      coverage_b: 25,
      exact_coverage_a: 40,
      exact_coverage_b: 20,
      modified_coverage_a: 10,
      modified_coverage_b: 5,
      passages: [],
      excluded: [],
    };
    expect(cellCoverage(pair, "a")).toBe(50);
    expect(cellCoverage(pair, "b")).toBe(25);
    expect(cellCoverage(pair, "c")).toBeNull();
    expect(cellCoverageByKind(pair, "a", "exact")).toBe(40);
    expect(cellCoverageByKind(pair, "b", "modified")).toBe(5);
    expect(cellCoverageByKind(pair, "c", "exact")).toBeNull();
  });
});

describe("analysis section", () => {
  it("asks for more submissions when fewer than two exist", async () => {
    setInvokeImpl(
      createFakeSessions({
        sessions: [sessionFixture({ id: "sess-5", name: "History Test" })],
        students: [studentFixture({ id: "st-a", session_id: "sess-5" })],
      }).invoke,
    );
    renderRouteAt("/sessions/sess-5");
    await screen.findByRole("heading", { name: /history test/i });
    expect(await screen.findByText(/not enough submissions/i)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /analyze session/i })).not.toBeInTheDocument();
  });

  it("runs analysis and shows the pairwise matrix", async () => {
    const user = userEvent.setup();
    setInvokeImpl(createFakeSessions(seedThreeStudents()).invoke);
    renderRouteAt("/sessions/sess-5");
    await screen.findByRole("heading", { name: /history test/i });

    await user.click(await screen.findByRole("button", { name: /analyze session/i }));
    const cell = await screen.findByRole("button", { name: /amit vs priya: \d+% overlap/i });
    expect(cell.textContent).not.toBe("0%");
    // Unrelated pair stays clean.
    expect(screen.getByRole("button", { name: /amit vs chen: 0% overlap/i })).toBeInTheDocument();
  });

  it("shows live monotonic progress from the analysis event channel", async () => {
    const user = userEvent.setup();
    const { invoke } = createFakeSessions(seedThreeStudents());
    setListenImpl(async (event, handler) => {
      expect(event).toBe("analysis-progress");
      handler({
        payload: {
          session_id: "sess-5",
          stage: "exact",
          fraction: 0.5,
          completed_pairs: 2,
          total_pairs: 3,
          cached_pairs: 1,
        },
      });
      return () => {};
    });
    setInvokeImpl((cmd, args) => {
      if (cmd === "analyze_session") {
        return new Promise((_, reject) => {
          setTimeout(() => reject({ code: "cancelled", message: "Test stopped." }), 80);
        });
      }
      return invoke(cmd, args);
    });
    renderRouteAt("/sessions/sess-5");
    await screen.findByRole("heading", { name: /history test/i });
    await user.click(await screen.findByRole("button", { name: /analyze session/i }));

    const progress = await screen.findByRole("status", { name: /analysis progress/i });
    expect(within(progress).getByText(/finding exact matches/i)).toBeInTheDocument();
    expect(progress.querySelector("progress")).toHaveValue(0.5);
    expect(within(progress).getByText(/2\/3 pairs · 1 unchanged raw pair results reused/i)).toBeInTheDocument();
    expect(await screen.findByRole("alert")).toHaveTextContent(/test stopped/i);
  });

  it("restores a persisted analysis after reopening the session", async () => {
    const user = userEvent.setup();
    const { invoke } = createFakeSessions(seedThreeStudents());
    setInvokeImpl(invoke);
    const firstView = renderRouteAt("/sessions/sess-5");
    await screen.findByRole("heading", { name: /history test/i });
    await user.click(await screen.findByRole("button", { name: /analyze session/i }));
    await screen.findByRole("heading", { name: /per-student overlap/i });
    firstView.unmount();

    renderRouteAt("/sessions/sess-5");
    await screen.findByRole("heading", { name: /per-student overlap/i });
    expect(screen.getByRole("button", { name: /re-run analysis/i })).toBeInTheDocument();
  });

  it("opens pair detail with highlighted passages", async () => {
    const user = userEvent.setup();
    setInvokeImpl(createFakeSessions(seedThreeStudents()).invoke);
    renderRouteAt("/sessions/sess-5");
    await screen.findByRole("heading", { name: /history test/i });

    await user.click(await screen.findByRole("button", { name: /analyze session/i }));
    await user.click(await screen.findByRole("button", { name: /amit vs priya: \d+% overlap/i }));

    const detail = await screen.findByLabelText(/pair detail/i);
    expect(within(detail).getByText(/passage 1 · \d+ matching words/i)).toBeInTheDocument();
    expect(within(detail).getByText(/type values are subsets; combined coverage uses unique spans/i)).toBeInTheDocument();
    const marks = within(detail).getAllByText(/rapid expansion of railway networks/i);
    expect(marks.length).toBeGreaterThanOrEqual(2);
    expect(marks[0]?.tagName).toBe("MARK");
  });

  it("shows zero-match detail for clean pairs", async () => {
    const user = userEvent.setup();
    setInvokeImpl(createFakeSessions(seedThreeStudents()).invoke);
    renderRouteAt("/sessions/sess-5");
    await screen.findByRole("heading", { name: /history test/i });

    await user.click(await screen.findByRole("button", { name: /analyze session/i }));
    await user.click(await screen.findByRole("button", { name: /amit vs chen: 0% overlap/i }));
    expect(await screen.findByText(/no matching passages detected/i)).toBeInTheDocument();
  });

  it("lists per-student overlap with the clean student at zero", async () => {
    const user = userEvent.setup();
    setInvokeImpl(createFakeSessions(seedThreeStudents()).invoke);
    renderRouteAt("/sessions/sess-5");
    await screen.findByRole("heading", { name: /history test/i });

    await user.click(await screen.findByRole("button", { name: /analyze session/i }));
    await screen.findByRole("button", { name: /amit vs priya: \d+% overlap/i });
    const zero = await screen.findByText(/0% · 0\/\d+ eligible words/i);
    expect(zero.closest("li")).toHaveTextContent(/chen/i);
  });

  it("surfaces analysis failures kindly", async () => {
    const user = userEvent.setup();
    const { invoke: fake } = createFakeSessions(seedThreeStudents());
    setInvokeImpl((cmd, args) => {
      if (cmd === "analyze_session") {
        return Promise.reject({ code: "database", message: "Local database unavailable." });
      }
      return fake(cmd, args);
    });
    renderRouteAt("/sessions/sess-5");
    await screen.findByRole("heading", { name: /history test/i });

    await user.click(await screen.findByRole("button", { name: /analyze session/i }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/local database unavailable/i);
  });
});

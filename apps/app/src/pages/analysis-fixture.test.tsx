import { afterEach, describe, expect, it, vi } from "vitest";
import fs from "node:fs";
import path from "node:path";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { resetInvokeImpl, setInvokeImpl } from "../lib/tauri";
import {
  ExactAnalysisSchema,
  analyzeSession,
  type ExactAnalysis,
} from "../lib/analysis";
import {
  createFakeSessions,
  renderRouteAt,
  sessionFixture,
  studentFixture,
  submissionFixture,
} from "../test-utils";
// Recorded real-engine output: pinned by
// `recorded_fixture_matches_engine` in analysis_tests.rs. Never hand-edit
// the JSON; regenerate it from the engine. Loaded from disk (outside the
// Vite root, so plain `node:fs` instead of an import).
const fixtureJson = JSON.parse(
  fs.readFileSync(
    path.resolve(
      process.cwd(),
      "../../tests/fixtures/exact-analysis-sample.json",
    ),
    "utf-8",
  ),
) as unknown;

const Q =
  "Discuss the causes of the changes and their effects on daily life.";
const B = "All students must write their own answers in blue or black ink.";
const RAIL =
  "The rapid expansion of railway networks transformed trade across continents.";
const PHOTO_ORIG =
  "Photosynthesis converts light energy into chemical energy that can later be used by the plant.";
const PHOTO_PARA =
  "Through photosynthesis, plants convert light into stored chemical energy that can be used later.";

const TEXTS: Record<string, string> = {
  "stud-amit": `${Q} ${B} ${RAIL} ${PHOTO_ORIG} Amit closing words here.`,
  "stud-priya": `${Q} ${B} ${RAIL} Priya closing words here.`,
  "stud-chen": `${Q} ${B} Quantum tunneling lets particles cross barriers they cannot surmount ever.`,
  "stud-sara": `${Q} ${B} ${PHOTO_PARA} Sara closing words here.`,
};

const NAMES: Record<string, string> = {
  "stud-amit": "Amit",
  "stud-priya": "Priya",
  "stud-chen": "Chen",
  "stud-sara": "Sara",
};

function seedRecorded() {
  const ids = Object.keys(TEXTS);
  return {
    sessions: [sessionFixture({ id: "sess-sample", name: "Sample Session" })],
    students: ids.map((id) =>
      studentFixture({ id, session_id: "sess-sample", display_name: NAMES[id] ?? id }),
    ),
    submissions: ids.map((id, i) =>
      submissionFixture({
        id: `sub-${i}`,
        student_id: id,
        session_id: "sess-sample",
        original_text: TEXTS[id] ?? "",
      }),
    ),
  };
}

afterEach(() => {
  resetInvokeImpl();
});

describe("recorded engine fixture contract", () => {
  it("validates against the frontend schema", () => {
    const parsed = ExactAnalysisSchema.safeParse(fixtureJson);
    expect(parsed.success).toBe(true);
  });

  it("covers every evidence shape the UI renders", () => {
    const report = fixtureJson as unknown as ExactAnalysis;
    const kinds = new Set(
      report.pairs.flatMap((p) => p.passages.map((x) => x.kind)),
    );
    expect(kinds.has("exact")).toBe(true);
    expect(kinds.has("modified")).toBe(true);
    expect(
      report.pairs.some((p) => p.passages.some((x) => x.common_text)),
    ).toBe(true);
    expect(
      report.pairs.some((p) =>
        p.excluded.some((e) => e.reason === "Prompt"),
      ),
    ).toBe(true);
  });

  it("the fake harness output validates against the same schema", async () => {
    // Guards fake/real shape drift: whatever the fake returns must satisfy
    // the real command contract.
    const { invoke } = createFakeSessions(seedRecorded());
    const raw = await invoke("analyze_session_exact", { sessionId: "sess-sample" });
    expect(ExactAnalysisSchema.safeParse(raw).success).toBe(true);
  });
});

describe("recorded fixture renders real evidence", () => {
  it("shows the matrix, modified passage, common badge, and exclusions", async () => {
    const user = userEvent.setup();
    const { invoke: fake } = createFakeSessions(seedRecorded());
    setInvokeImpl((cmd, args) => {
      if (cmd === "analyze_session_exact") {
        return Promise.resolve(fixtureJson);
      }
      return fake(cmd, args);
    });
    renderRouteAt("/sessions/sess-sample");
    await screen.findByRole("heading", { name: /sample session/i });

    await user.click(screen.getByRole("button", { name: /analyze session/i }));
    // Amit ↔ Sara carries the recorded modified paraphrase passage.
    await user.click(
      await screen.findByRole("button", { name: /amit vs sara: \d+% overlap/i }),
    );

    const detail = await screen.findByLabelText(/pair detail/i);
    expect(within(detail).getByText(/modified match/i)).toBeInTheDocument();
    // Real recorded char spans highlight the actual paraphrased words.
    const marks = [...detail.querySelectorAll("mark")].filter((m) =>
      /stored chemical energy/i.test(m.textContent ?? ""),
    );
    expect(marks.length).toBeGreaterThanOrEqual(1);
    // The shared boilerplate passage carries the common classification.
    expect(within(detail).getByText(/shared across submissions/i)).toBeInTheDocument();
    // The teacher prompt is listed separately as excluded.
    const excludedList = within(detail).getByRole("list", { name: /excluded passages/i });
    expect(within(excludedList).getAllByText(/assignment question/i).length).toBeGreaterThanOrEqual(1);
  });

  it("analyzeSession accepts the recorded payload end to end", async () => {
    setInvokeImpl(vi.fn(async () => fixtureJson));
    const report = await analyzeSession("sess-sample");
    const pair = report.pairs.find(
      (p) => p.a_student_id === "stud-amit" && p.b_student_id === "stud-sara",
    );
    expect(pair).toBeDefined();
    expect(pair?.passages.some((x) => x.kind === "modified")).toBe(true);
  });
});

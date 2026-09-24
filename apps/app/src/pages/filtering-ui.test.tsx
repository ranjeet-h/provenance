import { afterEach, describe, expect, it } from "vitest";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { resetInvokeImpl, setInvokeImpl } from "../lib/tauri";
import { exclusionLabel } from "../lib/analysis";
import {
  createFakeSessions,
  renderRouteAt,
  sessionFixture,
  studentFixture,
  submissionFixture,
} from "../test-utils";

const QUESTION =
  "Discuss the causes and consequences of the industrial revolution with reference to urban growth.";
const ANSWER =
  "Crop rotation restored nitrogen and doubled wheat yields within three seasons of careful management.";

function seedWithPrompt() {
  return {
    sessions: [
      sessionFixture({ id: "sess-6", name: "Filtered Session", assignment_prompt: QUESTION }),
    ],
    students: [
      studentFixture({ id: "st-a", session_id: "sess-6", display_name: "Amit" }),
      studentFixture({ id: "st-b", session_id: "sess-6", display_name: "Priya" }),
    ],
    submissions: [
      submissionFixture({
        id: "sub-a",
        student_id: "st-a",
        session_id: "sess-6",
        original_text: `${QUESTION} ${ANSWER} Solo tail alpha words here.`,
      }),
      submissionFixture({
        id: "sub-b",
        student_id: "st-b",
        session_id: "sess-6",
        original_text: `${QUESTION} ${ANSWER} Solo tail beta words here.`,
      }),
    ],
  };
}

afterEach(() => {
  resetInvokeImpl();
});

describe("exclusion labels", () => {
  it("uses teacher-facing words, never engine terms", () => {
    expect(exclusionLabel("Prompt")).toBe("Assignment question");
    expect(exclusionLabel("Reference")).toBe("Reference text");
    expect(exclusionLabel("CommonSessionText")).toBe("Common session text");
  });
});

describe("session settings", () => {
  it("shows the saved question and toggle state", async () => {
    setInvokeImpl(createFakeSessions(seedWithPrompt()).invoke);
    renderRouteAt("/sessions/sess-6");
    await screen.findByRole("heading", { name: /filtered session/i });

    const settings = screen.getByRole("region", { name: /session settings/i });
    expect(
      within(settings).getByLabelText(/assignment question/i),
    ).toHaveValue(QUESTION);
    expect(
      within(settings).getByLabelText(/flag text shared across many submissions/i),
    ).toBeChecked();
  });

  it("saves a new question and persists it across reloads", async () => {
    const user = userEvent.setup();
    const { invoke } = createFakeSessions({
      sessions: [sessionFixture({ id: "sess-6", name: "Filtered Session" })],
      students: [studentFixture({ id: "st-a", session_id: "sess-6" })],
    });
    setInvokeImpl(invoke);

    const first = renderRouteAt("/sessions/sess-6");
    await screen.findByRole("heading", { name: /filtered session/i });
    const settings = screen.getByRole("region", { name: /session settings/i });
    const box = within(settings).getByLabelText(/assignment question/i);
    await user.type(box, QUESTION);
    await user.click(within(settings).getByRole("button", { name: /save settings/i }));
    expect(await within(settings).findByText(/settings saved/i)).toBeInTheDocument();
    first.unmount();

    renderRouteAt("/sessions/sess-6");
    await screen.findByRole("heading", { name: /filtered session/i });
    const reloaded = screen.getByRole("region", { name: /session settings/i });
    expect(within(reloaded).getByLabelText(/assignment question/i)).toHaveValue(QUESTION);
  });

  it("toggles common-text flagging off", async () => {
    const user = userEvent.setup();
    setInvokeImpl(createFakeSessions(seedWithPrompt()).invoke);
    renderRouteAt("/sessions/sess-6");
    await screen.findByRole("heading", { name: /filtered session/i });

    const settings = screen.getByRole("region", { name: /session settings/i });
    const toggle = within(settings).getByLabelText(/flag text shared across many submissions/i);
    expect(toggle).toBeChecked();
    await user.click(toggle);
    await user.click(within(settings).getByRole("button", { name: /save settings/i }));
    expect(await within(settings).findByText(/settings saved/i)).toBeInTheDocument();
    expect(within(settings).getByLabelText(/flag text shared across many submissions/i)).not.toBeChecked();
  });
});

describe("excluded evidence display", () => {  it("flags the answer while listing the question as excluded", async () => {
    const user = userEvent.setup();
    setInvokeImpl(createFakeSessions(seedWithPrompt()).invoke);
    renderRouteAt("/sessions/sess-6");
    await screen.findByRole("heading", { name: /filtered session/i });

    await user.click(screen.getByRole("button", { name: /analyze session/i }));
    await user.click(await screen.findByRole("button", { name: /amit vs priya: \d+% overlap/i }));

    const detail = await screen.findByLabelText(/pair detail/i);
    // Copied answer stays scored evidence (a <mark> on both sides).
    const nitrogenMarks = [...detail.querySelectorAll("mark")].filter((m) =>
      /nitrogen/i.test(m.textContent ?? ""),
    );
    expect(nitrogenMarks).toHaveLength(2);
    // Question is shown separately as excluded (both sides).
    const excludedList = within(detail).getByRole("list", { name: /excluded passages/i });
    expect(within(excludedList).getAllByText(/assignment question/i)).toHaveLength(2);
    expect(within(excludedList).getAllByText(/industrial revolution/i)).toHaveLength(2);
  });
});

describe("insufficient assessable text", () => {
  it("never reports a clean 0% for a prompt-only submission", async () => {
    const user = userEvent.setup();
    const prompt = "Discuss the water cycle and label every stage clearly.";
    setInvokeImpl(
      createFakeSessions({
        sessions: [sessionFixture({ id: "sess-7", name: "Prompt Only", assignment_prompt: prompt })],
        students: [
          studentFixture({ id: "st-a", session_id: "sess-7", display_name: "Amit" }),
          studentFixture({ id: "st-b", session_id: "sess-7", display_name: "Priya" }),
        ],
        submissions: [
          submissionFixture({ id: "sa", student_id: "st-a", session_id: "sess-7", original_text: prompt }),
          submissionFixture({ id: "sb", student_id: "st-b", session_id: "sess-7", original_text: prompt }),
        ],
      }).invoke,
    );
    renderRouteAt("/sessions/sess-7");
    await screen.findByRole("heading", { name: /prompt only/i });

    await user.click(screen.getByRole("button", { name: /analyze session/i }));
    await screen.findByRole("button", { name: /amit vs priya: not assessable/i });
    expect(await screen.findAllByText(/not assessable · insufficient assessable text/i)).not.toHaveLength(0);
    expect(screen.queryByText(/0% · 0\/0 eligible words/i)).not.toBeInTheDocument();
  });
});

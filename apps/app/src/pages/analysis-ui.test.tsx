import { afterEach, describe, expect, it, vi } from "vitest";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { resetInvokeImpl, resetListenImpl, setInvokeImpl, setListenImpl } from "../lib/tauri";
import {
  analyzeSession,
  cellCoverage,
  cellCoverageByKind,
  generateCertifiedStudentReport,
  generateStudentReportPdf,
  verifyCertifiedStudentReport,
} from "../lib/analysis";
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

  it("requests self-check PDFs and rejects malformed PDF bytes", async () => {
    const invoke = vi.fn(async () => [37, 80, 68, 70, 45]);
    setInvokeImpl(invoke);
    await expect(
      generateStudentReportPdf("sess-1", "student-1", true, "self_check"),
    ).resolves.toEqual([37, 80, 68, 70, 45]);
    expect(invoke).toHaveBeenCalledWith("generate_student_report_pdf", {
      sessionId: "sess-1",
      studentId: "student-1",
      anonymize: true,
      reportMode: "self_check",
    });

    setInvokeImpl(vi.fn(async () => [37, 999]));
    await expect(
      generateStudentReportPdf("sess-1", "student-1", false, "self_check"),
    ).rejects.toMatchObject({ code: "protocol" });
  });

  it("requires a complete signed export and verifies certificates through Tauri", async () => {
    const signedReportJson = JSON.stringify({
      schema_version: 1,
      lock_sha256: "a".repeat(64),
      report: { payload: { report_mode: "teacher" } },
      signature: {
        algorithm: "Ed25519",
        key_id: "b".repeat(64),
        public_key_hex: "c".repeat(64),
        signature_hex: "d".repeat(128),
      },
    });
    const invoke = vi.fn(async (cmd: string) => {
      if (cmd === "generate_certified_student_report") {
        return { pdf_bytes: [37, 80, 68, 70, 45], signed_report_json: signedReportJson };
      }
      return { status: "verified", signing_key_id: "b".repeat(64), note: "Signature valid." };
    });
    setInvokeImpl(invoke);
    await expect(generateCertifiedStudentReport("sess-1", "student-1", true)).resolves.toEqual({
      pdfBytes: [37, 80, 68, 70, 45],
      signedReportJson,
    });
    expect(invoke).toHaveBeenCalledWith("generate_certified_student_report", {
      sessionId: "sess-1",
      studentId: "student-1",
      anonymize: true,
    });
    await expect(verifyCertifiedStudentReport(signedReportJson)).resolves.toMatchObject({
      status: "verified",
      signing_key_id: "b".repeat(64),
    });

    setInvokeImpl(vi.fn(async () => ({ pdf_bytes: [37, 80, 68, 70, 45], signed_report_json: "{}" })));
    await expect(generateCertifiedStudentReport("sess-1", "student-1", false)).rejects.toMatchObject({ code: "protocol" });
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
  it("requires an explicit comparison source for a single submission", async () => {
    setInvokeImpl(
      createFakeSessions({
        sessions: [sessionFixture({ id: "sess-5", name: "History Test" })],
        students: [studentFixture({ id: "st-a", session_id: "sess-5" })],
        submissions: [submissionFixture({ id: "sub-a", student_id: "st-a", session_id: "sess-5" })],
      }).invoke,
    );
    renderRouteAt("/sessions/sess-5");
    await screen.findByRole("heading", { name: /history test/i });
    expect(await screen.findByRole("heading", { name: /choose a comparison source/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /choose a reference library/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /analyze session/i })).not.toBeInTheDocument();
  });

  it("analyzes one student against a selected library and downloads a clearly uncertified self-check", async () => {
    const user = userEvent.setup();
    const data = {
      sessions: [sessionFixture({ id: "sess-5", name: "History Test" })],
      students: [studentFixture({ id: "st-a", session_id: "sess-5", display_name: "Amit" })],
      submissions: [submissionFixture({
        id: "sub-a",
        student_id: "st-a",
        session_id: "sess-5",
        original_text: `An original opening paragraph. ${SHARED} An independent ending.`,
      })],
      referenceLibraries: [{
        id: "lib-1",
        name: "Course Archive",
        source_session_id: "old-session",
        source_session_name: "Previous history class",
        created_at: "2026-09-01T00:00:00Z",
        fingerprint_version: 1,
        normalization_version: 1,
        modified_version: 1,
      }],
      referenceSubmissions: [{
        id: "ref-1",
        library_id: "lib-1",
        source_label: "Archived student",
        source_filename: "essay.txt",
        source_type: "pasted_text" as const,
        original_text: SHARED,
        content_sha256: "fake-sha-1",
        created_at: "2026-09-01T00:00:00Z",
      }],
      selectedReferenceLibraries: { "sess-5": ["lib-1"] },
    };
    const { invoke } = createFakeSessions(data);
    const invokeSpy = vi.fn((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "generate_student_report_pdf") return Promise.resolve([37, 80, 68, 70, 45]);
      return invoke(cmd, args);
    });
    const originalCreate = Object.getOwnPropertyDescriptor(URL, "createObjectURL");
    const originalRevoke = Object.getOwnPropertyDescriptor(URL, "revokeObjectURL");
    const createObjectURL = vi.fn(() => "blob:self-check-report");
    const revokeObjectURL = vi.fn();
    Object.defineProperty(URL, "createObjectURL", { configurable: true, value: createObjectURL });
    Object.defineProperty(URL, "revokeObjectURL", { configurable: true, value: revokeObjectURL });
    const anchorClick = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
    try {
      setInvokeImpl(invokeSpy);
      renderRouteAt("/sessions/sess-5");
      await screen.findByRole("heading", { name: /history test/i });
      await user.click(await screen.findByRole("button", { name: /analyze session/i }));
      await screen.findByRole("heading", { name: /review against your selected sources/i });
      expect(screen.getByText(/course archive · 1 archived submission/i)).toBeInTheDocument();
      expect(screen.getAllByText(/not teacher certified/i).length).toBeGreaterThan(0);
      await user.click(screen.getByRole("button", { name: /download self check pdf/i }));
      await screen.findByText(/self check pdf downloaded\. it is not teacher certified/i);
      expect(invokeSpy).toHaveBeenCalledWith("generate_student_report_pdf", {
        sessionId: "sess-5",
        studentId: "st-a",
        anonymize: false,
        reportMode: "self_check",
      });
      expect(createObjectURL).toHaveBeenCalledTimes(1);
      expect(anchorClick).toHaveBeenCalledTimes(1);
    } finally {
      anchorClick.mockRestore();
      if (originalCreate) Object.defineProperty(URL, "createObjectURL", originalCreate);
      else Reflect.deleteProperty(URL, "createObjectURL");
      if (originalRevoke) Object.defineProperty(URL, "revokeObjectURL", originalRevoke);
      else Reflect.deleteProperty(URL, "revokeObjectURL");
    }
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

  it("locks a reviewed session and downloads the signed PDF plus verification JSON", async () => {
    const user = userEvent.setup();
    const { invoke } = createFakeSessions(seedThreeStudents());
    const invokeSpy = vi.fn((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "generate_certified_student_report") {
        return Promise.resolve({
          pdf_bytes: [37, 80, 68, 70, 45],
          signed_report_json: JSON.stringify({
            schema_version: 1,
            lock_sha256: "a".repeat(64),
            report: { payload: { report_mode: "teacher" } },
            signature: {
              algorithm: "Ed25519",
              key_id: "b".repeat(64),
              public_key_hex: "c".repeat(64),
              signature_hex: "d".repeat(128),
            },
          }),
        });
      }
      return invoke(cmd, args);
    });
    const originalCreate = Object.getOwnPropertyDescriptor(URL, "createObjectURL");
    const originalRevoke = Object.getOwnPropertyDescriptor(URL, "revokeObjectURL");
    const createObjectURL = vi.fn(() => "blob:student-report");
    const revokeObjectURL = vi.fn();
    Object.defineProperty(URL, "createObjectURL", { configurable: true, value: createObjectURL });
    Object.defineProperty(URL, "revokeObjectURL", { configurable: true, value: revokeObjectURL });
    const anchorClick = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
    try {
      setInvokeImpl(invokeSpy);
      renderRouteAt("/sessions/sess-5");
      await user.click(await screen.findByRole("button", { name: /analyze session/i }));
      await screen.findByRole("heading", { name: /freeze the reviewed evidence/i });
      expect(screen.getAllByRole("button", { name: /download self check pdf/i })).toHaveLength(3);
      await user.click(await screen.findByRole("button", { name: /lock session for certification/i }));
      const reportButton = await screen.findByRole("button", { name: /generate certified report for amit/i });
      await vi.waitFor(() => expect(reportButton).toBeEnabled());
      await user.click(screen.getByRole("checkbox", { name: /anonymize names and filenames/i }));
      await user.click(reportButton);
      await vi.waitFor(() => {
        expect(invokeSpy).toHaveBeenCalledWith("generate_certified_student_report", {
          sessionId: "sess-5",
          studentId: "st-a",
          anonymize: true,
        });
      });
      expect(createObjectURL).toHaveBeenCalledTimes(2);
      expect(anchorClick).toHaveBeenCalledTimes(2);
      expect(revokeObjectURL).toHaveBeenCalledWith("blob:student-report");
    } finally {
      anchorClick.mockRestore();
      if (originalCreate) Object.defineProperty(URL, "createObjectURL", originalCreate);
      else Reflect.deleteProperty(URL, "createObjectURL");
      if (originalRevoke) Object.defineProperty(URL, "revokeObjectURL", originalRevoke);
      else Reflect.deleteProperty(URL, "revokeObjectURL");
    }
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

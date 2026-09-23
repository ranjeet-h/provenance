import { afterEach, describe, expect, it } from "vitest";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { resetInvokeImpl, setInvokeImpl } from "../lib/tauri";
import {
  createFakeSessions,
  renderRouteAt,
  sessionFixture,
  studentFixture,
  submissionFixture,
} from "../test-utils";

const library = {
  id: "lib-biology-2025",
  name: "Biology 2025",
  source_session_id: "source-session",
  source_session_name: "Biology 2025 class",
  created_at: "2026-08-01T00:00:00.000Z",
  fingerprint_version: 1,
  normalization_version: 1,
  modified_version: 1,
};

const archivedText =
  "A stable archive preserves the original submitted words for future comparison without changing evidence.";

function currentSessionSeed() {
  return {
    sessions: [sessionFixture({ id: "current-session", name: "Biology 2026" })],
    students: [
      studentFixture({ id: "current-a", session_id: "current-session", display_name: "Amit" }),
      studentFixture({ id: "current-b", session_id: "current-session", display_name: "Priya" }),
    ],
    submissions: [
      submissionFixture({
        id: "current-sub-a",
        student_id: "current-a",
        session_id: "current-session",
        original_text: `${archivedText} with a little new context added at the end.`,
      }),
      submissionFixture({
        id: "current-sub-b",
        student_id: "current-b",
        session_id: "current-session",
        original_text: "Separate work about plant cells and energy conversion in leaves.",
      }),
    ],
  };
}

afterEach(() => resetInvokeImpl());

describe("reference libraries", () => {
  it("archives a completed analyzed session and lists the immutable library", async () => {
    const user = userEvent.setup();
    const seed = currentSessionSeed();
    setInvokeImpl(createFakeSessions(seed).invoke);
    renderRouteAt("/sessions/current-session");

    await screen.findByRole("heading", { name: /biology 2026/i });
    await user.click(await screen.findByRole("button", { name: /analyze session/i }));
    await screen.findByRole("heading", { name: /per-student overlap/i });
    await user.click(screen.getByRole("button", { name: /archive session/i }));
    expect(await screen.findByRole("status")).toHaveTextContent(/archived as “biology 2026”/i);

    await user.click(screen.getByRole("link", { name: /reference libraries/i }));
    expect(await screen.findByRole("heading", { name: /biology 2026/i })).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: /view sources/i })).toBeInTheDocument();
  });

  it("uses the selected library in a separate historical report section", async () => {
    const user = userEvent.setup();
    const seed = {
      ...currentSessionSeed(),
      referenceLibraries: [library],
      referenceSubmissions: [
        {
          id: "ref-one",
          library_id: library.id,
          source_label: "Archived student",
          source_filename: "answer.txt",
          source_type: "txt_file" as const,
          original_text: archivedText,
          content_sha256: "source-hash",
          created_at: library.created_at,
        },
      ],
    };
    const { db, invoke } = createFakeSessions(seed);
    setInvokeImpl(invoke);
    renderRouteAt("/sessions/current-session");

    const checkbox = await screen.findByRole("checkbox", { name: /biology 2025/i });
    await user.click(checkbox);
    expect(await screen.findByText(/comparison libraries saved/i)).toBeInTheDocument();
    expect(db.selectedReferenceLibraries["current-session"]).toEqual([library.id]);

    await user.click(await screen.findByRole("button", { name: /analyze session/i }));
    expect(await screen.findByRole("region", { name: /historical reference comparisons/i })).toBeInTheDocument();
    expect(screen.getByText(/1 historical submissions/i)).toBeInTheDocument();
    expect(screen.getByText(/not included in current-student pair scores/i)).toBeInTheDocument();
  });

  it("shows archived content and requires confirmation before removing a library", async () => {
    const user = userEvent.setup();
    const { invoke } = createFakeSessions({
      referenceLibraries: [library],
      referenceSubmissions: [
        {
          id: "ref-one",
          library_id: library.id,
          source_label: "Archived student",
          source_filename: null,
          source_type: "pasted_text",
          original_text: archivedText,
          content_sha256: "source-hash",
          created_at: library.created_at,
        },
      ],
    });
    setInvokeImpl(invoke);
    renderRouteAt("/reference-libraries");

    await user.click(await screen.findByRole("button", { name: /view sources/i }));
    expect(await screen.findByText(archivedText)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^remove$/i }));
    const dialog = await screen.findByRole("dialog", { name: /remove biology 2025/i });
    await user.click(within(dialog).getByRole("button", { name: /remove library/i }));
    expect(await screen.findByText(/no reference libraries yet/i)).toBeInTheDocument();
  });
});

import { afterEach, describe, expect, it } from "vitest";
import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { resetInvokeImpl, setInvokeImpl } from "../lib/tauri";
import {
  createFakeSessions,
  renderRouteAt,
  sessionFixture,
  studentFixture,
  submissionFixture,
} from "../test-utils";

function renderDetailWithFake(seedSubmissions = false) {
  const { invoke } = createFakeSessions({
    sessions: [sessionFixture({ id: "sess-9", name: "Writing Task" })],
    students: [
      studentFixture({
        id: "stud-9",
        session_id: "sess-9",
        display_name: "Amit",
      }),
    ],
    submissions: seedSubmissions
      ? [
          submissionFixture({
            id: "sub-9",
            student_id: "stud-9",
            session_id: "sess-9",
            original_text: "Original answer text.",
          }),
        ]
      : [],
  });
  setInvokeImpl(invoke);
  return renderRouteAt("/sessions/sess-9");
}

afterEach(() => {
  resetInvokeImpl();
});

describe("submission dialog", () => {
  it("pastes text, shows a preview, and saves", async () => {
    const user = userEvent.setup();
    renderDetailWithFake();
    await screen.findByRole("heading", { name: /writing task/i });

    await user.click(screen.getByRole("button", { name: /add submission/i }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveAccessibleName(/add submission — amit/i);

    const body = "Photosynthesis converts light energy into chemical energy.";
    await user.type(screen.getByLabelText(/submission text/i), body);
    expect(screen.getByLabelText(/submission preview/i)).toHaveTextContent(
      "Photosynthesis converts light",
    );
    await user.click(screen.getByRole("button", { name: /save submission/i }));

    const list = await screen.findByRole("list", { name: /student list/i });
    expect(
      await within(list).findByText(/pasted text · 58 characters/i),
    ).toBeInTheDocument();
    expect(
      within(list).getByText(
        "Photosynthesis converts light energy into chemical energy.",
      ),
    ).toBeInTheDocument();
  });

  it("blocks an empty paste with an inline error", async () => {
    const user = userEvent.setup();
    renderDetailWithFake();
    await screen.findByRole("heading", { name: /writing task/i });

    await user.click(screen.getByRole("button", { name: /add submission/i }));
    await screen.findByRole("dialog");
    await user.click(screen.getByRole("button", { name: /save submission/i }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      /enter or upload/i,
    );
    // Still open, nothing saved.
    expect(screen.getByText(/no submission yet/i)).toBeInTheDocument();
  });

  it("uploads a TXT file and saves it as a text file", async () => {
    const user = userEvent.setup();
    renderDetailWithFake();
    await screen.findByRole("heading", { name: /writing task/i });

    await user.click(screen.getByRole("button", { name: /add submission/i }));
    await screen.findByRole("dialog");
    await user.click(screen.getByRole("tab", { name: /upload file/i }));

    const file = new File(["First line.\nSecond line."], "essay.txt", {
      type: "text/plain",
    });
    // user-event upload does not dispatch change reliably in jsdom; fire it directly.
    fireEvent.change(screen.getByLabelText(/assignment file/i), {
      target: { files: [file] },
    });
    expect(
      await screen.findByText(/selected: essay\.txt/i),
    ).toBeInTheDocument();
    expect(
      await screen.findByLabelText(/submission preview/i),
    ).toHaveTextContent("First line.");

    await user.click(screen.getByRole("button", { name: /save submission/i }));
    const txtList = await screen.findByRole("list", { name: /student list/i });
    expect(
      await within(txtList).findByText(
        /text file · essay\.txt · 24 characters/i,
      ),
    ).toBeInTheDocument();
  });

  it("uploads a Markdown file with the markdown label", async () => {
    const user = userEvent.setup();
    renderDetailWithFake();
    await screen.findByRole("heading", { name: /writing task/i });

    await user.click(screen.getByRole("button", { name: /add submission/i }));
    await screen.findByRole("dialog");
    await user.click(screen.getByRole("tab", { name: /upload file/i }));

    const file = new File(["# Title\n\nBody text."], "notes.md", {
      type: "text/markdown",
    });
    fireEvent.change(screen.getByLabelText(/assignment file/i), {
      target: { files: [file] },
    });
    expect(await screen.findByText(/selected: notes\.md/i)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /save submission/i }));
    const mdList = await screen.findByRole("list", { name: /student list/i });
    expect(
      await within(mdList).findByText(/markdown · notes\.md · 19 characters/i),
    ).toBeInTheDocument();
  });

  it("rejects a binary file with a friendly error", async () => {
    const user = userEvent.setup();
    renderDetailWithFake();
    await screen.findByRole("heading", { name: /writing task/i });

    await user.click(screen.getByRole("button", { name: /add submission/i }));
    await screen.findByRole("dialog");
    await user.click(screen.getByRole("tab", { name: /upload file/i }));

    const file = new File(
      [new Uint8Array([0x89, 0x50, 0xff, 0xfe])],
      "photo.png",
      {
        type: "image/png",
      },
    );
    fireEvent.change(screen.getByLabelText(/assignment file/i), {
      target: { files: [file] },
    });
    expect(await screen.findByRole("alert")).toHaveTextContent(
      /unsupported file type/i,
    );
  });

  it("replaces an existing submission, keeping a single row", async () => {
    const user = userEvent.setup();
    renderDetailWithFake(true);
    await screen.findByText(/original answer text\./i);

    await user.click(screen.getByRole("button", { name: /view \/ replace/i }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveAccessibleName(/replace submission — amit/i);
    const box = screen.getByLabelText(/submission text/i);
    expect(box).toHaveValue("Original answer text.");
    await user.clear(box);
    await user.type(box, "Revised answer text here.");
    await user.click(
      screen.getByRole("button", { name: /replace submission/i }),
    );

    const replacedList = await screen.findByRole("list", {
      name: /student list/i,
    });
    expect(
      await within(replacedList).findByText(/revised answer text here\./i),
    ).toBeInTheDocument();
    expect(
      within(replacedList).getByText(/pasted text · 25 characters/i),
    ).toBeInTheDocument();
  });

  it("failed replacement uploads keep the existing submission", async () => {
    // Regression: a replacement file that cannot be read must surface an
    // error and leave the stored submission untouched (never a stale or
    // half-written save).
    const user = userEvent.setup();
    renderDetailWithFake(true);
    await screen.findByText(/original answer text\./i);

    await user.click(screen.getByRole("button", { name: /view \/ replace/i }));
    await screen.findByRole("dialog");
    await user.click(screen.getByRole("tab", { name: /upload file/i }));

    // Invalid UTF-8 bytes behind a .txt name.
    const bad = new File(
      [new Uint8Array([0xff, 0xfe, 0x00, 0x28])],
      "broken.txt",
      {
        type: "text/plain",
      },
    );
    fireEvent.change(screen.getByLabelText(/assignment file/i), {
      target: { files: [bad] },
    });
    await user.click(
      screen.getByRole("button", { name: /replace submission/i }),
    );
    // The save path rejects the undecodable bytes (client already warned on
    // choose); either friendly message proves the failure surfaced.
    expect(await screen.findByRole("alert")).toHaveTextContent(
      /not valid UTF-8 text/i,
    );

    // Dialog stays open on failure; dismiss and verify the old text survived.
    await user.click(screen.getByRole("button", { name: /cancel/i }));
    const list = await screen.findByRole("list", { name: /student list/i });
    expect(
      within(list).getByText(/original answer text\./i),
    ).toBeInTheDocument();
  });

  it("oversize replacement uploads are rejected before saving", async () => {
    const user = userEvent.setup();
    renderDetailWithFake(true);
    await screen.findByText(/original answer text\./i);

    await user.click(screen.getByRole("button", { name: /view \/ replace/i }));
    await screen.findByRole("dialog");
    await user.click(screen.getByRole("tab", { name: /upload file/i }));

    const big = new File([new Uint8Array(5_000_001).fill(0x61)], "big.txt", {
      type: "text/plain",
    });
    fireEvent.change(screen.getByLabelText(/assignment file/i), {
      target: { files: [big] },
    });
    await user.click(
      screen.getByRole("button", { name: /replace submission/i }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(/too large/i);

    await user.click(screen.getByRole("button", { name: /cancel/i }));
    const list = await screen.findByRole("list", { name: /student list/i });
    expect(
      within(list).getByText(/original answer text\./i),
    ).toBeInTheDocument();
  });

  it("submission survives a full page reload", async () => {
    const user = userEvent.setup();
    const { invoke } = createFakeSessions({
      sessions: [sessionFixture({ id: "sess-9", name: "Writing Task" })],
      students: [
        studentFixture({
          id: "stud-9",
          session_id: "sess-9",
          display_name: "Amit",
        }),
      ],
    });
    setInvokeImpl(invoke);

    const first = renderRouteAt("/sessions/sess-9");
    await screen.findByRole("heading", { name: /writing task/i });
    await user.click(screen.getByRole("button", { name: /add submission/i }));
    await screen.findByRole("dialog");
    await user.type(
      screen.getByLabelText(/submission text/i),
      "Persisted words.",
    );
    await user.click(screen.getByRole("button", { name: /save submission/i }));
    const savedList = await screen.findByRole("list", {
      name: /student list/i,
    });
    await within(savedList).findByText(/pasted text · 16 characters/i);
    first.unmount();

    // Fresh app boot against the same local database.
    renderRouteAt("/sessions/sess-9");
    const reloadedList = await screen.findByRole("list", {
      name: /student list/i,
    });
    expect(
      await within(reloadedList).findByText(/persisted words\./i),
    ).toBeInTheDocument();
    expect(
      within(reloadedList).getByText(/pasted text · 16 characters/i),
    ).toBeInTheDocument();
  });
});

describe("file uploads", () => {
  it("tracks how the text was obtained: source, filename, size", async () => {
    setInvokeImpl(
      createFakeSessions({
        sessions: [sessionFixture({ id: "sess-9", name: "Writing Task" })],
        students: [
          studentFixture({
            id: "stud-9",
            session_id: "sess-9",
            display_name: "Amit",
          }),
        ],
        submissions: [
          submissionFixture({
            id: "sub-9",
            student_id: "stud-9",
            session_id: "sess-9",
            source_type: "pdf_digital",
            source_filename: "assignment.pdf",
            original_text: "Extracted PDF words here.",
          }),
        ],
      }).invoke,
    );
    renderRouteAt("/sessions/sess-9");
    const list = await screen.findByRole("list", { name: /student list/i });
    expect(
      await within(list).findByText(
        /pdf document · assignment\.pdf · 25 characters/i,
      ),
    ).toBeInTheDocument();
  });
  it("uploads a PDF and labels it a PDF document", async () => {
    const user = userEvent.setup();
    renderDetailWithFake();
    await screen.findByRole("heading", { name: /writing task/i });

    await user.click(screen.getByRole("button", { name: /add submission/i }));
    await screen.findByRole("dialog");
    await user.click(screen.getByRole("tab", { name: /upload file/i }));

    const file = new File(
      ["Digital PDF assignment text here."],
      "assignment.pdf",
      {
        type: "application/pdf",
      },
    );
    fireEvent.change(screen.getByLabelText(/assignment file/i), {
      target: { files: [file] },
    });
    expect(
      await screen.findByText(/selected: assignment\.pdf/i),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/text will be extracted when you save/i),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /save submission/i }));

    const list = await screen.findByRole("list", { name: /student list/i });
    expect(
      await within(list).findByText(
        /pdf document · assignment\.pdf · 33 characters/i,
      ),
    ).toBeInTheDocument();
  });

  it("uploads a DOCX and labels it a Word document", async () => {
    const user = userEvent.setup();
    renderDetailWithFake();
    await screen.findByRole("heading", { name: /writing task/i });

    await user.click(screen.getByRole("button", { name: /add submission/i }));
    await screen.findByRole("dialog");
    await user.click(screen.getByRole("tab", { name: /upload file/i }));

    const file = new File(["Word assignment text here."], "essay.docx", {
      type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    });
    fireEvent.change(screen.getByLabelText(/assignment file/i), {
      target: { files: [file] },
    });
    await user.click(screen.getByRole("button", { name: /save submission/i }));

    const list = await screen.findByRole("list", { name: /student list/i });
    expect(
      await within(list).findByText(
        /word document · essay\.docx · 26 characters/i,
      ),
    ).toBeInTheDocument();
  });

  it("shows the explicit unsupported message for scanned PDFs", async () => {
    const user = userEvent.setup();
    renderDetailWithFake();
    await screen.findByRole("heading", { name: /writing task/i });

    await user.click(screen.getByRole("button", { name: /add submission/i }));
    await screen.findByRole("dialog");
    await user.click(screen.getByRole("tab", { name: /upload file/i }));

    const file = new File(["   "], "scan.pdf", { type: "application/pdf" });
    fireEvent.change(screen.getByLabelText(/assignment file/i), {
      target: { files: [file] },
    });
    await user.click(screen.getByRole("button", { name: /save submission/i }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      /scanned\/image-only PDFs are unsupported/i,
    );
    expect(screen.getByText(/no submission yet/i)).toBeInTheDocument();
  });
});

describe("filename routing helpers", () => {
  it("routes extensions to preview behavior and sources", async () => {
    const {
      extensionOf,
      isPreviewableText,
      isSupportedFile,
      sourceForFilename,
    } = await import("../lib/filenames");
    expect(extensionOf("Essay.PDF")).toBe("pdf");
    expect(extensionOf("no-ext")).toBe("");
    expect(isSupportedFile("a.docx")).toBe(true);
    expect(isSupportedFile("photo.png")).toBe(false);
    expect(isPreviewableText("notes.md")).toBe(true);
    expect(isPreviewableText("scan.pdf")).toBe(false);
    expect(sourceForFilename("notes.markdown")).toBe("markdown_file");
    expect(sourceForFilename("scan.pdf")).toBe("pdf_digital");
    expect(sourceForFilename("essay.docx")).toBe("docx_file");
    expect(sourceForFilename("a.txt")).toBe("txt_file");
  });
});

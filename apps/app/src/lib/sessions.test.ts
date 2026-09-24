import { afterEach, describe, expect, it, vi } from "vitest";
import { resetInvokeImpl, setInvokeImpl } from "./tauri";
import {
  addStudent,
  createSession,
  deleteSession,
  getSession,
  getSubmission,
  listSessions,
  listStudents,
  listSubmissions,
  removeStudent,
  saveFileSubmission,
  saveTextSubmission,
  updateSession,
} from "./sessions";
import { sessionFixture, studentFixture, submissionFixture } from "../test-utils";

afterEach(() => {
  resetInvokeImpl();
});

describe("sessions API", () => {
  it("lists sessions through the Tauri boundary", async () => {
    const sessions = [sessionFixture()];
    setInvokeImpl(vi.fn(async () => sessions));
    await expect(listSessions()).resolves.toEqual(sessions);
  });

  it("creates a session with name and subject", async () => {
    const mock = vi.fn(async () => sessionFixture());
    setInvokeImpl(mock);
    await createSession({ name: "Chem", subject: "Science" });
    expect(mock).toHaveBeenCalledWith("create_session", {
      name: "Chem",
      subject: "Science",
    });
  });

  it("sends null subject when omitted", async () => {
    const mock = vi.fn(async () => sessionFixture({ subject: null }));
    setInvokeImpl(mock);
    await createSession({ name: "Chem" });
    expect(mock).toHaveBeenCalledWith("create_session", { name: "Chem", subject: null });
  });

  it("maps core errors to stable codes", async () => {
    setInvokeImpl(vi.fn(async () => {
      throw { code: "validation", message: "session name must not be empty" };
    }));
    await expect(createSession({ name: "  " })).rejects.toMatchObject({
      code: "validation",
    });
  });

  it("rejects malformed payloads as protocol errors", async () => {
    setInvokeImpl(vi.fn(async () => ({ bogus: true })));
    await expect(listSessions()).rejects.toMatchObject({ code: "protocol" });
  });

  it("rejects non-object failures as unknown errors", async () => {
    setInvokeImpl(vi.fn(async () => {
      throw new Error("boom");
    }));
    await expect(getSession("x")).rejects.toMatchObject({ code: "unknown" });
  });

  it("updates, deletes, and manages students", async () => {
    const mock = vi.fn(async (cmd: string) => {
      if (cmd === "update_session") return sessionFixture({ name: "Renamed" });
      if (cmd === "list_students") return [studentFixture()];
      if (cmd === "add_student") return studentFixture({ display_name: "Priya" });
      return null;
    });
    setInvokeImpl(mock);
    await expect(
      updateSession("sess-1", { name: "Renamed" }).then((s) => s.name),
    ).resolves.toBe("Renamed");
    await expect(listStudents("sess-1")).resolves.toHaveLength(1);
    await expect(
      addStudent("sess-1", "Priya").then((s) => s.display_name),
    ).resolves.toBe("Priya");
    await expect(deleteSession("sess-1")).resolves.toBeUndefined();
    await expect(removeStudent("stud-1")).resolves.toBeUndefined();
  });
});

describe("submission API", () => {
  it("saves text with source, filename, and text", async () => {
    const mock = vi.fn(async () => submissionFixture());
    setInvokeImpl(mock);
    await saveTextSubmission({
      studentId: "stud-1",
      sourceType: "txt_file",
      filename: "essay.txt",
      text: "hello",
    });
    expect(mock).toHaveBeenCalledWith("save_text_submission", {
      studentId: "stud-1",
      sourceType: "txt_file",
      filename: "essay.txt",
      text: "hello",
    });
  });

  it("sends null filename for pasted text", async () => {
    const mock = vi.fn(async () => submissionFixture({ source_filename: null }));
    setInvokeImpl(mock);
    await saveTextSubmission({ studentId: "stud-1", sourceType: "pasted_text", text: "hi" });
    expect(mock).toHaveBeenCalledWith("save_text_submission", {
      studentId: "stud-1",
      sourceType: "pasted_text",
      filename: null,
      text: "hi",
    });
  });

  it("fetches one submission and lists a session's submissions", async () => {
    const mock = vi.fn(async (cmd: string) => {
      if (cmd === "get_submission") return submissionFixture();
      return [submissionFixture()];
    });
    setInvokeImpl(mock);
    await expect(getSubmission("stud-1")).resolves.toMatchObject({ id: "sub-1" });
    await expect(listSubmissions("sess-1")).resolves.toHaveLength(1);
  });

  it("rejects malformed submission payloads as protocol errors", async () => {
    setInvokeImpl(vi.fn(async () => ({ nope: true })));
    await expect(getSubmission("stud-1")).rejects.toMatchObject({ code: "protocol" });
  });

  it("uploads file bytes and parses a saved digital outcome", async () => {
    const mock = vi.fn(async (cmd: string) => {
      if (cmd === "save_file_submission") {
        return {
          Digital: { text: "hello", pages: 2, source: "pdf_digital", filename: "a.pdf" },
        };
      }
      return null;
    });
    setInvokeImpl(mock);
    const saved = await saveFileSubmission({ studentId: "s", filename: "a.pdf", bytes: [104, 105] });
    expect(mock).toHaveBeenCalledWith("save_file_submission", {
      studentId: "s",
      filename: "a.pdf",
      bytes: [104, 105],
    });
    expect("Digital" in saved).toBe(true);

    setInvokeImpl(vi.fn(async () => ({ Digital: { nope: true } })));
    await expect(
      saveFileSubmission({ studentId: "s", filename: "a.pdf", bytes: [] }),
    ).rejects.toMatchObject({ code: "protocol" });
  });
});

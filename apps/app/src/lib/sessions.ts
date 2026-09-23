import { z } from "zod";
import { getInvokeImpl } from "./tauri";

// Keep in sync with provenance-core domain + packages/shared-types.

export const SessionSchema = z.object({
  id: z.string().min(1),
  name: z.string(),
  subject: z.string().nullish(),
  status: z.enum(["draft", "locked", "analyzed"]),
  assignment_prompt: z.string().nullish(),
  excluded_reference_text: z.string().nullish(),
  exclude_common_text: z.boolean(),
  created_at: z.string(),
  updated_at: z.string(),
});

export type Session = z.infer<typeof SessionSchema>;

export const SessionLockSummarySchema = z.object({
  session_id: z.string().min(1),
  locked_at: z.string().min(1),
  manifest_sha256: z.string().regex(/^[a-f0-9]{64}$/),
  signing_key_id: z.string().regex(/^[a-f0-9]{64}$/),
});

export type SessionLockSummary = z.infer<typeof SessionLockSummarySchema>;

export const StudentSchema = z.object({
  id: z.string().min(1),
  session_id: z.string().min(1),
  display_name: z.string(),
  created_at: z.string(),
});

export type Student = z.infer<typeof StudentSchema>;

export const SubmissionSchema = z.object({
  id: z.string().min(1),
  student_id: z.string().min(1),
  session_id: z.string().min(1),
  source_type: z.enum(["pasted_text", "txt_file", "markdown_file", "pdf_digital", "docx_file"]),
  status: z.enum(["draft", "ready"]),
  original_text: z.string(),
  source_filename: z.string().nullish(),
  content_sha256: z.string(),
  created_at: z.string(),
  updated_at: z.string(),
});

export type Submission = z.infer<typeof SubmissionSchema>;

export type SubmissionSource = Submission["source_type"];

export const FileIngestResultSchema = z.object({
  Digital: z.object({
    text: z.string(),
    pages: z.number(),
    source: z.enum(["pasted_text", "txt_file", "markdown_file", "pdf_digital", "docx_file"]),
    filename: z.string(),
  }),
});

export type FileIngestResult = z.infer<typeof FileIngestResultSchema>;

/** Must stay in sync with provenance-core MAX_SUBMISSION_BYTES. */
export const MAX_UPLOAD_BYTES = 5_000_000;

const SOURCE_LABELS: Record<SubmissionSource, string> = {
  pasted_text: "Pasted text",
  txt_file: "Text file",
  markdown_file: "Markdown",
  pdf_digital: "PDF document",
  docx_file: "Word document",
};

/** Teacher-facing label for a submission source. Never a technical term. */
export function sourceLabel(source: SubmissionSource): string {
  return SOURCE_LABELS[source];
}

export interface SessionsError {
  code: string;
  message: string;
}

function toSessionsError(raw: unknown): SessionsError {
  if (typeof raw === "string") {
    return { code: "unknown", message: raw };
  }
  const parsed = z
    .object({ code: z.string(), message: z.string() })
    .safeParse(raw);
  if (parsed.success) {
    return parsed.data;
  }
  return { code: "unknown", message: "Something went wrong. Please try again." };
}

/** Normalize any thrown value into a safe user-facing error. */
export function asSessionsError(raw: unknown): SessionsError {
  return toSessionsError(raw);
}

async function call<T>(cmd: string, args: Record<string, unknown>, schema: z.ZodType<T>): Promise<T> {
  let raw: unknown;
  try {
    raw = await getInvokeImpl()(cmd, args);
  } catch (err) {
    throw toSessionsError(err);
  }
  const parsed = schema.safeParse(raw);
  if (!parsed.success) {
    throw {
      code: "protocol",
      message: "Unexpected response from the app core.",
    } satisfies SessionsError;
  }
  return parsed.data;
}

async function callVoid(cmd: string, args: Record<string, unknown>): Promise<void> {
  try {
    await getInvokeImpl()(cmd, args);
  } catch (err) {
    throw toSessionsError(err);
  }
}

export function listSessions(): Promise<Session[]> {
  return call("list_sessions", {}, z.array(SessionSchema));
}

export function createSession(input: { name: string; subject?: string | null }): Promise<Session> {
  return call(
    "create_session",
    { name: input.name, subject: input.subject ?? null },
    SessionSchema,
  );
}

export function getSession(id: string): Promise<Session> {
  return call("get_session", { id }, SessionSchema);
}

export function lockSession(sessionId: string): Promise<SessionLockSummary> {
  return call("lock_session", { sessionId }, SessionLockSummarySchema);
}

export function getSessionLock(sessionId: string): Promise<SessionLockSummary | null> {
  return call(
    "get_session_lock",
    { sessionId },
    SessionLockSummarySchema.nullable(),
  );
}

export function updateSession(
  id: string,
  patch: {
    name?: string;
    subject?: string | null;
    assignmentPrompt?: string | null;
    excludedReferenceText?: string | null;
    excludeCommonText?: boolean;
  },
): Promise<Session> {
  return call("update_session", { id, ...patch }, SessionSchema);
}

export function deleteSession(id: string): Promise<void> {
  return callVoid("delete_session", { id });
}

export function listStudents(sessionId: string): Promise<Student[]> {
  return call("list_students", { sessionId }, z.array(StudentSchema));
}

export function addStudent(sessionId: string, displayName: string): Promise<Student> {
  return call("add_student", { sessionId, displayName }, StudentSchema);
}

export function removeStudent(id: string): Promise<void> {
  return callVoid("remove_student", { id });
}

export async function saveFileSubmission(input: {
  studentId: string;
  filename: string;
  bytes: number[];
}): Promise<FileIngestResult> {
  let raw: unknown;
  try {
    raw = await getInvokeImpl()("save_file_submission", {
      studentId: input.studentId,
      filename: input.filename,
      bytes: input.bytes,
    });
  } catch (err) {
    throw toSessionsError(err);
  }
  const parsed = FileIngestResultSchema.safeParse(raw);
  if (!parsed.success) {
    throw {
      code: "protocol",
      message: "Unexpected response from the app core.",
    } satisfies SessionsError;
  }
  return parsed.data;
}

export function saveTextSubmission(input: {
  studentId: string;
  sourceType: SubmissionSource;
  filename?: string | null;
  text: string;
}): Promise<Submission> {
  return call(
    "save_text_submission",
    {
      studentId: input.studentId,
      sourceType: input.sourceType,
      filename: input.filename ?? null,
      text: input.text,
    },
    SubmissionSchema,
  );
}

export function getSubmission(studentId: string): Promise<Submission> {
  return call("get_submission", { studentId }, SubmissionSchema);
}

export function listSubmissions(sessionId: string): Promise<Submission[]> {
  return call("list_submissions", { sessionId }, z.array(SubmissionSchema));
}

import { z } from "zod";
import { asSessionsError } from "./sessions";
import { getInvokeImpl } from "./tauri";

export const ReferenceLibrarySchema = z.object({
  id: z.string().min(1),
  name: z.string(),
  source_session_id: z.string(),
  source_session_name: z.string(),
  created_at: z.string(),
  fingerprint_version: z.number(),
  normalization_version: z.number(),
  modified_version: z.number(),
});

export type ReferenceLibrary = z.infer<typeof ReferenceLibrarySchema>;

export const ReferenceSubmissionSchema = z.object({
  id: z.string().min(1),
  library_id: z.string().min(1),
  source_label: z.string(),
  source_filename: z.string().nullable(),
  source_type: z.enum(["pasted_text", "txt_file", "markdown_file", "pdf_digital", "docx_file"]),
  original_text: z.string(),
  content_sha256: z.string(),
  created_at: z.string(),
});

export type ReferenceSubmission = z.infer<typeof ReferenceSubmissionSchema>;

async function call<T>(cmd: string, args: Record<string, unknown>, schema: z.ZodType<T>): Promise<T> {
  let raw: unknown;
  try {
    raw = await getInvokeImpl()(cmd, args);
  } catch (err) {
    throw asSessionsError(err);
  }
  const parsed = schema.safeParse(raw);
  if (!parsed.success) {
    throw { code: "protocol", message: "Unexpected response from the app core." };
  }
  return parsed.data;
}

async function callVoid(cmd: string, args: Record<string, unknown>): Promise<void> {
  try {
    await getInvokeImpl()(cmd, args);
  } catch (err) {
    throw asSessionsError(err);
  }
}

export function listReferenceLibraries(): Promise<ReferenceLibrary[]> {
  return call("list_reference_libraries", {}, z.array(ReferenceLibrarySchema));
}

export function listReferenceSubmissions(libraryId: string): Promise<ReferenceSubmission[]> {
  return call(
    "list_reference_submissions",
    { libraryId },
    z.array(ReferenceSubmissionSchema),
  );
}

export function archiveCompletedSession(
  sessionId: string,
  libraryName: string,
): Promise<ReferenceLibrary> {
  return call(
    "archive_completed_session",
    { sessionId, libraryName },
    ReferenceLibrarySchema,
  );
}

export function selectedReferenceLibraryIds(sessionId: string): Promise<string[]> {
  return call("selected_reference_library_ids", { sessionId }, z.array(z.string()));
}

export function setSessionReferenceLibraries(
  sessionId: string,
  libraryIds: string[],
): Promise<string[]> {
  return call(
    "set_session_reference_libraries",
    { sessionId, libraryIds },
    z.array(z.string()),
  );
}

export function deleteReferenceLibrary(id: string): Promise<void> {
  return callVoid("delete_reference_library", { id });
}

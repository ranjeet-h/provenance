import { z } from "zod";
import { getInvokeImpl } from "./tauri";
import { asSessionsError } from "./sessions";

export const PassageSchema = z.object({
  kind: z.enum(["exact", "modified"]),
  identity: z.number().nullish(),
  /** Explanatory flag only: common passages are fully counted. */
  common_text: z.boolean(),
  a_token_start: z.number(),
  a_token_end: z.number(),
  b_token_start: z.number(),
  b_token_end: z.number(),
  a_char_start: z.number(),
  a_char_end: z.number(),
  b_char_start: z.number(),
  b_char_end: z.number(),
  tokens: z.number(),
});

export type Passage = z.infer<typeof PassageSchema>;

export const ExcludedEvidenceSchema = z.object({
  side: z.enum(["a", "b"]),
  token_start: z.number(),
  token_end: z.number(),
  char_start: z.number(),
  char_end: z.number(),
  reason: z.enum(["Prompt", "Reference", "CommonSessionText"]),
  tokens: z.number(),
});

export type ExcludedEvidence = z.infer<typeof ExcludedEvidenceSchema>;

const EXCLUSION_LABELS: Record<ExcludedEvidence["reason"], string> = {
  Prompt: "Assignment question",
  Reference: "Reference text",
  CommonSessionText: "Common session text",
};

/** Teacher-facing label for an exclusion reason. */
export function exclusionLabel(reason: ExcludedEvidence["reason"]): string {
  return EXCLUSION_LABELS[reason];
}

export const PairAnalysisSchema = z.object({
  a_student_id: z.string(),
  b_student_id: z.string(),
  coverage_a: z.number().nullable(),
  coverage_b: z.number().nullable(),
  passages: z.array(PassageSchema),
  excluded: z.array(ExcludedEvidenceSchema),
});

export type PairAnalysis = z.infer<typeof PairAnalysisSchema>;

export const StudentCoverageSchema = z.object({
  student_id: z.string(),
  coverage: z.number().nullable(),
  matched_tokens: z.number(),
  total_tokens: z.number(),
  eligible_tokens: z.number(),
});

export type StudentCoverage = z.infer<typeof StudentCoverageSchema>;

export const ExactAnalysisSchema = z.object({
  fingerprint_version: z.number(),
  normalization_version: z.number(),
  common_text_version: z.number(),
  modified_version: z.number(),
  common_text_applied: z.boolean(),
  prompt_applied: z.boolean(),
  pairs: z.array(PairAnalysisSchema),
  per_student: z.array(StudentCoverageSchema),
});

export type ExactAnalysis = z.infer<typeof ExactAnalysisSchema>;

export async function analyzeSession(sessionId: string): Promise<ExactAnalysis> {
  let raw: unknown;
  try {
    raw = await getInvokeImpl()("analyze_session_exact", { sessionId });
  } catch (err) {
    throw asSessionsError(err);
  }
  const parsed = ExactAnalysisSchema.safeParse(raw);
  if (!parsed.success) {
    throw { code: "protocol", message: "Unexpected response from the app core." };
  }
  return parsed.data;
}

/** Coverage of the row student against the column student in a pair. */
export function cellCoverage(
  pair: PairAnalysis,
  rowId: string,
): number | null {
  if (pair.a_student_id === rowId) return pair.coverage_a;
  if (pair.b_student_id === rowId) return pair.coverage_b;
  return null;
}

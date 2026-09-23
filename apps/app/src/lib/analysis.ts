import { z } from "zod";
import { getInvokeImpl, getListenImpl, type UnlistenFn } from "./tauri";
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
  exact_coverage_a: z.number().nullable(),
  exact_coverage_b: z.number().nullable(),
  modified_coverage_a: z.number().nullable(),
  modified_coverage_b: z.number().nullable(),
  passages: z.array(PassageSchema),
  excluded: z.array(ExcludedEvidenceSchema),
});

export type PairAnalysis = z.infer<typeof PairAnalysisSchema>;

export const StudentCoverageSchema = z.object({
  student_id: z.string(),
  coverage: z.number().nullable(),
  exact_coverage: z.number().nullable(),
  modified_coverage: z.number().nullable(),
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

export const AnalysisProgressSchema = z.object({
  session_id: z.string(),
  stage: z.enum([
    "preparing",
    "exact",
    "modified",
    "aligning",
    "scoring",
    "saving",
    "complete",
    "failed",
  ]),
  fraction: z.number().min(0).max(1),
  completed_pairs: z.number().int().nonnegative(),
  total_pairs: z.number().int().nonnegative(),
  cached_pairs: z.number().int().nonnegative(),
});

export type AnalysisProgress = z.infer<typeof AnalysisProgressSchema>;

export async function analyzeSession(
  sessionId: string,
  onProgress?: (progress: AnalysisProgress) => void,
): Promise<ExactAnalysis> {
  let raw: unknown;
  let unlisten: UnlistenFn | undefined;
  try {
    if (onProgress) {
      unlisten = await getListenImpl()("analysis-progress", (event) => {
        const parsed = AnalysisProgressSchema.safeParse(event.payload);
        if (parsed.success && parsed.data.session_id === sessionId) {
          onProgress(parsed.data);
        }
      });
    }
    raw = await getInvokeImpl()("analyze_session", { sessionId });
  } catch (err) {
    throw asSessionsError(err);
  } finally {
    unlisten?.();
  }
  const parsed = ExactAnalysisSchema.safeParse(raw);
  if (!parsed.success) {
    throw { code: "protocol", message: "Unexpected response from the app core." };
  }
  return parsed.data;
}

export async function loadSessionAnalysis(sessionId: string): Promise<ExactAnalysis | null> {
  let raw: unknown;
  try {
    raw = await getInvokeImpl()("get_session_analysis", { sessionId });
  } catch (err) {
    throw asSessionsError(err);
  }
  if (raw === null) return null;
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

/** Per-kind directional coverage; each type is a subset of the combined union. */
export function cellCoverageByKind(
  pair: PairAnalysis,
  rowId: string,
  kind: "exact" | "modified",
): number | null {
  if (pair.a_student_id !== rowId && pair.b_student_id !== rowId) return null;
  const side = pair.a_student_id === rowId ? "a" : "b";
  return kind === "exact"
    ? pair[`exact_coverage_${side}`]
    : pair[`modified_coverage_${side}`];
}

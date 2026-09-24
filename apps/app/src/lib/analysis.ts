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

export const ComparedLibrarySchema = z.object({
  id: z.string(),
  name: z.string(),
  source_session_name: z.string(),
  source_count: z.number().int().nonnegative(),
});

export const HistoricalPairAnalysisSchema = z.object({
  student_id: z.string(),
  library_id: z.string(),
  library_name: z.string(),
  reference_submission_id: z.string(),
  reference_label: z.string(),
  reference_filename: z.string().nullable(),
  reference_text: z.string(),
  coverage_current: z.number().nullable(),
  exact_coverage_current: z.number().nullable(),
  modified_coverage_current: z.number().nullable(),
  passages: z.array(PassageSchema),
  excluded: z.array(ExcludedEvidenceSchema),
});

export type HistoricalPairAnalysis = z.infer<
  typeof HistoricalPairAnalysisSchema
>;

export const ExactAnalysisSchema = z.object({
  fingerprint_version: z.number(),
  normalization_version: z.number(),
  common_text_version: z.number(),
  modified_version: z.number(),
  common_text_applied: z.boolean(),
  prompt_applied: z.boolean(),
  pairs: z.array(PairAnalysisSchema),
  per_student: z.array(StudentCoverageSchema),
  compared_libraries: z.array(ComparedLibrarySchema).default([]),
  historical_matches: z.array(HistoricalPairAnalysisSchema).default([]),
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
    "historical",
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
    throw {
      code: "protocol",
      message: "Unexpected response from the app core.",
    };
  }
  return parsed.data;
}

export async function loadSessionAnalysis(
  sessionId: string,
): Promise<ExactAnalysis | null> {
  let raw: unknown;
  try {
    raw = await getInvokeImpl()("get_session_analysis", { sessionId });
  } catch (err) {
    throw asSessionsError(err);
  }
  if (raw === null) return null;
  const parsed = ExactAnalysisSchema.safeParse(raw);
  if (!parsed.success) {
    throw {
      code: "protocol",
      message: "Unexpected response from the app core.",
    };
  }
  return parsed.data;
}

export type StudentReportMode = "self_check";

/** Request a locally rendered Tauri PDF. Malformed bytes are a protocol error, never a success. */
export async function generateStudentReportPdf(
  sessionId: string,
  studentId: string,
  anonymize: boolean,
  reportMode: StudentReportMode,
): Promise<number[]> {
  let raw: unknown;
  try {
    raw = await getInvokeImpl()("generate_student_report_pdf", {
      sessionId,
      studentId,
      anonymize,
      reportMode,
    });
  } catch (err) {
    throw asSessionsError(err);
  }
  const parsed = z.array(z.number().int().min(0).max(255)).safeParse(raw);
  if (
    !parsed.success ||
    parsed.data.length < 5 ||
    parsed.data[0] !== 37 ||
    parsed.data[1] !== 80 ||
    parsed.data[2] !== 68 ||
    parsed.data[3] !== 70 ||
    parsed.data[4] !== 45
  ) {
    throw {
      code: "protocol",
      message: "The app core did not return a valid PDF report.",
    };
  }
  return parsed.data;
}

/** Save a generated report through the native Tauri file picker. A null path means the user canceled. */
export async function saveReportFiles(
  pdfBytes: number[],
  suggestedFileName: string,
  companionJson?: string,
): Promise<string | null> {
  let raw: unknown;
  try {
    raw = await getInvokeImpl()("save_report_files", {
      pdfBytes,
      suggestedFileName,
      companionJson: companionJson ?? null,
    });
  } catch (err) {
    throw asSessionsError(err);
  }
  const parsed = z.string().min(1).nullable().safeParse(raw);
  if (!parsed.success) {
    throw {
      code: "protocol",
      message: "The app core did not return a valid report save location.",
    };
  }
  return parsed.data;
}

const PdfBytesSchema = z.array(z.number().int().min(0).max(255));
const CertifiedStudentReportShapeSchema = z.object({
  schema_version: z.literal(1),
  lock_sha256: z.string().regex(/^[a-f0-9]{64}$/),
  report: z.object({
    payload: z.object({ report_mode: z.literal("teacher") }),
  }),
  signature: z.object({
    algorithm: z.literal("Ed25519"),
    key_id: z.string().regex(/^[a-f0-9]{64}$/),
    public_key_hex: z.string().regex(/^[a-f0-9]{64}$/),
    signature_hex: z.string().regex(/^[a-f0-9]{128}$/),
  }),
});

export interface CertifiedReportExport {
  pdfBytes: number[];
  signedReportJson: string;
}

export async function generateCertifiedStudentReport(
  sessionId: string,
  studentId: string,
  anonymize: boolean,
): Promise<CertifiedReportExport> {
  let raw: unknown;
  try {
    raw = await getInvokeImpl()("generate_certified_student_report", {
      sessionId,
      studentId,
      anonymize,
    });
  } catch (err) {
    throw asSessionsError(err);
  }
  const result = z
    .object({
      pdf_bytes: PdfBytesSchema,
      signed_report_json: z.string().min(1),
    })
    .safeParse(raw);
  if (!result.success) {
    throw {
      code: "protocol",
      message: "The app core did not return a complete signed report.",
    };
  }
  const pdf = result.data.pdf_bytes;
  if (
    pdf.length < 5 ||
    pdf[0] !== 37 ||
    pdf[1] !== 80 ||
    pdf[2] !== 68 ||
    pdf[3] !== 70 ||
    pdf[4] !== 45
  ) {
    throw {
      code: "protocol",
      message: "The app core did not return a valid certified PDF.",
    };
  }
  let envelope: unknown;
  try {
    envelope = JSON.parse(result.data.signed_report_json);
  } catch {
    throw {
      code: "protocol",
      message: "The app core returned invalid signed report JSON.",
    };
  }
  if (!CertifiedStudentReportShapeSchema.safeParse(envelope).success) {
    throw {
      code: "protocol",
      message: "The app core returned an incomplete signed report certificate.",
    };
  }
  return { pdfBytes: pdf, signedReportJson: result.data.signed_report_json };
}

const SignatureVerificationSchema = z.object({
  status: z.literal("verified"),
  signing_key_id: z.string().regex(/^[a-f0-9]{64}$/),
  note: z.string().min(1),
});

export type SignatureVerification = z.infer<typeof SignatureVerificationSchema>;

export async function verifyCertifiedStudentReport(
  reportJson: string,
): Promise<SignatureVerification> {
  let raw: unknown;
  try {
    raw = await getInvokeImpl()("verify_certified_student_report", {
      reportJson,
    });
  } catch (err) {
    throw asSessionsError(err);
  }
  const parsed = SignatureVerificationSchema.safeParse(raw);
  if (!parsed.success) {
    throw {
      code: "protocol",
      message: "Unexpected response from report verification.",
    };
  }
  return parsed.data;
}

/** Coverage of the row student against the column student in a pair. */
export function cellCoverage(pair: PairAnalysis, rowId: string): number | null {
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

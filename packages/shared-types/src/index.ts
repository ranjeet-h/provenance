// Shared Tauri/IPC + domain DTOs. Mirror of provenance-core domain (JSON shape).
// Frontend validates these at runtime with Zod (see apps/app/src/lib/sessions.ts).

export type SessionStatus = "draft" | "locked" | "analyzed";

export interface SessionDto {
  id: string;
  name: string;
  subject: string | null;
  status: SessionStatus;
  assignment_prompt: string | null;
  excluded_reference_text: string | null;
  exclude_common_text: boolean;
  created_at: string;
  updated_at: string;
}

export interface StudentDto {
  id: string;
  session_id: string;
  display_name: string;
  created_at: string;
}

export type SourceTypeDto =
  | "pasted_text"
  | "txt_file"
  | "markdown_file"
  | "pdf_digital"
  | "docx_file";

export type SubmissionStatusDto = "draft" | "ready";

export interface SubmissionDto {
  id: string;
  student_id: string;
  session_id: string;
  source_type: SourceTypeDto;
  status: SubmissionStatusDto;
  original_text: string;
  source_filename: string | null;
  content_sha256: string;
  created_at: string;
  updated_at: string;
}

export interface CommandErrorDto {
  code: string;
  message: string;
}

export type FileIngestResultDto =
  {
    Digital: {
      text: string;
      pages: number;
      source: SourceTypeDto;
      filename: string;
    };
  };

export type PassageKindDto = "exact" | "modified";

export interface PassageDto {
  kind: PassageKindDto;
  identity: number | null;
  /** Explanatory flag only: common passages are fully counted. */
  common_text: boolean;
  a_token_start: number;
  a_token_end: number;
  b_token_start: number;
  b_token_end: number;
  a_char_start: number;
  a_char_end: number;
  b_char_start: number;
  b_char_end: number;
  tokens: number;
}

export interface PairAnalysisDto {
  a_student_id: string;
  b_student_id: string;
  coverage_a: number | null;
  coverage_b: number | null;
  passages: PassageDto[];
  excluded: ExcludedEvidenceDto[];
}

export interface StudentCoverageDto {
  student_id: string;
  coverage: number | null;
  matched_tokens: number;
  total_tokens: number;
  eligible_tokens: number;
}

export interface ExactAnalysisDto {
  fingerprint_version: number;
  normalization_version: number;
  common_text_version: number;
  modified_version: number;
  common_text_applied: boolean;
  prompt_applied: boolean;
  pairs: PairAnalysisDto[];
  per_student: StudentCoverageDto[];
}

export type ExclusionReasonDto = "Prompt" | "Reference" | "CommonSessionText";

export interface ExcludedEvidenceDto {
  side: "a" | "b";
  token_start: number;
  token_end: number;
  char_start: number;
  char_end: number;
  reason: ExclusionReasonDto;
  tokens: number;
}

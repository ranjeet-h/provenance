import type { SubmissionSource } from "./sessions";

export const SUPPORTED_EXTENSIONS = ["txt", "md", "markdown", "pdf", "docx"];

export function extensionOf(name: string): string {
  const parts = name.split(".");
  return (parts.length > 1 ? (parts[parts.length - 1] ?? "") : "").toLowerCase();
}

export function isSupportedFile(name: string): boolean {
  return SUPPORTED_EXTENSIONS.includes(extensionOf(name));
}

/** Files previewable as text before saving (PDF/DOCX extract on save). */
export function isPreviewableText(name: string): boolean {
  const ext = extensionOf(name);
  return ext === "txt" || ext === "md" || ext === "markdown";
}

export function sourceForFilename(name: string): SubmissionSource {
  const ext = extensionOf(name);
  if (ext === "pdf") return "pdf_digital";
  if (ext === "docx") return "docx_file";
  return /\.(md|markdown)$/i.test(name) ? "markdown_file" : "txt_file";
}

# Local-First Plagiarism Detection App

## Product definition

Provenance is a local-first application for reviewing text reuse in English
academic assignments. It keeps submissions, analysis, reference libraries, and
reports on the user's device. It supports teacher-certified sessions and a
student self-check mode; neither mode automatically declares plagiarism.

The product accepts pasted text, TXT, Markdown, digital PDFs with embedded
text, and DOCX files. These formats are parsed directly. Image files and
image-only PDFs are outside the product boundary and are rejected explicitly.

## Principles

- Native Tauri 2 application with React and TypeScript UI.
- Shared Rust core for validation, normalization, matching, storage, and
  reports.
- SQLite for local structured data.
- Fully usable offline.
- Passage-level evidence and source ranges instead of one opaque score.
- Current-session comparison plus historical reference libraries.
- Portable, anonymized reference packs.
- Tamper-evident reports for teacher-certified sessions.
- No automatic alternate extraction path for unsupported inputs.

## User modes

### Teacher-certified session

The teacher creates an assignment, adds students, collects submissions, locks
the session, compares the complete available corpus, and generates a report for
each student. The report names the current and historical sources that were
checked.

### Student self-check

A student compares their pasted or digital-file submission against the local
material they have selected. This is informational and is not equivalent to a
teacher-certified result.

## Submission and import contract

Every submission becomes one canonical representation:

- original and validated text;
- normalized text, tokens, and sentences;
- source type and filename;
- stable content hash;
- mappings from normalized tokens to original character offsets.

Supported files:

- `.txt` and `.md`/`.markdown`: UTF-8 text is preserved and validated;
- digital PDF: embedded text is extracted page by page in document order;
- DOCX: paragraphs, headings, tables, Unicode, tabs, and line breaks are
  extracted in document order.

Rejected files receive an actionable validation error before storage. Mixed
documents are never partially stored. The import test suite covers valid,
empty, corrupt, oversized, binary, image-only, and mixed inputs.

## Plagiarism architecture

The engine combines explainable evidence-producing stages:

1. Normalize and tokenize.
2. Detect exact copying with fingerprints and winnowing.
3. Detect lightly modified copying with token overlap, fuzzy comparison, and
   local alignment.
4. Align candidate passages and preserve source ranges.
5. Merge overlapping evidence.
6. Explain common-text and assignment-template exclusions.
7. Calculate directional matched coverage.
8. Produce reports for teacher review.

Similarity is evidence, not a verdict. Reports direct the teacher or qualified
reviewer to the highlighted passages.

## Session, report, and reference data

Sessions contain the assignment settings, students, submission state, lock
state, engine versions, and generated reports. For `N` submissions, every
unique pair is compared.

Reports contain:

- student and assignment identity;
- corpus scope;
- exact and modified evidence;
- matched passages and both source ranges;
- coverage values and explanatory exclusions;
- hashes, signatures, and version metadata.

Reference libraries contain completed submissions and rebuildable canonical
text. `.plagpack` exports are versioned, compressed, anonymized by default, and
include integrity/signature metadata.

## Technology stack

UI: React, TypeScript, and Tauri 2.

Core: Rust, SQLite, SHA-256, Ed25519, deterministic normalization, fingerprint
matching, local alignment, and report generation.

## Runtime and platform targets

The same Rust core targets Android, iOS/iPadOS, Windows, macOS, and Linux.
Platform validation covers clean install, session CRUD, every supported digital
file type, analysis, reports, reference-pack round trips, persistence, and
offline restart. A build is not treated as a real-device validation result.

## Quality plan

The repository keeps deterministic examples for:

- TXT and Markdown Unicode/newline preservation;
- DOCX paragraphs, headings, tables, breaks, fields, deletion handling, empty
  documents, corrupt packages, and ZIP expansion limits;
- digital PDF page order, short valid pages, empty/corrupt files, and mixed or
  image-only rejection;
- submission persistence, replacement, locking, hashing, and source labels;
- exact, modified, historical, and false-positive analysis.

Release gates require Rust tests, frontend tests, typecheck, lint, formatting,
Tauri compilation, clean-diff validation, and a fresh-checkout import run.

## V1 scope

Included: English natural-language assignments, digital text imports, pasted
text, exact matching, modified matching, pairwise evidence, teacher sessions, student self-check, reference libraries,
`.plagpack` import/export, signed reports, and offline operation.

Not included: image inputs, camera capture, image-to-text processing, source
code plagiarism, mathematical-expression plagiarism, global web crawling,
multilingual text, or external plagiarism APIs.

## Future extensions

Possible later work includes multilingual text, cross-language comparison,
institution-shared encrypted libraries, code/AST matching, mathematical
expression matching, citation-aware analysis, and an explicitly approved
hosted document-understanding integration. Each extension needs its own product
decision and evidence before implementation.

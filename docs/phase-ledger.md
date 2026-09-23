# Provenance phase ledger

Manual approval boundary: no phase is complete until its automated gates pass,
its manual test is performed by the user, and the user explicitly approves.

## Approval history

| Phase | Status | Evidence | Approved |
|---|---|---|---|
| 0 — Bootstrap | approved | clean app shell and repository gates | user |
| 1 — Shell + design system | approved | frontend tests, typecheck, lint, build | user |
| 2 — Domain + SQLite | approved | Rust persistence tests and reopen flow | user |
| 3 — Digital text | approved | paste/upload/replace/reload and binary rejection | user |
| 4 — Normalization | approved | token inspection and Unicode mapping tests | user |
| 5 — Exact engine | approved | pairwise matrix and highlighted evidence | user |
| 6 — Common-text filtering | approved | prompt/reference exclusion tests | user |
| 6.1 — Reliability + scoring repair | approved | deterministic scoring, Unicode, and bounded-work gates | user |
| 7 — Modified matching | approved | modified-passage fixtures and matrix evidence | user |
| 8 — Digital file imports | approved | TXT, Markdown, DOCX, digital PDF, corruption, size, and rejection tests | user |

## Phase 8 import contract

The supported file contract is intentionally limited to pasted text, TXT,
Markdown, digital PDF, and DOCX. Digital parsers preserve document order and
return source filenames and source types. Image files and image-only or mixed
PDFs return validation errors before a submission is stored. No parser may
silently discard unreadable pages or create a partial submission.

The integration fixture suite covers:

- UTF-8 text, Markdown, Unicode, and newline preservation;
- DOCX paragraphs, headings, tables, tabs, line breaks, fields, deletions,
  empty documents, corrupt packages, and expansion limits;
- digital PDF page order, short valid pages, empty/corrupt files, inherited
  image resources, image-only pages, and mixed pages;
- service persistence, replacement, locking, hashes, and no-partial-save
  behavior.

## Repository decisions

- Browser E2E is not used as native Tauri validation; Layer D uses recorded
  engine fixtures, command contracts, and manual native gates.
- Image inputs remain outside the product contract. The root-level decision
  record explains the boundary and the only allowed future reconsideration
  path.

## Implemented phases awaiting user acceptance

These entries record implementation and automated evidence only. They are
not approved phases: the user still needs to perform the corresponding manual
test and explicitly approve them.

| Phase | Implementation evidence | Manual test / approval |
|---|---|---|
| 15 — Evidence merge and final coverage scoring | Exact/modified directional and per-student breakdowns; unique-span combined coverage; Rust engine and frontend contract tests | Pending user test |
| 16 — Full session analysis engine | 20-submission/190-pair test; monotonic progress; persisted digest-validated reports; versioned raw pair cache; explicit stale-input rejection; Tauri event and frontend reload/progress tests | Pending user test |
| 17 — Reference Libraries | Current-input-gated immutable session snapshots; read-only corpus management; per-session selection; selected-library digest invalidation; separate exact/modified historical evidence; detached archive and current-vs-historical separation tests; Tauri/UI flows | Pending user test |
| 18 — `.plagpack` Portable Reference Format | Strict two-entry compressed package; anonymized labels/no source filenames; original + canonical text and engine metadata; payload/document hash verification; duplicate deduplication; corrupt, oversized, unsupported-version, and non-text rejection; export-delete-import-compare integration; Tauri/UI import/export | Pending user test |
| 19 — Report Generation | Digest-gated per-student evidence payloads; separate current/historical matches; anonymization, exclusions, span/source integrity and SHA-256 checks; local PDF with embedded licensed font; validated two-sided colored spans, long/zero/current/historical/mixed fixtures, exact footer; Tauri export command and tested UI download | Pending user test |
| 20 — Certified Sessions and Cryptographic Integrity | Ed25519 signing and verification; SHA-256 signed session lock covering current submissions, selected archives, engine/configuration, and exact saved analysis; operating-system credential store; locked mutation rejection; certified PDF with QR payload and signed JSON companion; tamper, wrong-key, self-check rejection, lock persistence, and Tauri command tests | Pending user test |
| 21 — Student Self-Check Mode | Explicit local comparison-source requirement; selected corpus shown in UI and report; separate unsigned Self Check PDF marked Not Teacher Certified; no signing-path access; single-student historical-corpus, source-guard, and frontend flow tests | Pending user test |

## App-wide UX update awaiting user review

The desktop UI now has a unified light palette and type scale, persistent
workspace navigation with a mobile drawer, clearer dashboard/session/library
hierarchy, session search and counts, responsive student/submission controls,
and consistent cards, status badges, forms, analysis, empty, loading, and error
states. This is an implementation note, not user approval; inspect at desktop
and narrow window sizes during the manual test.

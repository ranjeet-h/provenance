# Provenance

Local-first plagiarism detection for English academic assignments (Tauri 2 + React + Rust core).

- Idea / architecture: `main-project-idea.md`
- Execution contract: `plan_v1.md` (work phase-by-phase, manual gate per phase)
- App: `apps/app` — `pnpm dev` / `pnpm tauri dev`
- Rust engines: `crates/provenance-core`, `crates/provenance-match`, `crates/provenance-report`
- Supported imports: pasted text, TXT, Markdown, digital PDF, and DOCX. See
  [`OCR_REMOVED.md`](OCR_REMOVED.md) for the deliberate unsupported-image
  boundary.
- Full gates: `scripts/check-all.sh`

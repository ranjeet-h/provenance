# Provenance

**Local-first evidence review for digital academic submissions.**

Provenance helps educators compare student-submitted text, inspect matching
passages in context, and keep a review record on the same device. It reports
textual overlap as evidence for a person to review—not as a verdict about
intent or misconduct.

> **Desktop preview.** Build the app from source to try it; public installers
> are not available yet.

[Open the product page](https://ranjeet-h.github.io/provenance/) ·
[Build and usage guide](docs/GETTING_STARTED.md)

## What it does

- Organizes an assignment into a local session with student submissions.
- Finds exact and modified textual overlap and links passages to their source
  submissions for review.
- Excludes the supplied assignment question and configured reference text from
  overlap scoring.
- Compares against selected, read-only reference libraries.
- Exports and imports portable `.plagpack` reference archives.
- Creates self-check and signed report PDFs with clear status labels.
- Runs the comparison engine in the Tauri desktop app; no hosted AI or account
  is required.

The comparison corpus is only the material supplied to the app. Provenance is
not a web-wide search service, does not establish who copied from whom, and
does not make a misconduct decision.

## Supported input

| Input                                     | Support         |
| ----------------------------------------- | --------------- |
| Pasted text                               | Supported       |
| TXT and Markdown                          | Supported       |
| Digital PDF with selectable/embedded text | Supported       |
| DOCX                                      | Supported       |
| Images and image-only or scanned PDFs     | **Unsupported** |

For PDFs, use a document with selectable text.

## Run it locally

Install the native Tauri prerequisites for your operating system, Node.js, the
repository-pinned pnpm version, and stable Rust. Then, from the repository root:

```sh
pnpm install --frozen-lockfile
pnpm --filter provenance-app tauri dev
```

The detailed [getting-started guide](docs/GETTING_STARTED.md) covers platform
dependencies, building an app bundle, the workflow inside the app, and the
manual test fixtures. See the official
[Tauri prerequisites](https://tauri.app/start/prerequisites/) before installing
native system packages.

## Developer checks

```sh
pnpm typecheck
pnpm lint
pnpm test
pnpm build
cargo test --workspace
```

For the full repository gate—including Rust formatting and Clippy—run:

```sh
bash scripts/check-all.sh
```

The full Rust gate writes build artifacts under `target/`; Tauri packaging also
uses `apps/app/src-tauri/target/`. Both are ignored by Git. Avoid repeated
clean/full builds unless you need them.

## Project layout

```text
apps/app/                  React + TypeScript desktop interface (Vite/Tauri 2)
apps/app/src-tauri/        Native Tauri commands and local database setup
crates/provenance-core/    Import, storage, session, and analysis orchestration
crates/provenance-match/   Deterministic exact and modified matching
crates/provenance-report/  Reports, signatures, and .plagpack format
packages/shared-types/     Frontend/backend command types
manual-test-pack/          Synthetic manual acceptance fixtures
docs/                      Local build and use guide
site/                      Static product page published with GitHub Pages
```

## Data and safety

Assignment sessions, submissions, and reference libraries are stored in the
local application workspace. The current app has no hosted analysis service,
account system, or telemetry pipeline. Treat student submissions as sensitive
even when they remain on your device: restrict device access and protect your
backups and exported reports.

Overlap is a review signal. Read the matched passages and their surrounding
context; do not use a percentage alone as a conclusion. Self-check PDFs are
marked **Not Teacher Certified**. A signed report records integrity of the
reviewed local inputs; it is not an institutional certificate.

## Status and source terms

This repository is public for inspection, security review, and proof of work.
It is **not open-source licensed for reuse**. All rights are reserved; see
[RIGHTS.md](RIGHTS.md). Public visibility does not grant a general license to
copy, modify, redistribute, or create derivative works.

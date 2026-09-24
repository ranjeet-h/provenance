# Build and use Provenance

Provenance is a Tauri 2 desktop app with a React/TypeScript interface and a
local Rust engine. This guide is for running it from source; there are no signed
public installers yet.

## 1. Install prerequisites

Install:

- Node.js LTS.
- pnpm **12.3.3**, the version pinned in the root `package.json`.
- The stable Rust toolchain through rustup.
- The native dependencies required by Tauri for your operating system.

Tauri's operating-system packages change over time. Follow its current
[prerequisites guide](https://tauri.app/start/prerequisites/). In brief:

- macOS desktop development needs Xcode Command Line Tools (`xcode-select --install`).
- Windows development needs Microsoft C++ Build Tools and WebView2.
- Linux needs the WebKitGTK 4.1 and related system packages for your distribution.

After installing prerequisites, verify the tools:

```sh
node --version
pnpm --version
rustc --version
cargo --version
```

The pnpm command should report `12.3.3`. Install that version using the
[official pnpm installation instructions](https://pnpm.io/installation) if
needed.

## 2. Install dependencies and launch

From the repository root:

```sh
pnpm install --frozen-lockfile
pnpm --filter provenance-app tauri dev
```

The first native build can take several minutes and uses Rust build artifacts.
Keep the terminal open while developing; stop the app with Ctrl-C in that
terminal. The Tauri desktop shell is required for native database and file
dialog commands—running only the Vite browser server is not the full app.

## 3. Build a local app bundle

From the repository root:

```sh
pnpm --filter provenance-app tauri build
```

This builds for the current host. Installers and bundles are written under
`apps/app/src-tauri/target/release/bundle/`. Building successfully on one
machine does not mean that platform is release-validated; no signed or
notarized public installer is currently provided.

## 4. Use the app

1. In **Sessions**, create a session for one assignment.
2. Open the session settings and paste the assignment question/instructions.
   This text is excluded from overlap scoring.
3. Add the students, then add each student's submission by pasting text or
   selecting a supported digital document.
4. Choose **Analyze session**. Review the pairwise matrix and each student's
   matched passages in context. The results describe overlap in the selected
   comparison corpus; they do not determine intent or misconduct.
5. To compare a later assignment with prior work, first archive a session with
   a saved current analysis and at least two non-empty submissions. Select the
   archive for a future session, or move it with a `.plagpack` file.
6. Export one analyzed session from its detail page, or select several sessions
   on **Sessions**. Each selected session is exported as a separate pack. The
   native save dialog asks where to write the files.
7. Open **Reference Libraries** and choose **Import archive** to bring in a
   `.plagpack`. Imported archives are local comparison sources; they are not
   merged into current-student scores unless selected for that session.
8. Per-student self-check PDFs are explicitly marked **Not Teacher Certified**.
   Certified reports are available only after the reviewed session is locked.
   Locking is intentionally irreversible in the app, so review the inputs and
   results before taking that step.

For a complete set of synthetic test submissions and expected observations,
follow [manual-test-pack/README.md](../manual-test-pack/README.md). The pack
covers exact and modified overlaps, all supported import formats, archive
round-trips, signed-report verification, and self-check behavior.

## Supported documents

- Pasted text.
- UTF-8 TXT.
- Markdown.
- Digital PDFs with selectable/embedded text.
- DOCX documents.

Images and image-only/scanned PDFs are not supported. Use a document with
selectable text instead.

The analysis corpus is limited to submissions and reference sources the user
has added. Provenance does not search the public web or a commercial plagiarism
database.

## Local data and privacy

The app stores its SQLite database, `provenance.db`, in Tauri's per-application
data directory on the current device. Student text and generated reports are
sensitive: protect the operating-system account, backups, and any exported
files. The product has no hosted analysis backend or account sign-in.

The private signing key for session certification is held by the
operating-system credential store. A signed report records the integrity of
local review inputs; it is not a certificate issued by a school or authority.

## Run checks

Focused frontend checks:

```sh
pnpm typecheck
pnpm lint
pnpm test
pnpm build
```

Rust checks:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

The complete sequence is:

```sh
bash scripts/check-all.sh
```

Rust artifacts are ignored by Git and can use substantial disk space. The
workspace sets reduced debug information by default; avoid repeated full clean
builds. Use the optional `debugging` profile only when a debugger needs full
symbols.

## Repository map

- `apps/app`: React, TypeScript, Vite, and Tauri desktop application.
- `apps/app/src-tauri`: Tauri commands, native dialogs, SQLite setup, and app
  bundle configuration.
- `crates/provenance-core`: document import, local persistence, and analysis
  orchestration.
- `crates/provenance-match`: deterministic exact and modified text matching.
- `crates/provenance-report`: PDF reports, signatures, and `.plagpack`
  serialization.
- `packages/shared-types`: shared frontend/native command types.
- `manual-test-pack`: synthetic test data for manual acceptance.

## Availability

The desktop preview is available to build from source. Public signed installers
are not available yet. A successful local build confirms that it builds on
your machine; it is not a platform release certification.

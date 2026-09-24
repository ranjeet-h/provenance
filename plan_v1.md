# Provenance — Autonomous Implementation Plan V1

Status: Execution contract for coding agents  
Internal project name: `Provenance`  
Primary goal: Build the V1 local-first plagiarism detection application defined in the finalized architecture document.

---

# 0. How the Coding Agent Must Use This File

This file is not a suggestion list. It is the execution order.

The coding agent must:

1. Work phase-by-phase in the exact order defined here.
2. Complete all tasks inside the current phase autonomously.
3. Apply strict TDD for every new behavior:
   - RED: write a failing test first.
   - GREEN: write the minimum implementation required to make the test pass.
   - REFACTOR: clean the implementation without breaking tests.
4. Never implement a user-visible behavior without at least one automated test covering it.
5. Never add a Rust function with meaningful business behavior without a Rust test first.
6. Never add a React component with behavior without a component/integration test first.
7. Never add a database migration without migration tests.
8. Never add a Tauri command without contract/integration tests.
9. Never add a file format without round-trip tests.
10. Never add a scoring algorithm without deterministic fixtures and expected outputs.
11. Never add an import integration without fixture-based tests.
12. Run the full phase test suite before declaring a phase complete.
13. Fix all test, lint, type-check, formatting, and build failures before stopping.
14. At the end of every phase:
    - summarize exactly what was implemented;
    - provide the automated test results;
    - provide exact manual test steps;
    - STOP;
    - ask the user to manually validate;
    - do not start the next phase until the user explicitly approves.
15. If the user reports a failure:
    - reproduce it;
    - write a failing regression test first;
    - fix it;
    - rerun the phase gate;
    - ask the user to test again.
16. Do not silently skip features because they are difficult.
17. Do not replace finalized architectural decisions without explicit user approval.
18. Use the latest stable package versions available at implementation time unless this file explicitly pins a major version.
19. Keep Tauri on major version 2.
20. Keep the application local-first. No server dependency may be introduced into V1.
21. Never add silent fallbacks to make an implementation pass. A fallback is any secondary path that masks a failure: default values, empty results treated as success, downgraded behavior, caught errors that continue quietly, or approximate outputs accepted where the phase requires exact expected results.
22. Always produce the expected result or fail loudly with an explicit error. If the expected result cannot be produced, return a clear error naming what is missing and how to fix it; never substitute a fallback and report success.
23. Any degraded or alternative path is allowed only when this plan explicitly requires it as a product behavior, and then it must be a named, tested behavior with its own contract tests — never an undocumented safety net. Tests must assert the expected result itself, not a fallback-acceptable range.

The autonomous loop is:

```text
READ CURRENT PHASE
        ↓
WRITE FAILING TESTS
        ↓
IMPLEMENT
        ↓
REFACTOR
        ↓
RUN TARGETED TESTS
        ↓
RUN FULL PHASE GATE
        ↓
FIX EVERYTHING
        ↓
DOCUMENT RESULT
        ↓
ASK USER TO MANUALLY TEST
        ↓
STOP
        ↓
USER APPROVES
        ↓
NEXT PHASE
```

The coding agent must not interpret "autonomous" as permission to skip manual phase gates.

---

# 1. Product Definition

Provenance is a local-first plagiarism detection application for English natural-language academic assignments across all subjects including:

- medicine
- engineering
- science
- law
- business
- humanities
- history
- arts
- literature
- general education

V1 is not a global internet plagiarism crawler.

V1 compares a submission against:

1. other submissions in the current session;
2. locally stored historical Reference Libraries;
3. imported `.plagpack` Reference Libraries.

The application must support:

- PDF;
- DOCX;
- TXT;
- Markdown;
- pasted text;
- exact-copy detection;
- modified-copy detection;
- pairwise comparison matrix;
- detailed passage alignment;
- per-student reports;
- teacher-certified sessions;
- historical libraries;
- portable `.plagpack`;
- local report signing;
- Android;
- iOS/iPadOS;
- Windows;
- macOS;
- Linux.

---

# 2. Final Architecture — Do Not Change Without Approval

## Application

```text
React + TypeScript UI
        │
      Tauri 2
        │
   Shared Rust Core
```

## Storage

```text
SQLite
+
native filesystem
```

## Security / integrity

```text
SHA-256
Ed25519
```

## Core plagiarism architecture

```text
Canonical Text
      │
      ├── Exact Detector
      │     └── token shingles + rolling hash + Winnowing
      │
      ├── Modified Detector
      │     └── fuzzy token matching + local alignment
      │
      └── Passage Alignment
                    │
                    ▼
               Span Merging
                    │
                    ▼
           False Positive Filters
                    │
                    ▼
            Coverage Calculation
                    │
                    ▼
                  Report
```

## Evidence-versus-judgement contract

The engine answers:

```text
Is there meaningful text reuse in the comparison corpus?
```

The teacher or qualified reviewer answers:

```text
Does this reuse constitute plagiarism in its assignment context?
```

Similarity alone cannot establish intent, copying direction, authorship, or
misconduct. Evidence strength may use passage rarity, matched length, exactness,
unusual shared errors, order/structure, repeated independent matches, quotation
and citation context, and available source provenance.

Every detector-confirmed matched span must remain visible and highlighted in
both source texts, including overlapping matches. Prompt/template exclusions
and common session text use separate visual treatment and explanation; they
never silently disappear.

Required result labels:

```text
No significant overlap detected
Meaningful overlap found — review required
Strong evidence of likely text reuse
```

Never emit `Student plagiarised` or `Student definitely copied Student B` as an
automated conclusion.

The primary percentage is:

```text
unique matched meaningful words
──────────────────────────────── × 100
total meaningful words
```

---

# 3. V1 Non-Goals

Do not implement these in V1 unless the user explicitly expands scope:

- cloud plagiarism processing;
- central backend;
- SaaS account system;
- web crawling;
- Google search integration;
- central vector database;
- multilingual plagiarism;
- cross-language plagiarism;
- code plagiarism;
- mathematical-expression plagiarism;
- institution cloud synchronization;
- multi-device live sync;
- payment/subscription system;
- web authentication;
- remote user accounts;
- an LLM making the final plagiarism decision.

## Future optional LLM review

When a suitable LLM becomes free or affordable, add it as an optional review
layer after deterministic evidence generation. It may summarize evidence,
compare explanations, and suggest a reviewer outcome, but it must receive the
highlighted evidence and corpus metadata—not replace them. V1 remains fully
usable offline without an LLM, and the teacher or qualified reviewer remains
final decision-maker unless a later product decision explicitly changes this
rule.

---

# 4. Repository Setup — Required Final Structure

Use a monorepo.

Recommended structure:

```text
provenance/
├── apps/
│   └── app/
│       ├── src/
│       ├── src-tauri/
│       └── tests/
│
├── crates/
│   ├── core-domain/
│   ├── text-normalization/
│   ├── fingerprint-engine/
│   ├── alignment-engine/
│   ├── scoring-engine/
│   ├── reference-library/
│   ├── report-engine/
│   ├── crypto-integrity/
│   ├── storage/
│   ├── image-preprocess/
│
├── packages/
│   ├── shared-types/
│   └── test-fixtures/
│
├── tests/
│   ├── fixtures/
│   ├── integration/
│   ├── cross-platform/
│   └── benchmarks/
│
├── scripts/
├── docs/
├── Cargo.toml
├── package.json
├── pnpm-workspace.yaml
├── eslint.config.*
├── prettier.config.*
├── vitest.workspace.*
└── README.md
```

Use `pnpm` as the JavaScript package manager.

Use a Rust Cargo workspace for all Rust crates.

---

# 5. Technology Stack

At implementation time, install the latest stable releases unless a major version is explicitly fixed.

Required frontend stack:

```text
React latest stable
TypeScript latest stable
Vite latest stable
Tauri 2 latest stable
shadcn latest compatible CLI/components
Tailwind CSS latest stable compatible with shadcn
TanStack Router latest stable
TanStack Query latest stable
React Hook Form latest stable
Zod latest stable
Lucide React latest stable
```

Testing:

```text
Vitest
React Testing Library
@testing-library/user-event
cargo test
cargo nextest if available
cargo clippy
cargo fmt
```

Rust:

```text
stable Rust toolchain
serde
serde_json
thiserror
anyhow only at application boundaries where appropriate
sqlx with SQLite
sha2
ed25519-dalek or equivalent audited Ed25519 crate
uuid only if an identifier is required and no project-specific ID strategy exists
```

Tauri plugins should be installed only when required.

Likely V1 plugins:

```text
tauri-plugin-dialog
tauri-plugin-fs
tauri-plugin-os
tauri-plugin-shell only if genuinely required
```

Do not install large dependency collections "just in case".

---

# 6. Global Engineering Rules

## 6.1 TypeScript

Must use:

```json
{
  "strict": true,
  "noUncheckedIndexedAccess": true,
  "exactOptionalPropertyTypes": true
}
```

No `any` except at unavoidable third-party boundaries, and those must be isolated.

All boundaries must validate unknown external data with Zod or an equivalent schema.

## 6.2 Rust

Required:

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Avoid panics in production code.

`unwrap()` and `expect()` are acceptable in tests.

Production `unwrap()` requires an explicit invariant comment and should be rare.

## 6.3 Tauri boundary

React must not contain plagiarism algorithms.

Tauri commands must be thin adapters.

Business behavior belongs in Rust crates.

## 6.4 Database

Every schema change requires:

1. migration;
2. migration test;
3. repository test;
4. rollback/recovery strategy where practical.

## 6.5 Error handling

Every user-facing failure must produce:

- a safe technical error internally;
- a useful user message;
- no stack trace in production UI.

## 6.6 Logging

Use structured local logging.

Never log:

- full student assignments;
- private encryption keys;
- complete report payloads.

## 6.7 Privacy

Network access is not required for analysis. V1 must work offline.

---

# 7. Testing Strategy

Every phase must contain four test layers where applicable.

## Layer A — Rust unit tests

Test pure algorithms exhaustively.

Examples:

- Unicode normalization;
- token mapping;
- fingerprints;
- Winnowing;
- Jaccard;
- local alignment;
- span union;
- scoring;
- `.plagpack`;
- signatures.

## Layer B — Rust integration tests

Test crate boundaries.

Examples:

```text
raw text
→ normalize
→ fingerprint
→ match
→ align
→ score
```

## Layer C — React tests

Test:

- rendering;
- forms;
- navigation;
- loading;
- errors;
- keyboard use;
- state transitions.

## Layer D — application E2E (Tauri-native, no browser automation)

Browser-driven E2E (e.g. Playwright) is removed from this project: it drives
a desktop browser, not the Tauri WebView plus native Rust core, so it cannot
validate what ships. Layer D instead means: recorded real-engine fixtures
rendered through the full UI, command-contract tests on every Tauri boundary,
and manual on-device gates per phase.

Native mobile validation is a manual gate until reliable platform automation is introduced.

## Continuous Local Quality Rule

Every phase validates local-first behavior, correctness, speed, memory, and
recovery when its change can affect them. A phase is not allowed to defer an
introduced regression to Phase 25, 26, 28, or 30.

Use this matrix:

| Change category | Required phase validation |
| --- | --- |
| normalization, matching, filtering, scoring | deterministic fixtures, property/oracle tests where practical, release-build latency and peak-memory regression check on representative text |
| document import | offline fixture test, page-count correctness, parser limits, source filename, cancel/retry behavior |
| SQLite, session state, cache, library, `.plagpack` | migration/round-trip test, interrupted-write or corruption test where applicable, stale-cache/invalidation test |
| desktop/mobile integration | real target-device smoke flow, offline check, memory/latency measurement for affected operation |
| report, lock, signature | deterministic payload/verification test, partial-analysis rejection, regression check that report language and corpus disclosure remain accurate |

Run the full automated regression suite every phase as already required. Do not
rerun the complete multi-device benchmark after a UI-only change. Rerun it when
the change affects native runtime, matching, storage, cache, or platform
integration. Phase 25 consolidates performance work, Phase 26
consolidates failure/recovery work, Phase 28 provides final independent quality
evidence, and Phase 30 enforces release results; none replaces earlier relevant
phase validation.

---

# 8. Coverage Requirements

These are minimum gates, not targets to game.

Rust core algorithm crates:

```text
>= 90% line coverage where measurable
```

Critical algorithms:

```text
normalization
fingerprinting
alignment
scoring
plagpack
crypto

Target: >= 95%
```

Frontend:

```text
>= 80% meaningful application code coverage
```

Do not add meaningless tests purely to inflate coverage.

Mutation/property testing should be introduced for core deterministic algorithms where useful.

---

# PHASE 0 — Machine and Repository Bootstrap

Goal: A clean repository that builds and tests before feature work starts.

## 0.1 Preflight

Agent must check:

```text
node
pnpm
rustc
cargo
tauri prerequisites
Android toolchain if available
Xcode/iOS toolchain if running on macOS
```

Do not install system-level software without telling the user what is missing.

## 0.2 Create project

Use latest stable project generators.

Expected conceptual bootstrap:

```bash
pnpm create tauri-app@latest
```

Select:

```text
Tauri v2
React
TypeScript
Vite
```

Then install/configure:

```text
shadcn
Tailwind
TanStack Router
TanStack Query
Vitest
React Testing Library
```

Initialize Rust workspace.

## TDD requirements

Before adding custom application behavior, add baseline tests:

Frontend:

```text
App boot smoke test
Router boot test
Tauri API adapter mock test
```

Rust:

```text
workspace smoke test
core-domain test module
```

## Required CI-like commands

Create scripts:

```bash
pnpm lint
pnpm typecheck
pnpm test
pnpm build

cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Create:

```bash
pnpm check
```

that runs all applicable JS checks.

Create:

```bash
scripts/check-all.sh
```

that runs frontend + Rust gates.

## Phase 0 automated acceptance

All must pass:

```text
pnpm install
pnpm check
pnpm build
cargo test --workspace
cargo clippy ...
cargo fmt --check
Tauri desktop dev build starts
```

## Phase 0 manual test

Agent must ask user to:

1. Run the desktop app.
2. Confirm the app window opens.
3. Confirm the initial shell renders.
4. Resize the window and confirm there is no layout break.
5. Close and reopen the app.

Agent must STOP after giving these steps.

Do not start Phase 1 until user says Phase 0 is working.

---

# PHASE 1 — Application Shell and Design System

Goal: Establish the final navigation and UI primitives without implementing plagiarism functionality.

Routes:

```text
/
 /sessions
 /sessions/new
 /sessions/:sessionId
 /reference-libraries
 /settings
```

Create shadcn-based app shell.

Required components:

```text
AppSidebar
TopBar
PageHeader
EmptyState
LoadingState
ErrorState
ConfirmDialog
StatusBadge
ProgressIndicator
```

Responsive behavior:

```text
desktop: persistent sidebar
tablet/mobile: drawer/sidebar sheet
```

## TDD sequence

For each route:

1. write failing route render test;
2. implement route;
3. write navigation interaction test;
4. implement navigation;
5. write mobile responsive behavior test where feasible;
6. implement responsive behavior.

Tests must cover:

- unknown route;
- navigation;
- active nav state;
- empty state;
- keyboard navigation;
- accessibility labels.

## Phase 1 automated gate

```text
all frontend tests
typecheck
lint
desktop build
recorded-fixture route navigation (Layer D)
```

## Phase 1 manual test

Ask user to verify:

1. Open Sessions.
2. Open Reference Libraries.
3. Open Settings.
4. Resize to phone-width.
5. Confirm sidebar becomes mobile navigation.
6. Confirm all pages feel fast and visually consistent.
7. Confirm no unfinished placeholder UI leaks outside deliberate empty states.

STOP.

---

# PHASE 2 — Core Domain and SQLite Persistence

Goal: Sessions and students work completely locally.

Entities:

```text
Session
Student/Solver
Submission
```

Session fields:

```text
id
name
subject optional
created_at
updated_at
status: draft | locked | analyzed
input_preferences
analysis_version optional
```

Student fields:

```text
id
session_id
display_name
created_at
```

Submission initially:

```text
id
student_id
session_id
source_type
status
created_at
updated_at
```

## TDD: database migration

RED:

- empty DB migrates successfully;
- migration is idempotent;
- expected tables exist;
- foreign keys are active.

GREEN:

Implement migrations.

## TDD: repositories

Tests first:

```text
create session
read session
list sessions
update session
delete draft session
create student
list students
prevent orphan student
prevent duplicate invalid IDs
```

Use temporary SQLite databases for tests.

## TDD: Tauri commands

Tests first around service layer.

Commands:

```text
create_session
list_sessions
get_session
update_session
delete_session
add_student
remove_student
```

## TDD: UI

Tests first:

```text
create session form validates
session appears in list
open session
add student
remove student with confirmation
persistence survives reload
```

## Manual test

User:

1. Create session "Biology Assignment 1".
2. Close app.
3. Reopen app.
4. Confirm session remains.
5. Add 3 students.
6. Rename session.
7. Remove one student.
8. Close/reopen.
9. Confirm all data is still correct.

STOP.

---

# PHASE 3 — Digital Text Submission

Goal: First complete digital submission path.

Support:

```text
Paste Text
TXT
Markdown
```

Create a canonical input pipeline.

Data to persist:

```text
original_text
source_type
source_filename optional
content_sha256
```

## TDD: hashing

RED tests:

```text
same bytes => same SHA-256
one-character difference => different SHA-256
Unicode bytes are deterministic
```

Implement.

## TDD: paste submission

Tests:

```text
cannot submit empty text
whitespace-only rejected
valid text saves
same student can replace draft before lock
content hash generated
```

## TDD: TXT/MD import

Fixtures:

```text
normal UTF-8
Unicode punctuation
CRLF
LF
empty file
large file
invalid binary masquerading as TXT
```

## UI

Student row should support:

```text
Add Submission
→ Paste Text
→ Upload TXT/MD
```

Submission preview required.

## Manual test

User:

1. Create a session.
2. Add 2 students.
3. Paste text for Student A.
4. Upload `.txt` for Student B.
5. Restart the app.
6. Confirm both submissions remain.
7. Edit Student A submission before locking.

STOP.

---

# PHASE 4 — Text Normalization and Canonical Mapping

Goal: Create the core representation used by every plagiarism algorithm.

Must preserve mappings back to original text.

Canonical structures:

```rust
Token {
    normalized: String,
    original_start: usize,
    original_end: usize,
    ordinal: usize,
}
```

Canonical document:

```text
original text
normalized text
tokens
sentence boundaries
meaningful token mask
```

Normalization rules must be versioned.

Initial rules:

- Unicode normalization;
- normalized case for matching;
- whitespace normalization;
- punctuation handling;
- preserve original offsets;
- do not destroy evidence mapping.

Do not aggressively stem or remove words in V1 unless tests prove it helps.

## TDD fixtures

Must include:

```text
case differences
multiple spaces
tabs
newlines
smart quotes
hyphen variants
Unicode apostrophes
English accented names
medical notation
engineering notation
legal citations
paragraph boundaries
```

Critical invariant test:

For every normalized token:

```text
original_text[token.original_start:token.original_end]
```

must map to the intended source span.

Property test:

```text
offsets never overlap incorrectly
offsets are monotonic
offsets are in bounds
```

## Manual test

Add a hidden/developer inspection screen or temporary debug UI showing:

```text
original text
normalized tokens
original ranges
```

Ask user to paste a paragraph with punctuation/capitalization and verify mapping looks correct.

STOP.

---

# PHASE 5 — Exact Plagiarism Engine: Shingles + Winnowing

Goal: Detect exact copied passages locally and deterministically.

Implement in Rust.

Components:

```text
token shingles
rolling/stable hashes
Winnowing window selection
fingerprint index
pair candidate extraction
contiguous passage reconstruction
```

Avoid language-runtime randomized hashes.

Use a deterministic stable hash suitable for fingerprints.

## TDD fixtures

Fixture A identical documents:

Expected:

```text
very high exact coverage
correct passage range
```

Fixture B no overlap:

Expected:

```text
0 meaningful exact coverage
```

Fixture C one copied paragraph inside larger documents:

Expected:

```text
only copied region returned
```

Fixture D capitalization/punctuation differences after normalization:

Expected:

```text
still matches
```

Fixture E common tiny phrase:

Expected:

```text
not reported when below minimum evidence length
```

Fixture F copied start/end boundaries:

Verify exact source offsets.

## Session integration

For N submissions:

```text
N * (N - 1) / 2
```

unique pairs.

Implement deterministic pair generation tests.

## UI

Add:

```text
Analyze Session
```

For this phase, analysis is exact-only.

Display:

```text
pairwise matrix
per-student exact coverage
exact matched passages
```

## Manual test

User creates three submissions:

A:
Original paragraph 1 + copied paragraph + original paragraph 2.

B:
Different opening + same copied paragraph + different ending.

C:
Completely unrelated text.

Expected:

```text
A ↔ B shows overlap
A ↔ C near zero
B ↔ C near zero
clicking A ↔ B shows correct highlighted copied passage
```

STOP.

---

# PHASE 6 — Common-Text and Assignment-Template Filtering

Goal: Avoid false positives caused by assignment questions and shared boilerplate.

Add:

```text
assignment instructions/reference text
session-level fingerprint document frequency
common text classifier/filter
```

Session may store optional:

```text
assignment_prompt
excluded_reference_text
```

## TDD fixtures

20 simulated submissions where all contain the same assignment question.

Expected:

```text
question excluded from plagiarism coverage
```

Two submissions share a rare paragraph.

Expected:

```text
rare paragraph remains evidence
```

Text appears in 18/20 submissions.

Expected:

```text
shown as common session text but retained as reviewable overlap
```

Frequency alone must never erase a matched answer. Only teacher-provided
assignment instructions or an explicit teacher-approved exclusion may remove
text from the score. Common session text remains visible with its reason and
raw overlap, so the teacher can inspect a widely shared answer.

## UI

Session settings:

```text
Assignment question/instructions
Exclude common session text
```

Show excluded passages separately where useful.

## Manual test

User:

1. Create 3 submissions containing the same assignment question.
2. Give only 2 students the same answer paragraph.
3. Analyze.
4. Confirm question is not counted.
5. Confirm copied answer is counted.

STOP.

---

# PHASE 6.1 — Reliability and Scoring Foundation Repair

Goal: Repair discovered correctness and trust-boundary risks before adding
modified matching or file imports.

This is a required corrective phase after completed Phase 6. It preserves the
existing architecture and user-visible workflow.

## 6.1.1 Unicode-safe canonical mapping

Replace any normalization-to-original mapping assumption that NFC only merges
characters. Valid Unicode may expand or reorder combining marks.

TDD fixtures:

```text
U+0344 normalization expansion
reordered combining marks
precomposed and decomposed equivalent input
mixed ASCII and combining-mark tokens
```

Required invariants:

```text
canonicalize never panics on valid Unicode
every reported original range is in bounds
ranges remain monotonic
matching evidence still highlights the intended original passage
```

## 6.1.2 Explainable exclusion reasons

When Prompt, Reference, and CommonSessionText exclusions touch or overlap,
preserve each reason accurately. Do not merge two adjacent spans into one span
with an incorrect reason.

TDD:

```text
adjacent Prompt + Reference spans retain both reasons
overlapping exclusions use documented precedence or multiple reasons
coverage is unchanged when only explanation metadata changes
```

## 6.1.3 Eligible-text coverage

Define scoring terms precisely:

```text
eligible meaningful tokens = meaningful tokens minus teacher-approved exclusions
matched eligible tokens = union of matched tokens within eligible text
coverage = matched eligible tokens / eligible meaningful tokens × 100
```

If eligible text is empty, report `insufficient assessable text`; never report
a clean `0%` result. Common session text is not automatically ineligible.

TDD fixture:

```text
900-token assignment prompt + 100-token copied answer
→ copied eligible answer coverage = 100%, not 10%
```

## 6.1.4 Exact-engine bounded-work repair

Prevent repeated-text inputs from expanding every duplicate fingerprint pair
before deduplication. Precompute document fingerprints once per analysis and
avoid re-expanding already-covered candidate intervals on the same alignment.

TDD and benchmark fixtures:

```text
100, 200, 400 repeated-token documents
identical long essays
realistic non-repetitive assignments
brute-force oracle preserves exact-match recall after optimization
```

Record release-build elapsed time and peak memory. No fixed performance claim
is allowed from debug-build measurements.

## Phase 6.1 gate

Run all existing Phase 4–6 tests plus the new tests. Manually paste the Unicode
fixtures, run analysis, and verify no crash and correct source highlighting.

STOP.

---

# PHASE 7 — Modified / Near-Exact Copying

Goal: Detect copied passages with small edits.

Implement:

```text
candidate token regions
Jaccard-style overlap
fuzzy token similarity
edit-tolerant local alignment
span reconstruction
```

## TDD fixture classes

1. punctuation changes;
2. capitalization;
3. one-word substitutions;
4. inserted adjectives;
5. deleted minor words;
6. minor sentence reordering;
7. unrelated text with shared terminology;
8. medical/engineering vocabulary that naturally overlaps.

Tests must verify:

```text
true modified copy is detected
domain terminology alone is not enough
span offsets remain correct
confidence is deterministic
```

## Evidence type

```text
EXACT
MODIFIED
```

## UI

Different visual labels for exact and modified evidence.

Do not use frightening accusation language.

## Manual test

Provide the user with two sample passages:

Original:
"Photosynthesis converts light energy into chemical energy that can later be used by the plant."

Modified:
"Through photosynthesis, plants convert light into stored chemical energy that can be used later."

At this phase heavily rewritten passages may not match strongly; that is acceptable.

Ask user to test a lightly edited copy.

STOP.

---

# PHASE 8 — Native File Imports: PDF and DOCX

Goal: Add common digital assignment formats.

## DOCX

Extract text locally.

Tests:

```text
single paragraph
multiple paragraphs
headings
tables with text
Unicode
empty document
corrupt document
```

## PDF

First determine whether usable embedded text exists.

Pipeline:

```text
PDF
├── digital text available → extract
└── image-only/no usable text → reject with an actionable validation error
```

Tests:

```text
digital PDF
multi-page PDF
empty PDF
corrupt PDF
image-only PDF rejection
```

No cloud parsing.

## Manual test

User uploads:

1. one DOCX;
2. one normal text PDF;
3. one image-only PDF.

Expected:

```text
DOCX text extracted
digital PDF text extracted
image-only PDF rejected without storing partial text
```

STOP.

# PHASE 15 — Evidence Merge and Final Coverage Scoring

Goal: Produce one stable report from overlapping evidence types.

Evidence may overlap:

```text
EXACT
MODIFIED
```

Need interval union.

## TDD

Tests:

```text
non-overlapping spans
fully overlapping spans
nested spans
adjacent spans
same source different evidence types
multiple sources
directional document coverage
```

Primary score:

```text
unique matched eligible meaningful tokens / eligible meaningful tokens
```

Teacher-provided instructions and explicit teacher-approved exclusions are
removed from both numerator and denominator. Common session text is reported
separately and remains reviewable; it is not automatically removed solely
because many students share it. If no eligible meaningful tokens remain, report
`insufficient assessable text` instead of `0%`.

Per-type coverage should be separately reported but not blindly summed.

Example deterministic fixture:

```text
100 meaningful tokens
30 unique matched
→ 30%
```

Add fixtures for a copied answer after a long assignment prompt, an all-excluded
submission, and a widely shared but inspectable answer.

## Pairwise directional scoring

Test:

```text
A 1000 words
B 2000 words
500 matched

A→B ≈ 50%
B→A ≈ 25%
```

## Manual test

User reviews a mixed exact + modified fixture and confirms:

- overall score is not double-counted;
- exact/modified evidence is visible;
- highlighted regions are correct.

STOP.

---

# PHASE 16 — Full Session Analysis Engine

Goal: End-to-end analysis of all current submissions.

Pipeline:

```text
load session
validate unlocked/locked state
canonicalize submissions
load/build indexes
generate unique pairs
run exact
run modified
merge evidence
score
persist results
```

Implement progress events.

States:

```text
preparing
exact
modified
aligning
scoring
saving
complete
failed
```

## TDD

20-submission synthetic session.

Tests:

```text
190 unique pairs
no duplicate comparisons
progress monotonic
failure can be retried
analysis results persisted
re-analysis invalidates stale result appropriately
```

Incremental behavior:

If one submission changes before lock:

```text
invalidate raw pair analyses involving changed submission
recompute session-dependent filtering/scoring for all pairs when document
frequency, prompt/reference exclusions, or session settings can affect them
```

Persist raw match evidence separately from session-dependent scored evidence.
Cache keys must include all filter, scoring, normalization, fingerprint, and
alignment versions that influence the result.

## UI

Session analysis screen.

Pairwise matrix.

Student report summary.

## Manual test

Create 5 submissions:

```text
2 copied
1 lightly modified
1 unrelated
```

Analyze and inspect matrix.

STOP.

---

# PHASE 17 — Reference Libraries

Goal: Historical assignments become first-class comparison sources.

Entities:

```text
ReferenceLibrary
ReferenceSource
ReferenceSubmission
```

Properties:

```text
read-only content
library name
created/imported date
source metadata
engine versions
```

Completed sessions may be archived into a Reference Library.

## TDD

Tests:

```text
archive completed session
cannot archive unfinished invalid session
library is read-only
historical submission indexed
new session compares against selected library
current and historical evidence separated
```

## UI

Reference Libraries page:

```text
list libraries
view library
archive completed session
select enabled libraries per current session
```

## Manual test

1. Complete Session A.
2. Archive as "Biology 2025".
3. Create Session B.
4. Add text copied only from Session A, not another current student.
5. Enable Biology 2025.
6. Analyze.
7. Verify historical match appears.

STOP.

---

# PHASE 18 — `.plagpack` Portable Reference Format

Goal: Export/import historical comparison corpora.

File extension:

```text
.plagpack
```

Implementation:

```text
versioned compressed container
manifest
compact SQLite reference DB or equivalent structured payload
canonical text
fingerprints
hashes
engine metadata
signature metadata
```

Default:

```text
anonymized students
no source images
```

Canonical text must always be included so indexes can be rebuilt under future engine versions.

## Format version

Start:

```text
plagpack_format_version = 1
```

## TDD

Round-trip:

```text
library
→ export
→ delete original
→ import
→ compare
```

Expected equivalent results.

Tests:

```text
corrupt archive rejected
manifest missing rejected
unsupported future version rejected clearly
duplicate content deduplicated
hash mismatch rejected
anonymous export does not leak names
images excluded by default
```

## Manual test

1. Export historical library.
2. Save file.
3. Remove library from app.
4. Import `.plagpack`.
5. Analyze test submission copied from imported pack.
6. Confirm historical evidence returns.

STOP.

---

# PHASE 19 — Report Generation

Goal: Generate useful per-student evidence reports.

Report sections:

```text
Student
Session
Comparison corpus
Current submissions compared
Historical libraries compared
Overall matched coverage
Exact coverage
Modified coverage
Top current matches
Top historical matches
Detailed passage evidence
Engine versions
Report timestamp
Integrity metadata
```

Language must say:

```text
matching/reused content
```

Not:

```text
Student A definitely copied Student B
```

The exact required report footer from the product design must appear at the
bottom of every human-readable report, including zero-match and historical-only
reports. Signing covers this footer as part of the canonical report payload.

```text
This report identifies matching or reused content in the comparison corpus. Similarity alone does not prove plagiarism. The final decision about whether plagiarism occurred must be made by the teacher or qualified reviewer after reviewing the highlighted evidence and assignment context.
```

## PDF

Generate locally.

Tests should inspect structured report model separately from PDF rendering.

Snapshot/golden test only stable layout components.

## TDD

Tests:

```text
zero-match report
high-match report
historical-only match
current-only match
mixed match
long passages
student anonymization
every confirmed matched token maps to a highlighted source and comparison span
overlapping matches remain fully highlighted
excluded text remains visible with its exclusion reason
exact required reviewer-decision footer appears at report bottom
```

## Manual test

Generate and open PDF.

Check:

- readable;
- no clipping;
- passage evidence understandable;
- current vs historical sources clearly separated.
- every duplicate/matched region highlighted on both sides;
- required reviewer-decision footer visible at bottom.

STOP.

---

# PHASE 20 — Certified Sessions and Cryptographic Integrity

Goal: Teacher can lock a session and generate tamper-evident certified reports.

Use:

```text
SHA-256
Ed25519
```

Session locking must freeze:

```text
submission content hashes
comparison corpus configuration
analysis configuration
engine version
```

Generate canonical signed report payload.

## TDD: signing

Tests:

```text
sign then verify = valid
one byte payload mutation = invalid
wrong public key = invalid
serialize/deserialize preserves verification
```

## TDD: session lock

Tests:

```text
locked session cannot edit submission
locked session cannot remove student
locked session cannot silently change reference libraries
unlock requires explicit invalidation workflow if supported
```

If unlocking is supported:

```text
all prior certified results become invalid/stale
```

## QR

Encode verification payload or compact verification data.

Do not expose private key.

## Manual test

1. Lock session.
2. Try editing submission → blocked.
3. Generate report.
4. Verify report inside app.
5. Modify exported payload in test utility → verification fails.

STOP.

---

# PHASE 21 — Student Self-Check Mode

Goal: Self-check without pretending it is certified.

UI must clearly label:

```text
Self Check
Not Teacher Certified
```

Self-check can compare against:

```text
selected local Reference Libraries
other explicitly included local sources
```

It must never imply a universal plagiarism guarantee.

## TDD

Tests:

```text
self-check badge visible
report cannot render certified status
signing path is different/disabled
comparison corpus listed
```

## Manual test

Run self-check and compare report against teacher-certified report.

STOP.

---

# PHASE 22 — Android Production Integration

Goal: Android first-class support.

Minimum target should follow current Tauri requirements at implementation time.

Validate:

```text
filesystem
SQLite
PDF report
plagpack export/import
```

## Automated tests

Run Rust tests for Android-compatible crates.

UI tests where practical.

Add Android build CI/configuration if environment permits.

## Mandatory physical-device manual test

1. fresh install;
2. create session;
3. add students;
4. import TXT, Markdown, digital PDF, and DOCX examples;
5. analyze;
6. open matched passages;
7. create report;
8. export `.plagpack`;
9. close/reopen app;
10. verify persistence;
11. enable airplane mode;
12. repeat analysis;
13. verify no network dependency.

Record:

```text
device
Android version
RAM
analysis time
```

STOP.

---

# PHASE 23 — iOS / iPadOS Production Integration

Goal: iPhone/iPad first-class support.

Validate same complete flow as Android.

## Mandatory physical-device test

Same flow as Phase 22.

Also verify:

```text
Files export/import
app lifecycle suspend/resume
low-memory behavior
```

STOP.

---

# PHASE 24 — Windows, macOS and Linux Production Validation

Goal: Desktop parity.

For each OS:

```text
install
first launch
session CRUD
digital file import
analysis
report
plagpack
offline restart
```

At minimum maintain smoke-test scripts.

macOS:

```text
Apple Silicon
Intel where available or CI
```

Windows:

```text
x64
```

Linux:

Document supported distributions/runtime requirements.

## Manual gate

User validates on currently available desktop OS.

Other platforms remain CI/build-validation gates until physically tested.

STOP.

---

# PHASE 25 — Performance, Caching and Incremental Analysis

Goal: Make the application fast.

Persist:

```text
canonical text
fingerprints
analysis results
```

Cache key must include relevant engine version.

Raw pair-match caching and scored-result caching are separate. A submission
change may reuse unaffected raw pair matches, but any session-wide document
frequency or exclusion-setting change requires recomputing affected scoring for
every pair. Never reuse a score with incompatible filter or scoring versions.

Tests:

```text
same unchanged submission reuses cache
changed text invalidates fingerprint cache
normalization version change invalidates downstream indexes
```

Benchmark:

```text
20 submissions
50 submissions
100 submissions
historical library 1k docs
historical library 10k docs if feasible
repeated-token pathological input
identical long-assignment input
release-build measurements on Phase 6.2 target devices
```

No premature vector DB.

Use compact local structures until benchmarks prove otherwise.

## Manual test

User compares first analysis vs repeated unchanged analysis.

Expected second run meaningfully faster.

STOP.

---

# PHASE 26 — Reliability and Recovery

Goal: App survives interruption and corruption gracefully.

Test scenarios:

app closes during analysis
disk write failure simulation
corrupt DB copy
partial plagpack
invalid PDF
very large input
zero submissions
one submission
```

Implement resumable/retry behavior where appropriate.

Never produce a certified report from partial analysis.

## Manual test

Kill the app during an analysis.

Reopen.

Expected:

```text
no corrupt completed result
clear retry state
all previous submissions remain
```

STOP.

---

# PHASE 27 — Accessibility and UX Finalization

Goal: The app is usable by teachers under real conditions.

Must support:

```text
keyboard navigation desktop
visible focus
screen-reader labels
sufficient contrast
large touch targets mobile
loading/progress clarity
clear error recovery
```

No technical terms such as:

```text
Winnowing
```

in normal teacher-facing UI unless under diagnostics.

Use:

```text
Exact match
Modified match
Matching content
Historical source
```

## Automated tests

Accessibility test tooling where practical.

Component tests for keyboard use.

## Manual test

User completes a session without developer knowledge.

STOP.

---

# PHASE 28 — Benchmark Suite

Goal: Establish evidence for quality.

## Plagiarism dataset

Categories:

```text
exact copy
light edits
sentence reordering
same-topic independent writing
common terminology
quotes
references
historical reuse
```

Metrics:

```text
passage precision
passage recall
F1
false positive rate
coverage accuracy
```

Split all evaluation data into development, calibration, and held-out sets by
writer and assignment. Do not tune matching thresholds on held-out examples.
Held-out same-topic independent writing is mandatory for
false-positive reporting.

Do not publicly claim "best in the world" until benchmark evidence supports it.

No manual gate required if this phase is data-only, but agent must show benchmark results before continuing.

---

# PHASE 29 — Release Hardening

Goal: V1 release candidate.

Must pass:

```text
pnpm check
frontend tests
Layer D (recorded fixtures + command contracts + device gates)
cargo fmt
cargo clippy -D warnings
cargo test workspace
release desktop builds
Android build
iOS build
migration-from-clean test
offline test
plagpack round-trip
signature tests
plagiarism fixture suite
```

Security review:

```text
private keys not logged
student text not logged
path traversal prevented in imports
archive zip-bomb protection
plagpack extraction size limits
malformed input handling
SQL parameters bound
no shell interpolation with user input
```

Privacy review:

```text
no unexpected network request during analysis
```

Add a test or development network monitor where practical.

---

# PHASE 30 — V1 Final Acceptance

The project is not "done" because it compiles.

V1 is complete only if all below are true.

## Functional

- sessions work;
- students work;
- text/PDF/DOCX import works;
- exact matching works;
- modified matching works;
- false-positive filtering works;
- current-session matrix works;
- historical libraries work;
- `.plagpack` round-trip works;
- reports work;
- certified signing works;
- self-check works;
- offline mode works.

## Platform

Validated:

```text
Android
iOS/iPadOS
Windows
macOS
Linux
```

Build support is not equivalent to real-device validation.

At least Android + iOS + one desktop platform must have complete real manual flow validation before V1 release.

## Test quality

No skipped critical tests.

No known failing tests.

No warnings accepted from Clippy.

No TypeScript errors.

No lint errors.

Phase 28 records exist for the released engine versions. Measured passage
precision/recall and same-topic false-positive rate meet the acceptance
ceilings recorded before Phase 7.

## User manual acceptance

Agent must provide one final end-to-end acceptance script:

### Teacher certified flow

1. Create assignment.
2. Add 5 students.
3. Upload TXT, Markdown, DOCX, and digital PDF assignments.
4. Paste text assignment.
5. Add one intentionally copied assignment.
6. Enable historical library.
7. Lock session.
8. Analyze.
9. Inspect pairwise matrix.
10. Inspect exact evidence.
11. Inspect modified evidence.
13. Inspect historical match.
14. Generate reports.
15. Verify signed report.
16. Export historical `.plagpack`.
17. Restart app offline.
18. Confirm all results persist.

21. Open each generated report and verify every confirmed duplicate/matched
    region is highlighted on both sides.
22. Verify the required reviewer-decision footer appears at the bottom.

### Expected result

All steps succeed with no cloud dependency.

Every confirmed duplicate region is visible in report evidence. Reports do not
make an automated plagiarism verdict; footer directs the teacher or qualified
reviewer to make the final decision.

After the user confirms this final test, the coding agent may mark:

```text
PROVENANCE V1 COMPLETE
```

---

# 9. Required Agent Status Format

At the beginning of each work cycle:

```text
Current phase: PHASE X — <name>
Last approved phase: PHASE Y
Current objective: <one sentence>
```

During implementation, the agent may work autonomously.

At phase completion, output exactly these sections:

```text
PHASE X COMPLETE

Implemented
- ...

Automated tests
- command: ...
- result: PASS
- ...

Manual test required
1. ...
2. ...
3. ...

Expected result
- ...

Reply with:
"Phase X approved"
or describe what failed.
```

Then STOP.

---

# 10. Regression Rule

Any bug discovered after a phase was approved must follow:

```text
bug report
→ reproduce
→ failing regression test
→ fix
→ full affected phase tests
→ broader regression suite
→ manual re-test if user-visible
```

Never fix a production bug without first creating a test that proves the bug existed.

---

# 11. Dependency Update Rule

Because the project uses latest stable packages:

1. At project bootstrap, resolve latest stable versions.
2. Commit the lockfile.
3. Do not automatically change dependencies mid-phase without need.
4. At major phase boundaries, security updates may be applied.
5. Never accept breaking major upgrades automatically.
6. Tauri must remain on v2 for V1.
---

# 12. Data Versioning Rule

Persist these versions with processed submissions:

```text
schema_version
normalization_version
fingerprint_version
alignment_version
scoring_version
report_schema_version
plagpack_format_version
```

If a version changes, tests must define whether old cached data:

```text
remains valid
must be migrated
must be recomputed
```

No silent reuse of incompatible indexes.

---

# 14. Golden Test Corpus Required in Repository

Create small legally safe synthetic fixtures.

```text
tests/fixtures/text/
├── exact/
├── modified/
├── unrelated/
├── common-text/
├── medical/
├── engineering/
├── humanities/
└── historical/
```

Every algorithm change must run against this corpus.

Store expected evidence spans where possible.

---

# 15. Definition of Done for Every Task

A task is complete only when:

```text
[ ] failing test was written first
[ ] implementation passes test
[ ] relevant edge cases tested
[ ] code refactored
[ ] formatting passes
[ ] lint/clippy passes
[ ] typecheck passes
[ ] affected integration tests pass
[ ] no architectural decision was violated
[ ] user-facing change has manual test instructions
```

If any box is false, the task is not done.

---

# 16. Definition of Done for Every Phase

```text
[ ] all phase tasks complete
[ ] all targeted tests pass
[ ] full regression tests pass
[ ] build passes
[ ] no warnings/errors
[ ] migrations verified
[ ] docs updated
[ ] manual test script provided
[ ] agent stopped
[ ] user explicitly approved
```

The agent cannot self-approve a manual gate.

---

# 17. First Instruction to the Coding Agent

Use this exact starting instruction:

```text
Read PROVENANCE_AUTONOMOUS_IMPLEMENTATION_PLAN_V1.md completely before modifying the repository.

Treat it as the authoritative V1 execution contract.

Start only with PHASE 0.

Use strict RED → GREEN → REFACTOR TDD for every behavior.

Within a phase, work autonomously until all automated gates pass.

At the end of the phase, provide the required manual test instructions and STOP.

Do not begin the next phase until I explicitly reply that the current phase is approved.

Do not change finalized architecture, technology stack, local-first behavior,
scoring definition, or phase ordering unless I explicitly approve the change.
```

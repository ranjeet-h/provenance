# Matching Improvements — V1 Proposal

**Status:** the user-directed Mac-desktop matching approach and one model are recorded here; no weights, dependency, or matching code are added by this document.

**Scope boundary:** use one pretrained embedding model only inside semantic candidate matching on the macOS desktop app. Existing exact/modified matching, parsing, and file extraction stay model-free. Supported inputs remain pasted text, TXT, Markdown, digital PDF, and DOCX; OCR and image/scanned-PDF extraction remain prohibited. `OCR_REMOVED.md` still has an older blanket statement against local model downloads; reconcile that wording with this narrow matching-only decision before implementation, without changing the OCR boundary.

## Scope and product boundary

Improve matching of already-extracted digital text on the macOS desktop app only. This change does not alter supported input types, restore OCR or image/scanned-PDF extraction, add a hosted service, train or fine-tune a model, or produce an automated plagiarism verdict. It does not change the current comparison corpus: submissions in the same analysis session.

The current exact and modified matchers remain the auditable lexical detectors and must not call a model. The one embedding model below is used only by the separately labeled semantic candidate detector; it augments, never replaces, exact/modified matching. If it fails evaluation, stop and report the failure rather than silently switching models.

The reviewer, not the engine, decides whether detected reuse constitutes plagiarism in context. See the evidence-versus-judgement contract in `plan_v1.md`.

## Proposed matching pipeline

1. Preserve the current canonicalization and original-character offsets.
2. Split each submission into overlapping sentence windows sized for passage matching, well below the model's 32K-token limit. Preserve exact source offsets for every window.
3. Encode windows with the single selected pretrained embedding model below. Compare windows only within the session corpus to retrieve semantically similar candidate passages.
4. Refine each candidate with passage/sentence alignment. Produce source and submission spans that a reviewer can inspect; do not present cosine similarity alone as proof or as exact word overlap.
5. Keep evidence types distinct in the result: **Exact**, **Modified**, and **Semantic candidate**. Keep the current lexical coverage percentage separate from semantic similarity scores. Show both sides of each passage and any relevant prompt/reference exclusions.
6. Do not infer copying direction, intent, authorship, or an automated plagiarism conclusion.

Semantic similarity is a candidate-finding signal, not a plagiarism decision. Similar answers to the same prompt, standard definitions, quotations, and independently written explanations can all be semantically close.

## The single embedding model — macOS desktop only

**Use [`Qwen/Qwen3-Embedding-0.6B-GGUF`](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B-GGUF), Q8_0, only for semantic candidate matching.** This is the desktop choice because it is a stronger modern text-embedding model while remaining the smallest member of its 0.6B/4B/8B series: the publisher lists 100+ languages, up to 32K tokens, up to 1024 dimensions, Apache-2.0 licensing, and a 639 MB Q8_0 artifact. The exact [Q8_0 file](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B-GGUF/blob/d20cf9c/Qwen3-Embedding-0.6B-Q8_0.gguf) has SHA-256 `06507c7b42688469c4e7298b0a1e16deff06caf291cf0a5b278c308249c3e439`. Its model card reports 70.70 on English MTEB v2, but that benchmark table uses 2025 results; it is not an assignment-plagiarism benchmark and does not establish this app's quality.

The model publisher documents running this GGUF with llama.cpp on macOS and specifies `--pooling last`; that is a runtime route, not proof of stable Tauri integration. There are current upstream reports of Apple-Silicon Metal correctness/stability problems for Qwen3 embeddings ([heap-corruption report](https://github.com/ggml-org/llama.cpp/issues/28357), [long-input NaN report](https://github.com/ggml-org/llama.cpp/issues/27784)). Treat those as a release-blocking risk: pin a llama.cpp revision and backend, test the exact Q8_0 artifact on supported Macs with repeated and long-session runs, and do not ship unless outputs remain finite and stable. If it fails, stop and report the failure; do not silently switch models or runtime paths.

Keep this model call confined to semantic matching. Exact/modified algorithms, parsing, and all supported file extractors remain deterministic and model-free. Preserve offsets through chunking; use the model's correct last-token pooling and the same matching instruction for both passages, then compare vectors consistently. Pin the exact model revision and artifact hash before distribution. Do not fine-tune, add a second model, add a model selector, silently substitute another model, commit weights, or add an OCR/model-fetch path.

## Evaluation gate before implementation

Build a human-labeled, assignment-like evaluation set with source text and exact gold spans. Include:

- exact copies and lightly modified copies;
- human and machine-produced paraphrases of source passages;
- independently written answers to the same prompt (hard negatives);
- shared prompt text, quotations, citations, definitions, and common subject terminology;
- examples with no reuse and examples where only a small part is reused.

Split evaluation by source/assignment so near-duplicate passages cannot leak between development and test sets. Report passage-level precision, recall, and F1, plus false-positive rates at the **submission-pair** level. In an academic-integrity workflow, false positives require particular scrutiny. Choose thresholds on development data and report final results once on held-out data; do not claim a generic model benchmark predicts classroom performance.

Also test the single selected artifact on actual supported macOS devices: downloaded/installed size, cold-start time, per-assignment latency, peak memory, repeated-run stability, long-session stability, and numerical consistency across supported Apple-silicon Macs. If Intel Macs are supported, verify the same pinned model and runtime there too. A published macOS command is not proof this Tauri app works or meets performance requirements.

Prior work supports treating this as passage retrieval/alignment rather than whole-document similarity alone: a semantic paraphrase-span study reported gains over lexical and sentence-embedding retrieval baselines on its Finnish dataset ([paper](https://arxiv.org/abs/2112.04886)). That result motivates evaluation; it is not a claim about this app's expected accuracy.

## Decisions still required

- Reconcile the older blanket local-model wording in `OCR_REMOVED.md` with this user-directed, macOS-only semantic-matcher exception before implementation. Keep its OCR and scanned-image prohibitions unchanged.
- Verify the Apache-2.0 notices, pin the artifact and tokenizer, and prove the llama.cpp/Tauri path on actual supported Macs.
- Set release thresholds from labeled held-out evidence, not an unsupported “90% accurate” promise.

## Research checked

Qwen's model card/GGUF artifact and current llama.cpp macOS embedding reports checked 2026-09-23. The published benchmark is generic embedding evidence; this app has not benchmarked the model, and the reported Metal defects make a real-device integration gate mandatory. This direction selects one model for the semantic matcher only; implementation and this app's accuracy targets remain unproven.

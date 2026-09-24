# OCR removed by product decision

The local image-to-text/notebook workflow was removed from Provenance on
2026-09-07.

Why:

- Local/mobile models tested on notebook photographs did not produce reliable
  text for the expected camera conditions: perspective, shadows, ruled pages,
  handwriting, and full-page layouts.
- The product will not fine-tune a model, build a proprietary model, or ship a
  large vision model locally.
- The local semantic-model experiment and its fetch script were also removed;
  the current product uses deterministic exact/modified matching without any
  model download, bundle, or runtime dependency.
- A bad extraction is worse than a clear unsupported-input message for a
  plagiarism decision. There is no OCR fallback path.

Current contract:

- Supported: pasted text, TXT, Markdown, digital PDFs with embedded text, and
  DOCX files.
- Unsupported: image files and image-only/scanned PDFs. They are rejected and
  are never partially stored or routed to another extractor.
- No local model bundle or model-fetch script remains in the repository.

Future rule:

Do not reintroduce local OCR, notebook scanning, camera capture, image review,
local model downloads, or image fixtures. Reconsideration is allowed only
through an explicit future product decision for a hosted LLM service, with its
cost, privacy, latency, availability, and accuracy requirements documented
before any implementation begins. It must not be added as an automatic
fallback.

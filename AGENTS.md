# Agent operating rules

Read [`OCR_REMOVED.md`](OCR_REMOVED.md) before changing file extraction,
submission imports, or supported input types.

The product contract is digital extraction only: pasted text, TXT, Markdown,
digital PDF, and DOCX. Image files and image-only/scanned PDFs are
explicitly unsupported. Do not add preprocessing, camera capture, image
extraction, a hidden alternate path, or an automatic fallback. A hosted vision
service may be considered only after the explicit future decision described in
the root decision record.

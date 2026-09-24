//! Native digital file imports — TXT, Markdown, DOCX, and PDF with embedded text.
//!
//! - DOCX is read with a purpose-built reader (`zip` + `quick-xml`):
//!   paragraphs, headings, and tables in document order. Field codes,
//!   deletions, and drawings are skipped; line breaks/tabs are preserved.
//! - PDF text comes from `pdf-extract` per page. Product rule: a page with
//!   page images needs at least [`MIN_PDF_PAGE_CHARS`] non-whitespace
//!   characters to count as readable (noise floor against scan artifacts);
//!   an image-free page with ANY extracted non-whitespace text counts as
//!   digital, so short but valid documents (e.g. a one-line memo) import.
//!   Pages with images but no substantive text are scanned; pages with
//!   neither text nor images are unreadable.
//! - Image-only and mixed scanned PDFs are rejected with the affected page
//!   numbers. No page content may silently disappear and no image-to-text
//!   engine is invoked.

use std::io::Read;

use serde::{Deserialize, Serialize};

use crate::domain::SourceType;
use crate::error::CoreError;
use crate::ingest::MAX_UPLOAD_BYTES;

/// Noise floor: non-whitespace characters per page that count as readable
/// PDF text ON PAGES CONTAINING IMAGES (scan-artifact guard). Image-free
/// pages with any extracted text count as digital regardless — see
/// [`extract_pdf`]. Explicit product rule, not a silent cutoff.
pub const MIN_PDF_PAGE_CHARS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Txt,
    Markdown,
    Pdf,
    Docx,
}

/// Detect the file kind from its extension (case-insensitive).
pub fn detect_kind(filename: &str) -> Result<FileKind, CoreError> {
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match ext.as_str() {
        "txt" => Ok(FileKind::Txt),
        "md" | "markdown" => Ok(FileKind::Markdown),
        "pdf" => Ok(FileKind::Pdf),
        "docx" => Ok(FileKind::Docx),
        _ => Err(CoreError::validation(
            "unsupported file type (use TXT, Markdown, PDF, or DOCX)",
        )),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedDocument {
    pub text: String,
    pub pages: usize,
    pub source: SourceType,
}

/// What happened to an uploaded file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileIngestResult {
    /// Text extracted and validated, ready to store with [`SourceType`].
    Digital {
        text: String,
        pages: usize,
        source: SourceType,
        filename: String,
    },
}

/// Route raw upload bytes by filename into an import outcome.
/// Raw size is enforced BEFORE any parser runs. Returned text is NOT yet
/// validated — the save path validates size, emptiness, and newlines
/// exactly like pasted text.
pub fn extract_file(filename: &str, bytes: &[u8]) -> Result<FileIngestResult, CoreError> {
    let name = filename.trim();
    if name.is_empty() {
        return Err(CoreError::validation("a filename is required"));
    }
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(CoreError::validation(format!(
            "file is too large (max {} MB)",
            MAX_UPLOAD_BYTES / 1_000_000
        )));
    }
    match detect_kind(name)? {
        kind @ (FileKind::Txt | FileKind::Markdown) => {
            let text = std::str::from_utf8(bytes)
                .map_err(|_| CoreError::validation("file is not valid UTF-8 text"))?;
            Ok(FileIngestResult::Digital {
                text: text.to_string(),
                pages: 1,
                source: if matches!(kind, FileKind::Txt) {
                    SourceType::TxtFile
                } else {
                    SourceType::MarkdownFile
                },
                filename: name.to_string(),
            })
        }
        FileKind::Docx => {
            let text = extract_docx(bytes)?;
            Ok(FileIngestResult::Digital {
                text,
                pages: 1,
                source: SourceType::DocxFile,
                filename: name.to_string(),
            })
        }
        FileKind::Pdf => extract_pdf(name, bytes),
    }
}

/// Decode XML predefined entities and numeric character references with a
/// single left-to-right scan (no double-decoding hazards).
fn decode_entities(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        match after.find(';') {
            Some(semi) => {
                let entity = &after[..semi];
                let replacement = match entity {
                    "amp" => Some("&".to_string()),
                    "lt" => Some("<".to_string()),
                    "gt" => Some(">".to_string()),
                    "quot" => Some("\"".to_string()),
                    "apos" => Some("'".to_string()),
                    _ if entity.starts_with('#') => parse_char_ref(entity),
                    _ => None,
                };
                match replacement {
                    Some(text) => out.push_str(&text),
                    None => {
                        out.push('&');
                        out.push_str(entity);
                        out.push(';');
                    }
                }
                rest = &after[semi + 1..];
            }
            None => {
                out.push_str(&rest[amp..]);
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

fn parse_char_ref(entity: &str) -> Option<String> {
    let digits = entity.strip_prefix('#')?;
    let code = if let Some(hex) = digits
        .strip_prefix('x')
        .or_else(|| digits.strip_prefix('X'))
    {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        digits.parse::<u32>().ok()?
    };
    char::from_u32(code).map(|c| c.to_string())
}

/// Extract document-order text from DOCX `word/document.xml`.
pub fn extract_docx(bytes: &[u8]) -> Result<String, CoreError> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|_| CoreError::validation("could not read this Word file (not a valid DOCX)"))?;
    // Bounded read: a tiny zip can expand to gigabytes (ZIP bomb). Read at
    // most the cap + 1 byte; anything more is rejected before XML parsing.
    let mut xml_bytes = Vec::new();
    archive
        .by_name("word/document.xml")
        .map_err(|_| CoreError::validation("could not read this Word file (no document found)"))?
        .take(crate::ingest::MAX_DOCX_XML_BYTES as u64 + 1)
        .read_to_end(&mut xml_bytes)
        .map_err(|_| CoreError::validation("could not read this Word file"))?;
    if xml_bytes.len() > crate::ingest::MAX_DOCX_XML_BYTES {
        return Err(CoreError::validation(
            "Word document expands beyond the supported size",
        ));
    }
    let xml = String::from_utf8(xml_bytes)
        .map_err(|_| CoreError::validation("could not read this Word file"))?;

    #[derive(Default)]
    struct TableFrame {
        rows: Vec<String>,
        cells: Vec<String>,
        cell_paras: Vec<String>,
    }

    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(false);
    let mut blocks: Vec<String> = Vec::new();
    let mut para: Option<String> = None;
    let mut tables: Vec<TableFrame> = Vec::new();
    let mut skip_depth: usize = 0; // inside w:instrText / w:delText / w:del
    let mut buf = Vec::new();

    let flush_para =
        |para: &mut Option<String>, tables: &mut Vec<TableFrame>, blocks: &mut Vec<String>| {
            if let Some(p) = para.take() {
                if !p.trim().is_empty() {
                    if let Some(frame) = tables.last_mut() {
                        frame.cell_paras.push(p);
                    } else {
                        blocks.push(p);
                    }
                }
            }
        };

    // Shared open-tag handling for Start and Empty elements. `is_empty`
    // marks self-closing tags (`<w:del/>`), which have no matching End:
    // skipped deletions/fields must NOT change the nesting depth, or all
    // later paragraph text would be silently omitted.
    let handle_open = |tag: &str,
                       is_empty: bool,
                       para: &mut Option<String>,
                       tables: &mut Vec<TableFrame>,
                       blocks: &mut Vec<String>,
                       skip_depth: &mut usize| {
        // Note: w:tab / w:br are often self-closing (Empty); both
        // spellings are handled identically here.
        match tag {
            "p" => {
                let mut dummy = para.take();
                if let Some(p) = dummy.take() {
                    if !p.trim().is_empty() {
                        if let Some(frame) = tables.last_mut() {
                            frame.cell_paras.push(p);
                        } else {
                            blocks.push(p);
                        }
                    }
                }
                *para = Some(String::new());
            }
            "t" => {
                // text run (skipped inside field codes/deletions)
            }
            "instrText" | "delText" | "del" => {
                if !is_empty {
                    *skip_depth += 1;
                }
            }
            "tab" => {
                if *skip_depth == 0 {
                    if let Some(p) = para.as_mut() {
                        p.push('\t');
                    }
                }
            }
            "br" => {
                if *skip_depth == 0 {
                    if let Some(p) = para.as_mut() {
                        p.push('\n');
                    }
                }
            }
            "tbl" => {
                flush_para(para, tables, blocks);
                tables.push(TableFrame::default());
            }
            "tr" => {
                if let Some(frame) = tables.last_mut() {
                    frame.cells.clear();
                }
            }
            "tc" => {
                if let Some(frame) = tables.last_mut() {
                    frame.cell_paras.clear();
                }
            }
            _ => {}
        }
    };

    loop {
        match reader.read_event_into(&mut buf) {
            Err(_) => {
                return Err(CoreError::validation("could not read this Word file"));
            }
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let tag: &str = e.local_name().into_inner();
                handle_open(
                    tag,
                    false,
                    &mut para,
                    &mut tables,
                    &mut blocks,
                    &mut skip_depth,
                );
            }
            Ok(Event::Empty(e)) => {
                let tag: &str = e.local_name().into_inner();
                handle_open(
                    tag,
                    true,
                    &mut para,
                    &mut tables,
                    &mut blocks,
                    &mut skip_depth,
                );
            }
            Ok(Event::Text(e)) => {
                if skip_depth == 0 {
                    if let Some(p) = para.as_mut() {
                        // Raw content + single-pass entity decoding.
                        p.push_str(&decode_entities(e.as_ref()));
                    }
                }
            }
            Ok(Event::GeneralRef(e)) => {
                // Entity/character references (e.g. &amp;) arrive separately.
                if skip_depth == 0 {
                    if let Some(p) = para.as_mut() {
                        let raw: &str = e.as_ref();
                        p.push_str(&decode_entities(&format!("&{raw};")));
                    }
                }
            }
            Ok(Event::End(e)) => {
                let tag: &str = e.local_name().into_inner();
                match tag {
                    "p" => flush_para(&mut para, &mut tables, &mut blocks),
                    "instrText" | "delText" | "del" => {
                        skip_depth = skip_depth.saturating_sub(1);
                    }
                    "tc" => {
                        if let Some(frame) = tables.last_mut() {
                            let cell = frame.cell_paras.join(" ");
                            frame.cells.push(cell);
                            frame.cell_paras.clear();
                        }
                    }
                    "tr" => {
                        if let Some(frame) = tables.last_mut() {
                            let row = frame.cells.join("\t");
                            if !row.trim().is_empty() {
                                frame.rows.push(row);
                            }
                            frame.cells.clear();
                        }
                    }
                    "tbl" => {
                        if let Some(frame) = tables.pop() {
                            // Nested tables join into the parent cell.
                            let joined = frame.rows.join("\n");
                            if !joined.trim().is_empty() {
                                if let Some(outer) = tables.last_mut() {
                                    outer.cell_paras.push(joined);
                                } else {
                                    blocks.push(joined);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        buf.clear();
    }
    flush_para(&mut para, &mut tables, &mut blocks);
    Ok(blocks.join("\n").trim().to_string())
}

/// Extract a digital PDF or reject it when any page has no usable embedded
/// text. A mixed document is rejected — never partially stored — so no page
/// content disappears silently.
pub fn extract_pdf(filename: &str, bytes: &[u8]) -> Result<FileIngestResult, CoreError> {
    let pages = pdf_extract::extract_text_from_mem_by_pages(bytes)
        .map_err(|_| CoreError::validation("could not read this PDF file"))?;
    let page_count = pages.len();
    if page_count == 0 {
        return Err(CoreError::validation("this PDF has no readable text"));
    }
    // Per-page image presence, aligned with `pages` by document order.
    // Computed BEFORE readability: image-free pages with any usable text
    // count as digital (short-but-valid rule), image pages need the
    // MIN_PDF_PAGE_CHARS noise floor.
    let mut images = pdf_page_images(bytes).unwrap_or_else(|| vec![false; page_count]);
    images.resize(page_count, false);
    let readable: Vec<bool> = pages
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let count = t.chars().filter(|c| !c.is_whitespace()).count();
            // Substantive text always wins (digital page with figures).
            // Otherwise an image-free page with ANY usable text is digital,
            // so short-but-valid documents import; image pages need the
            // noise floor to not mistake scan artifacts for text.
            count >= MIN_PDF_PAGE_CHARS || (count > 0 && !images[i])
        })
        .collect();
    let scanned: Vec<usize> = (0..page_count)
        .filter(|&i| !readable[i] && images[i])
        .map(|i| i + 1)
        .collect();
    let unreadable: Vec<usize> = (0..page_count)
        .filter(|&i| !readable[i] && !images[i])
        .map(|i| i + 1)
        .collect();
    let readable_count = readable.iter().filter(|r| **r).count();
    if readable_count == page_count {
        let text: Vec<&str> = pages.iter().map(String::as_str).collect();
        return Ok(FileIngestResult::Digital {
            text: text.join("\n\n"),
            pages: page_count,
            source: SourceType::PdfDigital,
            filename: filename.to_string(),
        });
    }
    if readable_count == 0 && !scanned.is_empty() {
        return Err(CoreError::validation(format!(
            "this PDF contains scanned page{} {}; scanned/image-only PDFs are unsupported. Upload a digital PDF, DOCX, TXT, or Markdown file. Nothing was saved.",
            if scanned.len() == 1 { "" } else { "s" },
            page_list(&scanned),
        )));
    }
    if readable_count == 0 {
        return Err(CoreError::validation("this PDF has no readable text"));
    }
    Err(CoreError::validation(format!(
        "this PDF is incomplete: scanned page{} {} are unsupported{}. Nothing was saved.",
        if scanned.len() == 1 { "" } else { "s" },
        page_list(&scanned),
        if unreadable.is_empty() {
            String::new()
        } else {
            format!(
                "; unreadable page{} {}",
                if unreadable.len() == 1 { "" } else { "s" },
                page_list(&unreadable),
            )
        },
    )))
}

/// Format 1-based page numbers for messages: `[2]` → "2", `[2, 3]` → "2–3".
fn page_list(pages: &[usize]) -> String {
    match pages {
        [] => "none".to_string(),
        [single] => single.to_string(),
        _ => format!("{}–{}", pages[0], pages[pages.len() - 1]),
    }
}

/// Per-page image presence in document order (`None` when the PDF cannot
/// be parsed at all; callers fall back to treating pages as imageless).
fn pdf_page_images(bytes: &[u8]) -> Option<Vec<bool>> {
    let doc = lopdf::Document::load_mem(bytes).ok()?;
    Some(
        doc.get_pages()
            .values()
            .map(|page_id| page_references_image(&doc, *page_id))
            .collect(),
    )
}

fn page_references_image(doc: &lopdf::Document, page_id: lopdf::ObjectId) -> bool {
    use lopdf::Object;
    fn as_dict<'a>(doc: &'a lopdf::Document, obj: &'a Object) -> Option<&'a lopdf::Dictionary> {
        resolve_dict(doc, obj, 8)
    }

    /// Resolve dictionaries through references (depth-capped against cycles).
    fn resolve_dict<'a>(
        doc: &'a lopdf::Document,
        obj: &'a Object,
        depth: u8,
    ) -> Option<&'a lopdf::Dictionary> {
        if depth == 0 {
            return None;
        }
        match obj {
            Object::Dictionary(d) => Some(d),
            // Image and content objects are streams carrying a dict.
            Object::Stream(s) => Some(&s.dict),
            Object::Reference(id) => doc
                .objects
                .get(id)
                .and_then(|o| resolve_dict(doc, o, depth - 1)),
            _ => None,
        }
    }
    // Walk the page itself plus inheritable `Resources` on page-tree
    // ancestors (PDF 1.7 §3.6.2): scanned PDFs often store the image
    // XObject on the parent `/Pages` node. Cycle/depth-guarded.
    let mut current: Option<lopdf::ObjectId> = Some(page_id);
    let mut visited = std::collections::HashSet::new();
    let mut depth = 0u8;
    while let Some(id) = current {
        if depth >= 16 || !visited.insert(id) {
            break;
        }
        depth += 1;
        let dict = match doc.objects.get(&id).and_then(|o| resolve_dict(doc, o, 8)) {
            Some(d) => d,
            None => break,
        };
        if let Some(resources) = dict.get(b"Resources").ok().and_then(|o| as_dict(doc, o)) {
            if let Some(xobjects) = resources.get(b"XObject").ok().and_then(|o| as_dict(doc, o)) {
                let found = xobjects.iter().any(|(_, obj)| {
                    as_dict(doc, obj)
                        .and_then(|d| d.get(b"Subtype").ok())
                        .is_some_and(|s| s.as_name().ok() == Some(b"Image".as_slice()))
                });
                if found {
                    return true;
                }
            }
        }
        current = dict.get(b"Parent").ok().and_then(|o| match o {
            Object::Reference(parent) => Some(*parent),
            _ => None,
        });
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_detection_is_case_insensitive() {
        assert_eq!(detect_kind("Essay.PDF").expect("pdf"), FileKind::Pdf);
        assert_eq!(detect_kind("notes.Md").expect("md"), FileKind::Markdown);
        assert_eq!(detect_kind("a.DOCX").expect("docx"), FileKind::Docx);
        assert_eq!(detect_kind("a.txt").expect("txt"), FileKind::Txt);
        assert!(detect_kind("photo.png").is_err());
        assert!(detect_kind("no-extension").is_err());
    }

    #[test]
    fn rejects_missing_filename() {
        assert!(extract_file("   ", b"hi").is_err());
    }
}

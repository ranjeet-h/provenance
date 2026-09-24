//! Digital file import integration tests. Fixture binaries are built in-test
//! (no blobs in git), so every supported format and rejection rule is
//! reproducible in a clean checkout.

use provenance_core::domain::{NewSession, NewStudent, SourceType};
use provenance_core::error::CoreError;
use provenance_core::import::{extract_docx, extract_file, FileIngestResult};
use provenance_core::service;
use provenance_core::storage;
use std::io::Write;

fn block_on<F, T>(future: F) -> T
where
    F: std::future::Future<Output = T>,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

fn fresh_db() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.db");
    let pool = block_on(storage::open(&path)).expect("open+migrate");
    (dir, pool)
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn para_runs(runs: &[&str]) -> String {
    let inner: String = runs
        .iter()
        .map(|r| format!("<w:r><w:t>{}</w:t></w:r>", xml_escape(r)))
        .collect();
    format!("<w:p>{inner}</w:p>")
}

fn para(text: &str) -> String {
    para_runs(&[text])
}

fn heading(text: &str) -> String {
    format!(
        "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t>{}</w:t></w:r></w:p>",
        xml_escape(text)
    )
}

fn table(rows: &[&[&str]]) -> String {
    let body: String = rows
        .iter()
        .map(|row| {
            let cells: String = row
                .iter()
                .map(|c| {
                    format!(
                        "<w:tc><w:p><w:r><w:t>{}</w:t></w:r></w:p></w:tc>",
                        xml_escape(c)
                    )
                })
                .collect();
            format!("<w:tr>{cells}</w:tr>")
        })
        .collect();
    format!("<w:tbl>{body}</w:tbl>")
}

/// Build a structurally faithful minimal DOCX package in memory.
fn docx_bytes(body: &str) -> Vec<u8> {
    docx_bytes_with(body, zip::CompressionMethod::Stored)
}

/// Same package written with real DEFLATE compression, for expansion tests.
fn docx_bytes_compressed(body: &str) -> Vec<u8> {
    docx_bytes_with(body, zip::CompressionMethod::Deflated)
}

fn docx_bytes_with(body: &str, method: zip::CompressionMethod) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut buf);
    let options = zip::write::SimpleFileOptions::default().compression_method(method);
    for (name, content) in [
        (
            "[Content_Types].xml",
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>",
        ),
        (
            "_rels/.rels",
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>",
        ),
    ] {
        zip.start_file(name, options).expect("zip entry");
        zip.write_all(content.as_bytes()).expect("zip write");
    }
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}<w:sectPr/></w:body></w:document>"
    );
    zip.start_file("word/document.xml", options)
        .expect("zip entry");
    zip.write_all(document.as_bytes()).expect("zip write");
    zip.finish().expect("zip finish");
    buf.into_inner()
}

// ---------- minimal PDF builder (valid xref, uncompressed) ----------

fn pdf_escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

struct PdfBuilder {
    objects: Vec<Vec<u8>>,
}

impl PdfBuilder {
    fn new() -> Self {
        Self {
            objects: Vec::new(),
        }
    }

    fn push(&mut self, body: Vec<u8>) -> (u32, u16) {
        self.objects.push(body);
        (self.objects.len() as u32, 0)
    }

    fn stream(data: &[u8]) -> Vec<u8> {
        let mut out = format!("<< /Length {} >>\nstream\n", data.len()).into_bytes();
        out.extend_from_slice(data);
        out.extend_from_slice(b"\nendstream");
        out
    }

    fn finish(self, root: (u32, u16)) -> Vec<u8> {
        let mut out = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in self.objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            out.extend_from_slice(body);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref_at = out.len();
        out.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", self.objects.len() + 1).as_bytes(),
        );
        for off in offsets {
            out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root {} 0 R >>\nstartxref\n{xref_at}\n%%EOF",
                self.objects.len() + 1,
                root.0
            )
            .as_bytes(),
        );
        out
    }
}

fn text_page_content(lines: &[&str]) -> Vec<u8> {
    let mut ops = vec!["BT /F1 12 Tf 72 720 Td 14 TL".to_string()];
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            ops.push("T*".to_string());
        }
        ops.push(format!("({}) Tj", pdf_escape(line)));
    }
    ops.push("ET".to_string());
    ops.join("\n").into_bytes()
}

/// One digital page set; returns PDF bytes with a shared Helvetica font.
/// Object order: font, then (content, page)*, then pages, then catalog.
fn digital_pdf(pages: &[&[&str]]) -> Vec<u8> {
    let mut b = PdfBuilder::new();
    let font = b.push(
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .as_bytes()
            .to_vec(),
    );
    let mut kids = Vec::new();
    for lines in pages {
        let content = b.push(PdfBuilder::stream(&text_page_content(lines)));
        let page = b.push(
            format!(
                "<< /Type /Page /Parent 0 0 R /MediaBox [0 0 612 792] /Contents {} 0 R /Resources << /Font << /F1 {} 0 R >> >> >>",
                content.0, font.0
            )
            .into_bytes(),
        );
        kids.push(page);
    }
    // Fix Parent references now that the pages object number is known.
    let pages_at = (b.objects.len() + 1) as u32;
    for &kid in &kids {
        let idx = (kid.0 - 1) as usize;
        let fixed = String::from_utf8_lossy(&b.objects[idx])
            .replace("/Parent 0 0 R", &format!("/Parent {pages_at} 0 R"));
        b.objects[idx] = fixed.into_bytes();
    }
    let kids_str: String = kids
        .iter()
        .map(|(n, _)| format!("{n} 0 R"))
        .collect::<Vec<_>>()
        .join(" ");
    let pages_id = b.push(
        format!(
            "<< /Type /Pages /Kids [{kids_str}] /Count {} >>",
            kids.len()
        )
        .into_bytes(),
    );
    assert_eq!(pages_id.0, pages_at);
    let root = b.push(format!("<< /Type /Catalog /Pages {} 0 R >>", pages_id.0).into_bytes());
    b.finish(root)
}

/// Scanned pages: image XObjects, no text operators at all.
fn scanned_pdf(pages: usize) -> Vec<u8> {
    let mut b = PdfBuilder::new();
    let font = b.push(
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .as_bytes()
            .to_vec(),
    );
    let image_data = vec![0u8; 64];
    let mut img = format!(
        "<< /Type /XObject /Subtype /Image /Width 8 /Height 8 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length {} >>\nstream\n",
        image_data.len()
    )
    .into_bytes();
    img.extend_from_slice(&image_data);
    img.extend_from_slice(b"\nendstream");
    let image = b.push(img);
    let mut kids = Vec::new();
    for _ in 0..pages {
        let content = b.push(PdfBuilder::stream(b"q 8 0 0 8 0 0 cm /Im1 Do Q"));
        let page = b.push(
            format!(
                "<< /Type /Page /Parent 0 0 R /MediaBox [0 0 612 792] /Contents {} 0 R /Resources << /Font << /F1 {} 0 R >> /XObject << /Im1 {} 0 R >> >> >>",
                content.0, font.0, image.0
            )
            .into_bytes(),
        );
        kids.push(page);
    }
    let pages_at = (b.objects.len() + 1) as u32;
    for &kid in &kids {
        let idx = (kid.0 - 1) as usize;
        let fixed = String::from_utf8_lossy(&b.objects[idx])
            .replace("/Parent 0 0 R", &format!("/Parent {pages_at} 0 R"));
        b.objects[idx] = fixed.into_bytes();
    }
    let kids_str: String = kids
        .iter()
        .map(|(n, _)| format!("{n} 0 R"))
        .collect::<Vec<_>>()
        .join(" ");
    let pages_id = b.push(
        format!(
            "<< /Type /Pages /Kids [{kids_str}] /Count {} >>",
            kids.len()
        )
        .into_bytes(),
    );
    assert_eq!(pages_id.0, pages_at);
    let root = b.push(format!("<< /Type /Catalog /Pages {} 0 R >>", pages_id.0).into_bytes());
    b.finish(root)
}

/// One test page: content stream plus optional own `/Resources` dict body.
/// `None` resources means the page inherits from the `/Pages` node.
struct TestPage {
    content: Vec<u8>,
    resources: Option<String>,
}

/// Assemble pages with per-page resources plus optional page-tree resources
/// (exercises `/Resources` inheritance from the `/Pages` ancestor).
fn assemble_pdf(pages: &[TestPage], tree_resources: Option<&str>) -> Vec<u8> {
    let mut b = PdfBuilder::new();
    let font = b.push(
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .as_bytes()
            .to_vec(),
    );
    let image_data = vec![0u8; 64];
    let mut img = format!(
        "<< /Type /XObject /Subtype /Image /Width 8 /Height 8 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length {} >>\nstream\n",
        image_data.len()
    )
    .into_bytes();
    img.extend_from_slice(&image_data);
    img.extend_from_slice(b"\nendstream");
    let image = b.push(img);
    let font_ref = font.0.to_string();
    let image_ref = image.0.to_string();
    let expand = |template: &str| {
        template
            .replace("{FONT}", &font_ref)
            .replace("{IMAGE}", &image_ref)
    };
    let mut kids = Vec::new();
    for page in pages {
        let content = b.push(PdfBuilder::stream(&page.content));
        let res = match &page.resources {
            Some(r) => format!(" /Resources {}", expand(r)),
            None => String::new(),
        };
        let page_obj = b.push(
            format!(
                "<< /Type /Page /Parent 0 0 R /MediaBox [0 0 612 792] /Contents {} 0 R{} >>",
                content.0, res
            )
            .into_bytes(),
        );
        kids.push(page_obj);
    }
    let pages_at = (b.objects.len() + 1) as u32;
    for &kid in &kids {
        let idx = (kid.0 - 1) as usize;
        let fixed = String::from_utf8_lossy(&b.objects[idx])
            .replace("/Parent 0 0 R", &format!("/Parent {pages_at} 0 R"));
        b.objects[idx] = fixed.into_bytes();
    }
    let kids_str: String = kids
        .iter()
        .map(|(n, _)| format!("{n} 0 R"))
        .collect::<Vec<_>>()
        .join(" ");
    let tree_res = tree_resources
        .map(|r| format!(" /Resources {}", expand(r)))
        .unwrap_or_default();
    let pages_id = b.push(
        format!(
            "<< /Type /Pages /Kids [{kids_str}] /Count {}{} >>",
            kids.len(),
            tree_res
        )
        .into_bytes(),
    );
    assert_eq!(pages_id.0, pages_at);
    let root = b.push(format!("<< /Type /Catalog /Pages {} 0 R >>", pages_id.0).into_bytes());
    b.finish(root)
}

/// Page 1 digital text, page 2 scanned image: nothing may be stored.
fn mixed_digital_scanned_pdf() -> Vec<u8> {
    assemble_pdf(
        &[
            TestPage {
                content: text_page_content(&[
                    "Digital assignment prose with enough words to be readable on page one.",
                ]),
                resources: Some("<</Font <</F1 {FONT} 0 R>> >>".to_string()),
            },
            TestPage {
                content: b"q 8 0 0 8 0 0 cm /Im1 Do Q".to_vec(),
                resources: Some(
                    "<</Font <</F1 {FONT} 0 R>> /XObject <</Im1 {IMAGE} 0 R>> >>".to_string(),
                ),
            },
        ],
        None,
    )
}

/// Scanned pages whose image resources live on the parent `/Pages` node.
/// Pages carry no `/Resources` of their own (inheritance probe).
fn inherited_resources_scanned_pdf() -> Vec<u8> {
    assemble_pdf(
        &[TestPage {
            content: b"q 8 0 0 8 0 0 cm /Im1 Do Q".to_vec(),
            resources: None,
        }],
        Some("<</Font <</F1 {FONT} 0 R>> /XObject <</Im1 {IMAGE} 0 R>> >>"),
    )
}

// ---------- DOCX tests ----------

#[test]
fn plain_text_and_markdown_examples_preserve_user_text() {
    let txt = extract_file("lecture.txt", b"First line\nSecond line\n").expect("txt");
    assert!(matches!(
        txt,
        FileIngestResult::Digital {
            text,
            pages: 1,
            source: SourceType::TxtFile,
            filename,
        } if text == "First line\nSecond line\n" && filename == "lecture.txt"
    ));

    let markdown = extract_file(
        "assignment.MARKDOWN",
        "# Results\n\nUnicode: café and 📚.\n".as_bytes(),
    )
    .expect("markdown");
    assert!(matches!(
        markdown,
        FileIngestResult::Digital {
            text,
            pages: 1,
            source: SourceType::MarkdownFile,
            filename,
        } if text.starts_with("# Results") && text.contains("café") && filename == "assignment.MARKDOWN"
    ));
}

#[test]
fn docx_single_and_multiple_paragraphs_in_order() {
    let bytes = docx_bytes(&format!(
        "{}{}",
        para("First paragraph here."),
        para("Second one follows.")
    ));
    assert_eq!(
        extract_docx(&bytes).expect("read"),
        "First paragraph here.\nSecond one follows."
    );
}

#[test]
fn docx_split_runs_rejoin_without_spaces() {
    // Word splits words across runs; joining must not inject spaces.
    let bytes = docx_bytes(&para_runs(&["Photosyn", "thesis converts ", "light"]));
    assert_eq!(
        extract_docx(&bytes).expect("read"),
        "Photosynthesis converts light"
    );
}

#[test]
fn docx_headings_tables_and_unicode() {
    let bytes = docx_bytes(&format!(
        "{}{}{}",
        heading("Results"),
        para("Café naïve “quotes” and R&D work."),
        table(&[&["Name", "Score"], &["Amit", "42"]])
    ));
    assert_eq!(
        extract_docx(&bytes).expect("read"),
        "Results\nCafé naïve “quotes” and R&D work.\nName\tScore\nAmit\t42"
    );
}

#[test]
fn docx_skips_fields_deletions_and_empty_paragraphs() {
    let bytes = docx_bytes(&format!(
        "{}{}{}",
        "<w:p><w:r><w:instrText>PAGE</w:instrText></w:r><w:r><w:t>Visible</w:t></w:r></w:p>",
        "<w:p><w:r><w:delText>Gone</w:delText></w:r><w:r><w:t>Kept</w:t></w:r></w:p>",
        para(""),
    ));
    assert_eq!(extract_docx(&bytes).expect("read"), "Visible\nKept");
}

#[test]
fn docx_empty_document_extracts_nothing() {
    let bytes = docx_bytes("");
    assert_eq!(extract_docx(&bytes).expect("read"), "");
}

#[test]
fn docx_empty_self_closing_skips_do_not_swallow_later_text() {
    // Self-closing <w:del/>, <w:delText/>, <w:instrText/> have no matching
    // End tag: they must not change the skip depth, or every later
    // paragraph would be silently omitted.
    let bytes = docx_bytes(&format!(
        "{}{}{}{}",
        "<w:p><w:r><w:del/></w:r><w:r><w:t>After empty del.</w:t></w:r></w:p>",
        "<w:p><w:r><w:instrText/></w:r><w:r><w:t>After empty field.</w:t></w:r></w:p>",
        "<w:p><w:r><w:delText/></w:r><w:r><w:t>After empty delText.</w:t></w:r></w:p>",
        para("Final paragraph intact."),
    ));
    assert_eq!(
        extract_docx(&bytes).expect("read"),
        "After empty del.\nAfter empty field.\nAfter empty delText.\nFinal paragraph intact."
    );
}

#[test]
fn docx_paired_deletions_still_skip_only_their_content() {
    let bytes = docx_bytes(&format!(
        "{}{}",
        "<w:p><w:r><w:del><w:r><w:delText>Gone</w:delText></w:r></w:del></w:r><w:r><w:t>Kept</w:t></w:r></w:p>",
        para("Next paragraph kept."),
    ));
    assert_eq!(
        extract_docx(&bytes).expect("read"),
        "Kept\nNext paragraph kept."
    );
}

#[test]
fn docx_corrupt_inputs_rejected() {
    assert!(extract_docx(b"not a zip at all").is_err());
    // Valid zip without a document part.
    let mut buf = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut buf);
    zip.start_file("other.txt", zip::write::SimpleFileOptions::default())
        .expect("entry");
    zip.write_all(b"hi").expect("write");
    zip.finish().expect("finish");
    assert!(extract_docx(&buf.into_inner()).is_err());
}

// ---------- PDF tests ----------

#[test]
fn pdf_digital_single_page_extracts() {
    let bytes = digital_pdf(&[&["Hello PDF world, this is a readable assignment page."]]);
    match extract_file("essay.pdf", &bytes).expect("extract") {
        FileIngestResult::Digital {
            text,
            pages,
            source,
            ..
        } => {
            assert!(text.contains("Hello PDF world"));
            assert_eq!(pages, 1);
            assert_eq!(source, SourceType::PdfDigital);
        }
    }
}

#[test]
fn pdf_digital_multi_page_joins_pages() {
    let bytes = digital_pdf(&[
        &["First page content with enough words to count as readable text here."],
        &["Second page content with enough words to count as readable text here."],
    ]);
    match extract_file("two.pdf", &bytes).expect("extract") {
        FileIngestResult::Digital { text, pages, .. } => {
            assert_eq!(pages, 2);
            assert!(text.contains("First page") && text.contains("Second page"));
        }
    }
}

#[test]
fn pdf_short_but_valid_digital_page_imports() {
    // Product rule: an image-free page with ANY usable text is digital, even
    // below MIN_PDF_PAGE_CHARS. A one-line memo ("Hi") must import, not be
    // silently treated as unreadable.
    let bytes = digital_pdf(&[&["Hi"]]);
    match extract_file("memo.pdf", &bytes).expect("extract") {
        FileIngestResult::Digital { text, pages, .. } => {
            assert_eq!(pages, 1);
            assert!(text.contains("Hi"), "short text kept: {text:?}");
        }
    }
}

#[test]
fn pdf_scanned_pages_are_rejected_as_unsupported() {
    let bytes = scanned_pdf(2);
    let err = extract_file("scan.pdf", &bytes).expect_err("scanned PDF unsupported");
    let CoreError::Validation(message) = err else {
        panic!("expected validation error");
    };
    assert!(message.contains("scanned"), "actionable message: {message}");
    assert!(
        message.contains("digital PDF"),
        "supported formats: {message}"
    );
}

#[test]
fn pdf_inherited_resources_still_detected_as_scanned() {
    // The image XObject lives on the parent /Pages node; the page itself
    // carries no /Resources. It must still be rejected, not treated as an
    // empty digital PDF.
    let bytes = inherited_resources_scanned_pdf();
    let err = extract_file("inherited.pdf", &bytes).expect_err("scanned PDF unsupported");
    assert!(
        matches!(err, CoreError::Validation(ref message) if message.contains("scanned")),
        "expected scanned validation error, got {err:?}"
    );
}

#[test]
fn pdf_mixed_digital_and_scanned_rejected_without_loss() {
    // Page 1 readable, page 2 scanned: reject as incomplete, name page 2,
    // and store nothing.
    let bytes = mixed_digital_scanned_pdf();
    let err = extract_file("mixed.pdf", &bytes).expect_err("mixed must be rejected");
    let msg = match err {
        CoreError::Validation(m) => m,
        other => panic!("expected validation error, got {other:?}"),
    };
    assert!(msg.contains('2'), "names the scanned page: {msg}");
    assert!(
        msg.to_lowercase().contains("nothing was saved"),
        "states nothing stored: {msg}"
    );
}

#[test]
fn image_uploads_are_rejected_before_any_parser_runs() {
    let err = extract_file("notebook.jpg", b"JPEG bytes are not text").expect_err("image");
    assert!(
        matches!(err, CoreError::Validation(ref message) if message.contains("unsupported file type")),
        "expected unsupported image error, got {err:?}"
    );
}

#[test]
fn service_rejects_mixed_pdf_and_stores_nothing() {
    let (_dir, pool) = fresh_db();
    let session = new_session(&pool);
    let student = add_student(&pool, &session, "A");
    let bytes = mixed_digital_scanned_pdf();
    let err = block_on(service::save_file_submission(
        &pool,
        &student,
        Some("mixed.pdf".to_string()),
        &bytes,
    ))
    .expect_err("mixed rejected");
    assert!(matches!(err, CoreError::Validation(_)));
    assert!(
        matches!(
            block_on(service::get_submission(&pool, &student)),
            Err(CoreError::NotFound(_))
        ),
        "no partial submission may remain"
    );
}

#[test]
fn pdf_empty_and_corrupt_rejected() {
    let empty = digital_pdf(&[&[]]);
    let err = extract_file("empty.pdf", &empty).expect_err("empty rejected");
    assert!(matches!(err, CoreError::Validation(_)));
    let err = extract_file("junk.pdf", b"definitely not a pdf file").expect_err("corrupt rejected");
    assert!(matches!(err, CoreError::Validation(_)));
}

// ---------- service tests ----------

fn add_student(pool: &sqlx::SqlitePool, session: &str, name: &str) -> String {
    block_on(service::add_student(
        pool,
        session,
        NewStudent {
            display_name: name.to_string(),
        },
    ))
    .expect("student")
    .id
}

fn new_session(pool: &sqlx::SqlitePool) -> String {
    block_on(service::create_session(
        pool,
        NewSession {
            name: "Files".to_string(),
            subject: None,
        },
    ))
    .expect("session")
    .id
}

#[test]
fn service_saves_docx_with_filename_and_source() {
    let (_dir, pool) = fresh_db();
    let session = new_session(&pool);
    let student = add_student(&pool, &session, "A");
    let bytes = docx_bytes(&para("Docx assignment text with enough words to be valid."));
    match block_on(service::save_file_submission(
        &pool,
        &student,
        Some("essay.docx".to_string()),
        &bytes,
    ))
    .expect("save")
    {
        FileIngestResult::Digital {
            source, filename, ..
        } => {
            assert_eq!(source, SourceType::DocxFile);
            assert_eq!(filename, "essay.docx");
        }
    }
    let stored = block_on(service::get_submission(&pool, &student)).expect("stored");
    assert!(stored.original_text.contains("Docx assignment text"));
    assert_eq!(stored.source_type, SourceType::DocxFile);
}

#[test]
fn service_rejects_unsupported_and_unknown_files() {
    let (_dir, pool) = fresh_db();
    let session = new_session(&pool);
    let student = add_student(&pool, &session, "A");
    let err = block_on(service::save_file_submission(
        &pool,
        &student,
        Some("photo.png".to_string()),
        b"fake",
    ))
    .expect_err("unsupported");
    assert!(matches!(err, CoreError::Validation(_)));
    assert!(matches!(
        block_on(service::save_file_submission(
            &pool,
            "missing",
            Some("a.txt".to_string()),
            b"hi"
        )),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn service_saves_digital_pdf_and_rejects_scanned() {
    let (_dir, pool) = fresh_db();
    let session = new_session(&pool);
    let digital_student = add_student(&pool, &session, "D");
    let bytes = digital_pdf(&[&["Digital assignment prose with enough words to be readable."]]);
    match block_on(service::save_file_submission(
        &pool,
        &digital_student,
        Some("assignment.pdf".to_string()),
        &bytes,
    ))
    .expect("save pdf")
    {
        FileIngestResult::Digital {
            source, filename, ..
        } => {
            assert_eq!(source, SourceType::PdfDigital);
            assert_eq!(filename, "assignment.pdf");
        }
    }
    let stored = block_on(service::get_submission(&pool, &digital_student)).expect("stored");
    assert_eq!(stored.source_type, SourceType::PdfDigital);

    let scanned_student = add_student(&pool, &session, "S");
    let scanned = scanned_pdf(3);
    let err = block_on(service::save_file_submission(
        &pool,
        &scanned_student,
        Some("scan.pdf".to_string()),
        &scanned,
    ))
    .expect_err("scanned input is unsupported");
    assert!(matches!(
        err,
        CoreError::Validation(message) if message.contains("scanned")
    ));
    // Nothing was stored for the scanned upload.
    assert!(matches!(
        block_on(service::get_submission(&pool, &scanned_student)),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn oversized_raw_uploads_rejected_before_parsing() {
    use provenance_core::ingest::MAX_UPLOAD_BYTES;
    let big = vec![0u8; MAX_UPLOAD_BYTES + 1];
    for name in ["huge.docx", "huge.pdf", "huge.txt"] {
        let err = extract_file(name, &big).expect_err("oversize rejected");
        let msg = match err {
            CoreError::Validation(m) => m,
            other => panic!("expected validation, got {other:?}"),
        };
        assert!(msg.contains("too large"), "size message: {msg}");
    }
}

#[test]
fn zip_bomb_expansion_rejected_before_xml_parsing() {
    use provenance_core::ingest::MAX_DOCX_XML_BYTES;
    // Small on disk, huge when expanded: paragraphs of repeated text.
    let big_para = format!("<w:p><w:r><w:t>{}</w:t></w:r></w:p>", "x".repeat(1024));
    let repeats = MAX_DOCX_XML_BYTES / big_para.len() + 2;
    let mut body = String::new();
    for _ in 0..repeats {
        body.push_str(&big_para);
    }
    let bytes = docx_bytes_compressed(&body);
    assert!(
        bytes.len() < MAX_DOCX_XML_BYTES,
        "fixture must compress small"
    );
    let err = extract_file("bomb.docx", &bytes).expect_err("expansion rejected");
    let msg = match err {
        CoreError::Validation(m) => m,
        other => panic!("expected validation, got {other:?}"),
    };
    assert!(msg.contains("expands"), "expansion message: {msg}");
}

#[test]
fn service_rejects_oversized_docx_and_pdf_uploads() {
    use provenance_core::ingest::MAX_UPLOAD_BYTES;
    let (_dir, pool) = fresh_db();
    let session = new_session(&pool);
    let student = add_student(&pool, &session, "A");
    let big = vec![0u8; MAX_UPLOAD_BYTES + 1];
    for name in ["big.docx", "big.pdf"] {
        let err = block_on(service::save_file_submission(
            &pool,
            &student,
            Some(name.to_string()),
            &big,
        ))
        .expect_err("oversize rejected");
        assert!(matches!(err, CoreError::Validation(_)), "{name}: {err:?}");
    }
    assert!(
        matches!(
            block_on(service::get_submission(&pool, &student)),
            Err(CoreError::NotFound(_))
        ),
        "rejected uploads store nothing"
    );
}

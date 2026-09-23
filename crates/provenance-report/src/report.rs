//! Span-validated structured student reports and local PDF rendering.

use std::collections::{BTreeMap, HashMap, HashSet};

use printpdf::{
    Base64OrRaw, Color, GeneratePdfOptions, Op, PaintMode, PdfDocument, PdfParseOptions,
    PdfSaveOptions,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const REPORT_SCHEMA_VERSION: u32 = 1;
pub const REVIEWER_DISCLAIMER: &str = "This report identifies matching or reused content in the comparison corpus. Similarity alone does not prove plagiarism. The final decision about whether plagiarism occurred must be made by the teacher or qualified reviewer after reviewing the highlighted evidence and assignment context.";
const REPORT_FONT: &[u8] = include_bytes!("../assets/fonts/NotoSans-Regular.ttf");

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReportError {
    #[error("invalid student report: {0}")]
    Invalid(String),
    #[error("PDF report generation failed: {0}")]
    Pdf(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceCorpus {
    Current,
    Historical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    Exact,
    Modified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportMode {
    Teacher,
    SelfCheck,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportSource {
    pub id: String,
    pub label: String,
    pub corpus: SourceCorpus,
    pub text: String,
    pub content_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparedSubmission {
    pub id: String,
    pub display_name: String,
    pub source_id: String,
    pub content_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparedLibrary {
    pub id: String,
    pub name: String,
    pub source_session_name: String,
    pub source_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineVersions {
    pub fingerprint: u32,
    pub normalization: u32,
    pub common_text: u32,
    pub modified: u32,
}

/// Evidence ranges are byte offsets into the original UTF-8 source texts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportEvidence {
    pub id: String,
    pub corpus: SourceCorpus,
    pub historical_library_id: Option<String>,
    pub kind: MatchKind,
    pub student_source_id: String,
    pub comparison_source_id: String,
    pub student_start: usize,
    pub student_end: usize,
    pub comparison_start: usize,
    pub comparison_end: usize,
    pub tokens: usize,
    pub common_text: bool,
    pub coverage: Option<f64>,
    pub exact_coverage: Option<f64>,
    pub modified_coverage: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportExclusionInput {
    pub source_id: String,
    pub start: usize,
    pub end: usize,
    pub reason: String,
    pub tokens: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReportInput {
    pub session_id: String,
    pub session_name: String,
    pub subject: Option<String>,
    pub target_student_id: String,
    pub target_student_name: String,
    pub target_source_id: String,
    pub generated_at: String,
    pub report_mode: ReportMode,
    pub anonymize: bool,
    /// Current-session peers only; historical sources are kept separately.
    pub current_comparisons: Vec<ComparedSubmission>,
    pub historical_libraries: Vec<ComparedLibrary>,
    pub sources: Vec<ReportSource>,
    pub overall_coverage: Option<f64>,
    pub exact_coverage: Option<f64>,
    pub modified_coverage: Option<f64>,
    pub matches: Vec<ReportEvidence>,
    pub exclusions: Vec<ReportExclusionInput>,
    pub engine_versions: EngineVersions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportMatch {
    pub id: String,
    pub corpus: SourceCorpus,
    pub historical_library_id: Option<String>,
    pub kind: MatchKind,
    pub student_source_id: String,
    pub comparison_source_id: String,
    pub student_start: usize,
    pub student_end: usize,
    pub comparison_start: usize,
    pub comparison_end: usize,
    pub student_excerpt: String,
    pub comparison_excerpt: String,
    pub tokens: usize,
    pub common_text: bool,
    pub coverage: Option<f64>,
    pub exact_coverage: Option<f64>,
    pub modified_coverage: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportExclusion {
    pub source_id: String,
    pub start: usize,
    pub end: usize,
    pub excerpt: String,
    pub reason: String,
    pub tokens: usize,
}

/// All fields in this payload, including the reviewer disclaimer, are covered
/// by `ReportIntegrity.payload_sha256` and by the Phase 20 signature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportPayload {
    pub schema_version: u32,
    pub report_mode: ReportMode,
    pub anonymized: bool,
    pub session_id: String,
    pub session_name: String,
    pub subject: Option<String>,
    pub target_student_id: String,
    pub target_student_name: String,
    pub target_source_id: String,
    pub generated_at: String,
    pub current_comparisons: Vec<ComparedSubmission>,
    pub historical_libraries: Vec<ComparedLibrary>,
    pub current_submissions_compared: usize,
    pub historical_submissions_compared: usize,
    pub overall_coverage: Option<f64>,
    pub exact_coverage: Option<f64>,
    pub modified_coverage: Option<f64>,
    pub top_current_matches: Vec<String>,
    pub top_historical_matches: Vec<String>,
    pub matches: Vec<ReportMatch>,
    pub exclusions: Vec<ReportExclusion>,
    pub sources: Vec<ReportSource>,
    pub engine_versions: EngineVersions,
    pub reviewer_disclaimer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportIntegrity {
    pub algorithm: String,
    pub payload_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StudentReport {
    pub payload: ReportPayload,
    pub integrity: ReportIntegrity,
}

impl StudentReport {
    pub fn payload_sha256(&self) -> Result<String, ReportError> {
        let bytes = serde_json::to_vec(&self.payload)
            .map_err(|_| ReportError::Invalid("report payload could not be serialized".into()))?;
        Ok(sha256_hex(&bytes))
    }

    #[must_use]
    pub fn verify_integrity(&self) -> bool {
        self.integrity.algorithm == "SHA-256"
            && self
                .payload_sha256()
                .is_ok_and(|actual| actual == self.integrity.payload_sha256)
    }
}

pub fn build_report(mut input: ReportInput) -> Result<StudentReport, ReportError> {
    validate_input(&input)?;
    sort_input(&mut input);
    if input.anonymize {
        anonymize_input(&mut input);
    }

    let source_by_id: HashMap<&str, &ReportSource> = input
        .sources
        .iter()
        .map(|source| (source.id.as_str(), source))
        .collect();
    let mut matches = input
        .matches
        .into_iter()
        .map(|evidence| {
            let student_source = source_by_id[evidence.student_source_id.as_str()];
            let comparison_source = source_by_id[evidence.comparison_source_id.as_str()];
            ReportMatch {
                id: evidence.id,
                corpus: evidence.corpus,
                historical_library_id: evidence.historical_library_id,
                kind: evidence.kind,
                student_source_id: evidence.student_source_id,
                comparison_source_id: evidence.comparison_source_id,
                student_excerpt: student_source.text[evidence.student_start..evidence.student_end]
                    .to_string(),
                comparison_excerpt: comparison_source.text
                    [evidence.comparison_start..evidence.comparison_end]
                    .to_string(),
                student_start: evidence.student_start,
                student_end: evidence.student_end,
                comparison_start: evidence.comparison_start,
                comparison_end: evidence.comparison_end,
                tokens: evidence.tokens,
                common_text: evidence.common_text,
                coverage: evidence.coverage,
                exact_coverage: evidence.exact_coverage,
                modified_coverage: evidence.modified_coverage,
            }
        })
        .collect::<Vec<_>>();
    matches.sort_by(|a, b| {
        a.corpus
            .cmp(&b.corpus)
            .then_with(|| b.tokens.cmp(&a.tokens))
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut top_current_matches = Vec::new();
    let mut top_historical_matches = Vec::new();
    for item in &matches {
        let target = match item.corpus {
            SourceCorpus::Current => &mut top_current_matches,
            SourceCorpus::Historical => &mut top_historical_matches,
        };
        if target.len() < 5 {
            target.push(item.id.clone());
        }
    }

    let exclusions = input
        .exclusions
        .into_iter()
        .map(|exclusion| {
            let source = source_by_id[exclusion.source_id.as_str()];
            ReportExclusion {
                source_id: exclusion.source_id,
                excerpt: source.text[exclusion.start..exclusion.end].to_string(),
                start: exclusion.start,
                end: exclusion.end,
                reason: exclusion.reason,
                tokens: exclusion.tokens,
            }
        })
        .collect();

    let payload = ReportPayload {
        schema_version: REPORT_SCHEMA_VERSION,
        report_mode: input.report_mode,
        anonymized: input.anonymize,
        session_id: input.session_id,
        session_name: input.session_name,
        subject: input.subject,
        target_student_id: input.target_student_id,
        target_student_name: input.target_student_name,
        target_source_id: input.target_source_id,
        generated_at: input.generated_at,
        current_submissions_compared: input.current_comparisons.len(),
        historical_submissions_compared: input
            .historical_libraries
            .iter()
            .try_fold(0usize, |total, library| {
                total.checked_add(library.source_count)
            })
            .ok_or_else(|| ReportError::Invalid("historical source count overflowed".into()))?,
        current_comparisons: input.current_comparisons,
        historical_libraries: input.historical_libraries,
        overall_coverage: input.overall_coverage,
        exact_coverage: input.exact_coverage,
        modified_coverage: input.modified_coverage,
        top_current_matches,
        top_historical_matches,
        matches,
        exclusions,
        sources: input.sources,
        engine_versions: input.engine_versions,
        reviewer_disclaimer: REVIEWER_DISCLAIMER.to_string(),
    };
    let bytes = serde_json::to_vec(&payload)
        .map_err(|_| ReportError::Invalid("report payload could not be serialized".into()))?;
    Ok(StudentReport {
        payload,
        integrity: ReportIntegrity {
            algorithm: "SHA-256".into(),
            payload_sha256: sha256_hex(&bytes),
        },
    })
}

fn validate_input(input: &ReportInput) -> Result<(), ReportError> {
    if input.session_id.trim().is_empty()
        || input.session_name.trim().is_empty()
        || input.target_student_id.trim().is_empty()
        || input.target_student_name.trim().is_empty()
        || input.target_source_id.trim().is_empty()
        || input.generated_at.trim().is_empty()
    {
        return Err(invalid(
            "session, student, source, and timestamp fields are required",
        ));
    }
    for (name, value) in [
        ("overall", input.overall_coverage),
        ("exact", input.exact_coverage),
        ("modified", input.modified_coverage),
    ] {
        validate_coverage(name, value)?;
    }
    let mut source_by_id = HashMap::new();
    for source in &input.sources {
        if source.id.trim().is_empty() || source.label.trim().is_empty() {
            return Err(invalid("source IDs and labels must not be empty"));
        }
        if source_by_id.insert(source.id.as_str(), source).is_some() {
            return Err(invalid("source IDs must be unique"));
        }
        if sha256_hex(source.text.as_bytes()) != source.content_sha256 {
            return Err(invalid("a source content hash does not match its text"));
        }
    }
    let target = source_by_id
        .get(input.target_source_id.as_str())
        .ok_or_else(|| invalid("the target student's source text is missing"))?;
    if target.corpus != SourceCorpus::Current {
        return Err(invalid(
            "the target student's source must be a current submission",
        ));
    }

    let mut comparison_ids = HashSet::new();
    for comparison in &input.current_comparisons {
        if comparison.id.trim().is_empty()
            || comparison.display_name.trim().is_empty()
            || !comparison_ids.insert(comparison.id.as_str())
        {
            return Err(invalid(
                "current comparison IDs and names must be non-empty and unique",
            ));
        }
        let source = source_by_id
            .get(comparison.source_id.as_str())
            .ok_or_else(|| invalid("a current comparison's source text is missing"))?;
        if source.corpus != SourceCorpus::Current
            || source.content_sha256 != comparison.content_sha256
        {
            return Err(invalid(
                "a current comparison source does not match its corpus metadata",
            ));
        }
    }
    let mut library_ids = HashSet::new();
    for library in &input.historical_libraries {
        if library.id.trim().is_empty()
            || library.name.trim().is_empty()
            || library.source_session_name.trim().is_empty()
            || library.source_count == 0
            || !library_ids.insert(library.id.as_str())
        {
            return Err(invalid(
                "historical library metadata is invalid or duplicated",
            ));
        }
    }
    let mut match_ids = HashSet::new();
    for evidence in &input.matches {
        if evidence.id.trim().is_empty() || !match_ids.insert(evidence.id.as_str()) {
            return Err(invalid("match evidence IDs must be non-empty and unique"));
        }
        if evidence.student_source_id != input.target_source_id || evidence.tokens == 0 {
            return Err(invalid(
                "match evidence must target the report student and contain tokens",
            ));
        }
        validate_coverage("match overall", evidence.coverage)?;
        validate_coverage("match exact", evidence.exact_coverage)?;
        validate_coverage("match modified", evidence.modified_coverage)?;
        let student_source = source_by_id
            .get(evidence.student_source_id.as_str())
            .ok_or_else(|| invalid("a matched student source is missing"))?;
        let comparison_source = source_by_id
            .get(evidence.comparison_source_id.as_str())
            .ok_or_else(|| invalid("a matched comparison source is missing"))?;
        if student_source.corpus != SourceCorpus::Current
            || comparison_source.corpus != evidence.corpus
            || (evidence.corpus == SourceCorpus::Current
                && !input
                    .current_comparisons
                    .iter()
                    .any(|comparison| comparison.source_id == evidence.comparison_source_id))
            || (evidence.corpus == SourceCorpus::Historical
                && input.historical_libraries.is_empty())
        {
            return Err(invalid(
                "match sources do not agree with the declared comparison corpus",
            ));
        }
        match (evidence.corpus, evidence.historical_library_id.as_deref()) {
            (SourceCorpus::Current, None) => {}
            (SourceCorpus::Current, Some(_)) => {
                return Err(invalid(
                    "current-session evidence must not name a historical library",
                ));
            }
            (SourceCorpus::Historical, Some(library_id))
                if input
                    .historical_libraries
                    .iter()
                    .any(|library| library.id == library_id) => {}
            (SourceCorpus::Historical, _) => {
                return Err(invalid(
                    "historical evidence must identify a selected comparison library",
                ));
            }
        }
        validate_span(
            &student_source.text,
            evidence.student_start,
            evidence.student_end,
            "student match",
        )?;
        validate_span(
            &comparison_source.text,
            evidence.comparison_start,
            evidence.comparison_end,
            "comparison match",
        )?;
    }
    for exclusion in &input.exclusions {
        if exclusion.reason.trim().is_empty() || exclusion.tokens == 0 {
            return Err(invalid(
                "excluded evidence requires a reason and at least one token",
            ));
        }
        let source = source_by_id
            .get(exclusion.source_id.as_str())
            .ok_or_else(|| invalid("an excluded source is missing"))?;
        validate_span(
            &source.text,
            exclusion.start,
            exclusion.end,
            "excluded evidence",
        )?;
    }
    Ok(())
}

fn validate_coverage(name: &str, value: Option<f64>) -> Result<(), ReportError> {
    if value.is_some_and(|percent| !percent.is_finite() || !(0.0..=100.0).contains(&percent)) {
        return Err(invalid(format!(
            "{name} coverage must be a finite percentage from 0 to 100"
        )));
    }
    Ok(())
}

fn validate_span(text: &str, start: usize, end: usize, name: &str) -> Result<(), ReportError> {
    if start >= end
        || end > text.len()
        || !text.is_char_boundary(start)
        || !text.is_char_boundary(end)
    {
        return Err(invalid(format!(
            "{name} span is empty, out of bounds, or splits UTF-8"
        )));
    }
    Ok(())
}

fn sort_input(input: &mut ReportInput) {
    input
        .sources
        .sort_by(|a, b| a.corpus.cmp(&b.corpus).then_with(|| a.id.cmp(&b.id)));
    input.current_comparisons.sort_by(|a, b| a.id.cmp(&b.id));
    input.historical_libraries.sort_by(|a, b| a.id.cmp(&b.id));
    input.matches.sort_by(|a, b| a.id.cmp(&b.id));
    input.exclusions.sort_by(|a, b| {
        a.source_id
            .cmp(&b.source_id)
            .then_with(|| a.start.cmp(&b.start))
            .then_with(|| a.end.cmp(&b.end))
            .then_with(|| a.reason.cmp(&b.reason))
    });
}

fn anonymize_input(input: &mut ReportInput) {
    let mut current_ids = input
        .current_comparisons
        .iter()
        .map(|comparison| comparison.id.clone())
        .collect::<Vec<_>>();
    current_ids.sort();
    let mut source_ids = HashMap::from([(input.target_source_id.clone(), "source-1".to_string())]);
    for (offset, student_id) in current_ids.iter().enumerate() {
        let student_label = format!("Student {}", offset + 2);
        if let Some(comparison) = input
            .current_comparisons
            .iter_mut()
            .find(|comparison| &comparison.id == student_id)
        {
            source_ids.insert(
                comparison.source_id.clone(),
                format!("source-{}", offset + 2),
            );
            comparison.id = student_label.clone().replace(' ', "-").to_lowercase();
            comparison.display_name = student_label;
        }
    }
    let mut source_index = source_ids.len() + 1;
    let mut historical_index = 1usize;
    for source in input
        .sources
        .iter_mut()
        .filter(|source| source.corpus == SourceCorpus::Historical)
    {
        source_ids.insert(source.id.clone(), format!("source-{source_index}"));
        source.label = format!("Historical source {historical_index}");
        source_index += 1;
        historical_index += 1;
    }
    for source in &mut input.sources {
        if source.corpus == SourceCorpus::Current {
            source.label = if source.id == input.target_source_id {
                "Student 1 submission".into()
            } else {
                let name = input
                    .current_comparisons
                    .iter()
                    .find(|comparison| comparison.source_id == source.id)
                    .map(|comparison| comparison.display_name.as_str())
                    .unwrap_or("Current submission");
                format!("{name} submission")
            };
        }
        source.id = source_ids
            .get(&source.id)
            .expect("source mappings are complete after validation")
            .clone();
    }
    for comparison in &mut input.current_comparisons {
        comparison.source_id = source_ids
            .get(&comparison.source_id)
            .expect("comparison source mapping is complete after validation")
            .clone();
    }
    for evidence in &mut input.matches {
        evidence.student_source_id = source_ids
            .get(&evidence.student_source_id)
            .expect("student source mapping is complete after validation")
            .clone();
        evidence.comparison_source_id = source_ids
            .get(&evidence.comparison_source_id)
            .expect("comparison source mapping is complete after validation")
            .clone();
        if let Some(library_id) = &mut evidence.historical_library_id {
            let index = input
                .historical_libraries
                .iter()
                .position(|library| &library.id == library_id)
                .expect("historical library mapping is complete after validation");
            *library_id = format!("library-{}", index + 1);
        }
    }
    for exclusion in &mut input.exclusions {
        exclusion.source_id = source_ids
            .get(&exclusion.source_id)
            .expect("excluded source mapping is complete after validation")
            .clone();
    }
    for (index, library) in input.historical_libraries.iter_mut().enumerate() {
        library.id = format!("library-{}", index + 1);
        library.name = format!("Historical library {}", index + 1);
        library.source_session_name = "Archived session".into();
    }
    input.session_id = "session-1".into();
    input.session_name = "Session 1".into();
    input.subject = None;
    input.target_student_id = "student-1".into();
    input.target_student_name = "Student 1".into();
    input.target_source_id = "source-1".into();
    for (index, evidence) in input.matches.iter_mut().enumerate() {
        evidence.id = format!("match-{}", index + 1);
    }
}

pub fn report_html(report: &StudentReport) -> Result<String, ReportError> {
    if !report.verify_integrity() {
        return Err(invalid(
            "report payload integrity does not match its SHA-256 digest",
        ));
    }
    let mut html = String::from(
        "<!doctype html><html><head><meta charset=\"utf-8\"><style>\
         @page { size: A4; margin: 16mm 15mm 16mm 15mm; }\
         body { font-family: 'Noto Sans'; font-size: 10pt; color: #18212b; line-height: 1.35; }\
         h1 { font-size: 19pt; color: #172b4d; margin: 0 0 4mm 0; }\
         h2 { font-size: 13pt; color: #172b4d; margin: 6mm 0 2mm 0; }\
         h3 { font-size: 11pt; color: #172b4d; margin: 4mm 0 1mm 0; }\
         p { margin: 1.5mm 0; }\
         .muted { color: #596579; font-size: 8.5pt; }\
         .evidence { border: 1px solid #cad3df; padding: 3mm; margin: 2mm 0; }\
         pre { white-space: pre-wrap; word-wrap: break-word; font-family: 'Noto Sans'; font-size: 9pt; margin: 1mm 0; }\
         .match-highlight { background-color: #ffe58f; color: #111827; }\
         .historical { background-color: #d6e8ff; }\
         .excluded { text-decoration: underline; text-decoration-color: #bd3b32; }\
         .disclaimer { border-top: 1px solid #8793a5; margin-top: 8mm; padding-top: 3mm; font-size: 8.5pt; }\
         </style></head><body>",
    );
    let payload = &report.payload;
    html.push_str("<h1>Student evidence report</h1>");
    if payload.report_mode == ReportMode::SelfCheck {
        html.push_str(
            "<p><strong>Self Check</strong> · <strong>Not Teacher Certified</strong></p>",
        );
    } else {
        html.push_str("<p>Teacher review report · local evidence record</p>");
    }
    if payload.anonymized {
        html.push_str(
            "<p class=\"muted\">Student names and source filenames are anonymized. Original submitted text is unchanged and may itself identify a person.</p>",
        );
    }
    html.push_str(&format!(
        "<p><strong>Student:</strong> {}<br><strong>Session:</strong> {}<br><strong>Subject:</strong> {}<br><strong>Generated:</strong> {}</p>",
        escape_html(&payload.target_student_name),
        escape_html(&payload.session_name),
        escape_html(payload.subject.as_deref().unwrap_or("Not specified")),
        escape_html(&payload.generated_at),
    ));
    html.push_str("<h2>Comparison corpus</h2>");
    html.push_str(&format!(
        "<p>Current-session submissions compared: {}</p>",
        payload.current_submissions_compared
    ));
    if payload.current_comparisons.is_empty() {
        html.push_str("<p class=\"muted\">No other current-session submissions were compared.</p>");
    } else {
        html.push_str("<ul>");
        for comparison in &payload.current_comparisons {
            html.push_str(&format!(
                "<li>{}</li>",
                escape_html(&comparison.display_name)
            ));
        }
        html.push_str("</ul>");
    }
    html.push_str(&format!(
        "<p>Historical submissions compared: {}</p>",
        payload.historical_submissions_compared
    ));
    if payload.historical_libraries.is_empty() {
        html.push_str("<p class=\"muted\">No historical libraries were selected.</p>");
    } else {
        html.push_str("<ul>");
        for library in &payload.historical_libraries {
            html.push_str(&format!(
                "<li>{} · {} sources · from {}</li>",
                escape_html(&library.name),
                library.source_count,
                escape_html(&library.source_session_name),
            ));
        }
        html.push_str("</ul>");
    }
    html.push_str("<h2>Coverage</h2>");
    html.push_str(&format!(
        "<p>Overall matched coverage: {}<br>Exact matched coverage: {}<br>Modified matched coverage: {}</p>",
        format_coverage(payload.overall_coverage),
        format_coverage(payload.exact_coverage),
        format_coverage(payload.modified_coverage),
    ));
    for (corpus, heading) in [
        (SourceCorpus::Current, "Current-session matches"),
        (SourceCorpus::Historical, "Historical-library matches"),
    ] {
        html.push_str(&format!("<h2>{heading}</h2>"));
        let corpus_matches = payload
            .matches
            .iter()
            .filter(|evidence| evidence.corpus == corpus)
            .collect::<Vec<_>>();
        if corpus_matches.is_empty() {
            let message = match corpus {
                SourceCorpus::Current => {
                    "No matching passages were found between current submissions."
                }
                SourceCorpus::Historical if payload.historical_libraries.is_empty() => {
                    "No historical libraries were selected."
                }
                SourceCorpus::Historical => {
                    "No matching passages were found in the selected historical libraries."
                }
            };
            html.push_str(&format!("<p>{}</p>", escape_html(message)));
        }
        for evidence in corpus_matches {
            let student_source = find_source(payload, &evidence.student_source_id)?;
            let comparison_source = find_source(payload, &evidence.comparison_source_id)?;
            html.push_str("<div class=\"evidence\">");
            html.push_str(&format!(
                "<p>Match type: {} · {} words{}<br>Current-student coverage: {} · exact {} · modified {}</p>",
                match evidence.kind {
                    MatchKind::Exact => "Exact",
                    MatchKind::Modified => "Modified",
                },
                evidence.tokens,
                if evidence.common_text { " · also common in this session" } else { "" },
                format_coverage(evidence.coverage),
                format_coverage(evidence.exact_coverage),
                format_coverage(evidence.modified_coverage),
            ));
            if let Some(library_id) = evidence.historical_library_id.as_deref() {
                let library = payload
                    .historical_libraries
                    .iter()
                    .find(|library| library.id == library_id)
                    .ok_or_else(|| invalid("historical match references a missing library"))?;
                html.push_str(&format!(
                    "<p>Historical library: {}</p>",
                    escape_html(&library.name)
                ));
            }
            html.push_str(&format!(
                "<h3>Student source · {}</h3><pre>{}</pre>",
                escape_html(&student_source.label),
                render_source_excerpt(
                    payload,
                    student_source,
                    evidence.student_start,
                    evidence.student_end,
                )?,
            ));
            html.push_str(&format!(
                "<h3>Comparison source · {}</h3><pre>{}</pre>",
                escape_html(&comparison_source.label),
                render_source_excerpt(
                    payload,
                    comparison_source,
                    evidence.comparison_start,
                    evidence.comparison_end,
                )?,
            ));
            html.push_str("</div>");
        }
    }
    if payload.exclusions.is_empty() {
        html.push_str("<h2>Excluded text</h2><p>No text was excluded from scoring.</p>");
    } else {
        html.push_str("<h2>Excluded text</h2>");
        for exclusion in &payload.exclusions {
            let source = find_source(payload, &exclusion.source_id)?;
            html.push_str(&format!(
                "<div class=\"evidence\"><p>Excluded from scoring · {} · {} words</p><h3>{}</h3><pre>{}</pre></div>",
                escape_html(&exclusion.reason),
                exclusion.tokens,
                escape_html(&source.label),
                escape_html(&exclusion.excerpt),
            ));
        }
    }
    html.push_str(&format!(
        "<h2>Engine versions and integrity</h2><p class=\"muted\">Fingerprint {} · normalization {} · common text {} · modified {}<br>Report schema {} · SHA-256 {}</p>",
        payload.engine_versions.fingerprint,
        payload.engine_versions.normalization,
        payload.engine_versions.common_text,
        payload.engine_versions.modified,
        payload.schema_version,
        report.integrity.payload_sha256,
    ));
    html.push_str(&format!(
        "<p class=\"disclaimer\">{}</p></body></html>",
        escape_html(&payload.reviewer_disclaimer)
    ));
    Ok(html)
}

pub fn render_report_pdf(report: &StudentReport) -> Result<Vec<u8>, ReportError> {
    let html = report_html(report)?;
    validate_font_coverage(&html)?;
    let images = BTreeMap::new();
    let mut fonts = BTreeMap::new();
    fonts.insert(
        "Noto Sans".to_string(),
        Base64OrRaw::Raw(REPORT_FONT.to_vec()),
    );
    let options = GeneratePdfOptions {
        page_width: Some(210.0),
        page_height: Some(297.0),
        margin_top: Some(15.0),
        margin_right: Some(15.0),
        margin_bottom: Some(15.0),
        margin_left: Some(15.0),
        ..Default::default()
    };
    let mut warnings = Vec::new();
    let document = PdfDocument::from_html(&html, &images, &fonts, &options, &mut warnings)
        .map_err(ReportError::Pdf)?;
    if !warnings.is_empty() {
        return Err(ReportError::Pdf(format!(
            "HTML layout emitted renderer warnings: {warnings:?}"
        )));
    }
    if document.pages.is_empty() {
        return Err(ReportError::Pdf("renderer produced no pages".into()));
    }
    let mut save_warnings = Vec::new();
    let bytes = document.save(&PdfSaveOptions::default(), &mut save_warnings);
    if !save_warnings.is_empty() {
        return Err(ReportError::Pdf(format!(
            "PDF serialization emitted warnings: {save_warnings:?}"
        )));
    }
    if !bytes.starts_with(b"%PDF-") {
        return Err(ReportError::Pdf(
            "renderer did not produce a valid PDF header".into(),
        ));
    }
    let extracted = extract_pdf_text(&bytes)?;
    let extracted = collapse_whitespace(&extracted);
    let disclaimer = collapse_whitespace(REVIEWER_DISCLAIMER);
    if !extracted.ends_with(&disclaimer) {
        return Err(ReportError::Pdf(
            "generated PDF does not end with the complete required reviewer disclaimer".into(),
        ));
    }
    for evidence in &report.payload.matches {
        if !extracted.contains(&collapse_whitespace(&evidence.student_excerpt))
            || !extracted.contains(&collapse_whitespace(&evidence.comparison_excerpt))
        {
            return Err(ReportError::Pdf(format!(
                "generated PDF is missing one side of match {}",
                evidence.id
            )));
        }
    }
    for exclusion in &report.payload.exclusions {
        if !extracted.contains(&collapse_whitespace(&exclusion.excerpt))
            || !extracted.contains(&collapse_whitespace(&exclusion.reason))
        {
            return Err(ReportError::Pdf(format!(
                "generated PDF is missing excluded evidence for source {}",
                exclusion.source_id
            )));
        }
    }
    validate_pdf_highlights(&bytes, &report.payload)?;
    Ok(bytes)
}

pub fn extract_pdf_text(pdf: &[u8]) -> Result<String, ReportError> {
    pdf_extract::extract_text_from_mem(pdf).map_err(|error| {
        ReportError::Pdf(format!("generated PDF text could not be verified: {error}"))
    })
}

fn render_source_excerpt(
    payload: &ReportPayload,
    source: &ReportSource,
    focus_start: usize,
    focus_end: usize,
) -> Result<String, ReportError> {
    validate_span(&source.text, focus_start, focus_end, "report focus")?;
    let excerpt_start = source.text[..focus_start]
        .char_indices()
        .rev()
        .nth(120)
        .map_or(0, |(index, _)| index);
    let excerpt_end = source.text[focus_end..]
        .char_indices()
        .nth(120)
        .map_or(source.text.len(), |(index, _)| focus_end + index);
    let mut spans = Vec::<(usize, usize, bool)>::new();
    for evidence in &payload.matches {
        let range = if evidence.student_source_id == source.id {
            Some((evidence.student_start, evidence.student_end))
        } else if evidence.comparison_source_id == source.id {
            Some((evidence.comparison_start, evidence.comparison_end))
        } else {
            None
        };
        if let Some((start, end)) = range {
            let start = start.max(excerpt_start);
            let end = end.min(excerpt_end);
            if start >= end {
                continue;
            }
            spans.push((start, end, false));
        }
    }
    for exclusion in &payload.exclusions {
        if exclusion.source_id == source.id
            && exclusion.end > excerpt_start
            && exclusion.start < excerpt_end
        {
            spans.push((
                exclusion.start.max(excerpt_start),
                exclusion.end.min(excerpt_end),
                true,
            ));
        }
    }
    let mut boundaries = vec![excerpt_start, excerpt_end];
    for (start, end, _) in &spans {
        boundaries.push(*start);
        boundaries.push(*end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    let mut rendered = String::new();
    for pair in boundaries.windows(2) {
        let start = pair[0];
        let end = pair[1];
        if start == end {
            continue;
        }
        let matching = spans
            .iter()
            .filter(|(span_start, span_end, _)| *span_start <= start && end <= *span_end)
            .collect::<Vec<_>>();
        let mut classes = Vec::new();
        if matching.iter().any(|(_, _, excluded)| !excluded) {
            classes.push("match-highlight");
            classes.push(match source.corpus {
                SourceCorpus::Current => "current",
                SourceCorpus::Historical => "historical",
            });
        }
        if matching.iter().any(|(_, _, excluded)| *excluded) {
            classes.push("excluded");
        }
        let text = escape_html(&source.text[start..end]);
        if classes.is_empty() {
            rendered.push_str(&text);
        } else {
            rendered.push_str(&format!(
                "<mark class=\"{}\">{text}</mark>",
                classes.join(" ")
            ));
        }
    }
    if excerpt_start > 0 {
        rendered.insert(0, '…');
    }
    if excerpt_end < source.text.len() {
        rendered.push('…');
    }
    Ok(rendered)
}

fn find_source<'a>(payload: &'a ReportPayload, id: &str) -> Result<&'a ReportSource, ReportError> {
    payload
        .sources
        .iter()
        .find(|source| source.id == id)
        .ok_or_else(|| invalid("report evidence references a missing source"))
}

fn format_coverage(value: Option<f64>) -> String {
    value.map_or_else(
        || "Not assessable · insufficient assessable text".into(),
        |percent| format!("{percent:.1}%"),
    )
}

fn escape_html(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            other => output.push(other),
        }
    }
    output
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn validate_font_coverage(text: &str) -> Result<(), ReportError> {
    let face = ttf_parser::Face::parse(REPORT_FONT, 0)
        .map_err(|_| ReportError::Pdf("the embedded report font is invalid".into()))?;
    for character in text.chars().filter(|character| !character.is_whitespace()) {
        if face.glyph_index(character).is_none() {
            return Err(ReportError::Pdf(format!(
                "the embedded report font does not support U+{:04X}; replace this character before exporting",
                character as u32
            )));
        }
    }
    Ok(())
}

fn validate_pdf_highlights(pdf: &[u8], payload: &ReportPayload) -> Result<(), ReportError> {
    let mut warnings = Vec::new();
    let parsed = PdfDocument::parse(
        pdf,
        &PdfParseOptions {
            fail_on_error: true,
        },
        &mut warnings,
    )
    .map_err(|error| {
        ReportError::Pdf(format!(
            "generated PDF structure could not be verified: {error}"
        ))
    })?;
    if warnings
        .iter()
        .any(|warning| warning.severity == printpdf::PdfParseErrorSeverity::Error)
    {
        return Err(ReportError::Pdf(
            "generated PDF contains a structural parse error".into(),
        ));
    }
    let mut current_rectangles = 0usize;
    let mut historical_rectangles = 0usize;
    for page in &parsed.pages {
        let mut fill_kind = None;
        for operation in &page.ops {
            match operation {
                Op::SetFillColor { col } => fill_kind = highlight_color(col),
                Op::DrawPolygon { polygon } if polygon.mode == PaintMode::Fill => {
                    match fill_kind.take() {
                        Some(HighlightColor::Current) => current_rectangles += 1,
                        Some(HighlightColor::Historical) => historical_rectangles += 1,
                        None => {}
                    }
                }
                _ => {}
            }
        }
    }
    let expected_current = highlighted_source_count(payload, SourceCorpus::Current)?;
    let expected_historical = highlighted_source_count(payload, SourceCorpus::Historical)?;
    if current_rectangles < expected_current || historical_rectangles < expected_historical {
        return Err(ReportError::Pdf(format!(
            "PDF highlight verification failed: expected at least {expected_current} current and {expected_historical} historical source highlights, found {current_rectangles} and {historical_rectangles}"
        )));
    }
    Ok(())
}

fn highlighted_source_count(
    payload: &ReportPayload,
    source_corpus: SourceCorpus,
) -> Result<usize, ReportError> {
    let mut source_ids = HashSet::new();
    for evidence in &payload.matches {
        for id in [&evidence.student_source_id, &evidence.comparison_source_id] {
            if find_source(payload, id)?.corpus == source_corpus {
                source_ids.insert(id.as_str());
            }
        }
    }
    Ok(source_ids.len())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HighlightColor {
    Current,
    Historical,
}

fn highlight_color(color: &Color) -> Option<HighlightColor> {
    let Color::Rgb(rgb) = color else {
        return None;
    };
    if rgb.r > 0.98 && (0.86..0.93).contains(&rgb.g) && (0.50..0.63).contains(&rgb.b) {
        Some(HighlightColor::Current)
    } else if (0.80..0.88).contains(&rgb.r) && rgb.g > 0.88 && rgb.b > 0.97 {
        Some(HighlightColor::Historical)
    } else {
        None
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn invalid(message: impl Into<String>) -> ReportError {
    ReportError::Invalid(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const STUDENT_TEXT: &str = "alpha copied phrase beta";
    const PEER_TEXT: &str = "other copied phrase words";
    const HISTORY_TEXT: &str = "archive copied phrase words";

    fn source(id: &str, label: &str, corpus: SourceCorpus, text: &str) -> ReportSource {
        ReportSource {
            id: id.to_string(),
            label: label.to_string(),
            corpus,
            text: text.to_string(),
            content_sha256: sha256_hex(text.as_bytes()),
        }
    }

    fn input(current: bool, historical: bool) -> ReportInput {
        let mut sources = vec![source(
            "submission-student",
            "Avery - submission.txt",
            SourceCorpus::Current,
            STUDENT_TEXT,
        )];
        let mut comparisons = Vec::new();
        let mut matches = Vec::new();
        if current {
            sources.push(source(
                "submission-peer",
                "Blair - submission.txt",
                SourceCorpus::Current,
                PEER_TEXT,
            ));
            comparisons.push(ComparedSubmission {
                id: "peer".into(),
                display_name: "Blair Student".into(),
                source_id: "submission-peer".into(),
                content_sha256: sha256_hex(PEER_TEXT.as_bytes()),
            });
            matches.push(ReportEvidence {
                id: "current-1".into(),
                corpus: SourceCorpus::Current,
                historical_library_id: None,
                kind: MatchKind::Exact,
                student_source_id: "submission-student".into(),
                comparison_source_id: "submission-peer".into(),
                student_start: 6,
                student_end: 19,
                comparison_start: 6,
                comparison_end: 19,
                tokens: 3,
                common_text: false,
                coverage: Some(60.0),
                exact_coverage: Some(60.0),
                modified_coverage: Some(0.0),
            });
        }
        let mut libraries = Vec::new();
        if historical {
            sources.push(source(
                "reference-1",
                "Library · Document 1",
                SourceCorpus::Historical,
                HISTORY_TEXT,
            ));
            libraries.push(ComparedLibrary {
                id: "library-1".into(),
                name: "Archived course".into(),
                source_session_name: "Previous term".into(),
                source_count: 4,
            });
            matches.push(ReportEvidence {
                id: "historical-1".into(),
                corpus: SourceCorpus::Historical,
                historical_library_id: Some("library-1".into()),
                kind: MatchKind::Modified,
                student_source_id: "submission-student".into(),
                comparison_source_id: "reference-1".into(),
                student_start: 6,
                student_end: 19,
                comparison_start: 8,
                comparison_end: 21,
                tokens: 3,
                common_text: true,
                coverage: Some(40.0),
                exact_coverage: Some(0.0),
                modified_coverage: Some(40.0),
            });
        }
        ReportInput {
            session_id: "session-1".into(),
            session_name: "Research Methods".into(),
            subject: Some("Science".into()),
            target_student_id: "student".into(),
            target_student_name: "Avery Student".into(),
            target_source_id: "submission-student".into(),
            generated_at: "2026-09-23T00:00:00.000Z".into(),
            report_mode: ReportMode::Teacher,
            anonymize: false,
            current_comparisons: comparisons,
            historical_libraries: libraries,
            sources,
            overall_coverage: if current { Some(60.0) } else { None },
            exact_coverage: if current { Some(60.0) } else { None },
            modified_coverage: if historical { Some(40.0) } else { Some(0.0) },
            matches,
            exclusions: Vec::new(),
            engine_versions: EngineVersions {
                fingerprint: 1,
                normalization: 1,
                common_text: 1,
                modified: 1,
            },
        }
    }

    fn strip_tags(html: &str) -> String {
        let mut result = String::new();
        let mut inside_tag = false;
        for ch in html.chars() {
            match ch {
                '<' => inside_tag = true,
                '>' => inside_tag = false,
                _ if !inside_tag => result.push(ch),
                _ => {}
            }
        }
        result
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
    }

    #[test]
    fn zero_match_report_keeps_corpus_and_reviewer_footer() {
        let report = build_report(input(false, false)).expect("valid zero-match report");
        assert!(report.payload.matches.is_empty());
        assert_eq!(report.payload.current_comparisons.len(), 0);
        assert!(report
            .payload
            .reviewer_disclaimer
            .contains("Similarity alone does not prove plagiarism."));
        assert_eq!(
            report.integrity.payload_sha256,
            report.payload_sha256().unwrap()
        );
        let pdf = render_report_pdf(&report).expect("zero-match PDF renders and verifies");
        let text = collapse_whitespace(&extract_pdf_text(&pdf).unwrap());
        assert!(text.ends_with(&collapse_whitespace(REVIEWER_DISCLAIMER)));
        assert!(text.contains("Historical submissions compared: 0"));
        assert!(text.contains("No historical libraries were selected."));
    }

    #[test]
    fn current_only_historical_only_and_mixed_evidence_remain_separate() {
        let current = build_report(input(true, false)).unwrap();
        assert_eq!(current.payload.top_current_matches, ["current-1"]);
        assert!(current.payload.top_historical_matches.is_empty());
        let current_pdf = render_report_pdf(&current).expect("current-only PDF renders");
        let current_text = collapse_whitespace(&extract_pdf_text(&current_pdf).unwrap());
        assert!(current_text.contains("Current-session matches"));
        assert!(current_text.contains("copied phrase"));

        let historical = build_report(input(false, true)).unwrap();
        assert!(historical.payload.top_current_matches.is_empty());
        assert_eq!(historical.payload.top_historical_matches, ["historical-1"]);
        let historical_pdf = render_report_pdf(&historical).expect("historical-only PDF renders");
        let historical_text = collapse_whitespace(&extract_pdf_text(&historical_pdf).unwrap());
        assert!(historical_text.contains("Historical-library matches"));
        assert!(historical_text.contains("archive copied phrase words"));

        let mixed = build_report(input(true, true)).unwrap();
        assert_eq!(mixed.payload.matches.len(), 2);
        assert_eq!(mixed.payload.top_current_matches, ["current-1"]);
        assert_eq!(mixed.payload.top_historical_matches, ["historical-1"]);
        let mixed_pdf = render_report_pdf(&mixed).expect("mixed PDF renders both highlight kinds");
        let mixed_text = collapse_whitespace(&extract_pdf_text(&mixed_pdf).unwrap());
        assert!(mixed_text.contains("Current-session matches"));
        assert!(mixed_text.contains("Historical-library matches"));
        assert!(mixed_text.ends_with(&collapse_whitespace(REVIEWER_DISCLAIMER)));
    }

    #[test]
    fn every_match_span_is_valid_utf8_and_bounded_on_both_sources() {
        let mut input = input(true, true);
        input.sources[0].text = "🌱 alpha copied phrase beta".into();
        input.sources[0].content_sha256 = sha256_hex(input.sources[0].text.as_bytes());
        input.matches[0].student_start += "🌱 ".len();
        input.matches[0].student_end += "🌱 ".len();
        input.matches[1].student_start += "🌱 ".len();
        input.matches[1].student_end += "🌱 ".len();
        let report = build_report(input).expect("emoji-safe byte offsets");
        assert_eq!(report.payload.matches[0].student_excerpt, "copied phrase");
    }

    #[test]
    fn invalid_out_of_bounds_or_non_boundary_spans_fail_loudly() {
        let mut out_of_bounds = input(true, false);
        out_of_bounds.matches[0].student_end = usize::MAX;
        assert!(build_report(out_of_bounds).is_err());

        let mut split_utf8 = input(true, false);
        split_utf8.sources[0].text = "🌱 copied phrase".into();
        split_utf8.sources[0].content_sha256 = sha256_hex(split_utf8.sources[0].text.as_bytes());
        split_utf8.matches[0].student_start = 1;
        assert!(build_report(split_utf8).is_err());
    }

    #[test]
    fn overlapping_matches_are_unioned_in_source_highlight_markup() {
        let mut input = input(true, false);
        let mut overlap = input.matches[0].clone();
        overlap.id = "current-overlap".into();
        overlap.student_start = 11;
        overlap.student_end = 19;
        overlap.comparison_start = 11;
        overlap.comparison_end = 19;
        input.matches.push(overlap);
        let report = build_report(input).unwrap();
        let html = report_html(&report).unwrap();
        assert!(strip_tags(&html).contains("copied phrase"));
        assert!(html.contains("class=\"match-highlight current\""));
        assert!(html.matches("class=\"match-highlight current\"").count() >= 2);
        assert_eq!(html.matches("current-overlap").count(), 0);
    }

    #[test]
    fn excluded_text_stays_in_report_with_reason_and_highlighted_span() {
        let mut input = input(false, false);
        input.exclusions.push(ReportExclusionInput {
            source_id: "submission-student".into(),
            start: 0,
            end: 5,
            reason: "Assignment question".into(),
            tokens: 1,
        });
        let report = build_report(input).unwrap();
        let html = report_html(&report).unwrap();
        assert!(html.contains("alpha"));
        assert!(html.contains("Assignment question"));
        assert!(report.payload.exclusions[0].excerpt == "alpha");
    }

    #[test]
    fn anonymized_report_does_not_leak_names_or_filenames() {
        let mut input = input(true, true);
        input.anonymize = true;
        let report = build_report(input).unwrap();
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("Avery Student"));
        assert!(!json.contains("Blair Student"));
        assert!(!json.contains("submission.txt"));
        assert_eq!(report.payload.target_student_name, "Student 1");
        assert_eq!(
            report.payload.current_comparisons[0].display_name,
            "Student 2"
        );
    }

    #[test]
    fn long_passages_and_pdf_footer_survive_local_rendering() {
        let mut input = input(true, false);
        let long = "copied sentence ".repeat(320);
        input.sources[0].text = format!("begin {long} end");
        input.sources[1].text = format!("other {long} tail");
        input.sources[0].content_sha256 = sha256_hex(input.sources[0].text.as_bytes());
        input.sources[1].content_sha256 = sha256_hex(input.sources[1].text.as_bytes());
        input.current_comparisons[0].content_sha256 = input.sources[1].content_sha256.clone();
        input.matches[0].student_start = 6;
        input.matches[0].student_end = 6 + long.trim_end().len();
        input.matches[0].comparison_start = 6;
        input.matches[0].comparison_end = 6 + long.trim_end().len();
        let report = build_report(input).unwrap();
        let pdf = render_report_pdf(&report).expect("render valid report");
        assert!(pdf.starts_with(b"%PDF-"));
        let text = extract_pdf_text(&pdf).expect("extract generated report text");
        assert!(text.contains("copied sentence"));
        assert!(collapse_whitespace(&text).contains(&collapse_whitespace(REVIEWER_DISCLAIMER)));
    }

    #[test]
    fn pdf_generation_rejects_missing_font_glyphs_without_substitution() {
        let mut input = input(true, false);
        input.target_student_name = "Avery 🪴".into();
        let report = build_report(input).unwrap();
        let error =
            render_report_pdf(&report).expect_err("unsupported glyph must not be substituted");
        assert!(error.to_string().contains("U+1FAB4"));
    }

    #[test]
    fn report_payload_integrity_covers_the_reviewer_footer_and_rejects_mutation() {
        let report = build_report(input(true, false)).unwrap();
        assert!(report.verify_integrity());
        let mut mutated = report.clone();
        mutated.payload.reviewer_disclaimer.push('!');
        assert!(!mutated.verify_integrity());
    }
}

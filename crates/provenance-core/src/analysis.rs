//! Phase 5: exact-only session analysis over current submissions.
//! Phase 6: prompt + session-frequency exclusion applied before scoring.
//! Computed on demand (fast for class sizes); persisted reports arrive in Phase 16.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::domain::{ReferenceLibrary, ReferenceSubmission, Submission};
use crate::error::CoreError;
use crate::service::list_students;
use crate::storage::{AnalysisRepo, ReferenceLibraryRepo, SessionRepo, SubmissionRepo};
use provenance_match::{
    apply_exclusion, canonicalize, common_token_mask, compare_documents, find_modified_matches,
    is_common_range, prompt_token_mask, shingle_document_frequency, union_len, CommonTextConfig,
    ExactConfig, ExclusionReason, ModifiedConfig, ModifiedMatch, TokenMatch, COMMON_TEXT_VERSION,
    FINGERPRINT_VERSION, MODIFIED_VERSION, NORMALIZATION_VERSION,
};

/// Document-frequency exclusion needs statistical meaning: below this many
/// analyzed documents every shared passage is case evidence, so DF is off.
pub const MIN_DOCS_FOR_DF: usize = 4;
/// Bump when session-level exclusions, interval union, or coverage semantics change.
pub const SESSION_SCORING_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisStage {
    Preparing,
    Exact,
    Modified,
    Aligning,
    Scoring,
    Historical,
    Saving,
    Complete,
    Failed,
}

/// Monotonic progress for a single session analysis invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisProgress {
    pub session_id: String,
    pub stage: AnalysisStage,
    /// Monotonic fraction from 0.0 through 1.0 across all stages.
    pub fraction: f64,
    pub completed_pairs: usize,
    pub total_pairs: usize,
    pub cached_pairs: usize,
}

struct RawPairMatches {
    left_index: usize,
    right_index: usize,
    cache_key: String,
    cache_hit: bool,
    exact: Vec<TokenMatch>,
    modified: Vec<ModifiedMatch>,
}

#[derive(Serialize, Deserialize)]
struct RawPairEvidenceCache {
    exact: Vec<TokenMatch>,
    modified: Vec<ModifiedMatch>,
}

fn raw_pair_engine_key() -> String {
    let exact = ExactConfig::default();
    let modified = ModifiedConfig::default();
    format!("exact-{exact:?}-modified-{modified:?}-normalization-{NORMALIZATION_VERSION}")
}

fn raw_pair_cache_key(
    session_id: &str,
    student_a_id: &str,
    student_b_id: &str,
    source_hash_a: &str,
    source_hash_b: &str,
) -> String {
    let engine_key = raw_pair_engine_key();
    let canonical = serde_json::to_vec(&(
        session_id,
        student_a_id,
        student_b_id,
        source_hash_a,
        source_hash_b,
        engine_key,
    ))
    .expect("serializing a tuple of strings cannot fail");
    hex::encode(Sha256::digest(canonical))
}

/// Hash every input that affects scored session output. Cached analysis is
/// served only when this exact digest matches.
pub async fn session_analysis_input_hash(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<String, CoreError> {
    let session = SessionRepo::get(pool, session_id).await?;
    let mut submissions = SubmissionRepo::list_by_session(pool, session_id).await?;
    submissions.sort_by(|a, b| a.student_id.cmp(&b.student_id));
    let submission_inputs: Vec<(String, String)> = submissions
        .iter()
        .map(|submission| {
            (
                submission.student_id.clone(),
                hex::encode(Sha256::digest(submission.original_text.as_bytes())),
            )
        })
        .collect();
    let selected_references = ReferenceLibraryRepo::selected_submissions(pool, session_id).await?;
    let reference_inputs: Vec<(String, String, String, String)> = selected_references
        .iter()
        .map(|(library, reference)| {
            (
                library.id.clone(),
                library.name.clone(),
                reference.id.clone(),
                hex::encode(Sha256::digest(reference.original_text.as_bytes())),
            )
        })
        .collect();
    let input = serde_json::to_vec(&(
        session_id,
        session.assignment_prompt,
        session.excluded_reference_text,
        session.exclude_common_text,
        submission_inputs,
        reference_inputs,
        FINGERPRINT_VERSION,
        NORMALIZATION_VERSION,
        COMMON_TEXT_VERSION,
        MODIFIED_VERSION,
        SESSION_SCORING_VERSION,
    ))
    .map_err(|_| CoreError::validation("could not fingerprint session analysis inputs"))?;
    Ok(hex::encode(Sha256::digest(input)))
}

fn emit_progress<F: FnMut(AnalysisProgress)>(
    callback: &mut F,
    session_id: &str,
    stage: AnalysisStage,
    fraction: f64,
    completed_pairs: usize,
    total_pairs: usize,
    cached_pairs: usize,
) {
    callback(AnalysisProgress {
        session_id: session_id.to_string(),
        stage,
        fraction,
        completed_pairs,
        total_pairs,
        cached_pairs,
    });
}

/// One passage with evidence spans on both sides.
/// Token ranges are ordinals (end-exclusive); char ranges index original text.
///
/// `common_text` is an explanatory classification, never a score exclusion:
/// the passage is fully counted even when most of its tokens are shared
/// session-wide. Only teacher-provided prompt/reference material reduces
/// the score (see `excluded`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PassageEvidence {
    pub kind: PassageKind,
    /// Alignment identity for modified passages; always `None` for exact.
    pub identity: Option<f64>,
    /// True when at least half the passage tokens are shared session-wide.
    pub common_text: bool,
    pub a_token_start: usize,
    pub a_token_end: usize,
    pub b_token_start: usize,
    pub b_token_end: usize,
    pub a_char_start: usize,
    pub a_char_end: usize,
    pub b_char_start: usize,
    pub b_char_end: usize,
    pub tokens: usize,
}

/// Which engine produced a passage. Never accusatory wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PassageKind {
    Exact,
    Modified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExclusionSide {
    A,
    B,
}

/// A matched span excluded from scoring, with its reason and evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExcludedEvidence {
    pub side: ExclusionSide,
    pub token_start: usize,
    pub token_end: usize,
    pub char_start: usize,
    pub char_end: usize,
    pub reason: ExclusionReason,
    pub tokens: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairAnalysis {
    pub a_student_id: String,
    pub b_student_id: String,
    /// % of A's eligible tokens matched (exact + modified, exclusions
    /// removed). `None` means insufficient assessable text — never 0%.
    pub coverage_a: Option<f64>,
    /// % of B's eligible tokens matched. `None` when not assessable.
    pub coverage_b: Option<f64>,
    /// Exact-only coverage on each side. These values are diagnostic
    /// breakdowns; they must not be added to modified coverage because the
    /// overall score is calculated from the union of all evidence spans.
    pub exact_coverage_a: Option<f64>,
    pub exact_coverage_b: Option<f64>,
    /// Modified-only coverage on each side. `Some(0)` means assessable with
    /// no modified matches; `None` means insufficient assessable text.
    pub modified_coverage_a: Option<f64>,
    pub modified_coverage_b: Option<f64>,
    pub passages: Vec<PassageEvidence>,
    pub excluded: Vec<ExcludedEvidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StudentExactCoverage {
    pub student_id: String,
    /// `None` means insufficient assessable text — never a clean 0%.
    pub coverage: Option<f64>,
    /// Exact-only subset of `coverage`; do not add to modified coverage.
    pub exact_coverage: Option<f64>,
    /// Modified-only subset of `coverage`; do not add to exact coverage.
    pub modified_coverage: Option<f64>,
    pub matched_tokens: usize,
    pub total_tokens: usize,
    pub eligible_tokens: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExactAnalysis {
    pub fingerprint_version: u32,
    pub normalization_version: u32,
    pub common_text_version: u32,
    pub modified_version: u32,
    /// Whether DF exclusion ran (needs enough documents + the toggle).
    pub common_text_applied: bool,
    /// Whether prompt/reference exclusion ran.
    pub prompt_applied: bool,
    pub pairs: Vec<PairAnalysis>,
    pub per_student: Vec<StudentExactCoverage>,
    /// Selected read-only historical sources, included even with no matches.
    #[serde(default)]
    pub compared_libraries: Vec<ComparedLibrary>,
    /// Historical evidence remains separate from current-student pair scores.
    #[serde(default)]
    pub historical_matches: Vec<HistoricalPairAnalysis>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparedLibrary {
    pub id: String,
    pub name: String,
    pub source_session_name: String,
    pub source_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoricalPairAnalysis {
    pub student_id: String,
    pub library_id: String,
    pub library_name: String,
    pub reference_submission_id: String,
    pub reference_label: String,
    pub reference_filename: Option<String>,
    /// Embedded to make saved highlighted evidence self-contained.
    pub reference_text: String,
    pub coverage_current: Option<f64>,
    pub exact_coverage_current: Option<f64>,
    pub modified_coverage_current: Option<f64>,
    pub passages: Vec<PassageEvidence>,
    pub excluded: Vec<ExcludedEvidence>,
}

fn char_span(
    doc: &provenance_match::CanonicalDocument,
    start: usize,
    end: usize,
) -> (usize, usize) {
    (
        doc.tokens[start].original_start,
        doc.tokens[end - 1].original_end,
    )
}

fn intersect(range: (usize, usize), cuts: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for &(s, e) in cuts {
        let a = s.max(range.0);
        let b = e.min(range.1);
        if a < b {
            out.push((a, b));
        }
    }
    out
}

/// Scored coverage over eligible text. `None` means insufficient assessable
/// text — the caller must never render it as a clean 0%.
fn eligible_coverage(matched: &[(usize, usize)], eligible_tokens: usize) -> Option<f64> {
    if eligible_tokens == 0 {
        None
    } else {
        Some(union_len(matched) as f64 / eligible_tokens as f64 * 100.0)
    }
}

fn exclusion_evidence(
    side: ExclusionSide,
    spans: &[provenance_match::ExcludedSpan],
    doc: &provenance_match::CanonicalDocument,
) -> Vec<ExcludedEvidence> {
    spans
        .iter()
        .map(|span| {
            let (char_start, char_end) = char_span(doc, span.token_start, span.token_end);
            ExcludedEvidence {
                side,
                token_start: span.token_start,
                token_end: span.token_end,
                char_start,
                char_end,
                reason: span.reason,
                tokens: span.token_end - span.token_start,
            }
        })
        .collect()
}

fn analyze_historical_pair(
    student_id: &str,
    current: &provenance_match::CanonicalDocument,
    current_common: &[bool],
    library: &ReferenceLibrary,
    reference: &ReferenceSubmission,
    prompt_docs: &[Vec<String>],
    reference_docs: &[Vec<String>],
) -> HistoricalPairAnalysis {
    let historical = canonicalize(&reference.original_text);
    let current_tokens: Vec<String> = current
        .tokens
        .iter()
        .map(|token| token.normalized.clone())
        .collect();
    let historical_tokens: Vec<String> = historical
        .tokens
        .iter()
        .map(|token| token.normalized.clone())
        .collect();
    let cfg = CommonTextConfig::default();
    let mut current_exclusions =
        prompt_token_mask(&current_tokens, prompt_docs, cfg.prompt_min_tokens);
    current_exclusions.extend(
        prompt_token_mask(&current_tokens, reference_docs, cfg.prompt_min_tokens)
            .into_iter()
            .map(|(start, end, _)| (start, end, ExclusionReason::Reference)),
    );
    let mut historical_exclusions =
        prompt_token_mask(&historical_tokens, prompt_docs, cfg.prompt_min_tokens);
    historical_exclusions.extend(
        prompt_token_mask(&historical_tokens, reference_docs, cfg.prompt_min_tokens)
            .into_iter()
            .map(|(start, end, _)| (start, end, ExclusionReason::Reference)),
    );
    let current_exclusion_ranges: Vec<(usize, usize)> = current_exclusions
        .iter()
        .map(|(start, end, _)| (*start, *end))
        .collect();
    let eligible_current = current
        .tokens
        .len()
        .saturating_sub(union_len(&current_exclusion_ranges));
    let exact_cfg = ExactConfig::default();
    let modified_cfg = ModifiedConfig::default();
    let exact = compare_documents(current, &historical, exact_cfg);
    let (counted_current, excluded_current) = apply_exclusion(&exact, true, &current_exclusions);
    let (counted_reference, excluded_reference) =
        apply_exclusion(&exact, false, &historical_exclusions);
    let mut passages = Vec::new();
    for matched in &exact {
        let offset = matched.b_start as i64 - matched.a_start as i64;
        for (start, end) in intersect((matched.a_start, matched.a_end), &counted_current) {
            let reference_start = (start as i64 + offset) as usize;
            let reference_end = (end as i64 + offset) as usize;
            for (kept_start, kept_end) in
                intersect((reference_start, reference_end), &counted_reference)
            {
                let current_start = (kept_start as i64 - offset) as usize;
                let current_end = (kept_end as i64 - offset) as usize;
                let (a_char_start, a_char_end) = char_span(current, current_start, current_end);
                let (b_char_start, b_char_end) = char_span(&historical, kept_start, kept_end);
                passages.push(PassageEvidence {
                    kind: PassageKind::Exact,
                    identity: None,
                    common_text: is_common_range(current_start, current_end, current_common),
                    a_token_start: current_start,
                    a_token_end: current_end,
                    b_token_start: kept_start,
                    b_token_end: kept_end,
                    a_char_start,
                    a_char_end,
                    b_char_start,
                    b_char_end,
                    tokens: current_end - current_start,
                });
            }
        }
    }

    let exact_a: Vec<(usize, usize)> = exact
        .iter()
        .map(|matched| (matched.a_start, matched.a_end))
        .collect();
    let exact_b: Vec<(usize, usize)> = exact
        .iter()
        .map(|matched| (matched.b_start, matched.b_end))
        .collect();
    let modified = find_modified_matches(
        &current_tokens,
        &historical_tokens,
        &exact_a,
        &exact_b,
        exact_cfg,
        modified_cfg,
    );
    let mut modified_ranges = Vec::new();
    let mut excluded = exclusion_evidence(ExclusionSide::A, &excluded_current, current);
    excluded.extend(exclusion_evidence(
        ExclusionSide::B,
        &excluded_reference,
        &historical,
    ));
    for matched in modified {
        let (kept_a, excluded_a) = apply_exclusion(
            &[TokenMatch {
                a_start: matched.a_start,
                a_end: matched.a_end,
                b_start: matched.b_start,
                b_end: matched.b_end,
            }],
            true,
            &current_exclusions,
        );
        let (kept_b, excluded_b) = apply_exclusion(
            &[TokenMatch {
                a_start: matched.a_start,
                a_end: matched.a_end,
                b_start: matched.b_start,
                b_end: matched.b_end,
            }],
            false,
            &historical_exclusions,
        );
        excluded.extend(exclusion_evidence(ExclusionSide::A, &excluded_a, current));
        excluded.extend(exclusion_evidence(
            ExclusionSide::B,
            &excluded_b,
            &historical,
        ));
        for (start, end) in kept_a {
            let mut b_positions = Vec::new();
            for token in start..end {
                if let Some(position) = matched
                    .a_to_b
                    .get(token.wrapping_sub(matched.a_start))
                    .copied()
                    .flatten()
                {
                    b_positions.push(position);
                }
            }
            if b_positions.len() < modified_cfg.min_tokens {
                continue;
            }
            let b_start = b_positions[0];
            let b_end = b_positions[b_positions.len() - 1] + 1;
            for (kept_b_start, kept_b_end) in intersect((b_start, b_end), &kept_b) {
                let mut a_positions = Vec::new();
                for token in start..end {
                    if matches!(
                        matched
                            .a_to_b
                            .get(token.wrapping_sub(matched.a_start))
                            .copied()
                            .flatten(),
                        Some(position) if position >= kept_b_start && position < kept_b_end
                    ) {
                        a_positions.push(token);
                    }
                }
                if a_positions.len() < modified_cfg.min_tokens {
                    continue;
                }
                let a_start = a_positions[0];
                let a_end = a_positions[a_positions.len() - 1] + 1;
                let (a_char_start, a_char_end) = char_span(current, a_start, a_end);
                let (b_char_start, b_char_end) = char_span(&historical, kept_b_start, kept_b_end);
                modified_ranges.push((a_start, a_end));
                passages.push(PassageEvidence {
                    kind: PassageKind::Modified,
                    identity: Some(matched.identity),
                    common_text: is_common_range(a_start, a_end, current_common),
                    a_token_start: a_start,
                    a_token_end: a_end,
                    b_token_start: kept_b_start,
                    b_token_end: kept_b_end,
                    a_char_start,
                    a_char_end,
                    b_char_start,
                    b_char_end,
                    tokens: a_end - a_start,
                });
            }
        }
    }
    passages.sort_by_key(|passage| (passage.a_token_start, passage.b_token_start));
    excluded.sort_by_key(|item| (item.side as u8, item.token_start));
    let mut all_current = counted_current.clone();
    all_current.extend(modified_ranges.iter().copied());
    HistoricalPairAnalysis {
        student_id: student_id.to_string(),
        library_id: library.id.clone(),
        library_name: library.name.clone(),
        reference_submission_id: reference.id.clone(),
        reference_label: reference.source_label.clone(),
        reference_filename: reference.source_filename.clone(),
        reference_text: reference.original_text.clone(),
        coverage_current: eligible_coverage(&all_current, eligible_current),
        exact_coverage_current: eligible_coverage(&counted_current, eligible_current),
        modified_coverage_current: eligible_coverage(&modified_ranges, eligible_current),
        passages,
        excluded,
    }
}

/// Analyze one session's current submissions: exact engine with Phase 6
/// prompt + frequency exclusion applied before scoring.
pub async fn analyze_session_exact(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<ExactAnalysis, CoreError> {
    analyze_session_with_progress(pool, session_id, |_| {}).await
}

/// Analyze a session while reporting real exact, modified, alignment, and
/// scoring work. `fraction` never decreases; callbacks are synchronous and
/// must remain lightweight (for example, enqueueing a Tauri event).
pub async fn analyze_session_with_progress<F>(
    pool: &SqlitePool,
    session_id: &str,
    mut on_progress: F,
) -> Result<ExactAnalysis, CoreError>
where
    F: FnMut(AnalysisProgress),
{
    emit_progress(
        &mut on_progress,
        session_id,
        AnalysisStage::Preparing,
        0.0,
        0,
        0,
        0,
    );
    let session = SessionRepo::get(pool, session_id).await?;
    if session.status == crate::domain::SessionStatus::Locked {
        return Err(CoreError::LockedMutation {
            status: "locked".into(),
            action: "rerunning analysis".into(),
        });
    }
    let students = list_students(pool, session_id).await?;
    let submissions = SubmissionRepo::list_by_session(pool, session_id).await?;
    let selected_references = ReferenceLibraryRepo::selected_submissions(pool, session_id).await?;

    let by_student: HashMap<&str, &Submission> = submissions
        .iter()
        .filter(|s| !s.original_text.trim().is_empty())
        .map(|s| (s.student_id.as_str(), s))
        .collect();

    // Canonicalize every submission that has text.
    let mut ids: Vec<String> = Vec::new();
    let mut content_hashes: Vec<String> = Vec::new();
    let mut docs: Vec<provenance_match::CanonicalDocument> = Vec::new();
    let mut token_lists: Vec<Vec<String>> = Vec::new();
    for student in &students {
        if let Some(sub) = by_student.get(student.id.as_str()) {
            let doc = canonicalize(&sub.original_text);
            token_lists.push(doc.tokens.iter().map(|t| t.normalized.clone()).collect());
            ids.push(student.id.clone());
            content_hashes.push(hex::encode(Sha256::digest(sub.original_text.as_bytes())));
            docs.push(doc);
        }
    }

    let common_cfg = CommonTextConfig::default();
    let df_applied = session.exclude_common_text && docs.len() >= MIN_DOCS_FOR_DF;
    let df = if df_applied {
        shingle_document_frequency(&token_lists, common_cfg.shingle_k)
    } else {
        HashMap::new()
    };

    // Prompt (reason Prompt) and reference text (reason Reference) as
    // separate single-reference masks so reasons stay correct.
    let mut prompt_docs: Vec<Vec<String>> = Vec::new();
    let mut reference_docs: Vec<Vec<String>> = Vec::new();
    if let Some(prompt) = session.assignment_prompt.as_deref() {
        let tokens: Vec<String> = canonicalize(prompt)
            .tokens
            .iter()
            .map(|t| t.normalized.clone())
            .collect();
        if tokens.len() >= common_cfg.prompt_min_tokens {
            prompt_docs.push(tokens);
        }
    }
    if let Some(extra) = session.excluded_reference_text.as_deref() {
        let tokens: Vec<String> = canonicalize(extra)
            .tokens
            .iter()
            .map(|t| t.normalized.clone())
            .collect();
        if tokens.len() >= common_cfg.prompt_min_tokens {
            reference_docs.push(tokens);
        }
    }
    let prompt_applied = !prompt_docs.is_empty() || !reference_docs.is_empty();

    // Per-document exclusion inputs.
    let mut common_masks: Vec<Vec<bool>> = Vec::with_capacity(docs.len());
    let mut prompt_spans: Vec<Vec<(usize, usize, ExclusionReason)>> =
        Vec::with_capacity(docs.len());
    for tokens in &token_lists {
        if df_applied {
            common_masks.push(common_token_mask(tokens, &df, docs.len(), common_cfg));
        } else {
            common_masks.push(vec![false; tokens.len()]);
        }
        let mut spans = prompt_token_mask(tokens, &prompt_docs, common_cfg.prompt_min_tokens);
        for (s, e, _) in prompt_token_mask(tokens, &reference_docs, common_cfg.prompt_min_tokens) {
            spans.push((s, e, ExclusionReason::Reference));
        }
        prompt_spans.push(spans);
    }

    let exact_cfg = ExactConfig::default();
    let modified_cfg = ModifiedConfig::default();
    let total_pairs = docs.len().saturating_mul(docs.len().saturating_sub(1)) / 2;

    // Eligible text per document: all tokens minus teacher-approved
    // (prompt/reference) exclusions. Common session text stays eligible —
    // it is reported separately, never silently removed.
    let eligible: Vec<usize> = docs
        .iter()
        .zip(prompt_spans.iter())
        .map(|(doc, spans)| {
            let teacher: Vec<(usize, usize)> = spans.iter().map(|(s, e, _)| (*s, *e)).collect();
            doc.tokens.len().saturating_sub(union_len(&teacher))
        })
        .collect();

    // Compute raw pair evidence in separate passes. This keeps progress
    // truthful and creates a clean seam for the versioned raw-evidence cache.
    let mut raw_pairs = Vec::with_capacity(total_pairs);
    emit_progress(
        &mut on_progress,
        session_id,
        AnalysisStage::Exact,
        0.05,
        0,
        total_pairs,
        0,
    );
    let mut completed_pairs = 0;
    let mut cached_pairs = 0;
    for i in 0..docs.len() {
        for j in (i + 1)..docs.len() {
            let cache_key = raw_pair_cache_key(
                session_id,
                &ids[i],
                &ids[j],
                &content_hashes[i],
                &content_hashes[j],
            );
            let cached = AnalysisRepo::raw_pair_evidence(pool, &cache_key).await?;
            let (exact, modified, cache_hit) = if let Some(raw) = cached {
                if raw.session_id != session_id
                    || raw.student_a_id != ids[i]
                    || raw.student_b_id != ids[j]
                    || raw.source_hash_a != content_hashes[i]
                    || raw.source_hash_b != content_hashes[j]
                    || raw.engine_key != raw_pair_engine_key()
                {
                    return Err(CoreError::validation(
                        "stored pair evidence metadata does not match current inputs",
                    ));
                }
                let parsed: RawPairEvidenceCache = serde_json::from_str(&raw.evidence_json).map_err(|_| {
                    CoreError::validation(
                        "stored pair evidence is corrupt; clear the affected session cache before analyzing",
                    )
                })?;
                (parsed.exact, parsed.modified, true)
            } else {
                (
                    compare_documents(&docs[i], &docs[j], exact_cfg),
                    Vec::new(),
                    false,
                )
            };
            if cache_hit {
                cached_pairs += 1;
            }
            raw_pairs.push(RawPairMatches {
                left_index: i,
                right_index: j,
                cache_key,
                cache_hit,
                exact,
                modified,
            });
            completed_pairs += 1;
            emit_progress(
                &mut on_progress,
                session_id,
                AnalysisStage::Exact,
                0.05 + 0.25 * completed_pairs as f64 / total_pairs.max(1) as f64,
                completed_pairs,
                total_pairs,
                cached_pairs,
            );
        }
    }
    emit_progress(
        &mut on_progress,
        session_id,
        AnalysisStage::Modified,
        0.30,
        0,
        total_pairs,
        cached_pairs,
    );
    completed_pairs = 0;
    for raw in &mut raw_pairs {
        if !raw.cache_hit {
            let raw_exact_a: Vec<(usize, usize)> =
                raw.exact.iter().map(|m| (m.a_start, m.a_end)).collect();
            let raw_exact_b: Vec<(usize, usize)> =
                raw.exact.iter().map(|m| (m.b_start, m.b_end)).collect();
            raw.modified = find_modified_matches(
                &token_lists[raw.left_index],
                &token_lists[raw.right_index],
                &raw_exact_a,
                &raw_exact_b,
                exact_cfg,
                modified_cfg,
            );
            let evidence_json = serde_json::to_string(&RawPairEvidenceCache {
                exact: raw.exact.clone(),
                modified: raw.modified.clone(),
            })
            .map_err(|_| CoreError::validation("could not persist raw pair evidence"))?;
            AnalysisRepo::save_raw_pair_evidence(
                pool,
                &raw.cache_key,
                session_id,
                &ids[raw.left_index],
                &ids[raw.right_index],
                &content_hashes[raw.left_index],
                &content_hashes[raw.right_index],
                &raw_pair_engine_key(),
                &evidence_json,
            )
            .await?;
        }
        completed_pairs += 1;
        emit_progress(
            &mut on_progress,
            session_id,
            AnalysisStage::Modified,
            0.30 + 0.30 * completed_pairs as f64 / total_pairs.max(1) as f64,
            completed_pairs,
            total_pairs,
            cached_pairs,
        );
    }

    let mut pairs = Vec::new();
    let mut kept_spans: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
    let mut kept_exact_spans: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
    let mut kept_modified_spans: HashMap<String, Vec<(usize, usize)>> = HashMap::new();

    emit_progress(
        &mut on_progress,
        session_id,
        AnalysisStage::Aligning,
        0.60,
        0,
        total_pairs,
        cached_pairs,
    );
    completed_pairs = 0;
    for raw in raw_pairs {
        let i = raw.left_index;
        let j = raw.right_index;
        let matches = raw.exact;
        // Scoring exclusion is teacher-approved material only; common
        // session text stays scored and is classified per passage below.
        let (counted_a, excluded_a) = apply_exclusion(&matches, true, &prompt_spans[i]);
        let (counted_b, excluded_b) = apply_exclusion(&matches, false, &prompt_spans[j]);

        // Kept passages: intersect each match with its counted ranges,
        // mapping A-side segments to B via the match offset.
        let mut passages = Vec::new();
        for m in &matches {
            let offset = m.b_start as i64 - m.a_start as i64;
            for (s, e) in intersect((m.a_start, m.a_end), &counted_a) {
                let bs = (s as i64 + offset) as usize;
                let be = (e as i64 + offset) as usize;
                if be <= docs[j].tokens.len() {
                    let (acs, ace) = char_span(&docs[i], s, e);
                    let (bcs, bce) = char_span(&docs[j], bs, be);
                    passages.push(PassageEvidence {
                        kind: PassageKind::Exact,
                        identity: None,
                        common_text: is_common_range(s, e, &common_masks[i])
                            || is_common_range(bs, be, &common_masks[j]),
                        a_token_start: s,
                        a_token_end: e,
                        b_token_start: bs,
                        b_token_end: be,
                        a_char_start: acs,
                        a_char_end: ace,
                        b_char_start: bcs,
                        b_char_end: bce,
                        tokens: e - s,
                    });
                }
            }
        }

        // Modified evidence: aligned against the same token streams,
        // clipped against raw exact spans, then exclusion-filtered.
        let modified = raw.modified;
        let mut mod_a_ranges: Vec<(usize, usize)> = Vec::new();
        let mut mod_b_ranges: Vec<(usize, usize)> = Vec::new();
        let mut mod_excluded: Vec<ExcludedEvidence> = Vec::new();
        for mm in &modified {
            let (kept_a, excl_a) = apply_exclusion(
                &[TokenMatch {
                    a_start: mm.a_start,
                    a_end: mm.a_end,
                    b_start: mm.b_start,
                    b_end: mm.b_end,
                }],
                true,
                &prompt_spans[i],
            );
            let (kept_b, excl_b) = apply_exclusion(
                &[TokenMatch {
                    a_start: mm.a_start,
                    a_end: mm.a_end,
                    b_start: mm.b_start,
                    b_end: mm.b_end,
                }],
                false,
                &prompt_spans[j],
            );
            for (s, e) in kept_a {
                // Only spans that stay reportable length after filtering.
                if e - s < modified_cfg.min_tokens {
                    continue;
                }
                // Map to B through the alignment path carried on the
                // match (relative indexing): A and B ranges can differ
                // in length when insertions/deletions exist, so
                // constant-offset mapping is invalid here.
                let mut b_positions: Vec<usize> = Vec::new();
                for t in s..e {
                    if let Some(bp) = mm.a_to_b.get(t.wrapping_sub(mm.a_start)).copied().flatten() {
                        b_positions.push(bp);
                    }
                }
                if b_positions.len() < modified_cfg.min_tokens {
                    continue;
                }
                let bs = b_positions[0];
                let be = b_positions[b_positions.len() - 1] + 1;
                for (ks, ke) in intersect((bs, be), &kept_b) {
                    if ke - ks < modified_cfg.min_tokens {
                        continue;
                    }
                    // Map back through the path: A tokens whose B
                    // position falls inside the kept B piece.
                    let mut back: Vec<usize> = Vec::new();
                    for t in s..e {
                        match mm.a_to_b.get(t.wrapping_sub(mm.a_start)).copied().flatten() {
                            Some(bp) if bp >= ks && bp < ke => back.push(t),
                            _ => {}
                        }
                    }
                    if back.len() < modified_cfg.min_tokens {
                        continue;
                    }
                    let back_s = back[0];
                    let back_e = back[back.len() - 1] + 1;
                    if back_e > docs[i].tokens.len() || ke > docs[j].tokens.len() {
                        continue;
                    }
                    let (acs, ace) = char_span(&docs[i], back_s, back_e);
                    let (bcs, bce) = char_span(&docs[j], ks, ke);
                    mod_a_ranges.push((back_s, back_e));
                    mod_b_ranges.push((ks, ke));
                    passages.push(PassageEvidence {
                        kind: PassageKind::Modified,
                        identity: Some(mm.identity),
                        common_text: is_common_range(back_s, back_e, &common_masks[i])
                            || is_common_range(ks, ke, &common_masks[j]),
                        a_token_start: back_s,
                        a_token_end: back_e,
                        b_token_start: ks,
                        b_token_end: ke,
                        a_char_start: acs,
                        a_char_end: ace,
                        b_char_start: bcs,
                        b_char_end: bce,
                        tokens: back_e - back_s,
                    });
                }
            }
            for (side, spans, doc) in [
                (ExclusionSide::A, &excl_a, &docs[i]),
                (ExclusionSide::B, &excl_b, &docs[j]),
            ] {
                for span in spans {
                    let (cs, ce) = char_span(doc, span.token_start, span.token_end);
                    mod_excluded.push(ExcludedEvidence {
                        side,
                        token_start: span.token_start,
                        token_end: span.token_end,
                        char_start: cs,
                        char_end: ce,
                        reason: span.reason,
                        tokens: span.token_end - span.token_start,
                    });
                }
            }
        }
        passages.sort_by_key(|p| (p.a_token_start, p.b_token_start));

        // Coverage unions exact + modified kept spans over eligible text.
        let mut union_a = counted_a.clone();
        union_a.extend(mod_a_ranges.iter().copied());
        let mut union_b = counted_b.clone();
        union_b.extend(mod_b_ranges.iter().copied());
        let coverage_a = eligible_coverage(&union_a, eligible[i]);
        let coverage_b = eligible_coverage(&union_b, eligible[j]);
        let exact_coverage_a = eligible_coverage(&counted_a, eligible[i]);
        let exact_coverage_b = eligible_coverage(&counted_b, eligible[j]);
        let modified_coverage_a = eligible_coverage(&mod_a_ranges, eligible[i]);
        let modified_coverage_b = eligible_coverage(&mod_b_ranges, eligible[j]);

        let mut excluded = Vec::new();
        for (side, spans, doc) in [
            (ExclusionSide::A, &excluded_a, &docs[i]),
            (ExclusionSide::B, &excluded_b, &docs[j]),
        ] {
            for span in spans {
                let (cs, ce) = char_span(doc, span.token_start, span.token_end);
                excluded.push(ExcludedEvidence {
                    side,
                    token_start: span.token_start,
                    token_end: span.token_end,
                    char_start: cs,
                    char_end: ce,
                    reason: span.reason,
                    tokens: span.token_end - span.token_start,
                });
            }
        }
        excluded.sort_by_key(|e| (e.side as u8, e.token_start));
        excluded.extend(mod_excluded);
        excluded.sort_by_key(|e| (e.side as u8, e.token_start));

        for &(s, e) in &counted_a {
            kept_spans.entry(ids[i].clone()).or_default().push((s, e));
            kept_exact_spans
                .entry(ids[i].clone())
                .or_default()
                .push((s, e));
        }
        for &(s, e) in &counted_b {
            kept_spans.entry(ids[j].clone()).or_default().push((s, e));
            kept_exact_spans
                .entry(ids[j].clone())
                .or_default()
                .push((s, e));
        }
        for &(s, e) in &mod_a_ranges {
            kept_spans.entry(ids[i].clone()).or_default().push((s, e));
            kept_modified_spans
                .entry(ids[i].clone())
                .or_default()
                .push((s, e));
        }
        for &(s, e) in &mod_b_ranges {
            kept_spans.entry(ids[j].clone()).or_default().push((s, e));
            kept_modified_spans
                .entry(ids[j].clone())
                .or_default()
                .push((s, e));
        }

        pairs.push(PairAnalysis {
            a_student_id: ids[i].clone(),
            b_student_id: ids[j].clone(),
            coverage_a,
            coverage_b,
            exact_coverage_a,
            exact_coverage_b,
            modified_coverage_a,
            modified_coverage_b,
            passages,
            excluded,
        });
        completed_pairs += 1;
        emit_progress(
            &mut on_progress,
            session_id,
            AnalysisStage::Aligning,
            0.60 + 0.25 * completed_pairs as f64 / total_pairs.max(1) as f64,
            completed_pairs,
            total_pairs,
            cached_pairs,
        );
    }

    emit_progress(
        &mut on_progress,
        session_id,
        AnalysisStage::Scoring,
        0.90,
        total_pairs,
        total_pairs,
        cached_pairs,
    );

    let totals: HashMap<&str, usize> = ids
        .iter()
        .zip(docs.iter())
        .map(|(id, d)| (id.as_str(), d.tokens.len()))
        .collect();
    let eligible_by_student: HashMap<&str, usize> = ids
        .iter()
        .zip(eligible.iter())
        .map(|(id, e)| (id.as_str(), *e))
        .collect();
    let mut per_student: Vec<StudentExactCoverage> = students
        .iter()
        .map(|s| {
            let total = totals.get(s.id.as_str()).copied().unwrap_or(0);
            let eligible_tokens = eligible_by_student.get(s.id.as_str()).copied().unwrap_or(0);
            let matched = kept_spans
                .get(&s.id)
                .map(|r| union_len(r))
                .unwrap_or(0)
                .min(eligible_tokens);
            StudentExactCoverage {
                student_id: s.id.clone(),
                coverage: eligible_coverage(
                    &kept_spans.get(&s.id).cloned().unwrap_or_default(),
                    eligible_tokens,
                ),
                exact_coverage: eligible_coverage(
                    &kept_exact_spans.get(&s.id).cloned().unwrap_or_default(),
                    eligible_tokens,
                ),
                modified_coverage: eligible_coverage(
                    &kept_modified_spans.get(&s.id).cloned().unwrap_or_default(),
                    eligible_tokens,
                ),
                matched_tokens: matched,
                total_tokens: total,
                eligible_tokens,
            }
        })
        .collect();
    per_student.sort_by(|x, y| x.student_id.cmp(&y.student_id));

    let mut compared_libraries: Vec<ComparedLibrary> = Vec::new();
    for (library, _) in &selected_references {
        if let Some(existing) = compared_libraries
            .iter_mut()
            .find(|item| item.id == library.id)
        {
            existing.source_count += 1;
        } else {
            compared_libraries.push(ComparedLibrary {
                id: library.id.clone(),
                name: library.name.clone(),
                source_session_name: library.source_session_name.clone(),
                source_count: 1,
            });
        }
    }
    let historical_total = ids.len().saturating_mul(selected_references.len());
    let mut historical_matches = Vec::new();
    if historical_total > 0 {
        emit_progress(
            &mut on_progress,
            session_id,
            AnalysisStage::Historical,
            0.90,
            0,
            historical_total,
            cached_pairs,
        );
        let mut historical_completed = 0;
        for (index, student_id) in ids.iter().enumerate() {
            for (library, reference) in &selected_references {
                let comparison = analyze_historical_pair(
                    student_id,
                    &docs[index],
                    &common_masks[index],
                    library,
                    reference,
                    &prompt_docs,
                    &reference_docs,
                );
                if !comparison.passages.is_empty() {
                    historical_matches.push(comparison);
                }
                historical_completed += 1;
                emit_progress(
                    &mut on_progress,
                    session_id,
                    AnalysisStage::Historical,
                    0.90 + 0.08 * historical_completed as f64 / historical_total as f64,
                    historical_completed,
                    historical_total,
                    cached_pairs,
                );
            }
        }
    }

    Ok(ExactAnalysis {
        fingerprint_version: FINGERPRINT_VERSION,
        normalization_version: NORMALIZATION_VERSION,
        common_text_version: COMMON_TEXT_VERSION,
        modified_version: MODIFIED_VERSION,
        common_text_applied: df_applied,
        prompt_applied,
        pairs,
        per_student,
        compared_libraries,
        historical_matches,
    })
}

#[cfg(test)]
mod tests {
    use super::eligible_coverage;
    use provenance_match::union_len;

    #[test]
    fn combined_coverage_unions_evidence_types_instead_of_adding_them() {
        let exact = [(1, 5), (10, 14)];
        let modified = [(3, 7), (8, 12)];
        let exact_pct = eligible_coverage(&exact, 20).expect("eligible exact coverage");
        let modified_pct = eligible_coverage(&modified, 20).expect("eligible modified coverage");
        let mut combined = exact.to_vec();
        combined.extend(modified);
        let combined_pct = eligible_coverage(&combined, 20).expect("eligible combined coverage");

        assert_eq!(exact_pct, 40.0);
        assert_eq!(modified_pct, 40.0);
        assert_eq!(combined_pct, union_len(&combined) as f64 / 20.0 * 100.0);
        assert_eq!(combined_pct, 60.0);
        assert!(combined_pct < exact_pct + modified_pct);
    }
}

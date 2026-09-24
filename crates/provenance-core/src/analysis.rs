//! Phase 5: exact-only session analysis over current submissions.
//! Phase 6: prompt + session-frequency exclusion applied before scoring.
//! Computed on demand (fast for class sizes); persisted reports arrive in Phase 16.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::domain::Submission;
use crate::error::CoreError;
use crate::service::list_students;
use crate::storage::{SessionRepo, SubmissionRepo};
use provenance_match::{
    apply_exclusion, canonicalize, common_token_mask, compare_documents, find_modified_matches,
    is_common_range, prompt_token_mask, shingle_document_frequency, union_len, CommonTextConfig,
    ExactConfig, ExclusionReason, ModifiedConfig, TokenMatch, COMMON_TEXT_VERSION,
    FINGERPRINT_VERSION, MODIFIED_VERSION, NORMALIZATION_VERSION,
};

/// Document-frequency exclusion needs statistical meaning: below this many
/// analyzed documents every shared passage is case evidence, so DF is off.
pub const MIN_DOCS_FOR_DF: usize = 4;

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
    pub passages: Vec<PassageEvidence>,
    pub excluded: Vec<ExcludedEvidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StudentExactCoverage {
    pub student_id: String,
    /// `None` means insufficient assessable text — never a clean 0%.
    pub coverage: Option<f64>,
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

/// Analyze one session's current submissions: exact engine with Phase 6
/// prompt + frequency exclusion applied before scoring.
pub async fn analyze_session_exact(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<ExactAnalysis, CoreError> {
    let session = SessionRepo::get(pool, session_id).await?;
    let students = list_students(pool, session_id).await?;
    let submissions = SubmissionRepo::list_by_session(pool, session_id).await?;

    let by_student: HashMap<&str, &Submission> = submissions
        .iter()
        .filter(|s| !s.original_text.trim().is_empty())
        .map(|s| (s.student_id.as_str(), s))
        .collect();

    // Canonicalize every submission that has text.
    let mut ids: Vec<String> = Vec::new();
    let mut docs: Vec<provenance_match::CanonicalDocument> = Vec::new();
    let mut token_lists: Vec<Vec<String>> = Vec::new();
    for student in &students {
        if let Some(sub) = by_student.get(student.id.as_str()) {
            let doc = canonicalize(&sub.original_text);
            token_lists.push(doc.tokens.iter().map(|t| t.normalized.clone()).collect());
            ids.push(student.id.clone());
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

    let mut pairs = Vec::new();
    let mut kept_spans: HashMap<String, Vec<(usize, usize)>> = HashMap::new();

    for i in 0..docs.len() {
        for j in (i + 1)..docs.len() {
            let matches: Vec<TokenMatch> = compare_documents(&docs[i], &docs[j], exact_cfg);
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
            let raw_exact_a: Vec<(usize, usize)> =
                matches.iter().map(|m| (m.a_start, m.a_end)).collect();
            let raw_exact_b: Vec<(usize, usize)> =
                matches.iter().map(|m| (m.b_start, m.b_end)).collect();
            let modified = find_modified_matches(
                &token_lists[i],
                &token_lists[j],
                &raw_exact_a,
                &raw_exact_b,
                exact_cfg,
                modified_cfg,
            );
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
                        if let Some(bp) =
                            mm.a_to_b.get(t.wrapping_sub(mm.a_start)).copied().flatten()
                        {
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

            for (s, e) in counted_a {
                kept_spans.entry(ids[i].clone()).or_default().push((s, e));
            }
            for (s, e) in counted_b {
                kept_spans.entry(ids[j].clone()).or_default().push((s, e));
            }
            for (s, e) in mod_a_ranges {
                kept_spans.entry(ids[i].clone()).or_default().push((s, e));
            }
            for (s, e) in mod_b_ranges {
                kept_spans.entry(ids[j].clone()).or_default().push((s, e));
            }

            pairs.push(PairAnalysis {
                a_student_id: ids[i].clone(),
                b_student_id: ids[j].clone(),
                coverage_a,
                coverage_b,
                passages,
                excluded,
            });
        }
    }

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
                matched_tokens: matched,
                total_tokens: total,
                eligible_tokens,
            }
        })
        .collect();
    per_student.sort_by(|x, y| x.student_id.cmp(&y.student_id));

    Ok(ExactAnalysis {
        fingerprint_version: FINGERPRINT_VERSION,
        normalization_version: NORMALIZATION_VERSION,
        common_text_version: COMMON_TEXT_VERSION,
        modified_version: MODIFIED_VERSION,
        common_text_applied: df_applied,
        prompt_applied,
        pairs,
        per_student,
    })
}

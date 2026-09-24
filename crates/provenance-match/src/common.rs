//! Phase 6: common-text and assignment-template filtering.
//!
//! Product contract (plan_v1 evidence-versus-judgement rules): frequency
//! alone must never erase a matched answer. Two separate mechanisms:
//! 1. Prompt/reference text configured on the session — any run of at least
//!    `prompt_min_tokens` matching it is excluded from scoring. Only
//!    teacher-provided material (or explicit teacher-approved exclusions)
//!    may reduce the score.
//! 2. Session-level shingle document frequency — a shingle appearing in at
//!    least `df_threshold` fraction of session documents marks its tokens
//!    as common session text. This is an explanatory CLASSIFICATION attached
//!    to scored evidence, never a score exclusion: common matches stay
//!    visible and counted.
//!
//! Prompt/reference spans are reported separately (never silently dropped)
//! and never counted toward coverage. Versioned via `COMMON_TEXT_VERSION`.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::exact::{find_shingle_runs, shingle_hash, union_len, TokenMatch};

/// Bump when any rule or default below changes.
/// v2: exact interval segmentation with reason precedence (6.1.2).
/// v3: frequency-derived common text is classification only — only
///     teacher-provided prompt/reference spans exclude from scoring.
pub const COMMON_TEXT_VERSION: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CommonTextConfig {
    /// Master switch (per-session setting): compute document frequency and
    /// label common session text. Never affects the score.
    pub enabled: bool,
    /// Fraction of documents (0.0–1.0) at/above which a shingle is common.
    pub df_threshold: f64,
    /// Shingle length for document frequency.
    pub shingle_k: usize,
    /// Minimum run length to exclude via prompt/reference text.
    pub prompt_min_tokens: usize,
    pub version: u32,
}

impl Default for CommonTextConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            df_threshold: 0.8,
            shingle_k: 5,
            prompt_min_tokens: 5,
            version: COMMON_TEXT_VERSION,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExclusionReason {
    /// Matches the session's assignment question/instructions.
    Prompt,
    /// Matches additional excluded reference text.
    Reference,
    /// Appears in a large fraction of session submissions.
    CommonSessionText,
}

impl ExclusionReason {
    /// Specificity order for overlaps: teacher material beats statistics.
    /// Prompt (the assignment itself) outranks extra reference text, which
    /// outranks frequency-derived common session text.
    fn precedence(self) -> u8 {
        match self {
            Self::Prompt => 0,
            Self::Reference => 1,
            Self::CommonSessionText => 2,
        }
    }

    fn from_precedence(rank: u8) -> Self {
        match rank {
            0 => Self::Prompt,
            1 => Self::Reference,
            _ => Self::CommonSessionText,
        }
    }
}

/// A matched span excluded from scoring, with its reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExcludedSpan {
    pub token_start: usize,
    pub token_end: usize,
    pub reason: ExclusionReason,
}

/// Stable shingle hashes of one normalized token stream (deduped per doc,
/// so repeated phrases count once per document for DF).
fn doc_shingles(tokens: &[String], k: usize) -> HashSet<u64> {
    let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
    let mut set = HashSet::new();
    if tokens.len() < k || k == 0 {
        return set;
    }
    for i in 0..=refs.len() - k {
        set.insert(shingle_hash(&refs[i..i + k]));
    }
    set
}

/// Document frequency of every shingle across the session's documents.
#[must_use]
pub fn shingle_document_frequency(docs: &[Vec<String>], k: usize) -> HashMap<u64, usize> {
    let mut df: HashMap<u64, usize> = HashMap::new();
    for tokens in docs {
        for hash in doc_shingles(tokens, k) {
            *df.entry(hash).or_default() += 1;
        }
    }
    df
}

/// Token mask: `true` where the token is covered by a common shingle.
#[must_use]
pub fn common_token_mask(
    tokens: &[String],
    df: &HashMap<u64, usize>,
    doc_count: usize,
    config: CommonTextConfig,
) -> Vec<bool> {
    let mut mask = vec![false; tokens.len()];
    if tokens.len() < config.shingle_k || config.shingle_k == 0 || doc_count == 0 {
        return mask;
    }
    let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
    let threshold = (config.df_threshold * doc_count as f64).ceil() as usize;
    for i in 0..=refs.len() - config.shingle_k {
        let hash = shingle_hash(&refs[i..i + config.shingle_k]);
        if df.get(&hash).copied().unwrap_or(0) >= threshold.max(1) {
            mask[i..i + config.shingle_k].fill(true);
        }
    }
    mask
}

/// Token mask of runs matching prompt/reference text (min length applies).
/// Uses exhaustive shingle matching: prompt texts are short and every
/// run of at least `prompt_min_tokens` must be caught.
#[must_use]
pub fn prompt_token_mask(
    tokens: &[String],
    references: &[Vec<String>],
    prompt_min_tokens: usize,
) -> Vec<(usize, usize, ExclusionReason)> {
    let mut spans = Vec::new();
    for (ri, reference) in references.iter().enumerate() {
        if reference.len() < prompt_min_tokens || tokens.len() < prompt_min_tokens {
            continue;
        }
        let reason = if ri == 0 {
            ExclusionReason::Prompt
        } else {
            ExclusionReason::Reference
        };
        let k = prompt_min_tokens.min(5);
        for m in find_shingle_runs(tokens, reference, k, prompt_min_tokens) {
            spans.push((m.a_start, m.a_end, reason));
        }
    }
    spans
}

/// Apply teacher-approved exclusion to one side of a match list.
/// Only prompt/reference spans reduce the score; frequency-derived common
/// text never does (see [`common_fraction`] for its classification role).
/// Returns `(counted_ranges, excluded_spans)` in that document's coordinates.
#[must_use]
pub fn apply_exclusion(
    matches: &[TokenMatch],
    side_is_a: bool,
    prompt_spans: &[(usize, usize, ExclusionReason)],
) -> (Vec<(usize, usize)>, Vec<ExcludedSpan>) {
    let mut excluded: Vec<ExcludedSpan> = Vec::new();
    let mut counted: Vec<(usize, usize)> = Vec::new();
    for m in matches {
        let (start, end) = if side_is_a {
            (m.a_start, m.a_end)
        } else {
            (m.b_start, m.b_end)
        };
        // Subtract teacher-approved spans from [start, end).
        let mut cuts: Vec<(usize, usize, ExclusionReason)> = Vec::new();
        for &(ps, pe, reason) in prompt_spans {
            // CommonSessionText must never arrive here: frequency is
            // classification, not exclusion. Guard the contract explicitly.
            debug_assert_ne!(
                reason,
                ExclusionReason::CommonSessionText,
                "common text must not exclude from scoring"
            );
            if reason == ExclusionReason::CommonSessionText {
                continue;
            }
            let s = ps.max(start);
            let e = pe.min(end);
            if s < e {
                cuts.push((s, e, reason));
            }
        }
        cuts.sort_by_key(|(s, e, _)| (*s, *e));
        // Exact interval segmentation: split at every cut boundary and
        // label each elementary interval with the highest-precedence
        // (most specific) covering reason. Adjacent intervals with the
        // same reason fuse; different reasons never merge, so no span
        // ever carries an incorrect reason.
        let mut points: Vec<usize> = Vec::with_capacity(cuts.len() * 2);
        for (s, e, _) in &cuts {
            points.push(*s);
            points.push(*e);
        }
        points.sort_unstable();
        points.dedup();
        let mut merged: Vec<(usize, usize, ExclusionReason)> = Vec::new();
        for window in points.windows(2) {
            let (s, e) = (window[0], window[1]);
            let best = cuts
                .iter()
                .filter(|(cs, ce, _)| *cs <= s && e <= *ce)
                .map(|(_, _, r)| r.precedence())
                .min();
            if let Some(rank) = best {
                let reason = ExclusionReason::from_precedence(rank);
                if let Some(last) = merged.last_mut() {
                    if last.1 == s && last.2 == reason {
                        last.1 = e;
                        continue;
                    }
                }
                merged.push((s, e, reason));
            }
        }
        // Complement within [start, end) is counted.
        let mut cursor = start;
        for (s, e, reason) in &merged {
            if *s > cursor {
                counted.push((cursor, *s));
            }
            excluded.push(ExcludedSpan {
                token_start: *s,
                token_end: *e,
                reason: *reason,
            });
            cursor = cursor.max(*e);
        }
        if cursor < end {
            counted.push((cursor, end));
        }
    }
    // Merge adjacent counted ranges for a clean union input.
    counted.sort();
    let mut flat: Vec<(usize, usize)> = Vec::new();
    for (s, e) in counted {
        if let Some(last) = flat.last_mut() {
            if s <= last.1 {
                last.1 = last.1.max(e);
                continue;
            }
        }
        flat.push((s, e));
    }
    (flat, excluded)
}

/// Fraction of `[start, end)` covered by the common-session-text mask.
/// Used to classify (not exclude) scored passages: a passage whose
/// majority is shared session-wide still counts, but is labelled.
#[must_use]
pub fn common_fraction(start: usize, end: usize, common_mask: &[bool]) -> f64 {
    if end <= start {
        return 0.0;
    }
    let end = end.min(common_mask.len());
    let start = start.min(end);
    let total = end - start;
    if total == 0 {
        return 0.0;
    }
    let common = common_mask[start..end].iter().filter(|m| **m).count();
    common as f64 / total as f64
}

/// Classify a scored token range as common session text when at least half
/// of its tokens are shared session-wide.
#[must_use]
pub fn is_common_range(start: usize, end: usize, common_mask: &[bool]) -> bool {
    common_fraction(start, end, common_mask) >= 0.5
}

/// Coverage of counted ranges over a token total (0.0–100.0).
#[must_use]
pub fn filtered_coverage(counted: &[(usize, usize)], total_tokens: usize) -> f64 {
    if total_tokens == 0 {
        return 0.0;
    }
    union_len(counted) as f64 / total_tokens as f64 * 100.0
}

#[cfg(test)]
#[path = "common_tests.rs"]
mod common_tests;

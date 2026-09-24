//! Phase 7: modified / near-exact copy detection.
//!
//! Pipeline per document pair:
//! 1. Seed candidate regions from shared exact fingerprints (anchors) and,
//!    as a fallback, bag-of-token Jaccard over sliding windows — so passages
//!    with many small edits and no shared 5-gram are still found.
//! 2. Token-level Smith–Waterman local alignment with affine gaps (Gotoh)
//!    per region: exact (+2), fuzzy (+1 at ≥80% char similarity),
//!    mismatch (−1), gap open (−1.5) + extend (−0.5).
//! 3. Keep alignments with length ≥ `min_tokens` and identity ≥
//!    `min_identity`; clip against exact-match spans; filter short remainders.
//!
//! Everything is deterministic: no randomization, sorted outputs.
//! Versioned via `MODIFIED_VERSION`.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::exact::{winnow, ExactConfig, FingerprintIndex};

/// Bump when any rule, score, or default below changes.
pub const MODIFIED_VERSION: u32 = 1;

/// Full-document alignment cap: pairs at/below this size align end to end
/// (Smith–Waterman is quadratic, trivial at class sizes). Larger documents
/// use seeded candidate regions.
pub const FULL_ALIGNMENT_MAX_TOKENS: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModifiedConfig {
    /// Candidate window length in tokens for the Jaccard fallback.
    pub window_tokens: usize,
    /// Stride between candidate windows.
    pub window_stride: usize,
    /// Jaccard threshold to open a candidate region.
    pub jaccard_threshold: f64,
    /// Char similarity at/above which two tokens fuzzily match.
    pub fuzzy_threshold: f64,
    /// Smith–Waterman scores.
    pub match_score: f64,
    pub fuzzy_score: f64,
    pub mismatch_score: f64,
    /// Cost of the first token in a gap block (includes that token).
    pub gap_score: f64,
    /// Cost of each further token in the same gap block.
    pub gap_extend_score: f64,
    /// Minimum kept alignment length in tokens.
    pub min_tokens: usize,
    /// Minimum exact-token identity of a kept alignment.
    pub min_identity: f64,
    /// Anchor expansion around fingerprint seeds, in tokens.
    pub anchor_radius: usize,
    pub version: u32,
}

impl Default for ModifiedConfig {
    fn default() -> Self {
        Self {
            window_tokens: 24,
            window_stride: 12,
            jaccard_threshold: 0.35,
            fuzzy_threshold: 0.8,
            match_score: 2.0,
            fuzzy_score: 1.0,
            mismatch_score: -1.0,
            // Affine gaps: a lone inserted/deleted word costs about as much
            // as before (-2.0 total), while a consecutive block costs little
            // more — so deleted phrases bridge instead of splitting evidence.
            gap_score: -1.5,
            gap_extend_score: -0.5,
            min_tokens: 10,
            min_identity: 0.6,
            anchor_radius: 40,
            version: MODIFIED_VERSION,
        }
    }
}

/// One modified-copy passage with alignment quality.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModifiedMatch {
    pub a_start: usize,
    pub a_end: usize,
    pub b_start: usize,
    pub b_end: usize,
    /// Fraction of aligned columns that are exact token matches.
    pub identity: f64,
    /// Normalized alignment score in 0.0–1.0 (vs all-exact).
    pub confidence: f64,
    /// Alignment path: B position for each A token in `a_start..a_end`
    /// (`None` where the A token aligns to a gap). Callers MUST use this
    /// for A↔B subsegment mapping — the A and B ranges can differ in
    /// length when insertions/deletions exist, so constant-offset mapping
    /// is invalid. Internal only (never serialized into reports).
    pub a_to_b: Vec<Option<usize>>,
}

impl ModifiedMatch {
    #[must_use]
    pub fn len(&self) -> usize {
        self.a_end - self.a_start
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Match,
    Fuzzy,
    Subst,
    Ins,
    Del,
}

/// Normalized Levenshtein similarity in 0.0–1.0 (1.0 = identical).
pub fn token_similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    let ac: Vec<char> = a.chars().collect();
    let bc: Vec<char> = b.chars().collect();
    if ac.is_empty() || bc.is_empty() {
        return 0.0;
    }
    let mut prev: Vec<usize> = (0..=bc.len()).collect();
    for i in 1..=ac.len() {
        let mut cur = vec![i; bc.len() + 1];
        for j in 1..=bc.len() {
            let cost = usize::from(ac[i - 1] != bc[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        prev = cur;
    }
    let dist = prev[bc.len()];
    1.0 - dist as f64 / ac.len().max(bc.len()) as f64
}

fn pair_score(a: &str, b: &str, config: ModifiedConfig) -> (f64, Op) {
    if a == b {
        return (config.match_score, Op::Match);
    }
    // Cheap necessary condition for similarity ≥ threshold: length gap
    // alone must not exceed the allowed edit budget. Skips Levenshtein
    // on most cells with zero recall loss.
    let (la, lb) = (a.chars().count(), b.chars().count());
    let max = la.max(lb);
    let allowed = ((1.0 - config.fuzzy_threshold) * max as f64).floor() as usize;
    if la.abs_diff(lb) > allowed || max == 0 {
        return (config.mismatch_score, Op::Subst);
    }
    if token_similarity(a, b) >= config.fuzzy_threshold {
        (config.fuzzy_score, Op::Fuzzy)
    } else {
        (config.mismatch_score, Op::Subst)
    }
}

/// Minimum endpoint score worth tracing back. A qualifying span (length ≥ 10,
/// identity ≥ 0.6) always scores ≥ 6.0 under these affine costs, so no
/// qualifying alignment is ever skipped.
const ALIGN_MIN_SCORE: f64 = 3.0;
/// Cap on traced endpoints per region (highest first, deterministic ties).
const ALIGN_MAX_ENDPOINTS: usize = 32;

/// Smith–Waterman local alignments with affine gaps (Gotoh) over two
/// token slices. Returns every qualifying endpoint
/// `(score, ops, a_start, b_start)` with absolute coordinates, best first.
fn smith_waterman(
    a: &[String],
    b: &[String],
    a_off: usize,
    b_off: usize,
    config: ModifiedConfig,
) -> Vec<(f64, Vec<Op>, usize, usize)> {
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum Mat {
        M,
        Dx,
        Dy,
    }
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let (n, m) = (a.len(), b.len());
    let mut best = vec![vec![0.0f64; m + 1]; n + 1];
    let mut which: Vec<Vec<Mat>> = vec![vec![Mat::M; m + 1]; n + 1];
    let mut m_score = vec![vec![0.0f64; m + 1]; n + 1];
    let mut dx_score = vec![vec![0.0f64; m + 1]; n + 1];
    let mut dy_score = vec![vec![0.0f64; m + 1]; n + 1];
    let mut trace_m: Vec<Vec<Option<(Mat, Op)>>> = vec![vec![None; m + 1]; n + 1];
    let mut trace_dx: Vec<Vec<Option<Mat>>> = vec![vec![None; m + 1]; n + 1];
    let mut trace_dy: Vec<Vec<Option<Mat>>> = vec![vec![None; m + 1]; n + 1];
    for i in 1..=n {
        for j in 1..=m {
            // Deletion: consume a[i-1], gap in b.
            let open_dx = m_score[i - 1][j] + config.gap_score;
            let ext_dx = dx_score[i - 1][j] + config.gap_extend_score;
            if ext_dx > open_dx && ext_dx > 0.0 {
                dx_score[i][j] = ext_dx;
                trace_dx[i][j] = Some(Mat::Dx);
            } else if open_dx > 0.0 {
                dx_score[i][j] = open_dx;
                trace_dx[i][j] = Some(Mat::M);
            }
            // Insertion: consume b[j-1], gap in a.
            let open_dy = m_score[i][j - 1] + config.gap_score;
            let ext_dy = dy_score[i][j - 1] + config.gap_extend_score;
            if ext_dy > open_dy && ext_dy > 0.0 {
                dy_score[i][j] = ext_dy;
                trace_dy[i][j] = Some(Mat::Dy);
            } else if open_dy > 0.0 {
                dy_score[i][j] = open_dy;
                trace_dy[i][j] = Some(Mat::M);
            }
            // Match/substitution from any matrix, preferring M, Dx, Dy.
            let (ps, pop) = pair_score(&a[i - 1], &b[j - 1], config);
            let from_m = m_score[i - 1][j - 1] + ps;
            let from_dx = dx_score[i - 1][j - 1] + ps;
            let from_dy = dy_score[i - 1][j - 1] + ps;
            let mut cell = (from_m, (Mat::M, pop));
            if from_dx > cell.0 {
                cell = (from_dx, (Mat::Dx, pop));
            }
            if from_dy > cell.0 {
                cell = (from_dy, (Mat::Dy, pop));
            }
            if cell.0 > 0.0 {
                m_score[i][j] = cell.0;
                trace_m[i][j] = Some(cell.1);
            }
            let mut value = (m_score[i][j], Mat::M);
            if dx_score[i][j] > value.0 {
                value = (dx_score[i][j], Mat::Dx);
            }
            if dy_score[i][j] > value.0 {
                value = (dy_score[i][j], Mat::Dy);
            }
            best[i][j] = value.0;
            which[i][j] = value.1;
        }
    }
    // Endpoints worth tracing, best first with deterministic tie-breaks.
    let mut ends: Vec<(Mat, usize, usize)> = Vec::new();
    for i in 1..=n {
        for j in 1..=m {
            if best[i][j] >= ALIGN_MIN_SCORE {
                ends.push((which[i][j], i, j));
            }
        }
    }
    ends.sort_by(|&(m1, a1, b1), &(m2, a2, b2)| {
        best[a2][b2]
            .partial_cmp(&best[a1][b1])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then((m1, a1, b1).cmp(&(m2, a2, b2)))
    });
    ends.truncate(ALIGN_MAX_ENDPOINTS);

    let mut out = Vec::new();
    for (mat, bi, bj) in ends {
        let mut ops = Vec::new();
        let (mut i, mut j, mut state) = (bi, bj, mat);
        loop {
            match state {
                Mat::M => match trace_m[i][j] {
                    None => break,
                    Some((prev, op)) => {
                        ops.push(op);
                        state = prev;
                        i -= 1;
                        j -= 1;
                    }
                },
                Mat::Dx => match trace_dx[i][j] {
                    None => break,
                    Some(prev) => {
                        ops.push(Op::Del);
                        state = prev;
                        i -= 1;
                    }
                },
                Mat::Dy => match trace_dy[i][j] {
                    None => break,
                    Some(prev) => {
                        ops.push(Op::Ins);
                        state = prev;
                        j -= 1;
                    }
                },
            }
        }
        ops.reverse();
        out.push((best[bi][bj], ops, a_off + i, b_off + j));
    }
    out
}

/// Bag-of-tokens Jaccard similarity between two slices.
fn jaccard(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let set_a: HashSet<&str> = a.iter().map(String::as_str).collect();
    let set_b: HashSet<&str> = b.iter().map(String::as_str).collect();
    let inter = set_a.intersection(&set_b).count();
    let union = set_a.len() + set_b.len() - inter;
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

/// Candidate regions as `(a_start, a_end, b_start, b_end)` token ranges.
fn candidate_regions(
    a: &[String],
    b: &[String],
    exact_cfg: ExactConfig,
    config: ModifiedConfig,
) -> Vec<(usize, usize, usize, usize)> {
    let mut regions: Vec<(usize, usize, usize, usize)> = Vec::new();

    // Anchors from shared fingerprints, expanded both ways.
    let mut index = FingerprintIndex::new();
    index.add(1, b, exact_cfg);
    let mut anchored = HashSet::new();
    for fp in winnow(a, exact_cfg) {
        for &(_, pb) in index.positions(fp.hash) {
            let anchor = (
                fp.pos.saturating_sub(config.anchor_radius),
                (fp.pos + exact_cfg.k + config.anchor_radius).min(a.len()),
                pb.saturating_sub(config.anchor_radius),
                (pb + exact_cfg.k + config.anchor_radius).min(b.len()),
            );
            if anchored.insert(anchor) {
                regions.push(anchor);
            }
        }
    }

    // Jaccard fallback over sliding windows. The window must admit a
    // reportable span, otherwise alignment could never qualify.
    let w = config.window_tokens.min(a.len()).min(b.len());
    if w >= config.min_tokens && w > 0 {
        let step = config.window_stride.max(1);
        let mut a_starts: Vec<usize> = (0..a.len()).step_by(step).collect();
        let mut b_starts: Vec<usize> = (0..b.len()).step_by(step).collect();
        if a_starts.last() != Some(&(a.len().saturating_sub(w))) {
            a_starts.push(a.len().saturating_sub(w));
        }
        if b_starts.last() != Some(&(b.len().saturating_sub(w))) {
            b_starts.push(b.len().saturating_sub(w));
        }
        for &sa in &a_starts {
            let ea = (sa + w).min(a.len());
            if ea - sa < config.min_tokens {
                continue;
            }
            for &sb in &b_starts {
                let eb = (sb + w).min(b.len());
                if eb - sb < config.min_tokens {
                    continue;
                }
                if jaccard(&a[sa..ea], &b[sb..eb]) >= config.jaccard_threshold {
                    regions.push((sa, ea, sb, eb));
                }
            }
        }
    }
    regions.sort();
    regions.dedup();
    regions
}

/// Align one region pair into modified matches, if any qualify.
/// Returned matches carry their alignment path (`a_to_b`) for exact
/// subsegment mapping downstream.
fn align_region(
    a: &[String],
    b: &[String],
    region: (usize, usize, usize, usize),
    config: ModifiedConfig,
) -> Vec<ModifiedMatch> {
    let (sa, ea, sb, eb) = region;
    let mut found = Vec::new();
    for (score, ops, a_start, b_start) in smith_waterman(&a[sa..ea], &b[sb..eb], sa, sb, config) {
        // Walk ops to absolute ends + A→B map.
        let mut ai = a_start;
        let mut bi = b_start;
        let mut exact = 0usize;
        let mut columns = 0usize;
        let mut a_to_b = vec![None; a.len()];
        for op in &ops {
            match op {
                Op::Match => {
                    exact += 1;
                    columns += 1;
                    a_to_b[ai] = Some(bi);
                    ai += 1;
                    bi += 1;
                }
                Op::Fuzzy | Op::Subst => {
                    columns += 1;
                    a_to_b[ai] = Some(bi);
                    ai += 1;
                    bi += 1;
                }
                Op::Del => {
                    columns += 1;
                    ai += 1;
                }
                Op::Ins => {
                    columns += 1;
                    bi += 1;
                }
            }
        }
        let a_end = ai;
        let b_end = bi;
        let len = a_end - a_start;
        if len < config.min_tokens || b_end - b_start < config.min_tokens {
            continue;
        }
        let identity = exact as f64 / columns.max(1) as f64;
        if identity < config.min_identity {
            continue;
        }
        let max_score = 2.0 * columns.max(1) as f64;
        found.push(ModifiedMatch {
            a_start,
            a_end,
            b_start,
            b_end,
            identity,
            confidence: (score / max_score).clamp(0.0, 1.0),
            a_to_b: a_to_b[a_start..a_end].to_vec(),
        });
    }
    found
}

/// Find modified-copy passages, clipping against exact spans.
/// `exact_a`/`exact_b` are the exact-match token ranges per side; kept
/// modified segments below `min_tokens` are dropped. Deterministic output.
#[must_use]
pub fn find_modified_matches(
    a: &[String],
    b: &[String],
    exact_a: &[(usize, usize)],
    exact_b: &[(usize, usize)],
    exact_cfg: ExactConfig,
    config: ModifiedConfig,
) -> Vec<ModifiedMatch> {
    let regions = if a.len() <= FULL_ALIGNMENT_MAX_TOKENS && b.len() <= FULL_ALIGNMENT_MAX_TOKENS {
        // End-to-end alignment plus seeded regions for secondary copies.
        let mut all = vec![(0, a.len(), 0, b.len())];
        all.extend(candidate_regions(a, b, exact_cfg, config));
        all.sort();
        all.dedup();
        all
    } else {
        candidate_regions(a, b, exact_cfg, config)
    };
    let mut out: Vec<ModifiedMatch> = Vec::new();
    for region in regions {
        // Skip regions whose both sides are already exact-covered: any
        // alignment inside would clip to nothing. Exact-recall safe.
        if range_covered((region.0, region.1), exact_a)
            && range_covered((region.2, region.3), exact_b)
        {
            continue;
        }
        for m in align_region(a, b, region, config) {
            // Clip A side against exact spans, mapping kept parts through the path.
            let mut kept: Vec<(usize, usize)> = vec![(m.a_start, m.a_end)];
            for &(es, ee) in exact_a {
                let mut next = Vec::new();
                for (s, e) in kept {
                    if ee <= s || es >= e {
                        next.push((s, e));
                        continue;
                    }
                    if es > s {
                        next.push((s, es));
                    }
                    if ee < e {
                        next.push((ee, e));
                    }
                }
                kept = next;
            }
            for (s, e) in kept {
                if e - s < config.min_tokens {
                    continue;
                }
                // Map through the alignment path carried on the match
                // (relative indexing); require B side mostly intact
                // (not exact-covered) so both highlights stay meaningful.
                let mut b_positions: Vec<usize> = Vec::new();
                for t in s..e {
                    if let Some(bp) = m.a_to_b.get(t - m.a_start).copied().flatten() {
                        b_positions.push(bp);
                    }
                }
                if b_positions.len() < config.min_tokens {
                    continue;
                }
                let bs = b_positions[0];
                let be = b_positions[b_positions.len() - 1] + 1;
                // Skip segments whose B side is mostly exact-covered already.
                let b_exact: usize = exact_b
                    .iter()
                    .map(|&(es, ee)| overlap_len((es, ee), (bs, be)))
                    .sum();
                if b_exact * 2 >= be - bs {
                    continue;
                }
                out.push(ModifiedMatch {
                    a_start: s,
                    a_end: e,
                    b_start: bs,
                    b_end: be,
                    identity: m.identity,
                    confidence: m.confidence,
                    a_to_b: m.a_to_b[s - m.a_start..e - m.a_start].to_vec(),
                });
            }
        }
    }
    out.sort_by_key(|m| (m.a_start, m.b_start));
    out.dedup_by(|b, a| a.a_start == b.a_start && a.a_end == b.a_end);
    // Drop spans fully inside a longer kept span (longest first, so nested
    // tracebacks from several endpoints collapse to one passage).
    out.sort_by(|a, b| {
        (b.a_end - b.a_start)
            .cmp(&(a.a_end - a.a_start))
            .then((a.a_start, a.b_start).cmp(&(b.a_start, b.b_start)))
    });
    let mut pruned: Vec<ModifiedMatch> = Vec::new();
    for m in out {
        if pruned
            .iter()
            .any(|k: &ModifiedMatch| k.a_start <= m.a_start && k.a_end >= m.a_end)
        {
            continue;
        }
        pruned.push(m);
    }
    pruned.sort_by_key(|m| (m.a_start, m.b_start));
    pruned
        .into_iter()
        .filter(|m| m.len() >= config.min_tokens)
        .collect()
}

/// Length of overlap between two end-exclusive ranges.
fn overlap_len(a: (usize, usize), b: (usize, usize)) -> usize {
    a.1.min(b.1).saturating_sub(a.0.max(b.0))
}

/// True when every token of `range` lies inside `cover` (union semantics).
fn range_covered(range: (usize, usize), cover: &[(usize, usize)]) -> bool {
    if range.0 >= range.1 {
        return true;
    }
    let mut sorted = cover.to_vec();
    sorted.sort();
    let mut cursor = range.0;
    for (s, e) in sorted {
        if s > cursor {
            return false;
        }
        cursor = cursor.max(e);
        if cursor >= range.1 {
            return true;
        }
    }
    cursor >= range.1
}

/// Token ranges of modified matches on one side (for coverage union).
#[must_use]
pub fn modified_ranges(matches: &[ModifiedMatch], side_is_a: bool) -> Vec<(usize, usize)> {
    matches
        .iter()
        .map(|m| {
            if side_is_a {
                (m.a_start, m.a_end)
            } else {
                (m.b_start, m.b_end)
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "modified_tests.rs"]
mod modified_tests;

//! Phase 5: exact-copy detection — token shingles, stable hashing,
//! Winnowing fingerprints, indexed candidates, verified passages.
//!
//! Versioned config (`FINGERPRINT_VERSION`). All hashing is FNV-1a 64-bit:
//! deterministic across runs, processes, and platforms — never a
//! language-runtime randomized hash.
//!
//! Pipeline per document pair:
//! 1. shingle normalized tokens into `k`-grams, hashed stably;
//! 2. Winnowing selects a sparse fingerprint set per document;
//! 3. shared fingerprints become candidates via the index;
//! 4. each candidate is verified by direct token comparison and EXPANDED
//!    to the maximal exact run, so reported boundaries are exact;
//! 5. overlapping same-offset runs merge; runs below `min_match_tokens`
//!    are dropped (tiny shared phrases are not evidence).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::normalize::CanonicalDocument;

/// Bump when any rule, hash, or default below changes.
pub const FINGERPRINT_VERSION: u32 = 1;

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

/// Hash one token shingle. Tokens are joined with a unit separator so
/// `["ab", "c"]` and `["a", "bc"]` never collide.
pub(crate) fn shingle_hash(tokens: &[&str]) -> u64 {
    let mut hash = FNV_OFFSET;
    for tok in tokens {
        for &b in tok.as_bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash ^= 0x1f;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fingerprint {
    pub hash: u64,
    pub pos: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExactConfig {
    /// Shingle length in tokens.
    pub k: usize,
    /// Winnowing window over shingle hashes.
    pub window: usize,
    /// Minimum merged run length (tokens) to report.
    pub min_match_tokens: usize,
    pub version: u32,
}

impl Default for ExactConfig {
    fn default() -> Self {
        Self {
            k: 5,
            window: 4,
            min_match_tokens: 10,
            version: FINGERPRINT_VERSION,
        }
    }
}

/// One verified exact passage pair. Ranges are token ordinals, end-exclusive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenMatch {
    pub a_start: usize,
    pub a_end: usize,
    pub b_start: usize,
    pub b_end: usize,
}

impl TokenMatch {
    #[must_use]
    pub fn len(&self) -> usize {
        self.a_end - self.a_start
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Shingle a token stream and select Winnowing fingerprints.
/// Classic rule: minimum hash per window, ties to the rightmost occurrence;
/// a fingerprint is recorded when it differs from the previous selection.
pub fn winnow(tokens: &[String], config: ExactConfig) -> Vec<Fingerprint> {
    if tokens.len() < config.k || config.k == 0 || config.window == 0 {
        return Vec::new();
    }
    let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
    let hashes: Vec<u64> = (0..=refs.len() - config.k)
        .map(|i| shingle_hash(&refs[i..i + config.k]))
        .collect();

    let w = config.window.min(hashes.len());
    let mut out = Vec::new();
    let mut prev: Option<(u64, usize)> = None;
    for (i, window) in hashes.windows(w).enumerate() {
        let mut best = 0usize;
        for (j, &h) in window.iter().enumerate() {
            if h <= window[best] {
                best = j;
            }
        }
        let candidate = (window[best], i + best);
        if prev != Some(candidate) {
            out.push(Fingerprint {
                hash: candidate.0,
                pos: candidate.1,
            });
            prev = Some(candidate);
        }
    }
    out
}

/// Fingerprint index over reference documents: hash → (doc, shingle pos).
#[derive(Debug, Default)]
pub(crate) struct FingerprintIndex {
    entries: HashMap<u64, Vec<(usize, usize)>>,
}

impl FingerprintIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, doc: usize, tokens: &[String], config: ExactConfig) {
        for fp in winnow(tokens, config) {
            self.entries.entry(fp.hash).or_default().push((doc, fp.pos));
        }
    }

    pub(crate) fn positions(&self, hash: u64) -> &[(usize, usize)] {
        self.entries.get(&hash).map_or(&[], Vec::as_slice)
    }
}

/// Verify one candidate by expanding to the maximal exact run.
fn expand(a: &[String], b: &[String], mut s1: usize, mut s2: usize, k: usize) -> TokenMatch {
    let mut e1 = s1 + k;
    let mut e2 = s2 + k;
    while s1 > 0 && s2 > 0 && a[s1 - 1] == b[s2 - 1] {
        s1 -= 1;
        s2 -= 1;
    }
    while e1 < a.len() && e2 < b.len() && a[e1] == b[e2] {
        e1 += 1;
        e2 += 1;
    }
    TokenMatch {
        a_start: s1,
        a_end: e1,
        b_start: s2,
        b_end: e2,
    }
}

/// Exhaustive k-shingle matching (no Winnowing): every shared k-gram is a
/// candidate, expanded to the maximal run and merged. Use for short
/// reference texts where Winnowing's detection threshold (`w + k - 1`)
/// would miss reportable runs. Reference texts are tiny, so the cost is
/// negligible.
#[must_use]
pub fn find_shingle_runs(
    a: &[String],
    b: &[String],
    k: usize,
    min_tokens: usize,
) -> Vec<TokenMatch> {
    if k == 0 || a.len() < k || b.len() < k {
        return Vec::new();
    }
    let ra: Vec<&str> = a.iter().map(String::as_str).collect();
    let rb: Vec<&str> = b.iter().map(String::as_str).collect();
    let mut index: HashMap<u64, Vec<usize>> = HashMap::new();
    for i in 0..=rb.len() - k {
        index
            .entry(shingle_hash(&rb[i..i + k]))
            .or_default()
            .push(i);
    }
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for i in 0..=ra.len() - k {
        if let Some(positions) = index.get(&shingle_hash(&ra[i..i + k])) {
            for &pb in positions {
                pairs.push((i, pb));
            }
        }
    }
    let runs = expand_bounded(a, b, k, pairs);
    merge_runs(runs, min_tokens)
}

/// Verify candidates with maximal expansion, bounding work on repetitive
/// text (Phase 6.1.4). Candidates are grouped by diagonal offset
/// (`pb − pa`); on each diagonal only pairs reaching past all previously
/// verified runs are expanded. A skipped pair lies strictly inside an
/// already-verified maximal run on the same diagonal, so recall is exactly
/// preserved while repeated-token inputs collapse from quadratic to linear.
fn expand_bounded(
    a: &[String],
    b: &[String],
    k: usize,
    mut pairs: Vec<(usize, usize)>,
) -> Vec<TokenMatch> {
    pairs.sort_by_key(|&(pa, pb)| (pb as i64 - pa as i64, pa));
    let mut runs: Vec<TokenMatch> = Vec::new();
    let mut current_offset = i64::MIN;
    let mut reach = 0usize;
    for (pa, pb) in pairs {
        let offset = pb as i64 - pa as i64;
        if offset != current_offset {
            current_offset = offset;
            reach = 0;
        }
        if pa + k <= reach {
            continue;
        }
        let run = expand(a, b, pa, pb, k);
        reach = reach.max(run.a_end);
        runs.push(run);
    }
    runs.sort_by_key(|r| (r.a_start, r.b_start));
    runs.dedup();
    runs
}

/// Compare two normalized token streams, returning merged exact passages
/// sorted by `(a_start, b_start)`.
#[must_use]
pub fn find_exact_matches(a: &[String], b: &[String], config: ExactConfig) -> Vec<TokenMatch> {
    if a.len() < config.k || b.len() < config.k {
        return Vec::new();
    }
    let mut index = FingerprintIndex::new();
    index.add(1, b, config);
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for fp in winnow(a, config) {
        for &(_, pb) in index.positions(fp.hash) {
            pairs.push((fp.pos, pb));
        }
    }
    let runs = expand_bounded(a, b, config.k, pairs);
    merge_runs(runs, config.min_match_tokens)
}

/// Merge overlapping/touching runs that share one offset, then drop short ones.
pub(crate) fn merge_runs(mut runs: Vec<TokenMatch>, min_tokens: usize) -> Vec<TokenMatch> {
    fn offset(r: &TokenMatch) -> i64 {
        r.b_start as i64 - r.a_start as i64
    }
    let mut merged: Vec<TokenMatch> = Vec::new();
    for run in runs.drain(..) {
        if let Some(last) = merged.last_mut() {
            if offset(&run) == offset(last) && run.a_start <= last.a_end {
                last.a_end = last.a_end.max(run.a_end);
                last.b_end = last.b_end.max(run.b_end);
                continue;
            }
        }
        merged.push(run);
    }
    merged
        .into_iter()
        .filter(|r| r.len() >= min_tokens)
        .collect()
}

/// Union length of token ranges (end-exclusive) — no double counting.
#[must_use]
pub fn union_len(ranges: &[(usize, usize)]) -> usize {
    let mut sorted = ranges.to_vec();
    sorted.sort();
    let mut total = 0usize;
    let mut cur: Option<(usize, usize)> = None;
    for (s, e) in sorted {
        match cur {
            None => cur = Some((s, e)),
            Some((cs, ce)) if s <= ce => cur = Some((cs, ce.max(e))),
            Some((cs, ce)) => {
                total += ce - cs;
                cur = Some((s, e));
            }
        }
    }
    if let Some((cs, ce)) = cur {
        total += ce - cs;
    }
    total
}

/// Directional exact coverage of `tokens` by `matches` (0.0–100.0).
#[must_use]
pub fn directional_coverage(matches: &[TokenMatch], side_is_a: bool, total_tokens: usize) -> f64 {
    if total_tokens == 0 {
        return 0.0;
    }
    let ranges: Vec<(usize, usize)> = matches
        .iter()
        .map(|m| {
            if side_is_a {
                (m.a_start, m.a_end)
            } else {
                (m.b_start, m.b_end)
            }
        })
        .collect();
    union_len(&ranges) as f64 / total_tokens as f64 * 100.0
}

/// Deterministic pair enumeration: every unique `(i, j)`, `i < j`.
/// For N inputs there are exactly N·(N−1)/2 pairs.
#[must_use]
pub fn pairwise_indices(n: usize) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            pairs.push((i, j));
        }
    }
    pairs
}

/// Compare two canonical documents (normalized token streams).
#[must_use]
pub fn compare_documents(
    a: &CanonicalDocument,
    b: &CanonicalDocument,
    config: ExactConfig,
) -> Vec<TokenMatch> {
    let ta: Vec<String> = a.tokens.iter().map(|t| t.normalized.clone()).collect();
    let tb: Vec<String> = b.tokens.iter().map(|t| t.normalized.clone()).collect();
    find_exact_matches(&ta, &tb, config)
}

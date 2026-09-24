//! Phase 5 TDD fixtures: identical, disjoint, embedded copy, rewritten
//! variants, tiny phrases, boundary copies, pair enumeration.

use std::collections::HashSet;

use super::*;
use crate::normalize::canonicalize;

fn toks(doc: &CanonicalDocument) -> Vec<String> {
    doc.tokens.iter().map(|t| t.normalized.clone()).collect()
}

const ESSAY: &str = "Photosynthesis converts light energy into chemical energy that plants can store for later use. \
    Chlorophyll absorbs red and blue wavelengths while reflecting green light back to our eyes. \
    The Calvin cycle then fixes carbon dioxide into sugars inside the stroma of chloroplasts.";

const OTHER: &str = "Volcanic eruptions release ash and sulfur gases high into the atmosphere. \
    Tectonic plates diverge at mid ocean ridges where magma rises to form new crust. \
    Seismographs record elastic waves that reveal the structure of the deep interior.";

#[test]
fn fixture_a_identical_documents_match_fully() {
    let cfg = ExactConfig::default();
    let a = canonicalize(ESSAY);
    let b = canonicalize(ESSAY);
    let matches = compare_documents(&a, &b, cfg);
    assert_eq!(matches.len(), 1, "identical docs merge into one passage");
    let m = &matches[0];
    assert_eq!((m.a_start, m.a_end), (0, a.tokens.len()));
    assert_eq!((m.b_start, m.b_end), (0, b.tokens.len()));
    assert_eq!(directional_coverage(&matches, true, a.tokens.len()), 100.0);
    assert_eq!(directional_coverage(&matches, false, b.tokens.len()), 100.0);
    // Correct passage range back to the original text (trailing
    // punctuation is not a token, so spans end at the last word).
    let first = &a.tokens[m.a_start];
    let last = &a.tokens[m.a_end - 1];
    assert_eq!(first.original_start, 0);
    assert_eq!(
        &a.original[first.original_start..last.original_end],
        ESSAY.strip_suffix('.').expect("fixture ends with a period")
    );
}

#[test]
fn fixture_b_disjoint_documents_have_zero_coverage() {
    let cfg = ExactConfig::default();
    let a = canonicalize(ESSAY);
    let b = canonicalize(OTHER);
    let matches = compare_documents(&a, &b, cfg);
    assert!(matches.is_empty());
    assert_eq!(directional_coverage(&matches, true, a.tokens.len()), 0.0);
    assert_eq!(directional_coverage(&matches, false, b.tokens.len()), 0.0);
}

#[test]
fn fixture_c_embedded_copy_returns_only_copied_region() {
    let cfg = ExactConfig::default();
    let copied = "The mitochondria generate adenosine triphosphate through oxidative phosphorylation across folded inner membranes.";
    let a = canonicalize(&format!(
        "My own introduction written from scratch with personal observations. {copied} My own conclusion follows with independent analysis."
    ));
    let b = canonicalize(&format!(
        "A completely different opening about unrelated coursework. {copied} A different closing paragraph with fresh wording."
    ));
    let matches = compare_documents(&a, &b, cfg);
    assert_eq!(
        matches.len(),
        1,
        "exactly the copied paragraph, got {matches:?}"
    );
    let m = &matches[0];
    let got: Vec<String> = toks(&a)[m.a_start..m.a_end].to_vec();
    assert_eq!(got, toks(&canonicalize(copied)));
    // Surrounding original material is not covered.
    let cov = directional_coverage(&matches, true, a.tokens.len());
    assert!(cov > 0.0 && cov < 100.0, "partial coverage {cov}");
}

#[test]
fn fixture_d_case_and_punctuation_variants_still_match() {
    let cfg = ExactConfig::default();
    let a = canonicalize(
        "Photosynthesis CONVERTS light energy; into chemical energy that plants can store!",
    );
    let b = canonicalize(
        "photosynthesis converts light energy into chemical energy that plants can store",
    );
    let matches = compare_documents(&a, &b, cfg);
    assert_eq!(matches.len(), 1);
    assert_eq!(directional_coverage(&matches, true, a.tokens.len()), 100.0);
}

#[test]
fn fixture_e_tiny_shared_phrase_is_not_evidence() {
    let cfg = ExactConfig::default();
    let a = canonicalize("My results were surprising in conclusion of the long experiment series");
    let b = canonicalize("Their findings differed in conclusion from every prior published study");
    let matches = compare_documents(&a, &b, cfg);
    assert!(
        matches.is_empty(),
        "tiny phrase must not be reported: {matches:?}"
    );
}

#[test]
fn fixture_f_boundary_copies_keep_exact_source_offsets() {
    let cfg = ExactConfig::default();
    let copied = "Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu";
    let a = canonicalize(&format!(
        "{copied} then an entirely original tail continues here."
    ));
    let b = canonicalize(&format!("An entirely original head opens here. {copied}"));
    let matches = compare_documents(&a, &b, cfg);
    assert_eq!(matches.len(), 1);
    let m = &matches[0];
    assert_eq!(m.a_start, 0, "copy starts at the very first token of A");
    assert_eq!(
        m.b_end,
        b.tokens.len(),
        "copy runs to the very last token of B"
    );
    // Stored char offsets point at the copied words in each original.
    let a_first = &a.tokens[m.a_start];
    assert_eq!(
        &a.original[a_first.original_start..a_first.original_end],
        "Alpha"
    );
    let b_last = &b.tokens[m.b_end - 1];
    assert_eq!(
        &b.original[b_last.original_start..b_last.original_end],
        "mu"
    );
}

#[test]
fn winnowing_is_deterministic_and_positioned() {
    let cfg = ExactConfig::default();
    let tokens: Vec<String> = toks(&canonicalize(ESSAY));
    let first = winnow(&tokens, cfg);
    let second = winnow(&tokens, cfg);
    assert_eq!(first, second);
    assert!(!first.is_empty());
    let mut positions: Vec<usize> = first.iter().map(|f| f.pos).collect();
    let unique: HashSet<usize> = positions.iter().copied().collect();
    assert_eq!(positions.len(), unique.len(), "no duplicate selections");
    positions.sort_unstable();
    assert!(positions.windows(2).all(|w| w[0] < w[1]));
    assert!(positions.iter().all(|&p| p + cfg.k <= tokens.len()));
}

#[test]
fn short_documents_yield_no_fingerprints() {
    let cfg = ExactConfig::default();
    let tiny: Vec<String> = vec!["only".to_string(), "three".to_string(), "words".to_string()];
    assert!(winnow(&tiny, cfg).is_empty());
    assert!(find_exact_matches(&tiny, &tiny, cfg).is_empty());
}

#[test]
fn shingle_hash_is_stable_and_sensitive() {
    assert_eq!(
        shingle_hash(&["hello", "world"]),
        shingle_hash(&["hello", "world"])
    );
    assert_ne!(
        shingle_hash(&["hello", "world"]),
        shingle_hash(&["hello", "mars"])
    );
    // Separator prevents cross-boundary collisions.
    assert_ne!(shingle_hash(&["ab", "c"]), shingle_hash(&["a", "bc"]));
}

#[test]
fn union_len_merges_without_double_counting() {
    assert_eq!(union_len(&[]), 0);
    assert_eq!(union_len(&[(0, 10), (5, 15)]), 15);
    assert_eq!(union_len(&[(0, 5), (5, 10)]), 10);
    assert_eq!(union_len(&[(0, 5), (10, 15)]), 10);
    assert_eq!(union_len(&[(0, 20), (5, 10)]), 20);
}

#[test]
fn directional_coverage_matches_plan_example() {
    // 500 shared of 1000 vs 2000 tokens → 50% one way, 25% the other.
    let matches = vec![TokenMatch {
        a_start: 0,
        a_end: 500,
        b_start: 100,
        b_end: 600,
    }];
    assert_eq!(directional_coverage(&matches, true, 1000), 50.0);
    assert_eq!(directional_coverage(&matches, false, 2000), 25.0);
    assert_eq!(directional_coverage(&[], true, 100), 0.0);
    assert_eq!(directional_coverage(&[], true, 0), 0.0);
}

#[test]
fn pair_enumeration_is_complete_and_deterministic() {
    assert!(pairwise_indices(0).is_empty());
    assert!(pairwise_indices(1).is_empty());
    assert_eq!(pairwise_indices(2), vec![(0, 1)]);
    let pairs = pairwise_indices(20);
    assert_eq!(pairs.len(), 20 * 19 / 2);
    let unique: HashSet<(usize, usize)> = pairs.iter().copied().collect();
    assert_eq!(unique.len(), pairs.len(), "no duplicate comparisons");
    assert!(pairs.iter().all(|(i, j)| i < j));
    let mut sorted = pairs.clone();
    sorted.sort();
    assert_eq!(pairs, sorted, "canonical order");
}

#[test]
fn index_finds_shared_fingerprints_across_docs() {
    let cfg = ExactConfig::default();
    let a: Vec<String> = toks(&canonicalize(ESSAY));
    let b: Vec<String> = toks(&canonicalize(OTHER));
    let mut index = FingerprintIndex::new();
    index.add(0, &a, cfg);
    index.add(1, &b, cfg);
    // Same-document fingerprints are retrievable; unrelated docs share none.
    let probe = &winnow(&a, cfg)[0];
    assert!(index.positions(probe.hash).iter().any(|&(d, _)| d == 0));
    let cross: Vec<_> = winnow(&a, cfg)
        .iter()
        .flat_map(|f| index.positions(f.hash).to_vec())
        .filter(|(d, _)| *d == 1)
        .collect();
    assert!(cross.is_empty());
}

/// Independent brute-force oracle: every left-maximal exact run, merged.
/// O(n²·m²); only for small fixtures. The optimized engine must return the
/// identical run set (Phase 6.1.4 recall preservation).
fn brute_force_maximal(a: &[String], b: &[String], min_tokens: usize) -> Vec<TokenMatch> {
    let mut runs = Vec::new();
    for i in 0..a.len() {
        for j in 0..b.len() {
            if a[i] != b[j] {
                continue;
            }
            if i > 0 && j > 0 && a[i - 1] == b[j - 1] {
                continue;
            }
            let (mut e1, mut e2) = (i, j);
            while e1 < a.len() && e2 < b.len() && a[e1] == b[e2] {
                e1 += 1;
                e2 += 1;
            }
            if e1 - i >= min_tokens {
                runs.push(TokenMatch {
                    a_start: i,
                    a_end: e1,
                    b_start: j,
                    b_end: e2,
                });
            }
        }
    }
    runs
}

fn assert_oracle_parity(a: &[String], b: &[String]) {
    let cfg = ExactConfig::default();
    let mut expected = brute_force_maximal(a, b, cfg.min_match_tokens);
    expected.sort_by_key(|r| (r.a_start, r.b_start));
    let mut got = find_exact_matches(a, b, cfg);
    got.sort_by_key(|r| (r.a_start, r.b_start));
    assert_eq!(got, expected, "optimized engine must match brute force");
}

#[test]
fn bounded_work_preserves_brute_force_recall() {
    // Realistic small pair with an embedded copy.
    let copied = "the quick brown fox jumps over the lazy dog near the river";
    let a = toks(&canonicalize(&format!(
        "Opening words of my own here. {copied} Closing thoughts mine."
    )));
    let b = toks(&canonicalize(&format!(
        "Different opening entirely. {copied} Different ending here."
    )));
    assert_oracle_parity(&a, &b);
    // Highly repetitive pair.
    let unit: Vec<String> = toks(&canonicalize(
        "lorem ipsum dolor sit amet consectetur adipiscing elit sed do",
    ));
    let rep = |n: usize| {
        (0..n)
            .map(|i| unit[i % unit.len()].clone())
            .collect::<Vec<_>>()
    };
    assert_oracle_parity(&rep(36), &rep(36));
    // Identical short essay.
    let essay = toks(&canonicalize(ESSAY));
    assert_oracle_parity(&essay, &essay);
}

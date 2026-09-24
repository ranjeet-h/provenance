//! Phase 7 TDD fixtures: the eight required classes plus span,
//! determinism, and quality checks.

use super::*;
use crate::exact::{find_exact_matches, ExactConfig};
use crate::normalize::canonicalize;

fn toks(text: &str) -> Vec<String> {
    canonicalize(text)
        .tokens
        .iter()
        .map(|t| t.normalized.clone())
        .collect()
}

fn cfg() -> (ExactConfig, ModifiedConfig) {
    (ExactConfig::default(), ModifiedConfig::default())
}

fn modified(a: &str, b: &str) -> Vec<ModifiedMatch> {
    let (exact_cfg, mod_cfg) = cfg();
    let ta = toks(a);
    let tb = toks(b);
    find_modified_matches(&ta, &tb, &[], &[], exact_cfg, mod_cfg)
}

#[test]
fn punctuation_and_capitalization_variants_detected() {
    let a = "Solar panels absorb sunlight, and convert it into electricity for rural homes today";
    let b = "solar panels absorb sunlight and convert it into ELECTRICITY for rural homes today";
    let found = modified(a, b);
    assert_eq!(found.len(), 1);
    assert!(
        (found[0].identity - 1.0).abs() < f64::EPSILON,
        "only case/punct differ: {found:?}"
    );
}

#[test]
fn one_word_substitutions_detected() {
    let a = "The quick brown fox jumps over the lazy dog near the river bank at dawn";
    let b = "The quick brown fox leaps over the lazy dog near the river shore at dawn";
    let found = modified(a, b);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].identity >= 0.8, "identity {}", found[0].identity);
    assert!(found[0].identity < 1.0);
}

#[test]
fn inserted_adjectives_detected() {
    let a = "Solar panels absorb sunlight and convert it into electricity for homes";
    let b = "Solar panels efficiently absorb bright sunlight and convert it directly into clean electricity for rural homes";
    let found = modified(a, b);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].identity >= 0.6, "identity {}", found[0].identity);
}

#[test]
fn deleted_words_detected() {
    let a = "Regular exercise strengthens the heart and improves circulation throughout the entire body daily for health";
    let b = "Regular exercise strengthens circulation throughout the body daily for health";
    let found = modified(a, b);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].identity >= 0.6, "identity {}", found[0].identity);
}

#[test]
fn reordered_sentences_found_by_exact_engine_not_inflated() {
    let s1 =
        "The committee approved the annual budget after three hours of heated debate yesterday";
    let s2 = "New members received orientation packets containing maps schedules and contact directories";
    let a = format!("{s1}. {s2}.");
    let b = format!("{s2}. {s1}.");
    let (exact_cfg, mod_cfg) = cfg();
    let ta = toks(&a);
    let tb = toks(&b);
    // Each sentence is an exact run at a different offset — both found.
    let exact = find_exact_matches(&ta, &tb, exact_cfg);
    assert_eq!(exact.len(), 2, "{exact:?}");
    // Modified alignment must not invent extra evidence beyond exact spans.
    let exact_a: Vec<(usize, usize)> = exact.iter().map(|m| (m.a_start, m.a_end)).collect();
    let exact_b: Vec<(usize, usize)> = exact.iter().map(|m| (m.b_start, m.b_end)).collect();
    let found = find_modified_matches(&ta, &tb, &exact_a, &exact_b, exact_cfg, mod_cfg);
    assert!(
        found.is_empty(),
        "stable sentences are exact evidence: {found:?}"
    );
}

#[test]
fn unrelated_text_with_shared_terminology_not_flagged() {
    let a = "The patient received insulin for diabetes management and dietary counseling sessions";
    let b = "Diabetes screening guidelines recommend dietary counseling before insulin therapy decisions";
    assert!(modified(a, b).is_empty());
}

#[test]
fn medical_vocabulary_overlap_alone_is_not_enough() {
    let a = "Myocardial infarction requires rapid triage electrocardiogram and aspirin administration promptly";
    let b = "Aspirin resistance complicates stroke prevention although electrocardiogram monitoring remains routine practice";
    assert!(modified(a, b).is_empty());
}

#[test]
fn engineering_vocabulary_overlap_alone_is_not_enough() {
    let a = "The cantilever beam deflection depends on load distribution and material stiffness properties";
    let b = "Material fatigue testing measures stiffness degradation under cyclic load conditions daily";
    assert!(modified(a, b).is_empty());
}

#[test]
fn plan_manual_example_detected_as_modified_not_exact() {
    let original =
        "Photosynthesis converts light energy into chemical energy that can later be used by the plant.";
    let rewritten =
        "Through photosynthesis, plants convert light into stored chemical energy that can be used later.";
    let (exact_cfg, mod_cfg) = cfg();
    let ta = toks(original);
    let tb = toks(rewritten);
    assert!(
        find_exact_matches(&ta, &tb, exact_cfg).is_empty(),
        "too edited for the exact engine"
    );
    let found = find_modified_matches(&ta, &tb, &[], &[], exact_cfg, mod_cfg);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].identity >= 0.6, "identity {}", found[0].identity);
}

#[test]
fn spans_point_at_original_words() {
    let a = "Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu 추가로";
    let b = "Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu outro";
    let (exact_cfg, mod_cfg) = cfg();
    let da = canonicalize(a);
    let db = canonicalize(b);
    let ta: Vec<String> = da.tokens.iter().map(|t| t.normalized.clone()).collect();
    let tb: Vec<String> = db.tokens.iter().map(|t| t.normalized.clone()).collect();
    // Share a 12-word run so the exact engine takes it; modified clips it away.
    let exact = find_exact_matches(&ta, &tb, exact_cfg);
    assert_eq!(exact.len(), 1);
    let exact_a: Vec<(usize, usize)> = exact.iter().map(|m| (m.a_start, m.a_end)).collect();
    let exact_b: Vec<(usize, usize)> = exact.iter().map(|m| (m.b_start, m.b_end)).collect();
    let found = find_modified_matches(&ta, &tb, &exact_a, &exact_b, exact_cfg, mod_cfg);
    assert!(
        found.is_empty(),
        "fully-exact region leaves no modified remainder"
    );
}

#[test]
fn partial_exact_overlap_clips_but_keeps_edited_tail() {
    let shared = "one two three four five six seven eight nine ten eleven twelve";
    let a = format!("{shared} completely rewritten tail words here now today");
    let b = format!("{shared} totally different ending terms present currently");
    let (exact_cfg, mod_cfg) = cfg();
    let ta = toks(&a);
    let tb = toks(&b);
    let exact = find_exact_matches(&ta, &tb, exact_cfg);
    assert_eq!(exact.len(), 1);
    // Alignment spans the shared run plus edited tails; clipping removes the
    // exact part, leaving tails too short to report.
    let exact_a: Vec<(usize, usize)> = exact.iter().map(|m| (m.a_start, m.a_end)).collect();
    let exact_b: Vec<(usize, usize)> = exact.iter().map(|m| (m.b_start, m.b_end)).collect();
    let found = find_modified_matches(&ta, &tb, &exact_a, &exact_b, exact_cfg, mod_cfg);
    assert!(
        found.is_empty(),
        "short edited tails are not evidence: {found:?}"
    );
}

#[test]
fn results_are_deterministic() {
    let a = "The quick brown fox jumps over the lazy dog near the river bank at dawn";
    let b = "The quick brown fox leaps over the lazy dog near the river shore at dawn";
    assert_eq!(modified(a, b), modified(a, b));
}

#[test]
fn confidence_is_bounded_and_identity_consistent() {
    let a = "The quick brown fox jumps over the lazy dog near the river bank at dawn";
    let b = "The quick brown fox leaps over the lazy dog near the river shore at dawn";
    let found = modified(a, b);
    assert_eq!(found.len(), 1);
    assert!((0.0..=1.0).contains(&found[0].confidence));
    assert!((0.0..=1.0).contains(&found[0].identity));
}

#[test]
fn token_similarity_basics() {
    assert_eq!(token_similarity("photosynthesis", "photosynthesis"), 1.0);
    assert_eq!(token_similarity("", "x"), 0.0);
    assert_eq!(token_similarity("x", ""), 0.0);
    assert!(token_similarity("organize", "organise") >= 0.8);
    assert!(token_similarity("cat", "dog") < 0.8);
}

#[test]
fn empty_inputs_yield_nothing() {
    let (exact_cfg, mod_cfg) = cfg();
    let empty: Vec<String> = Vec::new();
    let some = toks("enough words here to pass every minimum length gate today");
    assert!(find_modified_matches(&empty, &some, &[], &[], exact_cfg, mod_cfg).is_empty());
    assert!(find_modified_matches(&some, &empty, &[], &[], exact_cfg, mod_cfg).is_empty());
}

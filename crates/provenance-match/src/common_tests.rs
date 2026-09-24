//! Phase 6 TDD fixtures: session-wide boilerplate stays scorable (flagged
//! common), rare passages kept, prompt/reference exclusion.

use super::*;
use crate::exact::{directional_coverage, find_exact_matches, ExactConfig};
use crate::normalize::canonicalize;

fn toks(text: &str) -> Vec<String> {
    canonicalize(text)
        .tokens
        .iter()
        .map(|t| t.normalized.clone())
        .collect()
}

const QUESTION: &str = "Discuss the causes and consequences of the industrial revolution with reference to urban growth and factory labour conditions.";
const RARE: &str =
    "Midnight owls hunt silently above moonlit barley fields while farmers sleep deeply.";

fn filler(i: usize) -> String {
    format!("zebra tungsten {i} quixotic fjord playlist vortex")
}

#[test]
fn session_question_stays_scorable_but_flagged_common() {
    // 20 submissions, all containing the same assignment question.
    // Frequency alone must never erase the match: it stays counted and is
    // classified as common session text.
    let cfg = CommonTextConfig::default();
    let docs: Vec<Vec<String>> = (0..20)
        .map(|i| toks(&format!("{} {}", QUESTION, filler(i))))
        .collect();
    let df = shingle_document_frequency(&docs, cfg.shingle_k);
    let masks: Vec<Vec<bool>> = docs
        .iter()
        .map(|d| common_token_mask(d, &df, docs.len(), cfg))
        .collect();
    // Every question token is common in every document.
    let q_len = toks(QUESTION).len();
    for mask in &masks {
        assert!(
            mask.iter().take(q_len).all(|&m| m),
            "question must be common"
        );
        // Filler past the shingle bleed zone (k-1 tokens) must not be common.
        assert!(
            mask.iter().skip(q_len + 4).all(|&m| !m),
            "filler must not be common"
        );
    }
    // Pairwise matches collapse to the question — fully counted, no prompt set.
    let exact = ExactConfig::default();
    let matches = find_exact_matches(&docs[0], &docs[1], exact);
    assert!(!matches.is_empty(), "question must still match lexically");
    let (counted, excluded) = apply_exclusion(&matches, true, &[]);
    assert!(excluded.is_empty(), "nothing teacher-approved to exclude");
    let counted_tokens: usize = counted.iter().map(|(s, e)| e - s).sum();
    assert!(counted_tokens > 0, "common text stays scored");
    assert_eq!(
        filtered_coverage(&counted, docs[0].len()),
        directional_coverage(&matches, true, docs[0].len())
    );
    // Classification: the question range is common, the filler is not.
    assert!(is_common_range(0, q_len, &masks[0]));
    assert!(!is_common_range(q_len + 4, docs[0].len(), &masks[0]));
    assert_eq!(common_fraction(0, q_len, &masks[0]), 1.0);
}

#[test]
fn rare_shared_paragraph_remains_evidence() {
    // Same 20-doc session, but two submissions share one rare paragraph.
    let cfg = CommonTextConfig::default();
    let docs: Vec<Vec<String>> = (0..20)
        .map(|i| {
            let extra = if i < 2 {
                format!(" {RARE}")
            } else {
                String::new()
            };
            toks(&format!("{} {}{extra}", QUESTION, filler(i)))
        })
        .collect();
    let df = shingle_document_frequency(&docs, cfg.shingle_k);
    let mask0 = common_token_mask(&docs[0], &df, docs.len(), cfg);
    let exact = ExactConfig::default();
    let matches = find_exact_matches(&docs[0], &docs[1], exact);
    assert!(matches.len() >= 2, "question + rare paragraph match");
    // No teacher-approved exclusions: everything counts, including the
    // unanimous question. Classification separates common from rare.
    let (counted, excluded) = apply_exclusion(&matches, true, &[]);
    assert!(excluded.is_empty());
    let counted_tokens: usize = counted.iter().map(|(s, e)| e - s).sum();
    assert!(
        counted_tokens >= toks(RARE).len() - 2,
        "rare kept, got {counted_tokens}"
    );
    let kept: Vec<String> = counted
        .iter()
        .flat_map(|(s, e)| docs[0][*s..*e].to_vec())
        .collect();
    assert!(kept
        .windows(3)
        .any(|w| w == ["moonlit", "barley", "fields"]));
    let cov = filtered_coverage(&counted, docs[0].len());
    let raw = directional_coverage(&matches, true, docs[0].len());
    assert_eq!(cov, raw, "frequency must not reduce coverage");
    // The rare paragraph is not common; the question is.
    let rare_at = docs[0]
        .windows(toks(RARE).len())
        .position(|w| w == toks(RARE).as_slice())
        .expect("rare paragraph present");
    assert!(!is_common_range(
        rare_at,
        rare_at + toks(RARE).len(),
        &mask0
    ));
    assert!(is_common_range(0, toks(QUESTION).len(), &mask0));
}

#[test]
fn eighteen_of_twenty_is_common_two_is_not() {
    let cfg = CommonTextConfig::default();
    // Phrase in 18 docs → common; different phrase in 2 docs → kept.
    let common_phrase = "all students must attach the signed declaration sheet on top";
    let rare_phrase = "only we two visited the abandoned lighthouse at dawn";
    let docs: Vec<Vec<String>> = (0..20)
        .map(|i| {
            let shared = if i < 18 { common_phrase } else { rare_phrase };
            toks(&format!("{shared} {}", filler(i)))
        })
        .collect();
    let df = shingle_document_frequency(&docs, cfg.shingle_k);
    let mask_common = common_token_mask(&docs[0], &df, docs.len(), cfg);
    let mask_rare = common_token_mask(&docs[18], &df, docs.len(), cfg);
    assert!(
        mask_common.iter().take(5).all(|&m| m),
        "18/20 must be common session text"
    );
    assert!(
        mask_rare.iter().all(|&m| !m),
        "2/20 is classification input, not an exclusion trigger"
    );
}

#[test]
fn prompt_text_excluded_without_session_frequency() {
    // The manual-test scenario: 3 docs share the question, 2 share an answer.
    // Prompt exclusion works even with DF disabled via an unreachable threshold.
    let cfg = CommonTextConfig {
        df_threshold: 2.0,
        ..CommonTextConfig::default()
    };
    let answer = "Crop rotation restored nitrogen and doubled wheat yields within three seasons.";
    let docs = [
        toks(&format!("{QUESTION} {answer} Solo tail alpha.")),
        toks(&format!("{QUESTION} {answer} Solo tail beta.")),
        toks(&format!("{QUESTION} Entirely different body gamma.")),
    ];
    let prompt = vec![toks(QUESTION)];
    // DF is disabled here (unreachable threshold), so masks are empty;
    // scoring exclusion comes from the teacher-approved prompt alone.
    let prompt_spans: Vec<Vec<(usize, usize, ExclusionReason)>> = docs
        .iter()
        .map(|d| prompt_token_mask(d, &prompt, cfg.prompt_min_tokens))
        .collect();
    for spans in &prompt_spans {
        assert!(!spans.is_empty(), "question must match the prompt");
        assert!(spans.iter().all(|&(_, _, r)| r == ExclusionReason::Prompt));
    }
    let exact = ExactConfig::default();
    let matches = find_exact_matches(&docs[0], &docs[1], exact);
    let (counted, excluded) = apply_exclusion(&matches, true, &prompt_spans[0]);
    assert!(
        excluded.iter().all(|s| s.reason == ExclusionReason::Prompt),
        "only teacher material excludes"
    );
    let kept: Vec<String> = counted
        .iter()
        .flat_map(|(s, e)| docs[0][*s..*e].to_vec())
        .collect();
    assert!(
        kept.contains(&"nitrogen".to_string()),
        "copied answer counted"
    );
    assert!(
        !kept.contains(&"revolution".to_string()),
        "question not counted"
    );
}

#[test]
fn short_prompt_fragments_below_minimum_are_kept() {
    let tokens = toks("The quick brown fox jumps over the lazy dog in the meadow");
    let reference = vec![toks("quick brown fox")]; // 3 < min 5
    let spans = prompt_token_mask(&tokens, &reference, 5);
    assert!(spans.is_empty());
}

#[test]
fn reference_text_uses_reference_reason() {
    let tokens = toks("Background reading covered the treaty of versailles extensively afterwards");
    let refs = vec![
        toks("unrelated"),
        toks("treaty of versailles extensively afterwards"),
    ];
    let spans = prompt_token_mask(&tokens, &refs, 5);
    assert!(!spans.is_empty());
    assert!(spans
        .iter()
        .all(|&(_, _, r)| r == ExclusionReason::Reference));
}

#[test]
fn disabled_config_counts_everything() {
    let tokens = toks(&format!("{QUESTION} {}", filler(0)));
    let other = toks(&format!("{QUESTION} {}", filler(1)));
    let exact = ExactConfig::default();
    let matches = find_exact_matches(&tokens, &other, exact);
    assert!(!matches.is_empty());
    // Caller skips exclusion entirely when disabled — raw coverage intact.
    assert!(directional_coverage(&matches, true, tokens.len()) > 0.0);
}

#[test]
fn adjacent_prompt_and_reference_spans_keep_both_reasons() {
    // One match fully covered by a Prompt half and a Reference half.
    let matches = vec![TokenMatch {
        a_start: 0,
        a_end: 12,
        b_start: 0,
        b_end: 12,
    }];
    let prompt = vec![(0, 6, ExclusionReason::Prompt)];
    let mut prompt_and_ref = prompt.clone();
    prompt_and_ref.push((6, 12, ExclusionReason::Reference));
    let (counted, excluded) = apply_exclusion(&matches, true, &prompt_and_ref);
    assert!(counted.is_empty());
    assert_eq!(
        excluded
            .iter()
            .map(|s| (s.token_start, s.token_end, s.reason))
            .collect::<Vec<_>>(),
        vec![
            (0, 6, ExclusionReason::Prompt),
            (6, 12, ExclusionReason::Reference)
        ]
    );
}

#[test]
fn overlapping_exclusions_resolve_to_prompt_precedence() {
    // A Reference span fully covered by a Prompt span: the overlap carries
    // the more specific Prompt reason; non-overlapping Reference edges keep
    // theirs. Frequency never participates in exclusion.
    let matches = vec![TokenMatch {
        a_start: 0,
        a_end: 10,
        b_start: 0,
        b_end: 10,
    }];
    let reference = vec![(0, 10, ExclusionReason::Reference)];
    let mut both = reference.clone();
    both.push((2, 6, ExclusionReason::Prompt));
    let (counted, excluded) = apply_exclusion(&matches, true, &both);
    assert!(counted.is_empty());
    assert_eq!(
        excluded
            .iter()
            .map(|s| (s.token_start, s.token_end, s.reason))
            .collect::<Vec<_>>(),
        vec![
            (0, 2, ExclusionReason::Reference),
            (2, 6, ExclusionReason::Prompt),
            (6, 10, ExclusionReason::Reference),
        ]
    );
}

#[test]
fn coverage_ignores_reason_metadata_only_changes() {
    let matches = vec![TokenMatch {
        a_start: 0,
        a_end: 20,
        b_start: 5,
        b_end: 25,
    }];
    let as_prompt = vec![(4, 12, ExclusionReason::Prompt)];
    let as_reference = vec![(4, 12, ExclusionReason::Reference)];
    let (counted_p, _) = apply_exclusion(&matches, true, &as_prompt);
    let (counted_r, _) = apply_exclusion(&matches, true, &as_reference);
    assert_eq!(counted_p, counted_r);
    assert_eq!(
        filtered_coverage(&counted_p, 20),
        filtered_coverage(&counted_r, 20)
    );
}

#[test]
fn common_boilerplate_counted_while_prompt_excluded() {
    // Required contract test: a match spanning teacher-approved prompt text
    // AND frequency-common boilerplate counts the boilerplate while the
    // prompt stays out of the score.
    let boilerplate =
        "all students must attach the signed declaration sheet on top of every submission packet";
    let prompt_text = "explain the water cycle and label every stage clearly today";
    let filler_a = "quixotic fjord playlist vortex tungsten zebra";
    let filler_b = "tungsten zebra quixotic fjord playlist vortex";
    let a = toks(&format!("{prompt_text} {boilerplate} {filler_a}"));
    let b = toks(&format!("{prompt_text} {boilerplate} {filler_b}"));
    let exact = ExactConfig::default();
    let matches = find_exact_matches(&a, &b, exact);
    // Prompt and boilerplate are adjacent with one shared offset, so they
    // merge into a single maximal run covering both.
    assert!(!matches.is_empty(), "shared runs must match");
    let prompt_spans = prompt_token_mask(&a, &[toks(prompt_text)], 5);
    assert!(!prompt_spans.is_empty());
    let (counted, excluded) = apply_exclusion(&matches, true, &prompt_spans);
    let kept: Vec<String> = counted
        .iter()
        .flat_map(|(s, e)| a[*s..*e].to_vec())
        .collect();
    assert!(
        kept.windows(2).any(|w| w == ["declaration", "sheet"]),
        "common boilerplate stays reportable/countable: {kept:?}"
    );
    assert!(
        !kept.contains(&"cycle".to_string()),
        "teacher-approved prompt stays out: {kept:?}"
    );
    assert!(excluded.iter().all(|s| s.reason == ExclusionReason::Prompt));
}

#[test]
fn common_classification_uses_majority_rule() {
    // mask: first 6 of 10 tokens common → common; 4 of 10 → not.
    let mostly = vec![
        true, true, true, true, true, true, false, false, false, false,
    ];
    let partly = vec![
        true, true, true, true, false, false, false, false, false, false,
    ];
    assert!(is_common_range(0, 10, &mostly));
    assert!(!is_common_range(0, 10, &partly));
    assert_eq!(common_fraction(0, 10, &mostly), 0.6);
    assert_eq!(common_fraction(5, 5, &mostly), 0.0);
}

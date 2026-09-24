//! Phase 6.1.4 benchmark fixtures: bounded work on repetitive input.
//!
//! Run with `--release` for meaningful numbers:
//! `cargo test --release -p provenance-match --test exact_perf`
//! These tests assert recall (never timing); timings and resident memory
//! print to stdout for the record. Debug-build numbers carry no claims.

use std::time::Instant;

use provenance_match::{canonicalize, compare_documents, directional_coverage, ExactConfig};

fn toks(text: &str) -> Vec<String> {
    canonicalize(text)
        .tokens
        .iter()
        .map(|t| t.normalized.clone())
        .collect()
}

fn rss_mb() -> u64 {
    memory_stats::memory_stats()
        .map(|s| (s.physical_mem / 1024 / 1024) as u64)
        .unwrap_or(0)
}

fn report(name: &str, tokens: usize, elapsed: std::time::Duration) {
    println!(
        "[perf] {name}: {tokens} tokens in {}ms ({:.1}k tok/s), rss={}MiB. \
         Run with --release for record numbers.",
        elapsed.as_millis(),
        tokens as f64 / elapsed.as_secs_f64() / 1000.0,
        rss_mb()
    );
}

#[test]
fn repeated_token_documents_stay_bounded() {
    let cfg = ExactConfig::default();
    for n in [100usize, 200, 400] {
        let words = vec!["lorem"; n].join(" ");
        let a = toks(&words);
        let b = toks(&words);
        let start = Instant::now();
        let matches = compare_documents(&canonicalize(&words), &canonicalize(&words), cfg);
        let elapsed = start.elapsed();
        report(&format!("repeated-{n}"), a.len(), elapsed);
        assert_eq!(directional_coverage(&matches, true, a.len()), 100.0);
        assert_eq!(directional_coverage(&matches, false, b.len()), 100.0);
        let _ = b;
    }
}

#[test]
fn identical_long_essays_recall_fully() {
    let cfg = ExactConfig::default();
    let paragraph = "Photosynthesis converts light energy into chemical energy that plants store for later use in leaves stems and roots throughout the growing season. ";
    let essay = paragraph.repeat(8);
    let a = canonicalize(&essay);
    let start = Instant::now();
    let matches = compare_documents(&a, &a, cfg);
    let elapsed = start.elapsed();
    report("identical-essay", a.tokens.len(), elapsed);
    assert_eq!(directional_coverage(&matches, true, a.tokens.len()), 100.0);
}

#[test]
fn realistic_assignments_keep_recall() {
    let cfg = ExactConfig::default();
    let copied = "The rapid expansion of railway networks during the nineteenth century transformed trade across continents and reshaped growing cities around busy stations. ";
    let a = canonicalize(&format!(
        "My own introduction with personal wording and observations. {copied} My own conclusion follows with independent analysis and final remarks."
    ));
    let b = canonicalize(&format!(
        "A completely different opening about unrelated coursework matters. {copied} A different closing paragraph with fresh wording and new ideas."
    ));
    let start = Instant::now();
    let matches = compare_documents(&a, &b, cfg);
    let elapsed = start.elapsed();
    report("realistic-pair", a.tokens.len() + b.tokens.len(), elapsed);
    assert_eq!(matches.len(), 1);
    assert!(directional_coverage(&matches, true, a.tokens.len()) > 30.0);
}

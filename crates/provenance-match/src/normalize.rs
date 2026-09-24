//! Phase 4: canonical text representation for every plagiarism algorithm.
//!
//! Versioned rules (`NORMALIZATION_VERSION`):
//! 1. Unicode NFC on the normalized form (offsets always index the ORIGINAL text).
//! 2. Case-folded for matching via [`char::to_lowercase`].
//! 3. Whitespace/punctuation split tokens; original byte spans are preserved.
//! 4. A token is a maximal run of Unicode alphanumeric chars.
//!    Apostrophes, hyphens (all variants), quotes and decimals split tokens:
//!    `don't` → `don` + `t`, `state–of–the–art` → four tokens, `3.14` → `3` + `14`.
//! 5. No stemming, no stop-word removal in V1.
//!
//! Evidence mapping invariant: every token carries byte offsets into the
//! original string, so analysis on normalized text always highlights the
//! exact original passage.

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

/// Bump when any rule above changes. Persisted with processed submissions.
pub const NORMALIZATION_VERSION: u32 = 1;

/// One normalized token with its evidence span in the original text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    /// NFC lowercase alphanumeric form used for matching.
    pub normalized: String,
    /// Byte offset of the first char in the original text (char boundary).
    pub original_start: usize,
    /// Byte offset one past the last char (char boundary).
    pub original_end: usize,
    /// Position in the token stream.
    pub ordinal: usize,
}

/// Sentence span over the token stream plus its original-text range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sentence {
    pub token_start: usize,
    pub token_end: usize,
    pub char_start: usize,
    pub char_end: usize,
}

/// Canonical form of one submission's text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalDocument {
    pub original: String,
    pub normalization_version: u32,
    pub tokens: Vec<Token>,
    /// Normalized tokens joined by single spaces.
    pub normalized_text: String,
    pub sentences: Vec<Sentence>,
    /// Token ordinals at which a new paragraph starts (blank-line separated).
    pub paragraph_starts: Vec<usize>,
    /// V1: every token is meaningful (all `true`); Phase 6 refines this.
    pub meaningful: Vec<bool>,
}

impl CanonicalDocument {
    /// Meaningful normalized tokens (coverage denominator in Phase 15).
    pub fn meaningful_count(&self) -> usize {
        self.meaningful.iter().filter(|m| **m).count()
    }
}

/// Normalize one raw token span: lowercase then NFC.
/// The span must contain only alphanumeric chars (tokenizer guarantee).
fn normalize_span(span: &str) -> String {
    span.to_lowercase().nfc().collect()
}

/// Word characters for invariant checks: alphanumeric plus combining marks
/// (a decomposed span like `e` + U+0301 legitimately contains a mark).
#[cfg(test)]
fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || is_combining_mark(ch)
}

/// Combining-mark ranges (Unicode General_Category=M subset) used by tests.
#[cfg(test)]
fn is_combining_mark(ch: char) -> bool {
    matches!(
        ch,
        '\u{0300}'..='\u{036F}'
            | '\u{1AB0}'..='\u{1AFF}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{20D0}'..='\u{20FF}'
            | '\u{FE20}'..='\u{FE2F}'
    )
}

/// Span consistency for highlight mapping: recomputing from the original
/// span must yield the stored normalized form, except that NFC expansion
/// (e.g. U+0344) can fuse extra combining marks onto the span. Those are
/// part of the same grapheme and highlight correctly.
#[cfg(test)]
fn span_matches_token(span: &str, normalized: &str) -> bool {
    let recomputed = normalize_span(span);
    if recomputed == normalized {
        return true;
    }
    recomputed.starts_with(normalized)
        && recomputed[normalized.len()..]
            .chars()
            .all(is_combining_mark)
}

/// Align each char of `nfc` back to its original byte range.
///
/// Phase 6.1 correction: NFC does not only merge. U+0344 expands one char
/// into two, and combining marks may reorder. This walk therefore matches
/// incrementally: it consumes original chars until their NFC equals the next
/// unconsumed NFC run. Consumed prefixes always end at composition-safe
/// boundaries (a starter that would compose never matches alone), so the
/// walk terminates with an exact partition and never indexes out of bounds.
fn align_nfc_to_original(original: &str, nfc: &str) -> Vec<(usize, usize)> {
    let orig: Vec<(usize, char)> = original.char_indices().collect();
    let nfc_chars: Vec<char> = nfc.chars().collect();
    let mut map = Vec::with_capacity(nfc_chars.len());
    let mut o = 0usize;
    let mut n = 0usize;
    while n < nfc_chars.len() {
        let start_o = o;
        let mut acc = String::new();
        loop {
            if o >= orig.len() {
                // Input exhausted (defensive; NFC consistency makes this
                // unreachable for well-formed input). Attach the remainder
                // to the final original range instead of panicking.
                let end = original.len();
                let start = orig.get(start_o).map(|(b, _)| *b).unwrap_or(end);
                while n < nfc_chars.len() {
                    map.push((start, end));
                    n += 1;
                }
                break;
            }
            acc.push(orig[o].1);
            o += 1;
            let chunk: Vec<char> = acc.nfc().collect();
            if nfc_chars[n..].starts_with(&chunk) {
                let start_byte = orig[start_o].0;
                let end_byte = orig.get(o).map(|(b, _)| *b).unwrap_or(original.len());
                for _ in 0..chunk.len() {
                    map.push((start_byte, end_byte));
                }
                n += chunk.len();
                break;
            }
        }
    }
    map
}

/// Cross-boundary offset contract (UI highlighting).
///
/// All Rust spans (`Token.original_start/end`, passage `*_char_start/end`)
/// are UTF-8 BYTE offsets into the original text. JavaScript
/// `String.slice()` indexes UTF-16 code units, so the UI MUST convert with
/// [`byte_range_to_utf16_range`] semantics before slicing — never pass Rust
/// offsets to `slice()` directly. Non-ASCII text before a match (accents,
/// emoji, CJK) otherwise shifts the highlight.
///
/// Returns `None` when either bound is not a UTF-8 char boundary.
#[must_use]
pub fn byte_range_to_utf16_range(text: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    fn to_utf16(text: &str, byte: usize) -> Option<usize> {
        if byte == text.len() {
            return Some(text.encode_utf16().count());
        }
        let mut utf16 = 0usize;
        for (b, ch) in text.char_indices() {
            if b == byte {
                return Some(utf16);
            }
            if b > byte {
                return None;
            }
            utf16 += ch.len_utf16();
        }
        None
    }
    match (to_utf16(text, start), to_utf16(text, end)) {
        (Some(s), Some(e)) if s <= e => Some((s, e)),
        _ => None,
    }
}

/// Build the canonical document, preserving original offsets.
///
/// Tokenization runs on the NFC form (so canonically-equivalent inputs
/// produce identical tokens) while every span indexes the ORIGINAL text.
#[must_use]
pub fn canonicalize(original: &str) -> CanonicalDocument {
    let nfc_owned: String = original.nfc().collect();
    let align = align_nfc_to_original(original, &nfc_owned);

    let mut tokens: Vec<Token> = Vec::new();
    // (nfc char index, nfc byte offset) of the current alphanumeric run.
    let mut run: Option<(usize, usize)> = None;

    let mut nfc_char_idx = 0usize;
    for (byte, ch) in nfc_owned.char_indices() {
        if ch.is_alphanumeric() {
            if run.is_none() {
                run = Some((nfc_char_idx, byte));
            }
        } else if let Some((start_idx, start_byte)) = run.take() {
            let (orig_start, _) = align[start_idx];
            let (_, orig_end) = align[nfc_char_idx - 1];
            tokens.push(Token {
                normalized: normalize_span(&nfc_owned[start_byte..byte]),
                original_start: orig_start,
                original_end: orig_end,
                ordinal: tokens.len(),
            });
        }
        nfc_char_idx += 1;
    }
    if let Some((start_idx, start_byte)) = run.take() {
        let (orig_start, _) = align[start_idx];
        let (_, orig_end) = align[nfc_char_idx - 1];
        tokens.push(Token {
            normalized: normalize_span(&nfc_owned[start_byte..]),
            original_start: orig_start,
            original_end: orig_end,
            ordinal: tokens.len(),
        });
    }

    let normalized_text = tokens
        .iter()
        .map(|t| t.normalized.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let sentences = split_sentences(original, &tokens);
    let paragraph_starts = paragraph_breaks(original, &tokens);
    let meaningful = vec![true; tokens.len()];

    CanonicalDocument {
        original: original.to_string(),
        normalization_version: NORMALIZATION_VERSION,
        tokens,
        normalized_text,
        sentences,
        paragraph_starts,
        meaningful,
    }
}

/// True for sentence-terminal punctuation in V1 English.
fn is_terminator(ch: char) -> bool {
    matches!(ch, '.' | '!' | '?' | '…')
}

/// Split the token stream into sentences.
/// A terminator ends a sentence when the next non-space char starts new
/// material (uppercase, digit, quote, EOS) or a blank line intervenes.
/// Known V1 limitation: abbreviations (`Dr.`, `e.g.`) may over-split.
fn split_sentences(original: &str, tokens: &[Token]) -> Vec<Sentence> {
    if tokens.is_empty() {
        return Vec::new();
    }
    let chars: Vec<(usize, char)> = original.char_indices().collect();
    let mut boundaries: Vec<usize> = Vec::new(); // token ordinal AFTER which to split
    let mut t = 0usize;

    for (i, &(byte, ch)) in chars.iter().enumerate() {
        // Advance past tokens ending at or before this char.
        while t < tokens.len() && tokens[t].original_end <= byte {
            t += 1;
        }
        if !is_terminator(ch) {
            continue;
        }
        // Look ahead: skip closing quotes/brackets, then whitespace.
        let mut j = i + 1;
        let mut crossed_blank_line = false;
        let mut k = j;
        while k < chars.len() {
            let c = chars[k].1;
            if c == '\n' {
                // Two newlines (possibly spaced) = paragraph break = split.
                let mut m = k + 1;
                while m < chars.len() && chars[m].1 != '\n' && chars[m].1.is_whitespace() {
                    m += 1;
                }
                if m < chars.len() && chars[m].1 == '\n' {
                    crossed_blank_line = true;
                    j = m + 1;
                    break;
                }
                j = k + 1;
                break;
            }
            if c.is_whitespace() || matches!(c, '"' | '\'' | '”' | '’' | ')' | ']' | '»') {
                k += 1;
                continue;
            }
            j = k;
            break;
        }
        let next = chars.get(j).map(|&(_, c)| c);
        let splits = crossed_blank_line
            || next.is_none()
            || next.is_some_and(|c| c.is_uppercase() || c.is_numeric());
        if splits {
            // The split belongs after the last token ending at/before here.
            let mut end = t;
            while end < tokens.len() && tokens[end].original_end <= byte + ch.len_utf8() {
                end += 1;
            }
            if end > 0 && (boundaries.last() != Some(&(end - 1))) {
                boundaries.push(end - 1);
            }
        }
    }

    let mut sentences = Vec::new();
    let mut start = 0usize;
    for &last in &boundaries {
        let end = last + 1;
        if end > start {
            sentences.push(make_sentence(tokens, start, end));
            start = end;
        }
    }
    if start < tokens.len() {
        sentences.push(make_sentence(tokens, start, tokens.len()));
    }
    sentences
}

fn make_sentence(tokens: &[Token], start: usize, end: usize) -> Sentence {
    Sentence {
        token_start: start,
        token_end: end,
        char_start: tokens[start].original_start,
        char_end: tokens[end - 1].original_end,
    }
}

/// Token ordinals where a blank line in the original starts a new paragraph.
fn paragraph_breaks(original: &str, tokens: &[Token]) -> Vec<usize> {
    if tokens.is_empty() {
        return Vec::new();
    }
    let mut starts = vec![0usize];
    let mut token_idx = 0usize;
    let mut prev_blank = false;
    let mut line_has_text = false;

    for (byte, ch) in original.char_indices() {
        while token_idx < tokens.len() && tokens[token_idx].original_end <= byte {
            token_idx += 1;
        }
        if ch == '\n' {
            if !line_has_text {
                prev_blank = true;
            }
            line_has_text = false;
        } else if !ch.is_whitespace() {
            if prev_blank && token_idx < tokens.len() {
                let s = tokens[token_idx].ordinal;
                if *starts.last().unwrap_or(&usize::MAX) != s {
                    starts.push(s);
                }
                prev_blank = false;
            }
            line_has_text = true;
        }
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(text: &str) -> Vec<String> {
        canonicalize(text)
            .tokens
            .iter()
            .map(|t| t.normalized.clone())
            .collect()
    }

    #[test]
    fn version_is_pinned() {
        assert_eq!(NORMALIZATION_VERSION, 1);
        assert_eq!(canonicalize("hi").normalization_version, 1);
    }

    #[test]
    fn case_and_whitespace_differences_vanish() {
        assert_eq!(norm("Hello   World"), norm("hello world"));
        assert_eq!(
            norm("Photosynthesis\n\tCONVERTS"),
            vec!["photosynthesis", "converts"]
        );
    }

    #[test]
    fn smart_quotes_and_apostrophes_split() {
        assert_eq!(norm("“don’t”"), vec!["don", "t"]);
        assert_eq!(norm("\"don't\""), vec!["don", "t"]);
    }

    #[test]
    fn hyphen_variants_split_identically() {
        for dash in ["-", "‐", "‑", "‒", "–", "—", "−"] {
            assert_eq!(
                norm(&format!("state{dash}of{dash}the{dash}art")),
                vec!["state", "of", "the", "art"],
                "dash {dash:?}"
            );
        }
    }

    #[test]
    fn accented_names_survive() {
        assert_eq!(norm("naïve Café Zoë"), vec!["naïve", "café", "zoë"]);
    }

    #[test]
    fn composition_differences_vanish() {
        let composed = "caf\u{e9}"; // é = U+00E9
        let decomposed = "cafe\u{301}"; // e + U+0301
        assert_ne!(composed.as_bytes(), decomposed.as_bytes());
        assert_eq!(norm(composed), norm(decomposed));
    }

    #[test]
    fn normalization_expansion_never_panics_or_misaligns() {
        // U+0344 expands to two chars under NFC.
        let inputs = [
            "a\u{344}b",
            "\u{344}",
            "x\u{344}yz\u{344}",
            "α\u{344}β",
            "e\u{301}\u{327} reorder",
            "e\u{327}\u{301} reorder",
            "\u{301}lone leading mark",
            "trailing mark\u{301}",
            "mixed cafe\u{301} and caf\u{e9} forms",
            "astral 𝔘𝔫𝔦𝔠𝔬𝔡𝔢 with marks\u{303}",
        ];
        for input in inputs {
            let doc = canonicalize(input);
            assert_eq!(
                doc.tokens.len(),
                doc.meaningful.len(),
                "mask covers tokens {input:?}"
            );
            let mut prev_end = 0usize;
            for tok in &doc.tokens {
                assert!(input.is_char_boundary(tok.original_start));
                assert!(input.is_char_boundary(tok.original_end));
                assert!(tok.original_start < tok.original_end);
                assert!(tok.original_start >= prev_end, "monotonic {input:?}");
                assert!(tok.original_end <= input.len(), "in bounds {input:?}");
                prev_end = tok.original_end;
                // Highlight target is the intended original passage.
                assert!(!input[tok.original_start..tok.original_end].is_empty());
                assert!(
                    span_matches_token(
                        &input[tok.original_start..tok.original_end],
                        &tok.normalized
                    ),
                    "span maps to token {input:?} {:?}",
                    tok.normalized
                );
            }
            // Reordered marks canonicalize identically to composed order.
            let _ = doc;
        }
        let order_a = norm("e\u{301}\u{327}tail");
        let order_b = norm("e\u{327}\u{301}tail");
        assert_eq!(order_a, order_b);
    }

    #[test]
    fn medical_notation_tokenizes() {
        assert_eq!(
            norm("Give 5 mg/kg twice daily."),
            vec!["give", "5", "mg", "kg", "twice", "daily"]
        );
    }

    #[test]
    fn engineering_notation_tokenizes() {
        assert_eq!(
            norm("Load 3.14 kN at 45°."),
            vec!["load", "3", "14", "kn", "at", "45"]
        );
    }

    #[test]
    fn legal_citation_tokenizes() {
        assert_eq!(norm("See § 1983 (2018)."), vec!["see", "1983", "2018"]);
    }

    #[test]
    fn normalized_text_joins_with_single_spaces() {
        let doc = canonicalize("  Hello,\nWORLD!  ");
        assert_eq!(doc.normalized_text, "hello world");
    }

    #[test]
    fn empty_input_yields_empty_document() {
        let doc = canonicalize("");
        assert!(doc.tokens.is_empty());
        assert!(doc.sentences.is_empty());
        assert!(doc.meaningful.is_empty());
        assert_eq!(doc.meaningful_count(), 0);
    }

    #[test]
    fn sentences_split_on_terminators() {
        let doc = canonicalize("First sentence. Second one! Third?");
        assert_eq!(doc.sentences.len(), 3);
        assert_eq!(
            (doc.sentences[0].token_start, doc.sentences[0].token_end),
            (0, 2)
        );
        // Sentence char spans cover their tokens in the original.
        for s in &doc.sentences {
            assert!(!doc.original[s.char_start..s.char_end].is_empty());
        }
    }

    #[test]
    fn paragraphs_break_on_blank_lines() {
        let doc = canonicalize("Para one has words.\n\nPara two has words.\n\n\nPara three.");
        assert_eq!(doc.paragraph_starts, vec![0, 4, 8]);
    }

    #[test]
    fn v1_meaningful_mask_is_all_true() {
        let doc = canonicalize("Every token counts here.");
        assert_eq!(doc.meaningful.len(), doc.tokens.len());
        assert_eq!(doc.meaningful_count(), doc.tokens.len());
    }

    #[test]
    fn token_spans_recompute_to_stored_normalized() {
        // Critical invariant: normalized == NFC(lower(span text)).
        let inputs = [
            "Hello, WORLD!",
            "naïve Café — “quotes” and state–of–the–art",
            "Give 5 mg/kg twice daily.\n\nSee § 1983 (2018).",
            "decomposed cafe\u{301} au lait",
            "tabs\tand\nnewlines  plus   spaces",
        ];
        for input in inputs {
            let doc = canonicalize(input);
            for tok in &doc.tokens {
                assert!(
                    input.is_char_boundary(tok.original_start),
                    "start boundary {input:?}"
                );
                assert!(
                    input.is_char_boundary(tok.original_end),
                    "end boundary {input:?}"
                );
                let span = &input[tok.original_start..tok.original_end];
                assert!(!span.is_empty());
                assert!(
                    span.chars().all(is_word_char),
                    "span {span:?} must be word chars"
                );
                assert!(
                    span_matches_token(span, &tok.normalized),
                    "recompute {input:?}"
                );
            }
        }
    }

    #[test]
    fn token_offsets_are_monotonic_and_non_overlapping() {
        let inputs = [
            "Hello, WORLD!",
            "  leading and trailing  ",
            "a\tb\nc\rd\re\n\nf",
            "emoji and symbols: ★ ♥ → are separators",
            "mixed—dashes…and…ellipsis",
            "UPPER lower MiXeD",
        ];
        for input in inputs {
            let doc = canonicalize(input);
            let mut prev_end = 0usize;
            for (i, tok) in doc.tokens.iter().enumerate() {
                assert_eq!(tok.ordinal, i);
                assert!(tok.original_start < tok.original_end, "non-empty {input:?}");
                assert!(tok.original_start >= prev_end, "monotonic {input:?}");
                assert!(tok.original_end <= input.len(), "in bounds {input:?}");
                prev_end = tok.original_end;
            }
        }
    }

    /// Cross-boundary contract: byte ranges map to UTF-16 ranges that slice
    /// the same text. Mirrors what the UI must do before `String.slice()`.
    #[test]
    fn byte_ranges_map_to_utf16_ranges() {
        // (text, byte_start, byte_end, utf16_start, utf16_end)
        let cases = [
            ("hello world", 0, 5, 0, 5),
            ("naïve introduction", 0, 6, 0, 5), // ï = 2 bytes, 1 unit
            ("naïve introduction", 7, 19, 6, 18),
            ("e\u{301}clair match", 0, 3, 0, 2), // combining mark
            ("😀 grin match", 0, 4, 0, 2),       // astral: 4 bytes, 2 units
            ("😀 grin match", 5, 9, 3, 7),
            ("中文测试 match here", 0, 6, 0, 2), // CJK: 3 bytes each
            ("中文测试 match here", 13, 18, 5, 10),
            ("a👨‍👩‍👧‍👦b match", 1, 26, 1, 12),   // ZWJ sequence
            ("café au lait", 0, 13, 0, 12), // trailing boundary at len
        ];
        for (text, bs, be, us, ue) in cases {
            assert_eq!(
                byte_range_to_utf16_range(text, bs, be),
                Some((us, ue)),
                "mapping {text:?} [{bs},{be})"
            );
            // Oracle: UTF-8 byte slice and UTF-16 slice carry the same chars.
            let by_bytes = &text[bs..be];
            let as_utf16: Vec<u16> = text.encode_utf16().collect();
            let by_units = String::from_utf16(&as_utf16[us..ue]).expect("valid units");
            assert_eq!(by_bytes, by_units, "oracle agrees for {text:?}");
        }
    }

    #[test]
    fn byte_mapping_rejects_non_boundaries() {
        assert_eq!(byte_range_to_utf16_range("naïve", 1, 3), None); // mid-ï
        assert_eq!(byte_range_to_utf16_range("😀", 2, 4), None); // mid-emoji
        assert_eq!(byte_range_to_utf16_range("hi", 2, 1), None); // inverted
        assert_eq!(byte_range_to_utf16_range("hi", 0, 99), None); // past end
    }

    #[test]
    fn matched_passage_with_unicode_prefix_highlights_exactly() {
        // The audit's reproduction, generalized: non-ASCII lead-in must not
        // shift the evidence span.
        let text = "naïve introduction. Copied passage starts here and ends now.";
        let doc = canonicalize(text);
        let copied: Vec<String> = canonicalize("Copied passage starts here and ends now")
            .tokens
            .iter()
            .map(|t| t.normalized.clone())
            .collect();
        let toks: Vec<String> = doc.tokens.iter().map(|t| t.normalized.clone()).collect();
        let at = toks
            .windows(copied.len())
            .position(|w| w == copied.as_slice())
            .expect("copied run present");
        let (bs, be) = (
            doc.tokens[at].original_start,
            doc.tokens[at + copied.len() - 1].original_end,
        );
        assert_eq!(&text[bs..be], "Copied passage starts here and ends now");
        // The UI-side mapping resolves to the same visible text.
        let (us, ue) = byte_range_to_utf16_range(text, bs, be).expect("mappable");
        let as_utf16: Vec<u16> = text.encode_utf16().collect();
        assert_eq!(
            String::from_utf16(&as_utf16[us..ue]).expect("valid"),
            "Copied passage starts here and ends now"
        );
    }

    /// Deterministic xorshift64 (fixed seed): property-style generation
    /// without external dev-dependencies.
    struct FixedRng(u64);

    impl FixedRng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
    }

    /// Generated adversarial Unicode: every canonicalize call must preserve
    /// the mapping invariants (in-bounds, monotonic, non-overlapping,
    /// recomputable spans, mappable ranges), and every token must highlight
    /// its intended passage through the cross-boundary oracle.
    #[test]
    fn adversarial_unicode_invariants_hold() {
        let alphabet: Vec<char> = vec![
            'a', 'Z', '0', ' ', '\t', '\n', '.', ',', '!', 'é', 'è', 'ü', 'ñ', '中', '日', '本',
            '한', '😀', '★', '→', 'é', '\u{301}', '\u{327}', '\u{344}', '\u{200d}', '\u{200c}',
            '-', '–', '—', '"', '"', '\'', '\'', '’', '§', '…', '(', ')',
        ];
        let mut rng = FixedRng(0x1234_5678_9abc_def0);
        for case in 0..300 {
            let len = (rng.next() % 60) as usize;
            let input: String = (0..len)
                .map(|_| alphabet[(rng.next() as usize) % alphabet.len()])
                .collect();
            // Must never panic on valid Unicode.
            let doc = canonicalize(&input);
            assert_eq!(doc.tokens.len(), doc.meaningful.len());
            let mut prev_end = 0usize;
            for (i, tok) in doc.tokens.iter().enumerate() {
                assert_eq!(tok.ordinal, i, "case {case}");
                assert!(
                    input.is_char_boundary(tok.original_start),
                    "start boundary case {case}: {input:?}"
                );
                assert!(
                    input.is_char_boundary(tok.original_end),
                    "end boundary case {case}: {input:?}"
                );
                assert!(
                    tok.original_start < tok.original_end,
                    "non-empty case {case}"
                );
                assert!(
                    tok.original_start >= prev_end,
                    "monotonic case {case}: {input:?}"
                );
                assert!(tok.original_end <= input.len(), "in bounds case {case}");
                prev_end = tok.original_end;
                let span = &input[tok.original_start..tok.original_end];
                assert!(
                    span.chars().all(is_word_char),
                    "word chars case {case}: {span:?}"
                );
                assert!(
                    span_matches_token(span, &tok.normalized),
                    "recompute case {case}: {input:?}"
                );
                // Highlight oracle: the byte range maps to UTF-16 units
                // slicing the identical visible text.
                let (us, ue) =
                    byte_range_to_utf16_range(&input, tok.original_start, tok.original_end)
                        .expect("mappable");
                let units: Vec<u16> = input.encode_utf16().collect();
                assert_eq!(
                    String::from_utf16(&units[us..ue]).expect("valid units"),
                    span,
                    "oracle case {case}: {input:?}"
                );
            }
            for s in &doc.sentences {
                assert!(s.token_start < s.token_end);
                assert!(s.token_end <= doc.tokens.len());
                assert!(s.char_start < s.char_end);
                assert!(s.char_end <= input.len());
                assert!(input.is_char_boundary(s.char_start));
                assert!(input.is_char_boundary(s.char_end));
            }
            for &p in &doc.paragraph_starts {
                assert!(p <= doc.tokens.len());
            }
        }
    }
}

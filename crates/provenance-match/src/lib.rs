//! provenance-match: normalization, fingerprints/Winnowing, alignment, scoring.
//! Phase 4: canonical normalization. Exact and modified matching build on it.

pub mod common;
pub mod exact;
pub mod modified;
pub mod normalize;

#[cfg(test)]
#[path = "exact_tests.rs"]
mod exact_tests;

pub use common::{
    apply_exclusion, common_fraction, common_token_mask, filtered_coverage, is_common_range,
    prompt_token_mask, shingle_document_frequency, CommonTextConfig, ExcludedSpan, ExclusionReason,
    COMMON_TEXT_VERSION,
};
pub use exact::{
    compare_documents, directional_coverage, find_exact_matches, find_shingle_runs,
    pairwise_indices, union_len, winnow, ExactConfig, Fingerprint, TokenMatch, FINGERPRINT_VERSION,
};
#[cfg(test)]
pub(crate) use exact::{shingle_hash, FingerprintIndex};
pub use modified::{
    find_modified_matches, modified_ranges, token_similarity, ModifiedConfig, ModifiedMatch,
    MODIFIED_VERSION,
};
pub use normalize::{canonicalize, CanonicalDocument, Sentence, Token, NORMALIZATION_VERSION};
use serde::{Deserialize, Serialize};

#[must_use]
pub fn workspace_marker() -> &'static str {
    "provenance-match-ok"
}

/// Evidence type. Full span model arrives with exact engine (Phase 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceType {
    Exact,
    Modified,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_smoke_marker() {
        assert_eq!(workspace_marker(), "provenance-match-ok");
    }

    #[test]
    fn evidence_types_are_distinct() {
        assert_ne!(EvidenceType::Exact, EvidenceType::Modified);
    }
}

//! provenance-report: per-student evidence reports, .plagpack manifest, signing.
//! Phase 0 baseline — full report + integrity arrives Phase 18-20.

#[must_use]
pub fn workspace_marker() -> &'static str {
    "provenance-report-ok"
}

/// .plagpack format version. Pinned at 1 per plan_v1 Phase 18.
pub const PLAGPACK_FORMAT_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_smoke_marker() {
        assert_eq!(workspace_marker(), "provenance-report-ok");
    }

    #[test]
    fn plagpack_format_version_starts_at_one() {
        assert_eq!(PLAGPACK_FORMAT_VERSION, 1);
    }
}

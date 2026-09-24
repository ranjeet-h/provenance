//! provenance-core: sessions, solvers/students, submissions domain + SQLite storage.
//! Phase 2: sessions + students fully local. Analysis arrives in later phases.

pub mod analysis;
pub mod domain;
pub mod error;
pub mod import;
pub mod ingest;
pub mod service;
pub mod storage;

pub use domain::SessionStatus;
pub use error::CoreError;

/// Workspace smoke marker. Proves the crate links and tests run.
#[must_use]
pub fn workspace_marker() -> &'static str {
    "provenance-core-ok"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_smoke_marker() {
        assert_eq!(workspace_marker(), "provenance-core-ok");
    }

    #[test]
    fn session_status_round_trips_through_json() {
        let s = SessionStatus::Draft;
        let json = serde_json::to_string(&s).expect("serialize");
        let back: SessionStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(s, back);
    }
}

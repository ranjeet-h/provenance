//! Phase 17: immutable archived corpora stay separate from current session pairs.

use provenance_core::analysis::{self, ExactAnalysis, PassageKind};
use provenance_core::domain::{NewSession, NewStudent, SourceType};
use provenance_core::error::CoreError;
use provenance_core::service;
use provenance_core::storage::{self, AnalysisRepo, ReferenceLibraryRepo};
use sqlx::SqlitePool;

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

fn fresh_db() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool =
        block_on(storage::open(&dir.path().join("reference-library.db"))).expect("open database");
    (dir, pool)
}

fn create_session(pool: &SqlitePool, name: &str) -> String {
    block_on(service::create_session(
        pool,
        NewSession {
            name: name.to_string(),
            subject: None,
        },
    ))
    .expect("create session")
    .id
}

fn add_student(pool: &SqlitePool, session_id: &str, name: &str) -> String {
    block_on(service::add_student(
        pool,
        session_id,
        NewStudent {
            display_name: name.to_string(),
        },
    ))
    .expect("add student")
    .id
}

fn save_text(pool: &SqlitePool, student_id: &str, text: &str) {
    block_on(service::save_text_submission(
        pool,
        student_id,
        SourceType::PastedText,
        None,
        text.as_bytes(),
    ))
    .expect("save submission");
}

fn save_current_analysis(pool: &SqlitePool, session_id: &str) {
    let digest =
        block_on(analysis::session_analysis_input_hash(pool, session_id)).expect("input digest");
    let report: ExactAnalysis =
        block_on(analysis::analyze_session_exact(pool, session_id)).expect("analyze");
    block_on(AnalysisRepo::save_result(
        pool,
        session_id,
        &digest,
        &serde_json::to_string(&report).expect("serialize report"),
    ))
    .expect("save analysis");
}

const COPIED: &str = "A stable archive preserves the original submitted words for future comparison without changing evidence or inventing missing context.";

#[test]
fn archiving_requires_a_current_analysis_and_two_nonempty_submissions() {
    let (_dir, pool) = fresh_db();
    let session = create_session(&pool, "Not analyzed");
    let first = add_student(&pool, &session, "First");
    let second = add_student(&pool, &session, "Second");
    save_text(&pool, &first, COPIED);
    save_text(
        &pool,
        &second,
        "Independent words with no overlap, useful for checking this session.",
    );

    let err = block_on(service::archive_completed_session(
        &pool, &session, "History",
    ))
    .expect_err("analysis must be current before archiving");
    assert!(matches!(err, CoreError::Validation(message) if message.contains("run analysis")));

    save_current_analysis(&pool, &session);
    let library = block_on(service::archive_completed_session(
        &pool, &session, "History",
    ))
    .expect("current session report permits archival");
    assert_eq!(library.name, "History");
    assert_eq!(
        block_on(ReferenceLibraryRepo::submissions(&pool, &library.id))
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn historical_matches_remain_separate_and_archive_survives_source_changes() {
    let (_dir, pool) = fresh_db();
    let source_session = create_session(&pool, "Biology 2025");
    let source_a = add_student(&pool, &source_session, "Source Student");
    let source_b = add_student(&pool, &source_session, "Other Student");
    save_text(&pool, &source_a, COPIED);
    save_text(
        &pool,
        &source_b,
        "Independent material about geology and mineral formation.",
    );
    save_current_analysis(&pool, &source_session);
    let library = block_on(service::archive_completed_session(
        &pool,
        &source_session,
        "Biology archive",
    ))
    .expect("archive analyzed source session");

    // The library is a detached snapshot, not a live view of source rows.
    save_text(
        &pool,
        &source_a,
        "Changed source session text after the immutable archive was created.",
    );
    block_on(service::delete_session(&pool, &source_session)).expect("source can be removed");
    let archived = block_on(ReferenceLibraryRepo::submissions(&pool, &library.id))
        .expect("archived corpus survives source removal");
    assert!(archived.iter().any(|item| item.original_text == COPIED));

    let current_session = create_session(&pool, "Biology 2026");
    let current_a = add_student(&pool, &current_session, "Current Student");
    let current_b = add_student(&pool, &current_session, "Unrelated Student");
    save_text(
        &pool,
        &current_a,
        &format!("A new opening before copied material. {COPIED} A new ending after it."),
    );
    save_text(
        &pool,
        &current_b,
        "Unrelated current work discusses marine ecosystems and ocean currents.",
    );
    block_on(service::set_session_reference_libraries(
        &pool,
        &current_session,
        vec![library.id.clone()],
    ))
    .expect("select historical library");
    let report = block_on(analysis::analyze_session_exact(&pool, &current_session))
        .expect("analyze current and historical corpora");

    assert_eq!(report.compared_libraries.len(), 1);
    assert_eq!(report.compared_libraries[0].id, library.id);
    assert_eq!(report.compared_libraries[0].source_count, 2);
    assert_eq!(report.historical_matches.len(), 1);
    let historical = &report.historical_matches[0];
    assert_eq!(historical.student_id, current_a);
    assert_eq!(historical.library_id, library.id);
    assert!(historical.coverage_current.unwrap() > 0.0);
    assert!(historical
        .passages
        .iter()
        .any(|passage| passage.kind == PassageKind::Exact));
    assert!(historical.reference_text.contains(COPIED));

    // The current pair has no shared passages: historical evidence did not
    // contaminate the current-student matrix or its scoring denominator.
    assert_eq!(report.pairs.len(), 1);
    assert!(report.pairs[0].passages.is_empty());
}

#[test]
fn selected_library_membership_changes_the_analysis_digest() {
    let (_dir, pool) = fresh_db();
    let source_session = create_session(&pool, "Source");
    let a = add_student(&pool, &source_session, "A");
    let b = add_student(&pool, &source_session, "B");
    save_text(&pool, &a, COPIED);
    save_text(
        &pool,
        &b,
        "A distinct source document with enough content to analyze safely.",
    );
    save_current_analysis(&pool, &source_session);
    let library = block_on(service::archive_completed_session(
        &pool,
        &source_session,
        "Archive",
    ))
    .expect("archive");

    let current_session = create_session(&pool, "Current");
    let current_a = add_student(&pool, &current_session, "A");
    let current_b = add_student(&pool, &current_session, "B");
    save_text(
        &pool,
        &current_a,
        "Some original current words in a long enough sample for comparison.",
    );
    save_text(
        &pool,
        &current_b,
        "Some more original current words in another long enough sample.",
    );
    let before = block_on(analysis::session_analysis_input_hash(
        &pool,
        &current_session,
    ))
    .unwrap();
    block_on(service::set_session_reference_libraries(
        &pool,
        &current_session,
        vec![library.id],
    ))
    .unwrap();
    let after = block_on(analysis::session_analysis_input_hash(
        &pool,
        &current_session,
    ))
    .unwrap();
    assert_ne!(before, after);
}

#[test]
fn historical_engine_reports_lightly_modified_copies_as_modified_evidence() {
    let (_dir, pool) = fresh_db();
    let source_session = create_session(&pool, "Photosynthesis source");
    let source_a = add_student(&pool, &source_session, "Source A");
    let source_b = add_student(&pool, &source_session, "Source B");
    save_text(
        &pool,
        &source_a,
        "Photosynthesis converts light energy into chemical energy that can later be used by the plant.",
    );
    save_text(
        &pool,
        &source_b,
        "A separate biology response about cell membranes and protein transport mechanisms.",
    );
    save_current_analysis(&pool, &source_session);
    let library = block_on(service::archive_completed_session(
        &pool,
        &source_session,
        "Photosynthesis archive",
    ))
    .expect("archive historical sources");

    let current_session = create_session(&pool, "Photosynthesis current");
    let current_a = add_student(&pool, &current_session, "Current A");
    let current_b = add_student(&pool, &current_session, "Current B");
    save_text(
        &pool,
        &current_a,
        "Through photosynthesis, plants convert light into stored chemical energy that can be used later.",
    );
    save_text(
        &pool,
        &current_b,
        "An independent response discusses population genetics and inheritance across generations.",
    );
    block_on(service::set_session_reference_libraries(
        &pool,
        &current_session,
        vec![library.id],
    ))
    .expect("select history");
    let report = block_on(analysis::analyze_session_exact(&pool, &current_session))
        .expect("run historical comparison");
    let match_report = report
        .historical_matches
        .iter()
        .find(|item| item.student_id == current_a)
        .expect("modified historical evidence");
    assert!(match_report
        .passages
        .iter()
        .any(|passage| passage.kind == PassageKind::Modified));
    assert!(match_report.modified_coverage_current.unwrap() > 0.0);
}

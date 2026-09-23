//! Phase 18: portable library export/import preserves comparison behavior.

use std::io::Read;

use provenance_core::analysis::{self, ExactAnalysis};
use provenance_core::domain::{NewSession, NewStudent, SourceType};
use provenance_core::service;
use provenance_core::storage::{self, AnalysisRepo};
use sqlx::SqlitePool;
use zip::ZipArchive;

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
        block_on(storage::open(&dir.path().join("portable-library.db"))).expect("open database");
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
    .expect("session")
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
    .expect("student")
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
    .expect("submission");
}

fn save_analysis(pool: &SqlitePool, session_id: &str) {
    let digest = block_on(analysis::session_analysis_input_hash(pool, session_id)).unwrap();
    let report: ExactAnalysis =
        block_on(analysis::analyze_session_exact(pool, session_id)).unwrap();
    block_on(AnalysisRepo::save_result(
        pool,
        session_id,
        &digest,
        &serde_json::to_string(&report).unwrap(),
    ))
    .unwrap();
}

const COPIED: &str = "A portable archive carries original text and canonical text with verified content hashes, so historical passages can be checked after the original session is removed.";

#[test]
fn export_delete_import_and_compare_preserves_anonymous_historical_evidence() {
    let (_dir, pool) = fresh_db();
    let source_session = create_session(&pool, "Cell Biology 2025");
    let source_a = add_student(&pool, &source_session, "Sensitive Student Name");
    let source_b = add_student(&pool, &source_session, "Another Sensitive Name");
    save_text(&pool, &source_a, COPIED);
    save_text(
        &pool,
        &source_b,
        "Independent text about cellular membranes and their selective properties.",
    );
    save_analysis(&pool, &source_session);
    let source_library = block_on(service::archive_completed_session(
        &pool,
        &source_session,
        "Cell Biology archive",
    ))
    .expect("archive source");

    let pack = block_on(service::export_reference_library(&pool, &source_library.id))
        .expect("export package");
    let portable = provenance_report::plagpack::import_plagpack(&pack).expect("verify package");
    assert_eq!(portable.documents.len(), 2);
    assert!(portable
        .documents
        .iter()
        .all(|document| document.source_label.starts_with("Document ")));
    let mut archive = ZipArchive::new(std::io::Cursor::new(pack.clone())).expect("zip package");
    let mut serialized = String::new();
    archive
        .by_name("documents.json")
        .expect("documents entry")
        .read_to_string(&mut serialized)
        .expect("document JSON");
    assert!(!serialized.contains("Sensitive Student Name"));
    assert!(!serialized.contains("Another Sensitive Name"));
    assert!(portable.documents.iter().any(|document| {
        document.original_text == COPIED
            && document.canonical_text == provenance_match::canonicalize(COPIED).normalized_text
    }));

    block_on(service::delete_reference_library(&pool, &source_library.id))
        .expect("remove original");
    let imported = block_on(service::import_reference_library(&pool, &pack)).expect("import pack");
    assert_eq!(imported.name, "Cell Biology archive");
    let imported_sources = block_on(service::list_reference_submissions(&pool, &imported.id))
        .expect("load imported sources");
    assert_eq!(imported_sources.len(), 2);
    assert!(imported_sources
        .iter()
        .all(|source| source.source_filename.is_none()));
    assert!(imported_sources
        .iter()
        .all(|source| source.source_label.starts_with("Document ")));

    let current_session = create_session(&pool, "Cell Biology 2026");
    let current_a = add_student(&pool, &current_session, "Current student");
    let current_b = add_student(&pool, &current_session, "Unrelated student");
    save_text(
        &pool,
        &current_a,
        &format!("Opening text. {COPIED} Closing text."),
    );
    save_text(
        &pool,
        &current_b,
        "An unrelated response about ecosystems and environmental balance.",
    );
    block_on(service::set_session_reference_libraries(
        &pool,
        &current_session,
        vec![imported.id.clone()],
    ))
    .expect("select imported archive");
    let report = block_on(analysis::analyze_session_exact(&pool, &current_session))
        .expect("compare imported text");
    assert_eq!(report.compared_libraries[0].name, "Cell Biology archive");
    assert_eq!(report.historical_matches.len(), 1);
    assert_eq!(report.historical_matches[0].student_id, current_a);
    assert!(!report.historical_matches[0].passages.is_empty());
}

#[test]
fn invalid_package_is_rejected_without_creating_a_partial_library() {
    let (_dir, pool) = fresh_db();
    let error = block_on(service::import_reference_library(&pool, b"not a package"))
        .expect_err("invalid package must be rejected");
    assert!(error.user_message().contains("corrupt"));
    assert!(block_on(service::list_reference_libraries(&pool))
        .expect("libraries")
        .is_empty());
}

use provenance_core::analysis;
use provenance_core::domain::{NewSession, NewStudent, SourceType};
use provenance_core::error::CoreError;
use provenance_core::{service, storage};
use provenance_report::report::{ReportMode, SourceCorpus};

fn block_on<F, T>(future: F) -> T
where
    F: std::future::Future<Output = T>,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

fn fresh_db() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = block_on(storage::open(&dir.path().join("report.db"))).expect("open database");
    (dir, pool)
}

fn create_session(pool: &sqlx::SqlitePool, name: &str) -> String {
    block_on(service::create_session(
        pool,
        NewSession {
            name: name.into(),
            subject: Some("History".into()),
        },
    ))
    .expect("create session")
    .id
}

fn add_student(pool: &sqlx::SqlitePool, session_id: &str, name: &str) -> String {
    block_on(service::add_student(
        pool,
        session_id,
        NewStudent {
            display_name: name.into(),
        },
    ))
    .expect("add student")
    .id
}

fn save_text(pool: &sqlx::SqlitePool, student_id: &str, filename: &str, text: &str) {
    block_on(service::save_text_submission(
        pool,
        student_id,
        SourceType::TxtFile,
        Some(filename.into()),
        text.as_bytes(),
    ))
    .expect("save submission");
}

fn persist_analysis(pool: &sqlx::SqlitePool, session_id: &str) {
    let report = block_on(analysis::analyze_session_exact(pool, session_id)).expect("analyze");
    let input_hash = block_on(analysis::session_analysis_input_hash(pool, session_id))
        .expect("analysis input digest");
    let json = serde_json::to_string(&report).expect("serialize analysis");
    block_on(storage::AnalysisRepo::save_result(
        pool,
        session_id,
        &input_hash,
        &json,
    ))
    .expect("persist current analysis");
}

const SHARED: &str = "The rapid expansion of railway networks during the nineteenth century transformed trade across continents and reshaped growing cities around busy stations.";

#[test]
fn report_uses_the_current_saved_digest_and_maps_both_pair_directions() {
    let (_dir, pool) = fresh_db();
    let session_id = create_session(&pool, "History Essay");
    let first = add_student(&pool, &session_id, "Morgan First");
    let target = add_student(&pool, &session_id, "Riley Target");
    let first_text = format!("Unmatched first introduction. {SHARED} Unique first conclusion.");
    let target_text = format!("Different target opening words. {SHARED} Target's own ending.");
    save_text(&pool, &first, "first.txt", &first_text);
    save_text(&pool, &target, "target.txt", &target_text);
    persist_analysis(&pool, &session_id);

    let report = block_on(service::build_student_report(
        &pool,
        &session_id,
        &target,
        false,
        ReportMode::Teacher,
    ))
    .expect("build report for current inputs");
    assert!(report.verify_integrity());
    assert_eq!(report.payload.current_submissions_compared, 1);
    assert_eq!(report.payload.matches.len(), 1);
    assert_eq!(report.payload.matches[0].corpus, SourceCorpus::Current);
    assert!(target_text
        [report.payload.matches[0].student_start..report.payload.matches[0].student_end]
        .contains("railway networks"));
    let target_source = report
        .payload
        .sources
        .iter()
        .find(|source| source.id == report.payload.target_source_id)
        .expect("target source embedded");
    assert_eq!(target_source.text, target_text);
    assert_eq!(
        report.payload.matches[0].student_excerpt,
        target_text[report.payload.matches[0].student_start..report.payload.matches[0].student_end]
    );

    let anonymized = block_on(service::build_student_report(
        &pool,
        &session_id,
        &target,
        true,
        ReportMode::Teacher,
    ))
    .expect("build anonymized report");
    let json = serde_json::to_string(&anonymized).expect("serialize report");
    assert!(!json.contains("Riley Target"));
    assert!(!json.contains("Morgan First"));
    assert!(!json.contains("target.txt"));

    save_text(
        &pool,
        &target,
        "changed.txt",
        "Changed content that invalidates the saved report.",
    );
    let stale = block_on(service::build_student_report(
        &pool,
        &session_id,
        &target,
        false,
        ReportMode::Teacher,
    ));
    assert!(
        matches!(stale, Err(CoreError::Validation(message)) if message.contains("current inputs"))
    );
}

#[test]
fn historical_report_keeps_library_identity_and_current_scores_separate() {
    let (_dir, pool) = fresh_db();
    let archived_session = create_session(&pool, "Previous course");
    let archive_a = add_student(&pool, &archived_session, "Archive One");
    let archive_b = add_student(&pool, &archived_session, "Archive Two");
    save_text(&pool, &archive_a, "archive-one.txt", SHARED);
    save_text(
        &pool,
        &archive_b,
        "archive-two.txt",
        "A distinct archived response about ocean currents and coastal ecosystems.",
    );
    persist_analysis(&pool, &archived_session);
    let library = block_on(service::archive_completed_session(
        &pool,
        &archived_session,
        "Previous course library",
    ))
    .expect("archive completed source session");

    let session_id = create_session(&pool, "Current course");
    let target = add_student(&pool, &session_id, "Current Student");
    let peer = add_student(&pool, &session_id, "Unrelated Peer");
    let target_text = format!("A fresh introduction to the topic. {SHARED} A new conclusion.");
    save_text(&pool, &target, "current.txt", &target_text);
    save_text(
        &pool,
        &peer,
        "peer.txt",
        "The independent answer discusses marine habitats and biodiversity.",
    );
    block_on(service::set_session_reference_libraries(
        &pool,
        &session_id,
        vec![library.id.clone()],
    ))
    .expect("select historical library");
    persist_analysis(&pool, &session_id);

    let report = block_on(service::build_student_report(
        &pool,
        &session_id,
        &target,
        false,
        ReportMode::Teacher,
    ))
    .expect("build historical report");
    assert_eq!(report.payload.current_submissions_compared, 1);
    assert_eq!(report.payload.historical_submissions_compared, 2);
    assert_eq!(report.payload.historical_libraries.len(), 1);
    assert!(report.payload.matches.iter().any(|evidence| {
        evidence.corpus == SourceCorpus::Historical
            && evidence.historical_library_id.as_deref() == Some(library.id.as_str())
            && evidence.comparison_excerpt.contains("rapid expansion")
            && evidence.comparison_excerpt.contains("busy stations")
    }));
    assert!(report.payload.matches.iter().all(|evidence| {
        evidence.corpus != SourceCorpus::Historical
            || evidence.comparison_source_id != report.payload.target_source_id
    }));
}

#[test]
fn report_generation_rejects_missing_analysis_and_target_without_text() {
    let (_dir, pool) = fresh_db();
    let session_id = create_session(&pool, "No analysis yet");
    let target = add_student(&pool, &session_id, "Student");
    save_text(
        &pool,
        &target,
        "student.txt",
        "A real saved student response.",
    );
    let error = block_on(service::build_student_report(
        &pool,
        &session_id,
        &target,
        false,
        ReportMode::Teacher,
    ))
    .expect_err("missing report inputs are not interpreted as a no-match result");
    assert!(matches!(error, CoreError::Validation(message) if message.contains("run analysis")));
}

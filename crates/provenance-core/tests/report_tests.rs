use ed25519_dalek::SigningKey;
use provenance_core::analysis;
use provenance_core::domain::{NewSession, NewStudent, SourceType};
use provenance_core::error::CoreError;
use provenance_core::{service, storage};
use provenance_report::report::{ReportMode, SourceCorpus};
use provenance_report::signature::verify_certified_student_report;

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

#[test]
fn locking_signs_the_complete_snapshot_and_freezes_session_mutations() {
    let (_dir, pool) = fresh_db();
    let session_id = create_session(&pool, "Lock contract");
    let first = add_student(&pool, &session_id, "One Student");
    let target = add_student(&pool, &session_id, "Two Student");
    save_text(
        &pool,
        &first,
        "one.txt",
        "An original answer with distinct content.",
    );
    save_text(
        &pool,
        &target,
        "two.txt",
        "A separate response with a different conclusion.",
    );
    let key = SigningKey::from_bytes(&[11; 32]);

    let unanalysed = block_on(service::lock_session(&pool, &session_id, &key))
        .expect_err("locking without a current analysis is rejected");
    assert!(
        matches!(unanalysed, CoreError::Validation(message) if message.contains("run analysis"))
    );

    persist_analysis(&pool, &session_id);
    let summary =
        block_on(service::lock_session(&pool, &session_id, &key)).expect("signed session lock");
    assert_eq!(summary.session_id, session_id);
    assert_eq!(summary.manifest_sha256.len(), 64);
    assert_eq!(
        block_on(service::get_session_lock(&pool, &session_id)).unwrap(),
        Some(summary.clone())
    );
    assert_eq!(
        block_on(service::get_session(&pool, &session_id))
            .unwrap()
            .status,
        provenance_core::domain::SessionStatus::Locked
    );

    let certified = block_on(service::certify_student_report(
        &pool,
        &session_id,
        &target,
        false,
        &key,
    ))
    .expect("certified report from locked inputs");
    verify_certified_student_report(&certified).unwrap();
    assert_eq!(certified.lock_sha256, summary.manifest_sha256);

    let edit = block_on(service::save_text_submission(
        &pool,
        &target,
        SourceType::TxtFile,
        None,
        b"This changed submission must be blocked after locking.",
    ))
    .expect_err("locked submissions cannot change");
    assert!(matches!(edit, CoreError::LockedMutation { .. }));
    let rerun = block_on(analysis::analyze_session_exact(&pool, &session_id))
        .expect_err("locked analysis cannot be rerun");
    assert!(matches!(rerun, CoreError::LockedMutation { .. }));
    let second_lock = block_on(service::lock_session(&pool, &session_id, &key))
        .expect_err("locked sessions cannot be re-signed or unlocked");
    assert!(matches!(second_lock, CoreError::LockedMutation { .. }));
}

#[test]
fn lock_verification_fails_if_the_stored_session_changes() {
    let (_dir, pool) = fresh_db();
    let session_id = create_session(&pool, "Tamper test");
    let first = add_student(&pool, &session_id, "First");
    let second = add_student(&pool, &session_id, "Second");
    save_text(&pool, &first, "one.txt", "Independent original response.");
    save_text(&pool, &second, "two.txt", "Another independent response.");
    persist_analysis(&pool, &session_id);
    let key = SigningKey::from_bytes(&[13; 32]);
    block_on(service::lock_session(&pool, &session_id, &key)).unwrap();

    block_on(async {
        sqlx::query("UPDATE sessions SET name = 'Tampered name' WHERE id = ?")
            .bind(&session_id)
            .execute(&pool)
            .await
            .unwrap();
    });
    let verified = block_on(service::get_session_lock(&pool, &session_id));
    assert!(
        matches!(verified, Err(CoreError::Validation(message)) if message.contains("changed after locking"))
    );
}

#[test]
fn lock_verification_rejects_changed_saved_analysis_json() {
    let (_dir, pool) = fresh_db();
    let session_id = create_session(&pool, "Analysis snapshot tamper test");
    let first = add_student(&pool, &session_id, "First");
    let second = add_student(&pool, &session_id, "Second");
    save_text(
        &pool,
        &first,
        "one.txt",
        "Independent source response alpha.",
    );
    save_text(
        &pool,
        &second,
        "two.txt",
        "Independent source response beta.",
    );
    persist_analysis(&pool, &session_id);
    let key = SigningKey::from_bytes(&[17; 32]);
    block_on(service::lock_session(&pool, &session_id, &key)).unwrap();

    block_on(async {
        let report_json: String =
            sqlx::query_scalar("SELECT report_json FROM analysis_results WHERE session_id = ?")
                .bind(&session_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let mut altered: analysis::ExactAnalysis = serde_json::from_str(&report_json).unwrap();
        altered.common_text_applied = !altered.common_text_applied;
        sqlx::query("UPDATE analysis_results SET report_json = ? WHERE session_id = ?")
            .bind(serde_json::to_string(&altered).unwrap())
            .bind(&session_id)
            .execute(&pool)
            .await
            .unwrap();
    });
    let verified = block_on(service::get_session_lock(&pool, &session_id));
    assert!(
        matches!(verified, Err(CoreError::Validation(message)) if message.contains("changed after locking"))
    );
}

#[test]
fn one_submission_self_check_uses_only_the_selected_reference_corpus() {
    let (_dir, pool) = fresh_db();
    let source_session = create_session(&pool, "Archived writing sample");
    let source_a = add_student(&pool, &source_session, "Archive source A");
    let source_b = add_student(&pool, &source_session, "Archive source B");
    save_text(&pool, &source_a, "source-a.txt", SHARED);
    save_text(
        &pool,
        &source_b,
        "source-b.txt",
        "An unrelated source about coastal ecosystems and marine biodiversity.",
    );
    persist_analysis(&pool, &source_session);
    let library = block_on(service::archive_completed_session(
        &pool,
        &source_session,
        "Writing examples",
    ))
    .expect("archive analyzed historical texts");

    let session_id = create_session(&pool, "Single student self-check");
    let student = add_student(&pool, &session_id, "Current student");
    let current_text = format!("An original opening statement. {SHARED} A separate ending.");
    save_text(&pool, &student, "current.txt", &current_text);
    block_on(service::set_session_reference_libraries(
        &pool,
        &session_id,
        vec![library.id.clone()],
    ))
    .expect("select explicit historical corpus");
    persist_analysis(&pool, &session_id);

    let self_check = block_on(service::build_student_report(
        &pool,
        &session_id,
        &student,
        false,
        ReportMode::SelfCheck,
    ))
    .expect("build one-submission self-check report");
    assert_eq!(self_check.payload.report_mode, ReportMode::SelfCheck);
    assert_eq!(self_check.payload.current_submissions_compared, 0);
    assert_eq!(self_check.payload.historical_submissions_compared, 2);
    assert_eq!(self_check.payload.historical_libraries.len(), 1);
    assert!(self_check.payload.matches.iter().any(|item| {
        item.corpus == SourceCorpus::Historical
            && item.comparison_excerpt.contains("railway networks")
    }));

    let key = SigningKey::from_bytes(&[19; 32]);
    let lock = block_on(service::lock_session(&pool, &session_id, &key))
        .expect("one current submission and selected archive can be locked");
    let remove_selected_library = block_on(service::delete_reference_library(&pool, &library.id))
        .expect_err("locked certificate corpus cannot be deleted");
    assert!(matches!(
        remove_selected_library,
        CoreError::LockedMutation { .. }
    ));
    assert_eq!(
        block_on(service::get_session_lock(&pool, &session_id)).unwrap(),
        Some(lock)
    );

    let no_sources = create_session(&pool, "No comparison corpus");
    let lone_student = add_student(&pool, &no_sources, "Lone student");
    save_text(
        &pool,
        &lone_student,
        "lone.txt",
        "A saved text without any selected comparison corpus.",
    );
    persist_analysis(&pool, &no_sources);
    let report = block_on(service::build_student_report(
        &pool,
        &no_sources,
        &lone_student,
        false,
        ReportMode::SelfCheck,
    ))
    .expect_err("self-check requires an explicitly selected comparison source");
    assert!(
        matches!(report, CoreError::Validation(message) if message.contains("self-check requires"))
    );
}

// Provenance Tauri shell: thin command adapters over provenance-core.
// Business behavior lives in the core crates; this file only adapts the IPC boundary.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ed25519_dalek::SigningKey;
use provenance_core::analysis::{self, AnalysisProgress, AnalysisStage, ExactAnalysis};
use provenance_core::domain::{
    NewSession, NewStudent, ReferenceLibrary, ReferenceSubmission, Session, SessionLockSummary,
    SessionUpdate, SourceType, Student, Submission,
};
use provenance_core::error::CoreError;
use provenance_core::import::FileIngestResult;
use provenance_core::{service, storage};
use provenance_report::report::{self, ReportMode};
use provenance_report::signature::{
    verify_certified_student_report as verify_signed_report, CertifiedStudentReport,
};
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, Manager, State};
use zeroize::Zeroize;

struct DbState(SqlitePool);
struct SigningKeyGate(Mutex<()>);

const SIGNING_KEY_SERVICE: &str = "Provenance Local Reports";
const SIGNING_KEY_ACCOUNT: &str = "teacher-ed25519-signing-key-v1";
const MAX_SIGNED_REPORT_JSON_BYTES: usize = 20_000_000;

#[derive(Debug, serde::Serialize)]
struct CertifiedReportExport {
    pdf_bytes: Vec<u8>,
    signed_report_json: String,
}

#[derive(Debug, serde::Serialize)]
struct SignatureVerification {
    status: &'static str,
    signing_key_id: String,
    note: &'static str,
}

fn load_or_create_signing_key(gate: &SigningKeyGate) -> Result<SigningKey, CoreError> {
    let _guard = gate.0.lock().map_err(|_| {
        CoreError::validation("local signing-key access is unavailable; restart the app")
    })?;
    let entry = keyring::Entry::new(SIGNING_KEY_SERVICE, SIGNING_KEY_ACCOUNT).map_err(|_| {
        CoreError::validation(
            "the operating-system credential store is unavailable; enable Keychain or the platform credential store to certify reports",
        )
    })?;
    let mut seed = [0_u8; 32];
    match entry.get_secret() {
        Ok(mut secret) => {
            if secret.len() != seed.len() {
                secret.zeroize();
                return Err(CoreError::validation(
                    "the stored signing key has an invalid length; certification is blocked",
                ));
            }
            seed.copy_from_slice(&secret);
            secret.zeroize();
        }
        Err(keyring::Error::NoEntry) => {
            getrandom::fill(&mut seed).map_err(|_| {
                CoreError::validation(
                    "the operating system could not generate a secure signing key",
                )
            })?;
            if entry.set_secret(&seed).is_err() {
                seed.zeroize();
                return Err(CoreError::validation(
                    "the signing key could not be saved in the operating-system credential store; certification is blocked",
                ));
            }
        }
        Err(_) => {
            return Err(CoreError::validation(
                "the operating-system credential store could not read the signing key; certification is blocked",
            ));
        }
    }
    let signing_key = SigningKey::from_bytes(&seed);
    seed.zeroize();
    Ok(signing_key)
}

/// Serializable IPC error: stable code + safe user message, never a stack trace.
#[derive(Debug, serde::Serialize)]
struct CommandError {
    code: &'static str,
    message: String,
}

impl From<CoreError> for CommandError {
    fn from(err: CoreError) -> Self {
        Self {
            code: err.code(),
            message: err.user_message(),
        }
    }
}

/// Resolve `<app_data>/provenance.db`, creating the directory. Pure for testability.
fn resolve_db_path(app_data_dir: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(app_data_dir)?;
    Ok(app_data_dir.join("provenance.db"))
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
async fn create_session(
    db: State<'_, DbState>,
    name: String,
    subject: Option<String>,
) -> Result<Session, CommandError> {
    service::create_session(&db.0, NewSession { name, subject })
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
async fn list_sessions(db: State<'_, DbState>) -> Result<Vec<Session>, CommandError> {
    service::list_sessions(&db.0)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
async fn get_session(db: State<'_, DbState>, id: String) -> Result<Session, CommandError> {
    service::get_session(&db.0, &id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
async fn update_session(
    db: State<'_, DbState>,
    id: String,
    name: Option<String>,
    subject: Option<Option<String>>,
    assignment_prompt: Option<Option<String>>,
    excluded_reference_text: Option<Option<String>>,
    exclude_common_text: Option<bool>,
) -> Result<Session, CommandError> {
    service::update_session(
        &db.0,
        &id,
        SessionUpdate {
            name,
            subject,
            assignment_prompt,
            excluded_reference_text,
            exclude_common_text,
        },
    )
    .await
    .map_err(CommandError::from)
}

#[tauri::command]
async fn delete_session(db: State<'_, DbState>, id: String) -> Result<(), CommandError> {
    service::delete_session(&db.0, &id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
async fn add_student(
    db: State<'_, DbState>,
    session_id: String,
    display_name: String,
) -> Result<Student, CommandError> {
    service::add_student(&db.0, &session_id, NewStudent { display_name })
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
async fn list_students(
    db: State<'_, DbState>,
    session_id: String,
) -> Result<Vec<Student>, CommandError> {
    service::list_students(&db.0, &session_id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
async fn remove_student(db: State<'_, DbState>, id: String) -> Result<(), CommandError> {
    service::remove_student(&db.0, &id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
async fn save_text_submission(
    db: State<'_, DbState>,
    student_id: String,
    source_type: String,
    filename: Option<String>,
    text: String,
) -> Result<Submission, CommandError> {
    let kind = SourceType::parse(&source_type).map_err(CommandError::from)?;
    service::save_text_submission(&db.0, &student_id, kind, filename, text.as_bytes())
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
async fn get_submission(
    db: State<'_, DbState>,
    student_id: String,
) -> Result<Submission, CommandError> {
    service::get_submission(&db.0, &student_id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
async fn list_submissions(
    db: State<'_, DbState>,
    session_id: String,
) -> Result<Vec<Submission>, CommandError> {
    service::list_submissions(&db.0, &session_id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
async fn archive_completed_session(
    db: State<'_, DbState>,
    session_id: String,
    library_name: String,
) -> Result<ReferenceLibrary, CommandError> {
    service::archive_completed_session(&db.0, &session_id, &library_name)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
async fn list_reference_libraries(
    db: State<'_, DbState>,
) -> Result<Vec<ReferenceLibrary>, CommandError> {
    service::list_reference_libraries(&db.0)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
async fn list_reference_submissions(
    db: State<'_, DbState>,
    library_id: String,
) -> Result<Vec<ReferenceSubmission>, CommandError> {
    service::list_reference_submissions(&db.0, &library_id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
async fn selected_reference_library_ids(
    db: State<'_, DbState>,
    session_id: String,
) -> Result<Vec<String>, CommandError> {
    service::selected_reference_library_ids(&db.0, &session_id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
async fn set_session_reference_libraries(
    db: State<'_, DbState>,
    session_id: String,
    library_ids: Vec<String>,
) -> Result<Vec<String>, CommandError> {
    service::set_session_reference_libraries(&db.0, &session_id, library_ids)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
async fn delete_reference_library(db: State<'_, DbState>, id: String) -> Result<(), CommandError> {
    service::delete_reference_library(&db.0, &id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
async fn export_reference_library(
    db: State<'_, DbState>,
    library_id: String,
) -> Result<Vec<u8>, CommandError> {
    service::export_reference_library(&db.0, &library_id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
async fn import_reference_library(
    db: State<'_, DbState>,
    bytes: Vec<u8>,
) -> Result<ReferenceLibrary, CommandError> {
    service::import_reference_library(&db.0, &bytes)
        .await
        .map_err(CommandError::from)
}

fn emit_analysis_progress(app: &AppHandle, progress: AnalysisProgress) -> Result<(), CommandError> {
    app.emit("analysis-progress", progress)
        .map_err(|_| CommandError::from(CoreError::validation("analysis progress delivery failed")))
}

/// Phase 16: run or load an input-versioned, persisted session analysis.
#[tauri::command]
async fn analyze_session(
    db: State<'_, DbState>,
    app: AppHandle,
    session_id: String,
) -> Result<ExactAnalysis, CommandError> {
    let input_hash = analysis::session_analysis_input_hash(&db.0, &session_id)
        .await
        .map_err(CommandError::from)?;
    if let Some(json) = storage::AnalysisRepo::result_for_input(&db.0, &session_id, &input_hash)
        .await
        .map_err(CommandError::from)?
    {
        let report: ExactAnalysis = serde_json::from_str(&json).map_err(|_| {
            CommandError::from(CoreError::validation(
                "stored session analysis is corrupt and must be re-created",
            ))
        })?;
        emit_analysis_progress(
            &app,
            AnalysisProgress {
                session_id,
                stage: AnalysisStage::Complete,
                fraction: 1.0,
                completed_pairs: report.pairs.len(),
                total_pairs: report.pairs.len(),
                cached_pairs: report.pairs.len(),
            },
        )?;
        return Ok(report);
    }

    let progress_app = app.clone();
    let mut progress_error = false;
    let mut last_fraction = 0.0;
    let result = analysis::analyze_session_with_progress(&db.0, &session_id, |progress| {
        last_fraction = progress.fraction;
        if progress_app.emit("analysis-progress", progress).is_err() {
            progress_error = true;
        }
    })
    .await;
    let report = match result {
        Ok(report) => report,
        Err(error) => {
            emit_analysis_progress(
                &app,
                AnalysisProgress {
                    session_id: session_id.clone(),
                    stage: AnalysisStage::Failed,
                    fraction: last_fraction,
                    completed_pairs: 0,
                    total_pairs: 0,
                    cached_pairs: 0,
                },
            )?;
            return Err(CommandError::from(error));
        }
    };
    if progress_error {
        return Err(CommandError::from(CoreError::validation(
            "analysis progress delivery failed",
        )));
    }

    let current_hash = analysis::session_analysis_input_hash(&db.0, &session_id)
        .await
        .map_err(CommandError::from)?;
    if current_hash != input_hash {
        emit_analysis_progress(
            &app,
            AnalysisProgress {
                session_id,
                stage: AnalysisStage::Failed,
                fraction: 0.95,
                completed_pairs: report.pairs.len(),
                total_pairs: report.pairs.len(),
                cached_pairs: 0,
            },
        )?;
        return Err(CommandError::from(CoreError::validation(
            "session data changed during analysis; run the analysis again",
        )));
    }
    let current_session = service::get_session(&db.0, &session_id)
        .await
        .map_err(CommandError::from)?;
    if current_session.status == provenance_core::domain::SessionStatus::Locked {
        return Err(CommandError::from(CoreError::LockedMutation {
            status: "locked".into(),
            action: "saving analysis results".into(),
        }));
    }

    emit_analysis_progress(
        &app,
        AnalysisProgress {
            session_id: session_id.clone(),
            stage: AnalysisStage::Saving,
            fraction: 0.95,
            completed_pairs: report.pairs.len(),
            total_pairs: report.pairs.len(),
            cached_pairs: 0,
        },
    )?;
    let json = serde_json::to_string(&report)
        .map_err(|_| CommandError::from(CoreError::validation("could not serialize analysis")))?;
    storage::AnalysisRepo::save_result(&db.0, &session_id, &input_hash, &json)
        .await
        .map_err(CommandError::from)?;
    emit_analysis_progress(
        &app,
        AnalysisProgress {
            session_id,
            stage: AnalysisStage::Complete,
            fraction: 1.0,
            completed_pairs: report.pairs.len(),
            total_pairs: report.pairs.len(),
            cached_pairs: 0,
        },
    )?;
    Ok(report)
}

#[tauri::command]
async fn get_session_analysis(
    db: State<'_, DbState>,
    session_id: String,
) -> Result<Option<ExactAnalysis>, CommandError> {
    let input_hash = analysis::session_analysis_input_hash(&db.0, &session_id)
        .await
        .map_err(CommandError::from)?;
    let Some(json) = storage::AnalysisRepo::result_for_input(&db.0, &session_id, &input_hash)
        .await
        .map_err(CommandError::from)?
    else {
        return Ok(None);
    };
    let report = serde_json::from_str(&json).map_err(|_| {
        CommandError::from(CoreError::validation(
            "stored session analysis is corrupt and must be re-created",
        ))
    })?;
    Ok(Some(report))
}

async fn generate_student_report_pdf_for(
    pool: &SqlitePool,
    session_id: &str,
    student_id: &str,
    anonymize: bool,
    report_mode: &str,
) -> Result<Vec<u8>, CoreError> {
    let mode = match report_mode {
        "teacher" => {
            return Err(CoreError::validation(
                "unsigned teacher PDFs are disabled; lock the session and export its signed report",
            ));
        }
        "self_check" => ReportMode::SelfCheck,
        other => {
            return Err(CoreError::validation(format!(
                "unknown report mode '{other}'; choose teacher or self_check"
            )));
        }
    };
    let student_report =
        service::build_student_report(pool, session_id, student_id, anonymize, mode).await?;
    report::render_report_pdf(&student_report)
        .map_err(|error| CoreError::validation(error.to_string()))
}

async fn generate_certified_student_report_for(
    pool: &SqlitePool,
    session_id: &str,
    student_id: &str,
    anonymize: bool,
    signing_key: &SigningKey,
) -> Result<CertifiedReportExport, CoreError> {
    let certified =
        service::certify_student_report(pool, session_id, student_id, anonymize, signing_key)
            .await?;
    let pdf_bytes = report::render_certified_report_pdf(&certified)
        .map_err(|error| CoreError::validation(error.to_string()))?;
    let signed_report_json = serde_json::to_string(&certified)
        .map_err(|_| CoreError::validation("could not serialize the signed report"))?;
    Ok(CertifiedReportExport {
        pdf_bytes,
        signed_report_json,
    })
}

#[tauri::command]
async fn generate_student_report_pdf(
    db: State<'_, DbState>,
    session_id: String,
    student_id: String,
    anonymize: bool,
    report_mode: String,
) -> Result<Vec<u8>, CommandError> {
    generate_student_report_pdf_for(&db.0, &session_id, &student_id, anonymize, &report_mode)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
async fn lock_session(
    db: State<'_, DbState>,
    signing_gate: State<'_, SigningKeyGate>,
    session_id: String,
) -> Result<SessionLockSummary, CommandError> {
    service::validate_session_can_lock(&db.0, &session_id)
        .await
        .map_err(CommandError::from)?;
    let signing_key = load_or_create_signing_key(&signing_gate).map_err(CommandError::from)?;
    service::lock_session(&db.0, &session_id, &signing_key)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
async fn get_session_lock(
    db: State<'_, DbState>,
    session_id: String,
) -> Result<Option<SessionLockSummary>, CommandError> {
    service::get_session_lock(&db.0, &session_id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
async fn generate_certified_student_report(
    db: State<'_, DbState>,
    signing_gate: State<'_, SigningKeyGate>,
    session_id: String,
    student_id: String,
    anonymize: bool,
) -> Result<CertifiedReportExport, CommandError> {
    let signing_key = load_or_create_signing_key(&signing_gate).map_err(CommandError::from)?;
    generate_certified_student_report_for(&db.0, &session_id, &student_id, anonymize, &signing_key)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
fn verify_certified_student_report(
    report_json: String,
) -> Result<SignatureVerification, CommandError> {
    if report_json.len() > MAX_SIGNED_REPORT_JSON_BYTES {
        return Err(CoreError::validation(
            "signed report file exceeds the 20 MB verification limit",
        )
        .into());
    }
    let certified: CertifiedStudentReport = serde_json::from_str(&report_json).map_err(|_| {
        CommandError::from(CoreError::validation(
            "signed report JSON is invalid or uses an unsupported schema",
        ))
    })?;
    verify_signed_report(&certified).map_err(|error| {
        CommandError::from(CoreError::validation(format!(
            "signed report verification failed: {error}"
        )))
    })?;
    Ok(SignatureVerification {
        status: "verified",
        signing_key_id: certified.signature.key_id,
        note: "Signature integrity is valid. Confirm the key ID with the teacher; the embedded key alone does not prove a person's identity.",
    })
}

/// Upload a digital text file (TXT/Markdown/PDF/DOCX). Scanned/image-only
/// files return a validation error and are never stored.
#[tauri::command]
async fn save_file_submission(
    db: State<'_, DbState>,
    student_id: String,
    filename: Option<String>,
    bytes: Vec<u8>,
) -> Result<FileIngestResult, CommandError> {
    service::save_file_submission(&db.0, &student_id, filename, &bytes)
        .await
        .map_err(CommandError::from)
}

/// Developer inspection for Phase 4: canonical tokens + original ranges.
/// Guarded in size; this is a diagnostics tool, not a submission path.
#[tauri::command]
fn inspect_text(text: String) -> Result<provenance_match::CanonicalDocument, CommandError> {
    if text.chars().count() > 200_000 {
        return Err(CoreError::validation("inspection text is too long").into());
    }
    Ok(provenance_match::canonicalize(&text))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir().map_err(|e| {
                Box::new(std::io::Error::other(e.to_string())) as Box<dyn std::error::Error>
            })?;
            let db_path = resolve_db_path(&data_dir)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            let pool = tauri::async_runtime::block_on(storage::open(&db_path)).map_err(|e| {
                Box::new(std::io::Error::other(e.to_string())) as Box<dyn std::error::Error>
            })?;
            app.manage(DbState(pool));
            app.manage(SigningKeyGate(Mutex::new(())));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            create_session,
            list_sessions,
            get_session,
            update_session,
            delete_session,
            add_student,
            list_students,
            remove_student,
            save_text_submission,
            get_submission,
            list_submissions,
            save_file_submission,
            analyze_session,
            get_session_analysis,
            generate_student_report_pdf,
            lock_session,
            get_session_lock,
            generate_certified_student_report,
            verify_certified_student_report,
            archive_completed_session,
            list_reference_libraries,
            list_reference_submissions,
            selected_reference_library_ids,
            set_session_reference_libraries,
            delete_reference_library,
            export_reference_library,
            import_reference_library,
            inspect_text
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_db_path_creates_dir_and_names_file() {
        let base = tempfile::tempdir().expect("tempdir");
        let nested = base.path().join("a").join("b");
        let db = resolve_db_path(&nested).expect("resolve");
        assert_eq!(db, nested.join("provenance.db"));
        assert!(nested.is_dir());
    }

    #[test]
    fn command_errors_carry_stable_codes_without_internals() {
        let err = CommandError::from(CoreError::not_found("session not found"));
        assert_eq!(err.code, "not_found");
        assert_eq!(err.message, "session not found");

        let db_err = CommandError::from(CoreError::Database(sqlx::Error::RowNotFound));
        assert_eq!(db_err.code, "database");
        assert!(!db_err.message.contains("RowNotFound"));

        let locked = CommandError::from(CoreError::LockedMutation {
            status: "locked".to_string(),
            action: "editing".to_string(),
        });
        assert_eq!(locked.code, "locked");

        // Serializable for the IPC boundary.
        let json = serde_json::to_string(&locked).expect("serialize");
        assert!(json.contains("\"code\":\"locked\""));
    }

    #[test]
    fn report_pdf_command_contract_returns_a_verifiable_local_pdf() {
        tauri::async_runtime::block_on(async {
            let base = tempfile::tempdir().expect("temp directory");
            let pool = storage::open(&base.path().join("report.db"))
                .await
                .expect("database");
            let session = service::create_session(
                &pool,
                NewSession {
                    name: "Report command test".into(),
                    subject: Some("History".into()),
                },
            )
            .await
            .expect("session");
            let student = service::add_student(
                &pool,
                &session.id,
                NewStudent {
                    display_name: "Report Student".into(),
                },
            )
            .await
            .expect("student");
            let peer = service::add_student(
                &pool,
                &session.id,
                NewStudent {
                    display_name: "Comparison Student".into(),
                },
            )
            .await
            .expect("comparison student");
            service::save_text_submission(
                &pool,
                &student.id,
                SourceType::PastedText,
                None,
                b"A complete text submission used to verify local report generation.",
            )
            .await
            .expect("submission");
            service::save_text_submission(
                &pool,
                &peer.id,
                SourceType::PastedText,
                None,
                b"A separate comparison response with different wording.",
            )
            .await
            .expect("comparison submission");
            let report = analysis::analyze_session_exact(&pool, &session.id)
                .await
                .expect("analysis");
            let input_hash = analysis::session_analysis_input_hash(&pool, &session.id)
                .await
                .expect("input hash");
            let json = serde_json::to_string(&report).expect("analysis JSON");
            storage::AnalysisRepo::save_result(&pool, &session.id, &input_hash, &json)
                .await
                .expect("persist analysis");

            let pdf = generate_student_report_pdf_for(
                &pool,
                &session.id,
                &student.id,
                false,
                "self_check",
            )
            .await
            .expect("self-check PDF command contract");
            assert!(pdf.starts_with(b"%PDF-"));
            let text = provenance_report::report::extract_pdf_text(&pdf).expect("PDF text");
            assert!(text.contains("Report Student"));
            assert!(text.contains("This report identifies matching or reused content"));
        });
    }

    #[test]
    fn certified_report_command_contract_signs_pdf_and_verifiable_json() {
        tauri::async_runtime::block_on(async {
            let base = tempfile::tempdir().expect("temp directory");
            let pool = storage::open(&base.path().join("certified-report.db"))
                .await
                .expect("database");
            let session = service::create_session(
                &pool,
                NewSession {
                    name: "Certified command test".into(),
                    subject: Some("History".into()),
                },
            )
            .await
            .expect("session");
            let first = service::add_student(
                &pool,
                &session.id,
                NewStudent {
                    display_name: "First Student".into(),
                },
            )
            .await
            .expect("first student");
            let target = service::add_student(
                &pool,
                &session.id,
                NewStudent {
                    display_name: "Report Student".into(),
                },
            )
            .await
            .expect("target student");
            service::save_text_submission(
                &pool,
                &first.id,
                SourceType::PastedText,
                None,
                b"An independent source passage for the signed report.",
            )
            .await
            .expect("first submission");
            service::save_text_submission(
                &pool,
                &target.id,
                SourceType::PastedText,
                None,
                b"A separate response used to verify the teacher certificate.",
            )
            .await
            .expect("target submission");
            let report = analysis::analyze_session_exact(&pool, &session.id)
                .await
                .expect("analysis");
            let input_hash = analysis::session_analysis_input_hash(&pool, &session.id)
                .await
                .expect("input digest");
            storage::AnalysisRepo::save_result(
                &pool,
                &session.id,
                &input_hash,
                &serde_json::to_string(&report).unwrap(),
            )
            .await
            .expect("persist analysis");
            let key = SigningKey::from_bytes(&[21; 32]);
            service::lock_session(&pool, &session.id, &key)
                .await
                .expect("lock session");

            let exported =
                generate_certified_student_report_for(&pool, &session.id, &target.id, false, &key)
                    .await
                    .expect("signed report export");
            assert!(exported.pdf_bytes.starts_with(b"%PDF-"));
            let pdf_text = report::extract_pdf_text(&exported.pdf_bytes).unwrap();
            assert!(pdf_text.contains("Teacher-certified session"));
            assert!(pdf_text.contains("This report identifies matching or reused content"));
            let verification = verify_certified_student_report(exported.signed_report_json.clone())
                .expect("JSON verification command");
            assert_eq!(verification.status, "verified");

            let mut altered: CertifiedStudentReport =
                serde_json::from_str(&exported.signed_report_json).unwrap();
            altered.report.payload.session_name.push('!');
            let altered_json = serde_json::to_string(&altered).unwrap();
            assert!(verify_certified_student_report(altered_json).is_err());
        });
    }
}

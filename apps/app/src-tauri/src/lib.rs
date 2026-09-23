// Provenance Tauri shell: thin command adapters over provenance-core.
// Business behavior lives in the core crates; this file only adapts the IPC boundary.

use std::path::{Path, PathBuf};

use provenance_core::analysis::{self, AnalysisProgress, AnalysisStage, ExactAnalysis};
use provenance_core::domain::{
    NewSession, NewStudent, Session, SessionUpdate, SourceType, Student, Submission,
};
use provenance_core::error::CoreError;
use provenance_core::import::FileIngestResult;
use provenance_core::{service, storage};
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, Manager, State};

struct DbState(SqlitePool);

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
}

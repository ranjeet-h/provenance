//! Use-cases for Phase 2: sessions + students, draft-guarded.
//! Tauri commands stay thin and delegate here.

use sqlx::SqlitePool;

use crate::domain::{
    now_iso, NewSession, NewStudent, Session, SessionStatus, SessionUpdate, SourceType, Student,
    Submission,
};
use crate::error::CoreError;
use crate::import::{extract_file, FileIngestResult};
use crate::ingest::{clean_filename, validate_submission_bytes};
use crate::storage::{SessionPatch, SessionRepo, StudentRepo, SubmissionRepo};

fn require_draft(status: SessionStatus, action: &str) -> Result<(), CoreError> {
    if status != SessionStatus::Draft {
        return Err(CoreError::LockedMutation {
            status: status.as_str().to_string(),
            action: action.to_string(),
        });
    }
    Ok(())
}

pub async fn create_session(pool: &SqlitePool, input: NewSession) -> Result<Session, CoreError> {
    let validated = input.validated()?;
    SessionRepo::insert(pool, &validated).await
}

pub async fn list_sessions(pool: &SqlitePool) -> Result<Vec<Session>, CoreError> {
    SessionRepo::list(pool).await
}

pub async fn get_session(pool: &SqlitePool, id: &str) -> Result<Session, CoreError> {
    SessionRepo::get(pool, id).await
}

pub async fn update_session(
    pool: &SqlitePool,
    id: &str,
    update: SessionUpdate,
) -> Result<Session, CoreError> {
    let current = SessionRepo::get(pool, id).await?;
    require_draft(current.status, "changing settings")?;
    let name = update.validated_name()?;
    let subject = update.validated_subject()?;
    let prompt = update.validated_prompt()?;
    let reference = update.validated_reference()?;
    SessionRepo::update(
        pool,
        id,
        SessionPatch {
            name: name.as_deref(),
            subject: opt_opt_str(&subject),
            assignment_prompt: opt_opt_str(&prompt),
            excluded_reference_text: opt_opt_str(&reference),
            exclude_common_text: update.exclude_common_text,
        },
        &now_iso(),
    )
    .await
}

fn opt_opt_str(update: &Option<Option<String>>) -> Option<Option<&str>> {
    update.as_ref().map(|inner| inner.as_deref())
}

pub async fn delete_session(pool: &SqlitePool, id: &str) -> Result<(), CoreError> {
    let current = SessionRepo::get(pool, id).await?;
    require_draft(current.status, "deleting")?;
    let removed = SessionRepo::delete(pool, id).await?;
    if removed == 0 {
        return Err(CoreError::not_found("session not found"));
    }
    Ok(())
}

pub async fn list_students(pool: &SqlitePool, session_id: &str) -> Result<Vec<Student>, CoreError> {
    SessionRepo::get(pool, session_id).await?;
    StudentRepo::list_by_session(pool, session_id).await
}

pub async fn add_student(
    pool: &SqlitePool,
    session_id: &str,
    input: NewStudent,
) -> Result<Student, CoreError> {
    let session = SessionRepo::get(pool, session_id).await?;
    require_draft(session.status, "adding students")?;
    let name = input.validated_name()?;
    StudentRepo::insert(pool, session_id, &name).await
}

pub async fn remove_student(pool: &SqlitePool, student_id: &str) -> Result<(), CoreError> {
    let student = StudentRepo::get(pool, student_id).await?;
    let session = SessionRepo::get(pool, &student.session_id).await?;
    require_draft(session.status, "removing students")?;
    let removed = StudentRepo::delete(pool, student_id).await?;
    if removed == 0 {
        return Err(CoreError::not_found("student not found"));
    }
    Ok(())
}

/// Save (or replace, while the session is a draft) a student's text submission.
pub async fn save_text_submission(
    pool: &SqlitePool,
    student_id: &str,
    source_type: SourceType,
    filename: Option<String>,
    bytes: &[u8],
) -> Result<Submission, CoreError> {
    let student = StudentRepo::get(pool, student_id).await?;
    let session = SessionRepo::get(pool, &student.session_id).await?;
    require_draft(session.status, "editing submissions")?;
    let text = validate_submission_bytes(bytes)?;
    let clean = clean_filename(filename.as_deref())?;
    SubmissionRepo::upsert(
        pool,
        student_id,
        &student.session_id,
        source_type,
        clean.as_deref(),
        &text.text,
        &text.sha256,
    )
    .await
}

pub async fn get_submission(pool: &SqlitePool, student_id: &str) -> Result<Submission, CoreError> {
    StudentRepo::get(pool, student_id).await?;
    SubmissionRepo::get_by_student(pool, student_id)
        .await?
        .ok_or_else(|| CoreError::not_found("no submission for this student yet"))
}

pub async fn list_submissions(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Vec<Submission>, CoreError> {
    SessionRepo::get(pool, session_id).await?;
    SubmissionRepo::list_by_session(pool, session_id).await
}

/// Save an uploaded digital text file (TXT/Markdown/PDF/DOCX).
pub async fn save_file_submission(
    pool: &SqlitePool,
    student_id: &str,
    filename: Option<String>,
    bytes: &[u8],
) -> Result<FileIngestResult, CoreError> {
    let student = StudentRepo::get(pool, student_id).await?;
    let session = SessionRepo::get(pool, &student.session_id).await?;
    require_draft(session.status, "editing submissions")?;
    let name = filename.unwrap_or_default();
    match extract_file(&name, bytes)? {
        FileIngestResult::Digital {
            text,
            pages,
            source,
            filename: clean,
        } => {
            let validated = validate_submission_bytes(text.as_bytes())?;
            let stored = clean_filename(Some(&clean))?;
            let submission = SubmissionRepo::upsert(
                pool,
                student_id,
                &student.session_id,
                source,
                stored.as_deref(),
                &validated.text,
                &validated.sha256,
            )
            .await?;
            Ok(FileIngestResult::Digital {
                text: submission.original_text,
                pages,
                source: submission.source_type,
                filename: submission.source_filename.unwrap_or(clean),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    // Covered by integration tests in tests/storage_tests.rs (needs a DB).
}

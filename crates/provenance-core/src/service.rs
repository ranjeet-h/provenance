//! Use-cases for Phase 2: sessions + students, draft-guarded.
//! Tauri commands stay thin and delegate here.

use std::collections::{HashMap, HashSet};

use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::analysis;
use crate::domain::{
    now_iso, NewSession, NewStudent, ReferenceLibrary, ReferenceSubmission, Session,
    SessionLockSummary, SessionStatus, SessionUpdate, SourceType, Student, Submission,
};
use crate::error::CoreError;
use crate::import::{extract_file, FileIngestResult};
use crate::ingest::{clean_filename, validate_submission_bytes};
use crate::storage::{
    AnalysisRepo, ReferenceLibraryRepo, SessionLockRecord, SessionLockRepo, SessionPatch,
    SessionRepo, StudentRepo, SubmissionRepo,
};
use provenance_report::report::{
    build_report, ComparedLibrary as ReportLibrary, ComparedSubmission, EngineVersions,
    MatchKind as ReportMatchKind, ReportEvidence, ReportExclusionInput, ReportInput, ReportMode,
    ReportSource, SourceCorpus, StudentReport,
};
use provenance_report::signature::{
    certify_student_report as certify_report, sign_bytes, signing_key_id, verify_bytes,
    CertifiedStudentReport, SignatureProof,
};

const SESSION_LOCK_CONTEXT: &[u8] = b"provenance/session-lock/v1\0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SessionLockManifest {
    schema_version: u32,
    session_id: String,
    session_name: String,
    subject: Option<String>,
    assignment_prompt: Option<String>,
    excluded_reference_text: Option<String>,
    exclude_common_text: bool,
    analysis_input_sha256: String,
    saved_analysis_sha256: String,
    engine: LockedEngine,
    students: Vec<LockedStudent>,
    submissions: Vec<LockedSubmission>,
    reference_libraries: Vec<LockedLibrary>,
    locked_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LockedEngine {
    fingerprint: u32,
    normalization: u32,
    common_text: u32,
    modified: u32,
    session_scoring: u32,
    exact_config: String,
    modified_config: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LockedStudent {
    id: String,
    display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LockedSubmission {
    id: String,
    student_id: String,
    source_type: SourceType,
    source_filename: Option<String>,
    content_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LockedLibrary {
    id: String,
    name: String,
    source_session_id: String,
    source_session_name: String,
    created_at: String,
    fingerprint_version: u32,
    normalization_version: u32,
    modified_version: u32,
    submissions: Vec<LockedReferenceSubmission>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LockedReferenceSubmission {
    id: String,
    source_label: String,
    source_filename: Option<String>,
    source_type: SourceType,
    content_sha256: String,
}

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

/// Archive a session only after its current input digest has a valid saved analysis.
/// The resulting library is a detached, read-only content snapshot.
pub async fn archive_completed_session(
    pool: &SqlitePool,
    session_id: &str,
    library_name: &str,
) -> Result<ReferenceLibrary, CoreError> {
    let session = SessionRepo::get(pool, session_id).await?;
    let name = library_name.trim();
    if name.is_empty() || name.chars().count() > crate::domain::MAX_NAME_LEN {
        return Err(CoreError::validation(
            "reference library name must contain 1 to 200 characters",
        ));
    }
    let submissions = SubmissionRepo::list_by_session(pool, session_id).await?;
    let source_count = submissions
        .iter()
        .filter(|submission| !submission.original_text.trim().is_empty())
        .count();
    if source_count < 2 {
        return Err(CoreError::validation(
            "archive requires at least two non-empty student submissions",
        ));
    }
    let input_hash = analysis::session_analysis_input_hash(pool, session_id).await?;
    let report = AnalysisRepo::result_for_input(pool, session_id, &input_hash)
        .await?
        .ok_or_else(|| {
            CoreError::validation("run analysis for the current session inputs before archiving")
        })?;
    let _: analysis::ExactAnalysis = serde_json::from_str(&report).map_err(|_| {
        CoreError::validation("the saved analysis is corrupt and the session cannot be archived")
    })?;
    ReferenceLibraryRepo::snapshot_session(
        pool,
        name,
        session_id,
        &session.name,
        provenance_match::FINGERPRINT_VERSION,
        provenance_match::NORMALIZATION_VERSION,
        provenance_match::MODIFIED_VERSION,
    )
    .await
}

pub async fn list_reference_libraries(
    pool: &SqlitePool,
) -> Result<Vec<ReferenceLibrary>, CoreError> {
    ReferenceLibraryRepo::list(pool).await
}

pub async fn list_reference_submissions(
    pool: &SqlitePool,
    library_id: &str,
) -> Result<Vec<ReferenceSubmission>, CoreError> {
    ReferenceLibraryRepo::submissions(pool, library_id).await
}

pub async fn selected_reference_library_ids(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Vec<String>, CoreError> {
    SessionRepo::get(pool, session_id).await?;
    ReferenceLibraryRepo::selected_ids(pool, session_id).await
}

pub async fn set_session_reference_libraries(
    pool: &SqlitePool,
    session_id: &str,
    library_ids: Vec<String>,
) -> Result<Vec<String>, CoreError> {
    let session = SessionRepo::get(pool, session_id).await?;
    require_draft(session.status, "changing selected reference libraries")?;
    let unique: std::collections::HashSet<&str> = library_ids.iter().map(String::as_str).collect();
    if unique.len() != library_ids.len() {
        return Err(CoreError::validation(
            "a reference library can only be selected once",
        ));
    }
    ReferenceLibraryRepo::replace_selection(pool, session_id, &library_ids).await?;
    ReferenceLibraryRepo::selected_ids(pool, session_id).await
}

pub async fn delete_reference_library(pool: &SqlitePool, id: &str) -> Result<(), CoreError> {
    if let Some(session_id) = SessionLockRepo::selected_by_locked_session(pool, id).await? {
        return Err(CoreError::LockedMutation {
            status: "locked".into(),
            action: format!("deleting a reference library selected by session {session_id}"),
        });
    }
    ReferenceLibraryRepo::delete(pool, id).await
}

pub async fn export_reference_library(
    pool: &SqlitePool,
    library_id: &str,
) -> Result<Vec<u8>, CoreError> {
    let library = ReferenceLibraryRepo::get(pool, library_id).await?;
    let references = ReferenceLibraryRepo::submissions(pool, library_id).await?;
    let sources: Vec<provenance_report::plagpack::PackSourceDocument> = references
        .into_iter()
        .map(
            |reference| provenance_report::plagpack::PackSourceDocument {
                source_type: reference.source_type.as_str().to_string(),
                original_text: reference.original_text,
                content_sha256: reference.content_sha256,
            },
        )
        .collect();
    provenance_report::plagpack::export_plagpack(
        &library.name,
        provenance_report::plagpack::PackEngineVersions {
            fingerprint: library.fingerprint_version,
            normalization: library.normalization_version,
            modified: library.modified_version,
        },
        &sources,
    )
    .map_err(|err| CoreError::validation(err.to_string()))
}

pub async fn import_reference_library(
    pool: &SqlitePool,
    bytes: &[u8],
) -> Result<ReferenceLibrary, CoreError> {
    let pack = provenance_report::plagpack::import_plagpack(bytes)
        .map_err(|err| CoreError::validation(err.to_string()))?;
    let documents: Vec<(String, SourceType, String, String)> = pack
        .documents
        .into_iter()
        .map(|document| {
            Ok((
                document.source_label,
                SourceType::parse(&document.source_type)?,
                document.original_text,
                document.content_sha256,
            ))
        })
        .collect::<Result<_, CoreError>>()?;
    ReferenceLibraryRepo::import_portable_pack(
        pool,
        &pack.library_name,
        pack.engine_versions.fingerprint,
        pack.engine_versions.normalization,
        pack.engine_versions.modified,
        &documents,
    )
    .await
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

/// Build a per-student report only from an analysis saved for the exact current
/// session input digest. Corrupt or stale analyses are errors; this function
/// never reruns analysis or substitutes an empty report.
pub async fn build_student_report(
    pool: &SqlitePool,
    session_id: &str,
    student_id: &str,
    anonymize: bool,
    report_mode: ReportMode,
) -> Result<StudentReport, CoreError> {
    let session = SessionRepo::get(pool, session_id).await?;
    let students = StudentRepo::list_by_session(pool, session_id).await?;
    let target_student = students
        .iter()
        .find(|student| student.id == student_id)
        .ok_or_else(|| CoreError::not_found("student does not belong to this session"))?;
    let submissions = SubmissionRepo::list_by_session(pool, session_id).await?;
    let by_student: HashMap<&str, &Submission> = submissions
        .iter()
        .filter(|submission| !submission.original_text.trim().is_empty())
        .map(|submission| (submission.student_id.as_str(), submission))
        .collect();
    let target_submission = by_student.get(student_id).ok_or_else(|| {
        CoreError::validation("a student report requires a non-empty saved submission")
    })?;

    let input_hash = analysis::session_analysis_input_hash(pool, session_id).await?;
    let analysis_json = AnalysisRepo::result_for_input(pool, session_id, &input_hash)
        .await?
        .ok_or_else(|| {
            CoreError::validation(
                "run analysis for the current inputs before generating a student report",
            )
        })?;
    let report: analysis::ExactAnalysis = serde_json::from_str(&analysis_json).map_err(|_| {
        CoreError::validation("the saved analysis is corrupt; run analysis again before reporting")
    })?;
    let target_coverage = report
        .per_student
        .iter()
        .find(|coverage| coverage.student_id == student_id)
        .ok_or_else(|| {
            CoreError::validation("the saved analysis does not contain this student's results")
        })?;

    let verified_submission_hash = sha256_hex(target_submission.original_text.as_bytes());
    if verified_submission_hash != target_submission.content_sha256 {
        return Err(CoreError::validation(
            "a saved submission hash does not match its text; re-import the affected submission",
        ));
    }
    let mut sources = vec![ReportSource {
        id: target_submission.id.clone(),
        label: source_label(target_student, target_submission.source_filename.as_deref()),
        corpus: SourceCorpus::Current,
        text: target_submission.original_text.clone(),
        content_sha256: verified_submission_hash,
    }];
    let mut current_comparisons = Vec::new();
    let mut source_by_student = HashMap::from([(student_id, target_submission.id.as_str())]);
    for student in &students {
        if student.id == student_id {
            continue;
        }
        let Some(submission) = by_student.get(student.id.as_str()) else {
            continue;
        };
        let digest = sha256_hex(submission.original_text.as_bytes());
        if digest != submission.content_sha256 {
            return Err(CoreError::validation(format!(
                "the saved submission hash for {} does not match its text; re-import that submission",
                student.display_name
            )));
        }
        sources.push(ReportSource {
            id: submission.id.clone(),
            label: source_label(student, submission.source_filename.as_deref()),
            corpus: SourceCorpus::Current,
            text: submission.original_text.clone(),
            content_sha256: digest.clone(),
        });
        current_comparisons.push(ComparedSubmission {
            id: student.id.clone(),
            display_name: student.display_name.clone(),
            source_id: submission.id.clone(),
            content_sha256: digest,
        });
        source_by_student.insert(&student.id, submission.id.as_str());
    }

    let historical_libraries = report
        .compared_libraries
        .iter()
        .map(|library| ReportLibrary {
            id: library.id.clone(),
            name: library.name.clone(),
            source_session_name: library.source_session_name.clone(),
            source_count: library.source_count,
        })
        .collect::<Vec<_>>();
    if report_mode == ReportMode::SelfCheck
        && current_comparisons.is_empty()
        && historical_libraries.is_empty()
    {
        return Err(CoreError::validation(
            "self-check requires at least one other current submission or a non-empty selected reference library",
        ));
    }
    let mut matches = Vec::new();
    let mut exclusions = Vec::new();
    let mut exclusion_keys = HashSet::new();
    for pair in report
        .pairs
        .iter()
        .filter(|pair| pair.a_student_id == student_id || pair.b_student_id == student_id)
    {
        let target_is_a = pair.a_student_id == student_id;
        let peer_id = if target_is_a {
            pair.b_student_id.as_str()
        } else {
            pair.a_student_id.as_str()
        };
        let target_source_id = target_submission.id.as_str();
        let peer_source_id = *source_by_student.get(peer_id).ok_or_else(|| {
            CoreError::validation("a compared student submission is missing from the report corpus")
        })?;
        for (index, passage) in pair.passages.iter().enumerate() {
            let (student_start, student_end, comparison_start, comparison_end) = if target_is_a {
                (
                    passage.a_char_start,
                    passage.a_char_end,
                    passage.b_char_start,
                    passage.b_char_end,
                )
            } else {
                (
                    passage.b_char_start,
                    passage.b_char_end,
                    passage.a_char_start,
                    passage.a_char_end,
                )
            };
            let (coverage, exact_coverage, modified_coverage) = if target_is_a {
                (
                    pair.coverage_a,
                    pair.exact_coverage_a,
                    pair.modified_coverage_a,
                )
            } else {
                (
                    pair.coverage_b,
                    pair.exact_coverage_b,
                    pair.modified_coverage_b,
                )
            };
            matches.push(ReportEvidence {
                id: format!("current-{peer_id}-{index}"),
                corpus: SourceCorpus::Current,
                historical_library_id: None,
                kind: match passage.kind {
                    analysis::PassageKind::Exact => ReportMatchKind::Exact,
                    analysis::PassageKind::Modified => ReportMatchKind::Modified,
                },
                student_source_id: target_source_id.to_string(),
                comparison_source_id: peer_source_id.to_string(),
                student_start,
                student_end,
                comparison_start,
                comparison_end,
                tokens: passage.tokens,
                common_text: passage.common_text,
                coverage,
                exact_coverage,
                modified_coverage,
            });
        }
        for excluded in &pair.excluded {
            let source_student_id = if excluded.side == analysis::ExclusionSide::A {
                pair.a_student_id.as_str()
            } else {
                pair.b_student_id.as_str()
            };
            let source_id = source_by_student.get(source_student_id).ok_or_else(|| {
                CoreError::validation("excluded current evidence references a missing submission")
            })?;
            let reason = match excluded.reason {
                provenance_match::ExclusionReason::Prompt => "Assignment question",
                provenance_match::ExclusionReason::Reference => "Instructor reference text",
                provenance_match::ExclusionReason::CommonSessionText => "Common session text",
            }
            .to_string();
            let key = (
                (*source_id).to_string(),
                excluded.char_start,
                excluded.char_end,
                reason.clone(),
            );
            if exclusion_keys.insert(key) {
                exclusions.push(ReportExclusionInput {
                    source_id: (*source_id).to_string(),
                    start: excluded.char_start,
                    end: excluded.char_end,
                    reason,
                    tokens: excluded.tokens,
                });
            }
        }
    }

    let mut historical_source_ids = HashSet::new();
    for historical in report
        .historical_matches
        .iter()
        .filter(|item| item.student_id == student_id)
    {
        let source_id = historical.reference_submission_id.clone();
        let content_sha256 = sha256_hex(historical.reference_text.as_bytes());
        if historical_source_ids.insert(source_id.clone()) {
            sources.push(ReportSource {
                id: source_id.clone(),
                label: format!(
                    "{} · {} · {}",
                    historical.library_name,
                    historical.reference_label,
                    historical
                        .reference_filename
                        .as_deref()
                        .unwrap_or("archived submission")
                ),
                corpus: SourceCorpus::Historical,
                text: historical.reference_text.clone(),
                content_sha256,
            });
        }
        for (index, passage) in historical.passages.iter().enumerate() {
            matches.push(ReportEvidence {
                id: format!("historical-{}-{index}", historical.reference_submission_id),
                corpus: SourceCorpus::Historical,
                historical_library_id: Some(historical.library_id.clone()),
                kind: match passage.kind {
                    analysis::PassageKind::Exact => ReportMatchKind::Exact,
                    analysis::PassageKind::Modified => ReportMatchKind::Modified,
                },
                student_source_id: target_submission.id.clone(),
                comparison_source_id: source_id.clone(),
                student_start: passage.a_char_start,
                student_end: passage.a_char_end,
                comparison_start: passage.b_char_start,
                comparison_end: passage.b_char_end,
                tokens: passage.tokens,
                common_text: passage.common_text,
                coverage: historical.coverage_current,
                exact_coverage: historical.exact_coverage_current,
                modified_coverage: historical.modified_coverage_current,
            });
        }
        for excluded in &historical.excluded {
            let excluded_source_id = match excluded.side {
                analysis::ExclusionSide::A => target_submission.id.as_str(),
                analysis::ExclusionSide::B => source_id.as_str(),
            };
            let reason = match excluded.reason {
                provenance_match::ExclusionReason::Prompt => "Assignment question",
                provenance_match::ExclusionReason::Reference => "Instructor reference text",
                provenance_match::ExclusionReason::CommonSessionText => "Common session text",
            }
            .to_string();
            let key = (
                excluded_source_id.to_string(),
                excluded.char_start,
                excluded.char_end,
                reason.clone(),
            );
            if exclusion_keys.insert(key) {
                exclusions.push(ReportExclusionInput {
                    source_id: excluded_source_id.to_string(),
                    start: excluded.char_start,
                    end: excluded.char_end,
                    reason,
                    tokens: excluded.tokens,
                });
            }
        }
    }

    build_report(ReportInput {
        session_id: session.id,
        session_name: session.name,
        subject: session.subject,
        target_student_id: target_student.id.clone(),
        target_student_name: target_student.display_name.clone(),
        target_source_id: target_submission.id.clone(),
        generated_at: now_iso(),
        report_mode,
        anonymize,
        current_comparisons,
        historical_libraries,
        sources,
        overall_coverage: target_coverage.coverage,
        exact_coverage: target_coverage.exact_coverage,
        modified_coverage: target_coverage.modified_coverage,
        matches,
        exclusions,
        engine_versions: EngineVersions {
            fingerprint: report.fingerprint_version,
            normalization: report.normalization_version,
            common_text: report.common_text_version,
            modified: report.modified_version,
        },
    })
    .map_err(|error| CoreError::validation(error.to_string()))
}

/// Freeze the complete current report/analysis corpus and persist its signed
/// manifest atomically with the session's `locked` state.
pub async fn lock_session(
    pool: &SqlitePool,
    session_id: &str,
    signing_key: &SigningKey,
) -> Result<SessionLockSummary, CoreError> {
    validate_session_can_lock(pool, session_id).await?;
    let locked_at = now_iso();
    let manifest = build_session_lock_manifest(pool, session_id, &locked_at).await?;
    let manifest_json = serde_json::to_string(&manifest)
        .map_err(|_| CoreError::validation("could not serialize session lock manifest"))?;
    let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
    let proof = sign_bytes(SESSION_LOCK_CONTEXT, manifest_json.as_bytes(), signing_key);
    let signature_json = serde_json::to_string(&proof)
        .map_err(|_| CoreError::validation("could not serialize session lock signature"))?;
    SessionLockRepo::insert_and_lock(
        pool,
        &SessionLockRecord {
            session_id: session_id.to_string(),
            manifest_json,
            manifest_sha256: manifest_sha256.clone(),
            signature_json,
            locked_at: locked_at.clone(),
        },
    )
    .await?;
    Ok(SessionLockSummary {
        session_id: session_id.to_string(),
        locked_at,
        manifest_sha256,
        signing_key_id: proof.key_id,
    })
}

/// Validate fresh analysis and all input hashes before the app touches the OS
/// keychain for a new signing key.
pub async fn validate_session_can_lock(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<(), CoreError> {
    let session = SessionRepo::get(pool, session_id).await?;
    if session.status == SessionStatus::Locked {
        return Err(CoreError::LockedMutation {
            status: "locked".into(),
            action: "locking again or replacing the existing certificate".into(),
        });
    }
    if SessionLockRepo::get(pool, session_id).await?.is_some() {
        return Err(CoreError::validation(
            "a session lock record already exists; it cannot be replaced or unlocked",
        ));
    }
    build_session_lock_manifest(pool, session_id, &now_iso()).await?;
    Ok(())
}

/// Verify the stored lock's digest, Ed25519 proof, frozen metadata and live
/// database inputs. Any discrepancy is an error, never an unlocked status.
pub async fn get_session_lock(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Option<SessionLockSummary>, CoreError> {
    let session = SessionRepo::get(pool, session_id).await?;
    let Some(record) = SessionLockRepo::get(pool, session_id).await? else {
        if session.status == SessionStatus::Locked {
            return Err(CoreError::validation(
                "session is marked locked but its signed lock record is missing",
            ));
        }
        return Ok(None);
    };
    if session.status != SessionStatus::Locked {
        return Err(CoreError::validation(
            "signed lock record exists but the session status is not locked",
        ));
    }
    validate_lock_record(pool, session_id, &record).await?;
    let proof: SignatureProof = serde_json::from_str(&record.signature_json)
        .map_err(|_| CoreError::validation("stored session lock signature is corrupt"))?;
    Ok(Some(SessionLockSummary {
        session_id: record.session_id,
        locked_at: record.locked_at,
        manifest_sha256: record.manifest_sha256,
        signing_key_id: proof.key_id,
    }))
}

/// Build and sign a teacher report only if it is covered by the verified
/// immutable session lock and the same OS-backed local signing key.
pub async fn certify_student_report(
    pool: &SqlitePool,
    session_id: &str,
    student_id: &str,
    anonymize: bool,
    signing_key: &SigningKey,
) -> Result<CertifiedStudentReport, CoreError> {
    let lock = get_session_lock(pool, session_id)
        .await?
        .ok_or_else(|| CoreError::validation("lock the session before certifying reports"))?;
    if signing_key_id(signing_key) != lock.signing_key_id {
        return Err(CoreError::validation(
            "the local signing key does not match this session's lock; this report cannot be certified",
        ));
    }
    let report =
        build_student_report(pool, session_id, student_id, anonymize, ReportMode::Teacher).await?;
    certify_report(
        report,
        &lock.manifest_sha256,
        &lock.locked_at,
        &now_iso(),
        signing_key,
    )
    .map_err(|error| CoreError::validation(error.to_string()))
}

async fn validate_lock_record(
    pool: &SqlitePool,
    session_id: &str,
    record: &SessionLockRecord,
) -> Result<SessionLockManifest, CoreError> {
    if record.session_id != session_id
        || sha256_hex(record.manifest_json.as_bytes()) != record.manifest_sha256
    {
        return Err(CoreError::validation(
            "session lock manifest SHA-256 verification failed",
        ));
    }
    let proof: SignatureProof = serde_json::from_str(&record.signature_json)
        .map_err(|_| CoreError::validation("stored session lock signature is corrupt"))?;
    verify_bytes(
        SESSION_LOCK_CONTEXT,
        record.manifest_json.as_bytes(),
        &proof,
    )
    .map_err(|error| {
        CoreError::validation(format!(
            "session lock signature verification failed: {error}"
        ))
    })?;
    let stored: SessionLockManifest = serde_json::from_str(&record.manifest_json)
        .map_err(|_| CoreError::validation("stored session lock manifest is corrupt"))?;
    if stored.session_id != session_id || stored.locked_at != record.locked_at {
        return Err(CoreError::validation(
            "session lock metadata does not match its signed manifest",
        ));
    }
    let current = build_session_lock_manifest(pool, session_id, &record.locked_at).await?;
    let current_json = serde_json::to_string(&current)
        .map_err(|_| CoreError::validation("could not serialize current session lock inputs"))?;
    if current != stored || current_json != record.manifest_json {
        return Err(CoreError::validation(
            "session inputs or comparison corpus changed after locking; the certificate is invalid",
        ));
    }
    Ok(stored)
}

async fn build_session_lock_manifest(
    pool: &SqlitePool,
    session_id: &str,
    locked_at: &str,
) -> Result<SessionLockManifest, CoreError> {
    let session = SessionRepo::get(pool, session_id).await?;
    let input_hash = analysis::session_analysis_input_hash(pool, session_id).await?;
    let analysis_json = AnalysisRepo::result_for_input(pool, session_id, &input_hash)
        .await?
        .ok_or_else(|| {
            CoreError::validation(
                "run analysis for the exact current session inputs before locking",
            )
        })?;
    let analyzed: analysis::ExactAnalysis = serde_json::from_str(&analysis_json).map_err(|_| {
        CoreError::validation("saved analysis is corrupt; re-run it before locking")
    })?;
    if analyzed.fingerprint_version != provenance_match::FINGERPRINT_VERSION
        || analyzed.normalization_version != provenance_match::NORMALIZATION_VERSION
        || analyzed.common_text_version != provenance_match::COMMON_TEXT_VERSION
        || analyzed.modified_version != provenance_match::MODIFIED_VERSION
    {
        return Err(CoreError::validation(
            "saved analysis engine versions do not match the current engine; re-run analysis before locking",
        ));
    }

    let mut students = StudentRepo::list_by_session(pool, session_id).await?;
    students.sort_by(|left, right| left.id.cmp(&right.id));
    let mut submissions = SubmissionRepo::list_by_session(pool, session_id).await?;
    submissions.sort_by(|left, right| left.student_id.cmp(&right.student_id));
    let mut locked_submissions = Vec::with_capacity(submissions.len());
    let mut current_nonempty = 0usize;
    for submission in submissions {
        let actual_hash = sha256_hex(submission.original_text.as_bytes());
        if actual_hash != submission.content_sha256 {
            return Err(CoreError::validation(format!(
                "submission {} content hash does not match; re-import it before locking",
                submission.id
            )));
        }
        current_nonempty += usize::from(!submission.original_text.trim().is_empty());
        locked_submissions.push(LockedSubmission {
            id: submission.id,
            student_id: submission.student_id,
            source_type: submission.source_type,
            source_filename: submission.source_filename,
            content_sha256: actual_hash,
        });
    }

    let selected_ids = ReferenceLibraryRepo::selected_ids(pool, session_id).await?;
    let mut reference_libraries = Vec::with_capacity(selected_ids.len());
    let mut historical_nonempty = 0usize;
    for library_id in selected_ids {
        let library = ReferenceLibraryRepo::get(pool, &library_id).await?;
        let mut references = ReferenceLibraryRepo::submissions(pool, &library_id).await?;
        references.sort_by(|left, right| left.id.cmp(&right.id));
        let mut locked_references = Vec::with_capacity(references.len());
        for reference in references {
            let actual_hash = sha256_hex(reference.original_text.as_bytes());
            if actual_hash != reference.content_sha256 {
                return Err(CoreError::validation(format!(
                    "reference submission {} content hash does not match; repair the library before locking",
                    reference.id
                )));
            }
            historical_nonempty += usize::from(!reference.original_text.trim().is_empty());
            locked_references.push(LockedReferenceSubmission {
                id: reference.id,
                source_label: reference.source_label,
                source_filename: reference.source_filename,
                source_type: reference.source_type,
                content_sha256: actual_hash,
            });
        }
        reference_libraries.push(LockedLibrary {
            id: library.id,
            name: library.name,
            source_session_id: library.source_session_id,
            source_session_name: library.source_session_name,
            created_at: library.created_at,
            fingerprint_version: library.fingerprint_version,
            normalization_version: library.normalization_version,
            modified_version: library.modified_version,
            submissions: locked_references,
        });
    }
    if current_nonempty < 2 && (current_nonempty == 0 || historical_nonempty == 0) {
        return Err(CoreError::validation(
            "lock requires at least two current submissions, or one current submission and a non-empty selected reference library",
        ));
    }
    reference_libraries.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(SessionLockManifest {
        schema_version: 1,
        session_id: session.id,
        session_name: session.name,
        subject: session.subject,
        assignment_prompt: session.assignment_prompt,
        excluded_reference_text: session.excluded_reference_text,
        exclude_common_text: session.exclude_common_text,
        analysis_input_sha256: input_hash,
        saved_analysis_sha256: sha256_hex(analysis_json.as_bytes()),
        engine: LockedEngine {
            fingerprint: provenance_match::FINGERPRINT_VERSION,
            normalization: provenance_match::NORMALIZATION_VERSION,
            common_text: provenance_match::COMMON_TEXT_VERSION,
            modified: provenance_match::MODIFIED_VERSION,
            session_scoring: analysis::SESSION_SCORING_VERSION,
            exact_config: format!("{:?}", provenance_match::ExactConfig::default()),
            modified_config: format!("{:?}", provenance_match::ModifiedConfig::default()),
        },
        students: students
            .into_iter()
            .map(|student| LockedStudent {
                id: student.id,
                display_name: student.display_name,
            })
            .collect(),
        submissions: locked_submissions,
        reference_libraries,
        locked_at: locked_at.to_string(),
    })
}

fn source_label(student: &Student, filename: Option<&str>) -> String {
    format!(
        "{} · {}",
        student.display_name,
        filename.unwrap_or("submission")
    )
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    // Covered by integration tests in tests/storage_tests.rs (needs a DB).
}

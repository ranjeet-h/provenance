//! SQLite storage: connection, versioned migrations, repositories.
//! All SQL is dynamic (`sqlx::query`); no compile-time database needed.

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use crate::domain::{
    new_id, now_iso, ReferenceLibrary, ReferenceSubmission, Session, SessionStatus, SourceType,
    Student, Submission, SubmissionStatus, ValidatedNewSession,
};
use crate::error::CoreError;

/// Current schema version. Bump with a new `MIGRATION_Vn` block.
pub const SCHEMA_VERSION: i64 = 7;

const MIGRATION_V1: &str = "
CREATE TABLE IF NOT EXISTS sessions (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL CHECK (length(trim(name)) > 0),
  subject TEXT NULL,
  status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'locked', 'analyzed')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS students (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
  display_name TEXT NOT NULL CHECK (length(trim(display_name)) > 0),
  created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_students_session ON students (session_id);
CREATE TABLE IF NOT EXISTS submissions (
  id TEXT PRIMARY KEY,
  student_id TEXT NOT NULL REFERENCES students (id) ON DELETE CASCADE,
  session_id TEXT NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
  source_type TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'ready')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_submissions_session ON submissions (session_id);
CREATE INDEX IF NOT EXISTS idx_submissions_student ON submissions (student_id);
";

/// Phase 3 migration: text content columns + one-submission-per-student rule.
const MIGRATION_V2: &str = "
ALTER TABLE submissions ADD COLUMN original_text TEXT NOT NULL DEFAULT '';
ALTER TABLE submissions ADD COLUMN source_filename TEXT NULL;
ALTER TABLE submissions ADD COLUMN content_sha256 TEXT NOT NULL DEFAULT '';
CREATE UNIQUE INDEX IF NOT EXISTS idx_submissions_student_unique ON submissions (student_id);
";

/// Phase 6 migration: assignment prompt/reference settings + DF toggle.
const MIGRATION_V3: &str = "
ALTER TABLE sessions ADD COLUMN assignment_prompt TEXT NULL;
ALTER TABLE sessions ADD COLUMN excluded_reference_text TEXT NULL;
ALTER TABLE sessions ADD COLUMN exclude_common_text INTEGER NOT NULL DEFAULT 1;
";

/// Phase 16: version-keyed raw pair evidence and a separately scored session result.
const MIGRATION_V4: &str = "
CREATE TABLE analysis_results (
  session_id TEXT PRIMARY KEY REFERENCES sessions (id) ON DELETE CASCADE,
  input_sha256 TEXT NOT NULL,
  report_json TEXT NOT NULL,
  completed_at TEXT NOT NULL
);
CREATE TABLE raw_pair_analyses (
  cache_key TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
  student_a_id TEXT NOT NULL,
  student_b_id TEXT NOT NULL,
  source_hash_a TEXT NOT NULL,
  source_hash_b TEXT NOT NULL,
  engine_key TEXT NOT NULL,
  evidence_json TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_raw_pair_analyses_session ON raw_pair_analyses (session_id);
";

/// Phase 17: immutable historical snapshots and per-session corpus selection.
const MIGRATION_V5: &str = "
CREATE TABLE reference_libraries (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL CHECK (length(trim(name)) > 0),
  source_session_id TEXT NOT NULL,
  source_session_name TEXT NOT NULL,
  created_at TEXT NOT NULL,
  fingerprint_version INTEGER NOT NULL,
  normalization_version INTEGER NOT NULL,
  modified_version INTEGER NOT NULL
);
CREATE INDEX idx_reference_libraries_created ON reference_libraries (created_at);
CREATE TABLE reference_submissions (
  id TEXT PRIMARY KEY,
  library_id TEXT NOT NULL REFERENCES reference_libraries (id) ON DELETE CASCADE,
  source_label TEXT NOT NULL,
  source_filename TEXT NULL,
  source_type TEXT NOT NULL,
  original_text TEXT NOT NULL,
  content_sha256 TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_reference_submissions_library ON reference_submissions (library_id);
CREATE TABLE session_reference_libraries (
  session_id TEXT NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
  library_id TEXT NOT NULL REFERENCES reference_libraries (id) ON DELETE CASCADE,
  created_at TEXT NOT NULL,
  PRIMARY KEY (session_id, library_id)
);
";

/// Phase 20: one immutable, signed manifest for every locked session.
const MIGRATION_V6: &str = "
CREATE TABLE session_locks (
  session_id TEXT PRIMARY KEY REFERENCES sessions (id) ON DELETE CASCADE,
  manifest_json TEXT NOT NULL,
  manifest_sha256 TEXT NOT NULL,
  signature_json TEXT NOT NULL,
  locked_at TEXT NOT NULL
);
";

/// Repair Phase 16 analysis tables when the recorded schema version is ahead
/// of the physical schema. This is additive and preserves any existing rows.
const MIGRATION_V7: &str = "
CREATE TABLE IF NOT EXISTS analysis_results (
  session_id TEXT PRIMARY KEY REFERENCES sessions (id) ON DELETE CASCADE,
  input_sha256 TEXT NOT NULL,
  report_json TEXT NOT NULL,
  completed_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS raw_pair_analyses (
  cache_key TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
  student_a_id TEXT NOT NULL,
  student_b_id TEXT NOT NULL,
  source_hash_a TEXT NOT NULL,
  source_hash_b TEXT NOT NULL,
  engine_key TEXT NOT NULL,
  evidence_json TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_raw_pair_analyses_session ON raw_pair_analyses (session_id);
";

async fn apply_migration(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    migration: &str,
    version: i64,
) -> Result<(), CoreError> {
    for statement in migration.split(';') {
        let sql = statement.trim();
        if sql.is_empty() {
            continue;
        }
        sqlx::query(sql)
            .execute(&mut **tx)
            .await
            .map_err(CoreError::Database)?;
    }
    sqlx::query("INSERT INTO schema_version (version) VALUES (?)")
        .bind(version)
        .execute(&mut **tx)
        .await
        .map_err(CoreError::Database)?;
    Ok(())
}

/// Open (creating if needed) the SQLite file with FK enforcement on every connection.
pub async fn connect(db_path: &Path) -> Result<SqlitePool, CoreError> {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .foreign_keys(true);
    SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await
        .map_err(CoreError::Database)
}

/// Apply pending migrations. Safe to call on every startup; idempotent.
pub async fn migrate(pool: &SqlitePool) -> Result<(), CoreError> {
    let mut tx = pool.begin().await.map_err(CoreError::Database)?;
    sqlx::query("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY)")
        .execute(&mut *tx)
        .await
        .map_err(CoreError::Database)?;
    let current: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM schema_version")
        .fetch_one(&mut *tx)
        .await
        .map_err(CoreError::Database)?;
    if current < 1 {
        apply_migration(&mut tx, MIGRATION_V1, 1).await?;
    }
    if current < 2 {
        // Phase 3: stored text + hash + filename; one submission per student.
        apply_migration(&mut tx, MIGRATION_V2, 2).await?;
    }
    if current < 3 {
        // Phase 6: assignment prompt/reference settings + DF toggle.
        apply_migration(&mut tx, MIGRATION_V3, 3).await?;
    }
    if current < 4 {
        // Phase 16: persist raw matching evidence separately from scored reports.
        apply_migration(&mut tx, MIGRATION_V4, 4).await?;
    }
    if current < 5 {
        apply_migration(&mut tx, MIGRATION_V5, 5).await?;
    }
    if current < 6 {
        apply_migration(&mut tx, MIGRATION_V6, 6).await?;
    }
    if current < 7 {
        apply_migration(&mut tx, MIGRATION_V7, 7).await?;
    }
    tx.commit().await.map_err(CoreError::Database)?;
    Ok(())
}

/// Convenience for tests and first-run: connect + migrate.
pub async fn open(db_path: &Path) -> Result<SqlitePool, CoreError> {
    let pool = connect(db_path).await?;
    migrate(&pool).await?;
    Ok(pool)
}

fn map_row_to_session(row: &sqlx::sqlite::SqliteRow) -> Result<Session, CoreError> {
    let status_raw: String = row.try_get("status").map_err(CoreError::Database)?;
    let exclude_common: i64 = row
        .try_get("exclude_common_text")
        .map_err(CoreError::Database)?;
    Ok(Session {
        id: row.try_get("id").map_err(CoreError::Database)?,
        name: row.try_get("name").map_err(CoreError::Database)?,
        subject: row.try_get("subject").map_err(CoreError::Database)?,
        status: SessionStatus::parse(&status_raw)?,
        assignment_prompt: row
            .try_get("assignment_prompt")
            .map_err(CoreError::Database)?,
        excluded_reference_text: row
            .try_get("excluded_reference_text")
            .map_err(CoreError::Database)?,
        exclude_common_text: exclude_common != 0,
        created_at: row.try_get("created_at").map_err(CoreError::Database)?,
        updated_at: row.try_get("updated_at").map_err(CoreError::Database)?,
    })
}

fn map_row_to_student(row: &sqlx::sqlite::SqliteRow) -> Result<Student, CoreError> {
    Ok(Student {
        id: row.try_get("id").map_err(CoreError::Database)?,
        session_id: row.try_get("session_id").map_err(CoreError::Database)?,
        display_name: row.try_get("display_name").map_err(CoreError::Database)?,
        created_at: row.try_get("created_at").map_err(CoreError::Database)?,
    })
}

fn map_row_to_submission(row: &sqlx::sqlite::SqliteRow) -> Result<Submission, CoreError> {
    let source_raw: String = row.try_get("source_type").map_err(CoreError::Database)?;
    let status_raw: String = row.try_get("status").map_err(CoreError::Database)?;
    Ok(Submission {
        id: row.try_get("id").map_err(CoreError::Database)?,
        student_id: row.try_get("student_id").map_err(CoreError::Database)?,
        session_id: row.try_get("session_id").map_err(CoreError::Database)?,
        source_type: SourceType::parse(&source_raw)?,
        status: SubmissionStatus::parse(&status_raw)?,
        original_text: row.try_get("original_text").map_err(CoreError::Database)?,
        source_filename: row
            .try_get("source_filename")
            .map_err(CoreError::Database)?,
        content_sha256: row.try_get("content_sha256").map_err(CoreError::Database)?,
        created_at: row.try_get("created_at").map_err(CoreError::Database)?,
        updated_at: row.try_get("updated_at").map_err(CoreError::Database)?,
    })
}

pub struct SessionRepo;

/// Validated column updates for one session row.
#[derive(Debug, Default)]
pub struct SessionPatch<'a> {
    pub name: Option<&'a str>,
    pub subject: Option<Option<&'a str>>,
    pub assignment_prompt: Option<Option<&'a str>>,
    pub excluded_reference_text: Option<Option<&'a str>>,
    pub exclude_common_text: Option<bool>,
}

impl SessionPatch<'_> {
    fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.subject.is_none()
            && self.assignment_prompt.is_none()
            && self.excluded_reference_text.is_none()
            && self.exclude_common_text.is_none()
    }
}

impl SessionRepo {
    pub async fn insert(
        pool: &SqlitePool,
        input: &ValidatedNewSession,
    ) -> Result<Session, CoreError> {
        let now = now_iso();
        let id = new_id();
        sqlx::query(
            "INSERT INTO sessions
               (id, name, subject, status, assignment_prompt, excluded_reference_text,
                exclude_common_text, created_at, updated_at)
             VALUES (?, ?, ?, 'draft', NULL, NULL, 1, ?, ?)",
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&input.subject)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(CoreError::Database)?;
        Ok(Session {
            id,
            name: input.name.clone(),
            subject: input.subject.clone(),
            status: SessionStatus::Draft,
            assignment_prompt: None,
            excluded_reference_text: None,
            exclude_common_text: true,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<Session, CoreError> {
        let row = sqlx::query("SELECT * FROM sessions WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => CoreError::not_found("session not found"),
                other => CoreError::Database(other),
            })?;
        map_row_to_session(&row)
    }

    pub async fn list(pool: &SqlitePool) -> Result<Vec<Session>, CoreError> {
        let rows = sqlx::query("SELECT * FROM sessions ORDER BY rowid ASC")
            .fetch_all(pool)
            .await
            .map_err(CoreError::Database)?;
        rows.iter().map(map_row_to_session).collect()
    }

    pub async fn update(
        pool: &SqlitePool,
        id: &str,
        patch: SessionPatch<'_>,
        now: &str,
    ) -> Result<Session, CoreError> {
        if patch.is_empty() {
            return Self::get(pool, id).await;
        }
        if let Some(n) = patch.name {
            sqlx::query("UPDATE sessions SET name = ?, updated_at = ? WHERE id = ?")
                .bind(n)
                .bind(now)
                .bind(id)
                .execute(pool)
                .await
                .map_err(CoreError::Database)?;
        }
        if let Some(s) = patch.subject {
            sqlx::query("UPDATE sessions SET subject = ?, updated_at = ? WHERE id = ?")
                .bind(s)
                .bind(now)
                .bind(id)
                .execute(pool)
                .await
                .map_err(CoreError::Database)?;
        }
        if let Some(p) = patch.assignment_prompt {
            sqlx::query("UPDATE sessions SET assignment_prompt = ?, updated_at = ? WHERE id = ?")
                .bind(p)
                .bind(now)
                .bind(id)
                .execute(pool)
                .await
                .map_err(CoreError::Database)?;
        }
        if let Some(r) = patch.excluded_reference_text {
            sqlx::query(
                "UPDATE sessions SET excluded_reference_text = ?, updated_at = ? WHERE id = ?",
            )
            .bind(r)
            .bind(now)
            .bind(id)
            .execute(pool)
            .await
            .map_err(CoreError::Database)?;
        }
        if let Some(e) = patch.exclude_common_text {
            sqlx::query("UPDATE sessions SET exclude_common_text = ?, updated_at = ? WHERE id = ?")
                .bind(i64::from(e))
                .bind(now)
                .bind(id)
                .execute(pool)
                .await
                .map_err(CoreError::Database)?;
        }
        Self::get(pool, id).await
    }

    /// Returns deleted row count (0 = unknown id).
    pub async fn delete(pool: &SqlitePool, id: &str) -> Result<u64, CoreError> {
        let done = sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await
            .map_err(CoreError::Database)?;
        Ok(done.rows_affected())
    }
}

pub struct StudentRepo;

impl StudentRepo {
    pub async fn insert(
        pool: &SqlitePool,
        session_id: &str,
        display_name: &str,
    ) -> Result<Student, CoreError> {
        let now = now_iso();
        let id = new_id();
        sqlx::query(
            "INSERT INTO students (id, session_id, display_name, created_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(session_id)
        .bind(display_name)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|e| map_fk_error(e, "session"))?;
        Ok(Student {
            id,
            session_id: session_id.to_string(),
            display_name: display_name.to_string(),
            created_at: now,
        })
    }

    pub async fn list_by_session(
        pool: &SqlitePool,
        session_id: &str,
    ) -> Result<Vec<Student>, CoreError> {
        let rows = sqlx::query("SELECT * FROM students WHERE session_id = ? ORDER BY rowid ASC")
            .bind(session_id)
            .fetch_all(pool)
            .await
            .map_err(CoreError::Database)?;
        rows.iter().map(map_row_to_student).collect()
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<Student, CoreError> {
        let row = sqlx::query("SELECT * FROM students WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => CoreError::not_found("student not found"),
                other => CoreError::Database(other),
            })?;
        map_row_to_student(&row)
    }

    /// Returns deleted row count (0 = unknown id).
    pub async fn delete(pool: &SqlitePool, id: &str) -> Result<u64, CoreError> {
        let done = sqlx::query("DELETE FROM students WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await
            .map_err(CoreError::Database)?;
        Ok(done.rows_affected())
    }
}

pub struct SubmissionRepo;

impl SubmissionRepo {
    /// Insert or replace the student's single submission (draft can be replaced pre-lock).
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert(
        pool: &SqlitePool,
        student_id: &str,
        session_id: &str,
        source_type: SourceType,
        source_filename: Option<&str>,
        original_text: &str,
        content_sha256: &str,
    ) -> Result<Submission, CoreError> {
        let now = now_iso();
        let id = new_id();
        sqlx::query(
            "INSERT INTO submissions
               (id, student_id, session_id, source_type, status,
                original_text, source_filename, content_sha256, created_at, updated_at)
             VALUES (?, ?, ?, ?, 'draft', ?, ?, ?, ?, ?)
             ON CONFLICT (student_id) DO UPDATE SET
               source_type = excluded.source_type,
               original_text = excluded.original_text,
               source_filename = excluded.source_filename,
               content_sha256 = excluded.content_sha256,
               updated_at = excluded.updated_at",
        )
        .bind(&id)
        .bind(student_id)
        .bind(session_id)
        .bind(source_type.as_str())
        .bind(original_text)
        .bind(source_filename)
        .bind(content_sha256)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|e| map_fk_error(e, "student or session"))?;
        Self::get_by_student(pool, student_id)
            .await?
            .ok_or_else(|| CoreError::not_found("submission not found"))
    }

    pub async fn get_by_student(
        pool: &SqlitePool,
        student_id: &str,
    ) -> Result<Option<Submission>, CoreError> {
        let row = sqlx::query("SELECT * FROM submissions WHERE student_id = ?")
            .bind(student_id)
            .fetch_optional(pool)
            .await
            .map_err(CoreError::Database)?;
        row.map(|r| map_row_to_submission(&r)).transpose()
    }

    pub async fn list_by_session(
        pool: &SqlitePool,
        session_id: &str,
    ) -> Result<Vec<Submission>, CoreError> {
        let rows = sqlx::query("SELECT * FROM submissions WHERE session_id = ? ORDER BY rowid ASC")
            .bind(session_id)
            .fetch_all(pool)
            .await
            .map_err(CoreError::Database)?;
        rows.iter().map(map_row_to_submission).collect()
    }
}

/// Storage for versioned raw pair evidence and current, session-scored output.
/// Results are returned only for an exact input digest match; stale rows are
/// never presented as current.
pub struct AnalysisRepo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLockRecord {
    pub session_id: String,
    pub manifest_json: String,
    pub manifest_sha256: String,
    pub signature_json: String,
    pub locked_at: String,
}

pub struct SessionLockRepo;

impl SessionLockRepo {
    pub async fn get(
        pool: &SqlitePool,
        session_id: &str,
    ) -> Result<Option<SessionLockRecord>, CoreError> {
        let row = sqlx::query(
            "SELECT session_id, manifest_json, manifest_sha256, signature_json, locked_at
             FROM session_locks WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_optional(pool)
        .await
        .map_err(CoreError::Database)?;
        row.map(|row| {
            Ok(SessionLockRecord {
                session_id: row.try_get("session_id").map_err(CoreError::Database)?,
                manifest_json: row.try_get("manifest_json").map_err(CoreError::Database)?,
                manifest_sha256: row
                    .try_get("manifest_sha256")
                    .map_err(CoreError::Database)?,
                signature_json: row.try_get("signature_json").map_err(CoreError::Database)?,
                locked_at: row.try_get("locked_at").map_err(CoreError::Database)?,
            })
        })
        .transpose()
    }

    pub async fn insert_and_lock(
        pool: &SqlitePool,
        record: &SessionLockRecord,
    ) -> Result<(), CoreError> {
        let mut tx = pool.begin().await.map_err(CoreError::Database)?;
        sqlx::query(
            "INSERT INTO session_locks
             (session_id, manifest_json, manifest_sha256, signature_json, locked_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&record.session_id)
        .bind(&record.manifest_json)
        .bind(&record.manifest_sha256)
        .bind(&record.signature_json)
        .bind(&record.locked_at)
        .execute(&mut *tx)
        .await
        .map_err(CoreError::Database)?;
        let changed = sqlx::query(
            "UPDATE sessions SET status = 'locked', updated_at = ?
             WHERE id = ? AND status != 'locked'",
        )
        .bind(&record.locked_at)
        .bind(&record.session_id)
        .execute(&mut *tx)
        .await
        .map_err(CoreError::Database)?
        .rows_affected();
        if changed != 1 {
            return Err(CoreError::LockedMutation {
                status: "locked".into(),
                action: "locking again or replacing the existing certificate".into(),
            });
        }
        tx.commit().await.map_err(CoreError::Database)
    }

    pub async fn selected_by_locked_session(
        pool: &SqlitePool,
        library_id: &str,
    ) -> Result<Option<String>, CoreError> {
        sqlx::query_scalar(
            "SELECT s.id FROM sessions s
             JOIN session_reference_libraries selected ON selected.session_id = s.id
             JOIN session_locks locks ON locks.session_id = s.id
             WHERE selected.library_id = ? AND s.status = 'locked' LIMIT 1",
        )
        .bind(library_id)
        .fetch_optional(pool)
        .await
        .map_err(CoreError::Database)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawPairEvidenceRecord {
    pub session_id: String,
    pub student_a_id: String,
    pub student_b_id: String,
    pub source_hash_a: String,
    pub source_hash_b: String,
    pub engine_key: String,
    pub evidence_json: String,
}

impl AnalysisRepo {
    pub async fn result_for_input(
        pool: &SqlitePool,
        session_id: &str,
        input_sha256: &str,
    ) -> Result<Option<String>, CoreError> {
        sqlx::query_scalar(
            "SELECT report_json FROM analysis_results
             WHERE session_id = ? AND input_sha256 = ?",
        )
        .bind(session_id)
        .bind(input_sha256)
        .fetch_optional(pool)
        .await
        .map_err(CoreError::Database)
    }

    pub async fn save_result(
        pool: &SqlitePool,
        session_id: &str,
        input_sha256: &str,
        report_json: &str,
    ) -> Result<(), CoreError> {
        sqlx::query(
            "INSERT INTO analysis_results (session_id, input_sha256, report_json, completed_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT (session_id) DO UPDATE SET
               input_sha256 = excluded.input_sha256,
               report_json = excluded.report_json,
               completed_at = excluded.completed_at",
        )
        .bind(session_id)
        .bind(input_sha256)
        .bind(report_json)
        .bind(now_iso())
        .execute(pool)
        .await
        .map_err(CoreError::Database)?;
        Ok(())
    }

    pub async fn raw_pair_evidence(
        pool: &SqlitePool,
        cache_key: &str,
    ) -> Result<Option<RawPairEvidenceRecord>, CoreError> {
        let row = sqlx::query(
            "SELECT session_id, student_a_id, student_b_id, source_hash_a,
                    source_hash_b, engine_key, evidence_json
             FROM raw_pair_analyses WHERE cache_key = ?",
        )
        .bind(cache_key)
        .fetch_optional(pool)
        .await
        .map_err(CoreError::Database)?;
        row.map(|row| {
            Ok(RawPairEvidenceRecord {
                session_id: row.try_get("session_id").map_err(CoreError::Database)?,
                student_a_id: row.try_get("student_a_id").map_err(CoreError::Database)?,
                student_b_id: row.try_get("student_b_id").map_err(CoreError::Database)?,
                source_hash_a: row.try_get("source_hash_a").map_err(CoreError::Database)?,
                source_hash_b: row.try_get("source_hash_b").map_err(CoreError::Database)?,
                engine_key: row.try_get("engine_key").map_err(CoreError::Database)?,
                evidence_json: row.try_get("evidence_json").map_err(CoreError::Database)?,
            })
        })
        .transpose()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn save_raw_pair_evidence(
        pool: &SqlitePool,
        cache_key: &str,
        session_id: &str,
        student_a_id: &str,
        student_b_id: &str,
        source_hash_a: &str,
        source_hash_b: &str,
        engine_key: &str,
        evidence_json: &str,
    ) -> Result<(), CoreError> {
        sqlx::query(
            "INSERT INTO raw_pair_analyses
               (cache_key, session_id, student_a_id, student_b_id,
                source_hash_a, source_hash_b, engine_key, evidence_json, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (cache_key) DO UPDATE SET
               evidence_json = excluded.evidence_json,
               updated_at = excluded.updated_at",
        )
        .bind(cache_key)
        .bind(session_id)
        .bind(student_a_id)
        .bind(student_b_id)
        .bind(source_hash_a)
        .bind(source_hash_b)
        .bind(engine_key)
        .bind(evidence_json)
        .bind(now_iso())
        .execute(pool)
        .await
        .map_err(CoreError::Database)?;
        Ok(())
    }
}

fn map_row_to_reference_library(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<ReferenceLibrary, CoreError> {
    Ok(ReferenceLibrary {
        id: row.try_get("id").map_err(CoreError::Database)?,
        name: row.try_get("name").map_err(CoreError::Database)?,
        source_session_id: row
            .try_get("source_session_id")
            .map_err(CoreError::Database)?,
        source_session_name: row
            .try_get("source_session_name")
            .map_err(CoreError::Database)?,
        created_at: row.try_get("created_at").map_err(CoreError::Database)?,
        fingerprint_version: row
            .try_get::<i64, _>("fingerprint_version")
            .map_err(CoreError::Database)? as u32,
        normalization_version: row
            .try_get::<i64, _>("normalization_version")
            .map_err(CoreError::Database)? as u32,
        modified_version: row
            .try_get::<i64, _>("modified_version")
            .map_err(CoreError::Database)? as u32,
    })
}

fn map_row_to_reference_submission(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<ReferenceSubmission, CoreError> {
    let source_raw: String = row.try_get("source_type").map_err(CoreError::Database)?;
    Ok(ReferenceSubmission {
        id: row.try_get("id").map_err(CoreError::Database)?,
        library_id: row.try_get("library_id").map_err(CoreError::Database)?,
        source_label: row.try_get("source_label").map_err(CoreError::Database)?,
        source_filename: row
            .try_get("source_filename")
            .map_err(CoreError::Database)?,
        source_type: SourceType::parse(&source_raw)?,
        original_text: row.try_get("original_text").map_err(CoreError::Database)?,
        content_sha256: row.try_get("content_sha256").map_err(CoreError::Database)?,
        created_at: row.try_get("created_at").map_err(CoreError::Database)?,
    })
}

/// Reference library snapshots are immutable through the repository API.
pub struct ReferenceLibraryRepo;

impl ReferenceLibraryRepo {
    #[allow(clippy::too_many_arguments)]
    pub async fn snapshot_session(
        pool: &SqlitePool,
        name: &str,
        source_session_id: &str,
        source_session_name: &str,
        fingerprint_version: u32,
        normalization_version: u32,
        modified_version: u32,
    ) -> Result<ReferenceLibrary, CoreError> {
        let mut tx = pool.begin().await.map_err(CoreError::Database)?;
        let id = new_id();
        let created_at = now_iso();
        sqlx::query(
            "INSERT INTO reference_libraries
             (id, name, source_session_id, source_session_name, created_at,
              fingerprint_version, normalization_version, modified_version)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(source_session_id)
        .bind(source_session_name)
        .bind(&created_at)
        .bind(i64::from(fingerprint_version))
        .bind(i64::from(normalization_version))
        .bind(i64::from(modified_version))
        .execute(&mut *tx)
        .await
        .map_err(CoreError::Database)?;
        sqlx::query(
            "INSERT INTO reference_submissions
             (id, library_id, source_label, source_filename, source_type,
              original_text, content_sha256, created_at)
             SELECT lower(hex(randomblob(16))), ?, st.display_name, s.source_filename,
                    s.source_type, s.original_text, s.content_sha256, s.created_at
             FROM submissions s
             JOIN students st ON st.id = s.student_id
             WHERE s.session_id = ? AND length(trim(s.original_text)) > 0
             ORDER BY st.rowid ASC",
        )
        .bind(&id)
        .bind(source_session_id)
        .execute(&mut *tx)
        .await
        .map_err(CoreError::Database)?;
        tx.commit().await.map_err(CoreError::Database)?;
        Ok(ReferenceLibrary {
            id,
            name: name.to_string(),
            source_session_id: source_session_id.to_string(),
            source_session_name: source_session_name.to_string(),
            created_at,
            fingerprint_version,
            normalization_version,
            modified_version,
        })
    }

    pub async fn import_portable_pack(
        pool: &SqlitePool,
        name: &str,
        fingerprint_version: u32,
        normalization_version: u32,
        modified_version: u32,
        documents: &[(String, SourceType, String, String)],
    ) -> Result<ReferenceLibrary, CoreError> {
        let mut tx = pool.begin().await.map_err(CoreError::Database)?;
        let id = new_id();
        let source_session_id = format!("imported-{}", new_id());
        let source_session_name = "Imported .plagpack".to_string();
        let created_at = now_iso();
        sqlx::query(
            "INSERT INTO reference_libraries
             (id, name, source_session_id, source_session_name, created_at,
              fingerprint_version, normalization_version, modified_version)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(&source_session_id)
        .bind(&source_session_name)
        .bind(&created_at)
        .bind(i64::from(fingerprint_version))
        .bind(i64::from(normalization_version))
        .bind(i64::from(modified_version))
        .execute(&mut *tx)
        .await
        .map_err(CoreError::Database)?;
        for (source_label, source_type, original_text, content_sha256) in documents {
            sqlx::query(
                "INSERT INTO reference_submissions
                 (id, library_id, source_label, source_filename, source_type,
                  original_text, content_sha256, created_at)
                 VALUES (?, ?, ?, NULL, ?, ?, ?, ?)",
            )
            .bind(new_id())
            .bind(&id)
            .bind(source_label)
            .bind(source_type.as_str())
            .bind(original_text)
            .bind(content_sha256)
            .bind(&created_at)
            .execute(&mut *tx)
            .await
            .map_err(CoreError::Database)?;
        }
        tx.commit().await.map_err(CoreError::Database)?;
        Ok(ReferenceLibrary {
            id,
            name: name.to_string(),
            source_session_id,
            source_session_name,
            created_at,
            fingerprint_version,
            normalization_version,
            modified_version,
        })
    }

    pub async fn list(pool: &SqlitePool) -> Result<Vec<ReferenceLibrary>, CoreError> {
        let rows =
            sqlx::query("SELECT * FROM reference_libraries ORDER BY created_at DESC, rowid DESC")
                .fetch_all(pool)
                .await
                .map_err(CoreError::Database)?;
        rows.iter().map(map_row_to_reference_library).collect()
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<ReferenceLibrary, CoreError> {
        let row = sqlx::query("SELECT * FROM reference_libraries WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => CoreError::not_found("reference library not found"),
                other => CoreError::Database(other),
            })?;
        map_row_to_reference_library(&row)
    }

    pub async fn submissions(
        pool: &SqlitePool,
        library_id: &str,
    ) -> Result<Vec<ReferenceSubmission>, CoreError> {
        Self::get(pool, library_id).await?;
        let rows = sqlx::query(
            "SELECT * FROM reference_submissions WHERE library_id = ? ORDER BY rowid ASC",
        )
        .bind(library_id)
        .fetch_all(pool)
        .await
        .map_err(CoreError::Database)?;
        rows.iter().map(map_row_to_reference_submission).collect()
    }

    pub async fn selected_ids(
        pool: &SqlitePool,
        session_id: &str,
    ) -> Result<Vec<String>, CoreError> {
        let rows = sqlx::query_scalar(
            "SELECT library_id FROM session_reference_libraries
             WHERE session_id = ? ORDER BY library_id",
        )
        .bind(session_id)
        .fetch_all(pool)
        .await
        .map_err(CoreError::Database)?;
        Ok(rows)
    }

    pub async fn selected_submissions(
        pool: &SqlitePool,
        session_id: &str,
    ) -> Result<Vec<(ReferenceLibrary, ReferenceSubmission)>, CoreError> {
        let rows = sqlx::query(
            "SELECT l.*, r.id AS ref_id, r.library_id AS ref_library_id,
                    r.source_label, r.source_filename, r.source_type,
                    r.original_text, r.content_sha256, r.created_at AS ref_created_at
             FROM session_reference_libraries selected
             JOIN reference_libraries l ON l.id = selected.library_id
             JOIN reference_submissions r ON r.library_id = l.id
             WHERE selected.session_id = ?
             ORDER BY l.created_at, r.rowid",
        )
        .bind(session_id)
        .fetch_all(pool)
        .await
        .map_err(CoreError::Database)?;
        rows.iter()
            .map(|row| {
                let library = map_row_to_reference_library(row)?;
                let submission = ReferenceSubmission {
                    id: row.try_get("ref_id").map_err(CoreError::Database)?,
                    library_id: row.try_get("ref_library_id").map_err(CoreError::Database)?,
                    source_label: row.try_get("source_label").map_err(CoreError::Database)?,
                    source_filename: row
                        .try_get("source_filename")
                        .map_err(CoreError::Database)?,
                    source_type: SourceType::parse(
                        &row.try_get::<String, _>("source_type")
                            .map_err(CoreError::Database)?,
                    )?,
                    original_text: row.try_get("original_text").map_err(CoreError::Database)?,
                    content_sha256: row.try_get("content_sha256").map_err(CoreError::Database)?,
                    created_at: row.try_get("ref_created_at").map_err(CoreError::Database)?,
                };
                Ok((library, submission))
            })
            .collect()
    }

    pub async fn replace_selection(
        pool: &SqlitePool,
        session_id: &str,
        library_ids: &[String],
    ) -> Result<(), CoreError> {
        let mut tx = pool.begin().await.map_err(CoreError::Database)?;
        sqlx::query("DELETE FROM session_reference_libraries WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(CoreError::Database)?;
        for library_id in library_ids {
            sqlx::query(
                "INSERT INTO session_reference_libraries (session_id, library_id, created_at)
                 VALUES (?, ?, ?)",
            )
            .bind(session_id)
            .bind(library_id)
            .bind(now_iso())
            .execute(&mut *tx)
            .await
            .map_err(|e| map_fk_error(e, "session or reference library"))?;
        }
        tx.commit().await.map_err(CoreError::Database)
    }

    pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), CoreError> {
        let result = sqlx::query("DELETE FROM reference_libraries WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await
            .map_err(CoreError::Database)?;
        if result.rows_affected() == 0 {
            return Err(CoreError::not_found("reference library not found"));
        }
        Ok(())
    }
}

fn map_fk_error(e: sqlx::Error, what: &str) -> CoreError {
    if let sqlx::Error::Database(db) = &e {
        let msg = db.message().to_ascii_uppercase();
        if msg.contains("FOREIGN KEY") {
            return CoreError::not_found(format!("{what} not found"));
        }
    }
    CoreError::Database(e)
}

//! SQLite storage: connection, versioned migrations, repositories.
//! All SQL is dynamic (`sqlx::query`); no compile-time database needed.

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use crate::domain::{
    new_id, now_iso, Session, SessionStatus, SourceType, Student, Submission, SubmissionStatus,
    ValidatedNewSession,
};
use crate::error::CoreError;

/// Current schema version. Bump with a new `MIGRATION_Vn` block.
pub const SCHEMA_VERSION: i64 = 3;

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

fn map_fk_error(e: sqlx::Error, what: &str) -> CoreError {
    if let sqlx::Error::Database(db) = &e {
        let msg = db.message().to_ascii_uppercase();
        if msg.contains("FOREIGN KEY") {
            return CoreError::not_found(format!("{what} not found"));
        }
    }
    CoreError::Database(e)
}

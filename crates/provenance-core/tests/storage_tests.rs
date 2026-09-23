//! Phase 2 integration tests: migrations, repositories, draft-guarded service.
//! Each test gets an isolated temp-file database.

use provenance_core::domain::{NewSession, NewStudent, SessionStatus, SessionUpdate, SourceType};
use provenance_core::error::CoreError;
use provenance_core::ingest;
use provenance_core::service;
use provenance_core::storage::{self, SCHEMA_VERSION};
use sqlx::{Row, SqlitePool};

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

fn fresh_db() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.db");
    let pool = block_on(storage::open(&path)).expect("open+migrate");
    (dir, pool)
}

fn new_session(name: &str) -> NewSession {
    NewSession {
        name: name.to_string(),
        subject: None,
    }
}

#[test]
fn empty_db_migrates_to_current_schema_version() {
    let (_dir, pool) = fresh_db();
    let version: i64 = block_on(
        sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM schema_version").fetch_one(&pool),
    )
    .expect("version readable");
    assert_eq!(version, SCHEMA_VERSION);
}

#[test]
fn migration_is_idempotent_and_tables_exist() {
    let (_dir, pool) = fresh_db();
    block_on(storage::migrate(&pool)).expect("second migrate");
    block_on(storage::migrate(&pool)).expect("third migrate");
    let tables: Vec<String> = block_on(
        sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name IN
             ('sessions', 'students', 'submissions', 'schema_version',
              'analysis_results', 'raw_pair_analyses') ORDER BY name",
        )
        .fetch_all(&pool),
    )
    .expect("tables readable");
    assert_eq!(
        tables,
        vec![
            "analysis_results",
            "raw_pair_analyses",
            "schema_version",
            "sessions",
            "students",
            "submissions"
        ]
    );

    let removed_feature_tables: Vec<String> = block_on(
        sqlx::query_scalar(
            "SELECT name FROM sqlite_master
             WHERE type = 'table' AND (lower(name) LIKE '%ocr%' OR lower(name) LIKE '%review%')
             ORDER BY name",
        )
        .fetch_all(&pool),
    )
    .expect("removed feature tables readable");
    assert!(
        removed_feature_tables.is_empty(),
        "removed image-review tables must not be recreated: {removed_feature_tables:?}"
    );
}

#[test]
fn foreign_keys_are_enforced() {
    let (_dir, pool) = fresh_db();
    let pragma: i64 = block_on(sqlx::query_scalar("PRAGMA foreign_keys").fetch_one(&pool))
        .expect("pragma readable");
    assert_eq!(pragma, 1);
    // Orphan student must fail at the database level.
    let orphan = block_on(storage::StudentRepo::insert(
        &pool,
        "missing-session",
        "Ghost",
    ));
    assert!(matches!(orphan, Err(CoreError::NotFound(_))));
}

#[test]
fn session_crud_round_trip() {
    let (_dir, pool) = fresh_db();
    let created =
        block_on(service::create_session(&pool, new_session("Biology 1"))).expect("create");
    assert_eq!(created.name, "Biology 1");
    assert_eq!(created.status, SessionStatus::Draft);

    let fetched = block_on(service::get_session(&pool, &created.id)).expect("get");
    assert_eq!(fetched, created);

    let listed = block_on(service::list_sessions(&pool)).expect("list");
    assert_eq!(listed.len(), 1);

    let renamed = block_on(service::update_session(
        &pool,
        &created.id,
        SessionUpdate {
            name: Some("Biology 1 renamed".to_string()),
            subject: Some(Some("Biology".to_string())),
            assignment_prompt: Some(Some("Discuss cells.".to_string())),
            excluded_reference_text: None,
            exclude_common_text: None,
        },
    ))
    .expect("rename");
    assert_eq!(renamed.name, "Biology 1 renamed");
    assert_eq!(renamed.subject.as_deref(), Some("Biology"));
    assert_eq!(renamed.assignment_prompt.as_deref(), Some("Discuss cells."));
    assert!(renamed.exclude_common_text);

    block_on(service::delete_session(&pool, &created.id)).expect("delete");
    assert!(block_on(service::list_sessions(&pool))
        .expect("list")
        .is_empty());
}

#[test]
fn unknown_session_ids_report_not_found() {
    let (_dir, pool) = fresh_db();
    assert!(matches!(
        block_on(service::get_session(&pool, "nope")),
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        block_on(service::delete_session(&pool, "nope")),
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        block_on(service::remove_student(&pool, "nope")),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn students_belong_to_one_session_and_cascade_on_delete() {
    let (_dir, pool) = fresh_db();
    let session = block_on(service::create_session(&pool, new_session("Chem"))).expect("create");
    let a = block_on(service::add_student(
        &pool,
        &session.id,
        NewStudent {
            display_name: "Amit".to_string(),
        },
    ))
    .expect("add a");
    block_on(service::add_student(
        &pool,
        &session.id,
        NewStudent {
            display_name: "Priya".to_string(),
        },
    ))
    .expect("add b");
    let students = block_on(service::list_students(&pool, &session.id)).expect("list");
    assert_eq!(students.len(), 2);

    block_on(service::remove_student(&pool, &a.id)).expect("remove");
    let remaining = block_on(service::list_students(&pool, &session.id)).expect("list");
    assert_eq!(remaining.len(), 1);

    // Deleting the session cascades to its students.
    block_on(service::delete_session(&pool, &session.id)).expect("delete session");
    let count: i64 = block_on(sqlx::query_scalar("SELECT COUNT(*) FROM students").fetch_one(&pool))
        .expect("count");
    assert_eq!(count, 0);
}

#[test]
fn cannot_add_student_to_missing_session() {
    let (_dir, pool) = fresh_db();
    let err = block_on(service::add_student(
        &pool,
        "missing",
        NewStudent {
            display_name: "Ghost".to_string(),
        },
    ))
    .expect_err("must fail");
    assert!(matches!(err, CoreError::NotFound(_)));
}

#[test]
fn generated_ids_are_unique() {
    let (_dir, pool) = fresh_db();
    let a = block_on(service::create_session(&pool, new_session("A"))).expect("a");
    let b = block_on(service::create_session(&pool, new_session("B"))).expect("b");
    assert_ne!(a.id, b.id);
}

fn lock_session_for_test(pool: &SqlitePool, id: &str) {
    block_on(
        sqlx::query("UPDATE sessions SET status = 'locked' WHERE id = ?")
            .bind(id)
            .execute(pool),
    )
    .expect("lock");
}

#[test]
fn locked_session_blocks_edits_deletes_and_membership_changes() {
    let (_dir, pool) = fresh_db();
    let session = block_on(service::create_session(&pool, new_session("Locked"))).expect("create");
    let student = block_on(service::add_student(
        &pool,
        &session.id,
        NewStudent {
            display_name: "Rahul".to_string(),
        },
    ))
    .expect("add while draft");
    lock_session_for_test(&pool, &session.id);

    let rename = block_on(service::update_session(
        &pool,
        &session.id,
        SessionUpdate {
            name: Some("Changed".to_string()),
            subject: None,
            assignment_prompt: None,
            excluded_reference_text: None,
            exclude_common_text: None,
        },
    ));
    assert!(
        matches!(rename, Err(CoreError::LockedMutation { .. })),
        "rename blocked"
    );

    let delete = block_on(service::delete_session(&pool, &session.id));
    assert!(
        matches!(delete, Err(CoreError::LockedMutation { .. })),
        "delete blocked"
    );

    let add = block_on(service::add_student(
        &pool,
        &session.id,
        NewStudent {
            display_name: "Sneha".to_string(),
        },
    ));
    assert!(
        matches!(add, Err(CoreError::LockedMutation { .. })),
        "add blocked"
    );

    let remove = block_on(service::remove_student(&pool, &student.id));
    assert!(
        matches!(remove, Err(CoreError::LockedMutation { .. })),
        "remove blocked"
    );

    // Nothing changed.
    let fetched = block_on(service::get_session(&pool, &session.id)).expect("get");
    assert_eq!(fetched.name, "Locked");
}

#[test]
fn duplicate_primary_key_is_rejected() {
    let (_dir, pool) = fresh_db();
    let session = block_on(service::create_session(&pool, new_session("Dup"))).expect("create");
    let again = block_on(
        sqlx::query(
            "INSERT INTO sessions (id, name, subject, status, created_at, updated_at)
             VALUES (?, 'x', NULL, 'draft', 't', 't')",
        )
        .bind(&session.id)
        .execute(&pool),
    );
    assert!(again.is_err(), "duplicate id must fail");
}

#[test]
fn session_row_maps_status_correctly() {
    let (_dir, pool) = fresh_db();
    let session = block_on(service::create_session(&pool, new_session("Map"))).expect("create");
    let row = block_on(
        sqlx::query("SELECT status FROM sessions WHERE id = ?")
            .bind(&session.id)
            .fetch_one(&pool),
    )
    .expect("row");
    let status: String = row.try_get("status").expect("status col");
    assert_eq!(status, "draft");
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

#[test]
fn v2_database_migrates_to_v3_with_sane_defaults() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("v2.db");
    let pool = block_on(storage::connect(&path)).expect("connect");
    block_on(sqlx::query(
        "CREATE TABLE schema_version (version INTEGER PRIMARY KEY);
         CREATE TABLE sessions (id TEXT PRIMARY KEY, name TEXT NOT NULL, subject TEXT NULL,
           status TEXT NOT NULL DEFAULT 'draft', created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
         CREATE TABLE students (id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
           display_name TEXT NOT NULL, created_at TEXT NOT NULL);
         CREATE TABLE submissions (id TEXT PRIMARY KEY, student_id TEXT NOT NULL REFERENCES students (id) ON DELETE CASCADE,
           session_id TEXT NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
           source_type TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'draft',
           original_text TEXT NOT NULL DEFAULT '', source_filename TEXT NULL,
           content_sha256 TEXT NOT NULL DEFAULT '', created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
         INSERT INTO schema_version (version) VALUES (2);
         INSERT INTO sessions (id, name, status, created_at, updated_at)
           VALUES ('keep-v2', 'Old Session', 'draft', 't', 't');",
    )
    .fetch_all(&pool))
    .expect("v2 setup");
    drop(pool);

    let pool = block_on(storage::open(&path)).expect("migrate to v3");
    let kept = block_on(service::get_session(&pool, "keep-v2")).expect("kept");
    assert_eq!(kept.name, "Old Session");
    assert_eq!(kept.assignment_prompt, None);
    assert_eq!(kept.excluded_reference_text, None);
    assert!(kept.exclude_common_text, "DF flagging defaults on");
}

#[test]
fn v3_database_migrates_analysis_result_and_raw_pair_tables() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("v3.db");
    let pool = block_on(storage::connect(&path)).expect("connect");
    block_on(
        sqlx::query(
            "CREATE TABLE schema_version (version INTEGER PRIMARY KEY);
         CREATE TABLE sessions (id TEXT PRIMARY KEY);
         INSERT INTO schema_version (version) VALUES (3);
         INSERT INTO sessions (id) VALUES ('kept-session');",
        )
        .execute(&pool),
    )
    .expect("v3 setup");
    drop(pool);

    let pool = block_on(storage::open(&path)).expect("migrate to v4");
    let version: i64 = block_on(
        sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM schema_version").fetch_one(&pool),
    )
    .expect("version");
    assert_eq!(version, SCHEMA_VERSION);
    let session_id: String =
        block_on(sqlx::query_scalar("SELECT id FROM sessions").fetch_one(&pool))
            .expect("session preserved");
    assert_eq!(session_id, "kept-session");
}

#[test]
fn session_analysis_results_are_only_returned_for_the_current_input_digest() {
    let (_dir, pool) = fresh_db();
    let session = block_on(service::create_session(
        &pool,
        new_session("Analysis cache"),
    ))
    .expect("session");
    block_on(storage::AnalysisRepo::save_result(
        &pool,
        &session.id,
        "input-v1",
        "{\"report\":1}",
    ))
    .expect("save result");

    assert_eq!(
        block_on(storage::AnalysisRepo::result_for_input(
            &pool,
            &session.id,
            "input-v1"
        ))
        .expect("read result"),
        Some("{\"report\":1}".to_string())
    );
    assert_eq!(
        block_on(storage::AnalysisRepo::result_for_input(
            &pool,
            &session.id,
            "input-v2"
        ))
        .expect("stale result is not served"),
        None
    );
}

#[test]
fn v1_database_migrates_to_v2_without_losing_sessions() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("old.db");
    // Simulate a Phase 2 database: create only the V1 objects.
    let pool = block_on(storage::connect(&path)).expect("connect");
    block_on(sqlx::query(
        "CREATE TABLE schema_version (version INTEGER PRIMARY KEY);
         CREATE TABLE sessions (id TEXT PRIMARY KEY, name TEXT NOT NULL, subject TEXT NULL,
           status TEXT NOT NULL DEFAULT 'draft', created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
         CREATE TABLE students (id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
           display_name TEXT NOT NULL, created_at TEXT NOT NULL);
         CREATE TABLE submissions (id TEXT PRIMARY KEY, student_id TEXT NOT NULL REFERENCES students (id) ON DELETE CASCADE,
           session_id TEXT NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
           source_type TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'draft', created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
         INSERT INTO schema_version (version) VALUES (1);
         INSERT INTO sessions (id, name, status, created_at, updated_at)
           VALUES ('keep-me', 'Old Session', 'draft', 't', 't');",
    )
    .fetch_all(&pool))
    .expect("v1 setup");
    drop(pool);

    let pool = block_on(storage::open(&path)).expect("migrate to v2");
    let version: i64 = block_on(
        sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM schema_version").fetch_one(&pool),
    )
    .expect("version");
    assert_eq!(version, SCHEMA_VERSION);
    let kept = block_on(service::get_session(&pool, "keep-me")).expect("session kept");
    assert_eq!(kept.name, "Old Session");
}

#[test]
fn paste_submission_saves_text_and_hash() {
    let (_dir, pool) = fresh_db();
    let session = block_on(service::create_session(&pool, new_session("Essays"))).expect("s");
    let student = add_student(&pool, &session.id, "Amit");
    let saved = block_on(service::save_text_submission(
        &pool,
        &student,
        SourceType::PastedText,
        None,
        b"Photosynthesis converts light energy.",
    ))
    .expect("save");
    assert_eq!(saved.original_text, "Photosynthesis converts light energy.");
    assert_eq!(saved.content_sha256.len(), 64);
    assert_eq!(saved.source_filename, None);

    let fetched = block_on(service::get_submission(&pool, &student)).expect("get");
    assert_eq!(fetched, saved);
}

#[test]
fn same_student_replaces_draft_before_lock() {
    let (_dir, pool) = fresh_db();
    let session = block_on(service::create_session(&pool, new_session("Replace"))).expect("s");
    let student = add_student(&pool, &session.id, "Priya");
    block_on(service::save_text_submission(
        &pool,
        &student,
        SourceType::PastedText,
        None,
        b"v1",
    ))
    .expect("v1");
    let v2 = block_on(service::save_text_submission(
        &pool,
        &student,
        SourceType::TxtFile,
        Some("essay.txt".to_string()),
        b"v2 revised",
    ))
    .expect("v2");
    assert_eq!(v2.original_text, "v2 revised");
    assert_eq!(v2.source_filename.as_deref(), Some("essay.txt"));
    let listed = block_on(service::list_submissions(&pool, &session.id)).expect("list");
    assert_eq!(listed.len(), 1, "one row per student");
}

#[test]
fn empty_and_whitespace_text_rejected() {
    let (_dir, pool) = fresh_db();
    let session = block_on(service::create_session(&pool, new_session("Empty"))).expect("s");
    let student = add_student(&pool, &session.id, "Ghost");
    for bad in [&b""[..], b"   ", b"\r\n \t"] {
        let err = block_on(service::save_text_submission(
            &pool,
            &student,
            SourceType::PastedText,
            None,
            bad,
        ))
        .expect_err("must reject");
        assert!(matches!(err, CoreError::Validation(_)));
    }
}

#[test]
fn crlf_upload_matches_lf_paste_hash() {
    let (_dir, pool) = fresh_db();
    let session = block_on(service::create_session(&pool, new_session("Endings"))).expect("s");
    let a = add_student(&pool, &session.id, "A");
    let b = add_student(&pool, &session.id, "B");
    let saved_a = block_on(service::save_text_submission(
        &pool,
        &a,
        SourceType::TxtFile,
        Some("a.txt".to_string()),
        b"line one\r\nline two\r\n",
    ))
    .expect("crlf");
    let saved_b = block_on(service::save_text_submission(
        &pool,
        &b,
        SourceType::PastedText,
        None,
        "line one\nline two\n".as_bytes(),
    ))
    .expect("lf");
    assert_eq!(saved_a.original_text, saved_b.original_text);
    assert_eq!(saved_a.content_sha256, saved_b.content_sha256);
}

#[test]
fn binary_and_oversize_uploads_rejected() {
    let (_dir, pool) = fresh_db();
    let session = block_on(service::create_session(&pool, new_session("Files"))).expect("s");
    let student = add_student(&pool, &session.id, "F");
    let binary = block_on(service::save_text_submission(
        &pool,
        &student,
        SourceType::TxtFile,
        Some("img.png".to_string()),
        &[0x89, 0x50, 0x4E, 0x47, 0xFF],
    ))
    .expect_err("binary rejected");
    assert!(matches!(binary, CoreError::Validation(_)));
    let huge = vec![b'x'; ingest::MAX_SUBMISSION_BYTES + 1];
    assert!(block_on(service::save_text_submission(
        &pool,
        &student,
        SourceType::TxtFile,
        None,
        &huge
    ))
    .is_err());
}

#[test]
fn large_valid_upload_accepted() {
    let (_dir, pool) = fresh_db();
    let session = block_on(service::create_session(&pool, new_session("Large"))).expect("s");
    let student = add_student(&pool, &session.id, "L");
    let big = vec![b'a'; 1_000_000];
    let saved = block_on(service::save_text_submission(
        &pool,
        &student,
        SourceType::MarkdownFile,
        Some("big.md".to_string()),
        &big,
    ))
    .expect("1MB accepted");
    assert_eq!(saved.original_text.len(), 1_000_000);
}

#[test]
fn submission_for_unknown_student_is_not_found() {
    let (_dir, pool) = fresh_db();
    assert!(matches!(
        block_on(service::save_text_submission(
            &pool,
            "missing",
            SourceType::PastedText,
            None,
            b"text"
        )),
        Err(CoreError::NotFound(_))
    ));
    let session = block_on(service::create_session(&pool, new_session("None"))).expect("s");
    let student = add_student(&pool, &session.id, "N");
    assert!(matches!(
        block_on(service::get_submission(&pool, &student)),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn locked_session_blocks_submission_edits() {
    let (_dir, pool) = fresh_db();
    let session = block_on(service::create_session(&pool, new_session("Lock"))).expect("s");
    let student = add_student(&pool, &session.id, "S");
    block_on(service::save_text_submission(
        &pool,
        &student,
        SourceType::PastedText,
        None,
        b"v1",
    ))
    .expect("draft save");
    lock_session_for_test(&pool, &session.id);
    let err = block_on(service::save_text_submission(
        &pool,
        &student,
        SourceType::PastedText,
        None,
        b"v2",
    ))
    .expect_err("locked");
    assert!(matches!(err, CoreError::LockedMutation { .. }));
}

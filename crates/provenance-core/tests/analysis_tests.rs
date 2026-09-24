//! Phase 5 integration tests: exact-only session analysis.

use provenance_core::analysis;
use provenance_core::analysis::{ExactAnalysis, PairAnalysis};
use provenance_core::domain::{NewSession, NewStudent, SessionUpdate, SourceType};
use provenance_core::error::CoreError;
use provenance_core::service;
use provenance_core::storage;

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
    let path = dir.path().join("test.db");
    let pool = block_on(storage::open(&path)).expect("open+migrate");
    (dir, pool)
}

const COPIED: &str = "The rapid expansion of railway networks during the nineteenth century transformed trade across continents and reshaped growing cities around busy stations.";

fn setup_session(pool: &sqlx::SqlitePool) -> (String, String, String, String) {
    let session = block_on(service::create_session(
        pool,
        NewSession {
            name: "History".to_string(),
            subject: None,
        },
    ))
    .expect("session");
    let add = |name: &str| {
        block_on(service::add_student(
            pool,
            &session.id,
            NewStudent {
                display_name: name.to_string(),
            },
        ))
        .expect("student")
        .id
    };
    let a = add("A");
    let b = add("B");
    let c = add("C");
    let save = |student: &str, text: &str| {
        block_on(service::save_text_submission(
            pool,
            student,
            SourceType::PastedText,
            None,
            text.as_bytes(),
        ))
        .expect("save");
    };
    save(
        &a,
        &format!("My own opening paragraph with personal wording here. {COPIED} My own closing paragraph with independent thoughts."),
    );
    save(
        &b,
        &format!("A different opening that shares nothing at all. {COPIED} A different ending with fresh vocabulary throughout."),
    );
    save(
        &c,
        "Quantum tunneling lets particles cross barriers they classically cannot surmount, enabling fusion inside stars and flash memory operation.",
    );
    (session.id, a, b, c)
}

#[test]
fn three_way_session_flags_only_the_copied_pair() {
    let (_dir, pool) = fresh_db();
    let (session, a, b, c) = setup_session(&pool);
    let report = block_on(analysis::analyze_session_exact(&pool, &session)).expect("analyze");
    assert_eq!(report.pairs.len(), 3, "3 students → 3 pairs");
    assert_eq!(report.fingerprint_version, 1);
    assert_eq!(report.normalization_version, 1);

    let pair = |x: &str, y: &str| {
        report
            .pairs
            .iter()
            .find(|p| {
                (p.a_student_id == x && p.b_student_id == y)
                    || (p.a_student_id == y && p.b_student_id == x)
            })
            .expect("pair exists")
    };
    let ab = pair(&a, &b);
    assert!(
        pair_cov(ab, &a) > 40.0,
        "A↔B high, got {:?}",
        pair_cov(ab, &a)
    );
    assert!(
        pair_cov(ab, &b) > 40.0,
        "A↔B high, got {:?}",
        pair_cov(ab, &b)
    );
    assert!(!ab.passages.is_empty());
    assert_eq!(pair(&a, &c).passages.len(), 0);
    assert_eq!(pair(&b, &c).passages.len(), 0);

    // Passage char spans slice the copied words out of the originals.
    let first = &ab.passages[0];
    assert!(first.tokens >= 10);
    let saved_a = block_on(service::get_submission(&pool, &a)).expect("sub");
    let excerpt = &saved_a.original_text[first.a_char_start..first.a_char_end];
    assert!(excerpt.contains("railway networks"), "excerpt: {excerpt}");

    // Per-student coverage: copiers flagged, unrelated clean.
    assert!(cov_of(&report, &a) > 0.0 && cov_of(&report, &b) > 0.0);
    assert_eq!(cov_of(&report, &c), 0.0);
}

fn cov_of(report: &ExactAnalysis, id: &str) -> f64 {
    report
        .per_student
        .iter()
        .find(|s| s.student_id == id)
        .expect("row")
        .coverage
        .expect("assessable")
}

fn pair_cov(pair: &PairAnalysis, id: &str) -> f64 {
    let side = if pair.a_student_id == id {
        pair.coverage_a
    } else {
        pair.coverage_b
    };
    side.expect("assessable")
}

#[test]
fn directional_coverage_reflects_length_difference() {
    let (_dir, pool) = fresh_db();
    let session = block_on(service::create_session(
        &pool,
        NewSession {
            name: "Len".to_string(),
            subject: None,
        },
    ))
    .expect("session");
    let short = block_on(service::add_student(
        &pool,
        &session.id,
        NewStudent {
            display_name: "Short".to_string(),
        },
    ))
    .expect("short")
    .id;
    let long = block_on(service::add_student(
        &pool,
        &session.id,
        NewStudent {
            display_name: "Long".to_string(),
        },
    ))
    .expect("long")
    .id;
    block_on(service::save_text_submission(
        &pool,
        &short,
        SourceType::PastedText,
        None,
        COPIED.as_bytes(),
    ))
    .expect("save short");
    block_on(service::save_text_submission(
        &pool,
        &long,
        SourceType::PastedText,
        None,
        format!("{COPIED} {COPIED} An extra original tail with many more words to lengthen this document substantially.").as_bytes(),
    ))
    .expect("save long");

    let report = block_on(analysis::analyze_session_exact(&pool, &session.id)).expect("analyze");
    assert_eq!(report.pairs.len(), 1);
    let pair = &report.pairs[0];
    // Short doc is ~fully covered; long doc only partially.
    let short_cov = pair_cov(pair, &short);
    let long_cov = pair_cov(pair, &long);
    assert!(short_cov > 90.0, "short nearly fully matched: {short_cov}");
    assert!(
        long_cov < short_cov && long_cov > 0.0,
        "long partially matched: {long_cov}"
    );
}

#[test]
fn sessions_without_enough_data_yield_empty_pairs() {
    let (_dir, pool) = fresh_db();
    let empty = block_on(service::create_session(
        &pool,
        NewSession {
            name: "Empty".to_string(),
            subject: None,
        },
    ))
    .expect("session");
    let report = block_on(analysis::analyze_session_exact(&pool, &empty.id)).expect("analyze");
    assert!(report.pairs.is_empty());
    assert!(report.per_student.is_empty());

    // A student with no submission is listed as not assessable, never paired.
    let student = block_on(service::add_student(
        &pool,
        &empty.id,
        NewStudent {
            display_name: "Solo".to_string(),
        },
    ))
    .expect("student")
    .id;
    let report = block_on(analysis::analyze_session_exact(&pool, &empty.id)).expect("analyze");
    assert!(report.pairs.is_empty());
    assert_eq!(report.per_student.len(), 1);
    assert_eq!(report.per_student[0].student_id, student);
    assert_eq!(report.per_student[0].coverage, None);
    assert_eq!(report.per_student[0].eligible_tokens, 0);
    assert_eq!(report.per_student[0].total_tokens, 0);
}

#[test]
fn unknown_session_is_not_found() {
    let (_dir, pool) = fresh_db();
    assert!(matches!(
        block_on(analysis::analyze_session_exact(&pool, "missing")),
        Err(CoreError::NotFound(_))
    ));
}

const PROMPT_Q: &str = "Discuss the causes and consequences of the industrial revolution with reference to urban growth and factory labour conditions.";
const SHARED_ANSWER: &str = "Crop rotation restored nitrogen and doubled wheat yields within three seasons of careful management.";

fn set_prompt(pool: &sqlx::SqlitePool, session: &str) {
    block_on(service::update_session(
        pool,
        session,
        SessionUpdate {
            name: None,
            subject: None,
            assignment_prompt: Some(Some(PROMPT_Q.to_string())),
            excluded_reference_text: None,
            exclude_common_text: None,
        },
    ))
    .expect("set prompt");
}

#[test]
fn assignment_question_excluded_but_copied_answer_counted() {
    // The plan's manual test as an automated fixture.
    let (_dir, pool) = fresh_db();
    let session = block_on(service::create_session(
        &pool,
        NewSession {
            name: "Manual".to_string(),
            subject: None,
        },
    ))
    .expect("session")
    .id;
    set_prompt(&pool, &session);
    let add = |name: &str| {
        block_on(service::add_student(
            &pool,
            &session,
            NewStudent {
                display_name: name.to_string(),
            },
        ))
        .expect("student")
        .id
    };
    let a = add("A");
    let b = add("B");
    let c = add("C");
    let save = |student: &str, text: String| {
        block_on(service::save_text_submission(
            &pool,
            student,
            SourceType::PastedText,
            None,
            text.as_bytes(),
        ))
        .expect("save");
    };
    save(
        &a,
        format!("{PROMPT_Q} {SHARED_ANSWER} Solo tail alpha words here."),
    );
    save(
        &b,
        format!("{PROMPT_Q} {SHARED_ANSWER} Solo tail beta words here."),
    );
    save(
        &c,
        format!("{PROMPT_Q} Entirely different body gamma delta epsilon."),
    );

    let report = block_on(analysis::analyze_session_exact(&pool, &session)).expect("analyze");
    assert!(report.prompt_applied);
    assert!(!report.common_text_applied, "3 docs: DF stays off");
    let ab = report
        .pairs
        .iter()
        .find(|p| {
            (p.a_student_id == a && p.b_student_id == b)
                || (p.a_student_id == b && p.b_student_id == a)
        })
        .expect("pair");
    assert!(!ab.passages.is_empty(), "copied answer stays evidence");
    let kept: String = ab
        .passages
        .iter()
        .map(|p| {
            let sub = block_on(service::get_submission(&pool, &ab.a_student_id)).expect("sub");
            sub.original_text[p.a_char_start..p.a_char_end].to_string()
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(kept.contains("nitrogen"), "answer counted: {kept}");
    assert!(!kept.contains("revolution"), "question not counted: {kept}");
    assert!(
        ab.excluded
            .iter()
            .any(|e| format!("{:?}", e.reason) == "Prompt"),
        "question reported as excluded"
    );
    // Clean pair stays clean.
    let ac = report
        .pairs
        .iter()
        .find(|p| {
            (p.a_student_id == a && p.b_student_id == c)
                || (p.a_student_id == c && p.b_student_id == a)
        })
        .expect("pair");
    assert!(ac.passages.is_empty());
}

#[test]
fn unanimous_text_scored_and_flagged_common_with_enough_documents() {
    let makesession = |pool: &sqlx::SqlitePool, n: usize| {
        let session = block_on(service::create_session(
            pool,
            NewSession {
                name: format!("S{n}"),
                subject: None,
            },
        ))
        .expect("session")
        .id;
        for i in 0..n {
            let student = block_on(service::add_student(
                pool,
                &session,
                NewStudent {
                    display_name: format!("S{i}"),
                },
            ))
            .expect("student")
            .id;
            block_on(service::save_text_submission(
                pool,
                &student,
                SourceType::PastedText,
                None,
                format!("{PROMPT_Q} Personal tail number {i} with different words.").as_bytes(),
            ))
            .expect("save");
        }
        session
    };
    // 3 docs: unanimous question survives DF (too few documents).
    let (_dir, pool) = fresh_db();
    let three = makesession(&pool, 3);
    let report = block_on(analysis::analyze_session_exact(&pool, &three)).expect("analyze");
    assert!(!report.common_text_applied);
    assert!(report.pairs.iter().all(|p| !p.passages.is_empty()));

    // 5 docs: unanimous question is common session text — still scored,
    // with every passage flagged common and positive coverage.
    let (_dir, pool) = fresh_db();
    let five = makesession(&pool, 5);
    let report = block_on(analysis::analyze_session_exact(&pool, &five)).expect("analyze");
    assert!(report.common_text_applied);
    assert!(!report.prompt_applied, "no teacher prompt was set");
    assert!(
        report.pairs.iter().all(|p| !p.passages.is_empty()),
        "unanimous text stays reportable"
    );
    assert!(report
        .pairs
        .iter()
        .flat_map(|p| &p.passages)
        .all(|p| p.common_text));
    for row in &report.per_student {
        let cov = row.coverage.expect("assessable text remains");
        assert!(cov > 0.0, "common matches count: {row:?}");
    }
    assert!(
        report
            .pairs
            .iter()
            .flat_map(|p| &p.excluded)
            .all(|e| format!("{:?}", e.reason) != "CommonSessionText"),
        "frequency never excludes"
    );
}

#[test]
fn df_toggle_off_keeps_frequency_text_scorable() {
    let (_dir, pool) = fresh_db();
    let session = block_on(service::create_session(
        &pool,
        NewSession {
            name: "Toggle".to_string(),
            subject: None,
        },
    ))
    .expect("session")
    .id;
    block_on(service::update_session(
        &pool,
        &session,
        SessionUpdate {
            name: None,
            subject: None,
            assignment_prompt: None,
            excluded_reference_text: None,
            exclude_common_text: Some(false),
        },
    ))
    .expect("toggle off");
    for i in 0..5 {
        let student = block_on(service::add_student(
            &pool,
            &session,
            NewStudent {
                display_name: format!("S{i}"),
            },
        ))
        .expect("student")
        .id;
        block_on(service::save_text_submission(
            &pool,
            &student,
            SourceType::PastedText,
            None,
            format!("{PROMPT_Q} Personal tail number {i} with different words.").as_bytes(),
        ))
        .expect("save");
    }
    let report = block_on(analysis::analyze_session_exact(&pool, &session)).expect("analyze");
    assert!(!report.common_text_applied);
    assert!(!report.prompt_applied);
    assert!(report.pairs.iter().all(|p| !p.passages.is_empty()));
    assert!(
        report
            .pairs
            .iter()
            .flat_map(|p| &p.passages)
            .all(|p| !p.common_text),
        "no classification without DF"
    );
    assert_eq!(report.modified_version, 1);
}

#[test]
fn common_boilerplate_scored_while_prompt_excluded_end_to_end() {
    // Contract test: with DF active and a teacher prompt set, a match on
    // frequency-common boilerplate counts (flagged common) while a match on
    // the prompt is excluded with the Prompt reason.
    const BOILERPLATE: &str = "all students must attach the signed declaration sheet on top of every submission packet for verification purposes";
    let (_dir, pool) = fresh_db();
    let session = block_on(service::create_session(
        &pool,
        NewSession {
            name: "Contract".to_string(),
            subject: None,
        },
    ))
    .expect("session")
    .id;
    set_prompt(&pool, &session);
    let mut ids = Vec::new();
    for i in 0..5 {
        let student = block_on(service::add_student(
            &pool,
            &session,
            NewStudent {
                display_name: format!("S{i}"),
            },
        ))
        .expect("student")
        .id;
        block_on(service::save_text_submission(
            &pool,
            &student,
            SourceType::PastedText,
            None,
            format!("{PROMPT_Q} {BOILERPLATE} Personal tail number {i} with different words.")
                .as_bytes(),
        ))
        .expect("save");
        ids.push(student);
    }
    let report = block_on(analysis::analyze_session_exact(&pool, &session)).expect("analyze");
    assert!(report.prompt_applied);
    assert!(report.common_text_applied, "5 docs activate DF");
    let pair = &report.pairs[0];
    // Boilerplate passages are scored and flagged common.
    assert!(!pair.passages.is_empty(), "boilerplate stays evidence");
    assert!(pair.passages.iter().any(|p| p.common_text));
    let texts: String = pair
        .passages
        .iter()
        .map(|p| {
            let sub = block_on(service::get_submission(&pool, &pair.a_student_id)).expect("sub");
            sub.original_text[p.a_char_start..p.a_char_end].to_string()
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        texts.contains("declaration"),
        "boilerplate counted: {texts}"
    );
    assert!(!texts.contains("revolution"), "prompt not scored: {texts}");
    assert!(pair
        .excluded
        .iter()
        .any(|e| format!("{:?}", e.reason) == "Prompt"));
    assert!(pair
        .excluded
        .iter()
        .all(|e| format!("{:?}", e.reason) != "CommonSessionText"));
    let _ = ids;
}

fn save_pair(
    pool: &sqlx::SqlitePool,
    session: &str,
    a_text: &str,
    b_text: &str,
) -> (String, String) {
    let add = |name: &str| {
        block_on(service::add_student(
            pool,
            session,
            NewStudent {
                display_name: name.to_string(),
            },
        ))
        .expect("student")
        .id
    };
    let a = add("A");
    let b = add("B");
    for (student, text) in [(&a, a_text), (&b, b_text)] {
        block_on(service::save_text_submission(
            pool,
            student,
            SourceType::PastedText,
            None,
            text.as_bytes(),
        ))
        .expect("save");
    }
    (a, b)
}

fn new_session_id(pool: &sqlx::SqlitePool) -> String {
    block_on(service::create_session(
        pool,
        NewSession {
            name: "Modified".to_string(),
            subject: None,
        },
    ))
    .expect("session")
    .id
}

#[test]
fn lightly_edited_copy_reported_as_modified_evidence() {
    // The plan's manual example, automated.
    let (_dir, pool) = fresh_db();
    let session = new_session_id(&pool);
    let (a, _b) = save_pair(
        &pool,
        &session,
        "Photosynthesis converts light energy into chemical energy that can later be used by the plant.",
        "Through photosynthesis, plants convert light into stored chemical energy that can be used later.",
    );
    let report = block_on(analysis::analyze_session_exact(&pool, &session)).expect("analyze");
    assert_eq!(report.pairs.len(), 1);
    let pair = &report.pairs[0];
    assert!(
        pair.passages
            .iter()
            .all(|p| p.kind == provenance_core::analysis::PassageKind::Modified),
        "no exact passages for edited copy: {:?}",
        pair.passages
    );
    assert_eq!(pair.passages.len(), 1);
    let identity = pair.passages[0].identity.expect("modified has identity");
    assert!(identity >= 0.6, "identity {identity}");
    // Char spans slice the paraphrased words from each original.
    let sub_a = block_on(service::get_submission(&pool, &a)).expect("sub");
    let excerpt = &sub_a.original_text[pair.passages[0].a_char_start..pair.passages[0].a_char_end];
    assert!(excerpt.contains("Photosynthesis"), "excerpt: {excerpt}");
    assert!(pair_cov(pair, &a) > 0.0);
}

#[test]
fn verbatim_copy_stays_exact_with_no_identity() {
    let (_dir, pool) = fresh_db();
    let session = new_session_id(&pool);
    let text = "The rapid expansion of railway networks during the nineteenth century transformed trade across continents.";
    save_pair(&pool, &session, text, text);
    let report = block_on(analysis::analyze_session_exact(&pool, &session)).expect("analyze");
    let pair = &report.pairs[0];
    assert!(!pair.passages.is_empty());
    assert!(
        pair.passages.iter().all(
            |p| p.kind == provenance_core::analysis::PassageKind::Exact && p.identity.is_none()
        ),
        "verbatim copy must not be relabeled: {:?}",
        pair.passages
    );
    assert_eq!(pair.coverage_a, Some(100.0));
}

#[test]
fn edited_prompt_excluded_with_reason() {
    let (_dir, pool) = fresh_db();
    let session = new_session_id(&pool);
    set_prompt(&pool, &session);
    // Student lightly rewrites the assignment question itself.
    let (a, b) = save_pair(
        &pool,
        &session,
        "Discuss the causes and consequences of the industrial revolution with reference to urban growth and factory labour conditions. My own tail words follow here.",
        "Discuss the causes and effects of the industrial revolution with regard to urban growth and factory labour conditions. Different tail words appear.",
    );
    let report = block_on(analysis::analyze_session_exact(&pool, &session)).expect("analyze");
    assert!(report.prompt_applied);
    let pair = &report.pairs[0];
    // Edited question is modified-like but excluded as prompt material.
    assert!(
        pair.passages.is_empty(),
        "edited prompt must not score: {:?}",
        pair.passages
    );
    assert!(
        pair.excluded
            .iter()
            .any(|e| format!("{:?}", e.reason) == "Prompt"),
        "excluded as prompt: {:?}",
        pair.excluded
    );
    let _ = (a, b);
}

#[test]
fn analysis_is_deterministic_across_runs() {
    let (_dir, pool) = fresh_db();
    let (session, _, _, _) = setup_session(&pool);
    let first = block_on(analysis::analyze_session_exact(&pool, &session)).expect("first");
    let second = block_on(analysis::analyze_session_exact(&pool, &session)).expect("second");
    assert_eq!(first, second);
}

#[test]
fn long_prompt_does_not_dilute_copied_answer_coverage() {
    // 6.1.3 fixture: 900-token prompt + 100-token copied answer.
    let (_dir, pool) = fresh_db();
    let session = new_session_id(&pool);
    let prompt =
        vec!["Discuss the causes and consequences of the industrial revolution."; 75].join(" ");
    block_on(service::update_session(
        &pool,
        &session,
        SessionUpdate {
            name: None,
            subject: None,
            assignment_prompt: Some(Some(prompt.clone())),
            excluded_reference_text: None,
            exclude_common_text: None,
        },
    ))
    .expect("set long prompt");
    let answer = ["Crop rotation restored nitrogen and doubled wheat yields within three seasons.";
        8]
        .join(" ");
    let (a, b) = save_pair(
        &pool,
        &session,
        &format!("{prompt} {answer}"),
        &format!("{prompt} {answer}"),
    );
    let report = block_on(analysis::analyze_session_exact(&pool, &session)).expect("analyze");
    assert!(report.prompt_applied);
    let row_a = report
        .per_student
        .iter()
        .find(|s| s.student_id == a)
        .expect("row");
    assert!(
        row_a.eligible_tokens < row_a.total_tokens,
        "prompt leaves the denominator"
    );
    assert_eq!(
        row_a.coverage,
        Some(100.0),
        "copied eligible answer scores 100%, not ~10%: {row_a:?}"
    );
    let _ = b;
}

#[test]
fn fully_excluded_submission_reports_insufficient_text() {
    // A submission containing only the assignment question has no
    // assessable text: report it as such, never as a clean 0%.
    let (_dir, pool) = fresh_db();
    let session = new_session_id(&pool);
    set_prompt(&pool, &session);
    let (a, b) = save_pair(&pool, &session, PROMPT_Q, PROMPT_Q);
    let report = block_on(analysis::analyze_session_exact(&pool, &session)).expect("analyze");
    for id in [&a, &b] {
        let row = report
            .per_student
            .iter()
            .find(|s| &s.student_id == id)
            .expect("row");
        assert_eq!(row.coverage, None, "insufficient, not 0%: {row:?}");
        assert_eq!(row.eligible_tokens, 0);
    }
    assert_eq!(report.pairs[0].coverage_a, None);
    assert_eq!(report.pairs[0].coverage_b, None);
}

/// Contract pin: the committed `tests/fixtures/exact-analysis-sample.json`
/// must equal fresh real-engine output (modulo generated ids). Regenerate
/// only by rerunning the engine, never by hand-editing the fixture.
#[test]
fn recorded_fixture_matches_engine() {
    let (_dir, pool) = fresh_db();
    let session = block_on(service::create_session(
        &pool,
        NewSession {
            name: "Sample".to_string(),
            subject: None,
        },
    ))
    .expect("session")
    .id;
    block_on(service::update_session(
        &pool,
        &session,
        SessionUpdate {
            name: None,
            subject: None,
            assignment_prompt: Some(Some(
                "Discuss the causes of the changes and their effects on daily life.".to_string(),
            )),
            excluded_reference_text: None,
            exclude_common_text: None,
        },
    ))
    .expect("prompt");
    const Q: &str = "Discuss the causes of the changes and their effects on daily life.";
    const B: &str = "All students must write their own answers in blue or black ink.";
    const RAIL: &str =
        "The rapid expansion of railway networks transformed trade across continents.";
    const PHOTO_ORIG: &str = "Photosynthesis converts light energy into chemical energy that can later be used by the plant.";
    const PHOTO_PARA: &str = "Through photosynthesis, plants convert light into stored chemical energy that can be used later.";
    for (name, text) in [
        ("Amit", format!("{Q} {B} {RAIL} {PHOTO_ORIG} Amit closing words here.")),
        ("Priya", format!("{Q} {B} {RAIL} Priya closing words here.")),
        (
            "Chen",
            format!("{Q} {B} Quantum tunneling lets particles cross barriers they cannot surmount ever."),
        ),
        ("Sara", format!("{Q} {B} {PHOTO_PARA} Sara closing words here.")),
    ] {
        let student = block_on(service::add_student(
            &pool,
            &session,
            NewStudent {
                display_name: name.to_string(),
            },
        ))
        .expect("student")
        .id;
        block_on(service::save_text_submission(
            &pool,
            &student,
            SourceType::PastedText,
            None,
            text.as_bytes(),
        ))
        .expect("save");
    }
    let report = block_on(analysis::analyze_session_exact(&pool, &session)).expect("analyze");
    let mut json = serde_json::to_string_pretty(&report).expect("serialize");
    // Normalize generated UUIDs to role labels via student names (stable
    // across runs; UUID sort order is not).
    let students = block_on(service::list_students(&pool, &session)).expect("students");
    let mut names: Vec<(String, String)> = students
        .iter()
        .map(|s| (s.id.clone(), s.display_name.to_lowercase()))
        .collect();
    names.sort_by(|a, b| a.1.cmp(&b.1));
    for (id, name) in &names {
        json = json.replace(id, &format!("stud-{name}"));
    }
    // Canonicalize: pair order follows insertion UUIDs and each pair is
    // oriented by document order, so orient every pair with a <= b
    // (swapping sides, coverages, passage spans, and exclusion sides),
    // then sort pairs. Formatting is irrelevant — compare parsed values.
    fn swap_fields(obj: &mut serde_json::Value, a: &str, b: &str) {
        let tmp = obj[a].take();
        obj[a] = obj[b].take();
        obj[b] = tmp;
    }
    fn orient_pair(pair: &mut serde_json::Value) {
        let swap = pair["a_student_id"].as_str() > pair["b_student_id"].as_str();
        if !swap {
            return;
        }
        swap_fields(pair, "a_student_id", "b_student_id");
        swap_fields(pair, "coverage_a", "coverage_b");
        if let Some(passages) = pair.get_mut("passages").and_then(|p| p.as_array_mut()) {
            for passage in passages.iter_mut() {
                swap_fields(passage, "a_token_start", "b_token_start");
                swap_fields(passage, "a_token_end", "b_token_end");
                swap_fields(passage, "a_char_start", "b_char_start");
                swap_fields(passage, "a_char_end", "b_char_end");
            }
        }
        if let Some(excluded) = pair.get_mut("excluded").and_then(|p| p.as_array_mut()) {
            for item in excluded.iter_mut() {
                let side = item["side"].as_str().unwrap_or("a");
                item["side"] = if side == "a" {
                    serde_json::Value::String("b".to_string())
                } else {
                    serde_json::Value::String("a".to_string())
                };
            }
        }
    }
    fn canonical(mut value: serde_json::Value) -> serde_json::Value {
        if let Some(pairs) = value.get_mut("pairs").and_then(|p| p.as_array_mut()) {
            for pair in pairs.iter_mut() {
                orient_pair(pair);
            }
            pairs.sort_by(|a, b| {
                (a["a_student_id"].as_str(), a["b_student_id"].as_str())
                    .cmp(&(b["a_student_id"].as_str(), b["b_student_id"].as_str()))
            });
        }
        // Engine sorts students by generated id; re-sort after relabeling.
        if let Some(students) = value.get_mut("per_student").and_then(|p| p.as_array_mut()) {
            students.sort_by(|a, b| a["student_id"].as_str().cmp(&b["student_id"].as_str()));
        }
        value
    }
    let fresh: serde_json::Value = serde_json::from_str(&json).expect("fresh parses");
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("exact-analysis-sample.json");
    let committed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("committed fixture"))
            .expect("fixture parses");
    let (cf, cc) = (canonical(fresh), canonical(committed));
    assert_eq!(
        cf, cc,
        "fixture drifted from engine output — regenerate from the engine, never hand-edit"
    );
    // The fixture must exercise every evidence shape the UI renders.
    assert!(json.contains("\"kind\": \"modified\""));
    assert!(json.contains("\"common_text\": true"));
    assert!(json.contains("\"reason\": \"Prompt\""));
}

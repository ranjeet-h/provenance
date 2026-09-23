//! Domain types: sessions, students, submissions + validation.
//! Storage-agnostic; SQLite mapping lives in [`crate::storage`].

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::CoreError;

pub const MAX_NAME_LEN: usize = 200;
pub const MAX_SUBJECT_LEN: usize = 200;
pub const MAX_DISPLAY_NAME_LEN: usize = 200;
/// Assignment question/instructions stored for exclusion (Phase 6).
pub const MAX_PROMPT_CHARS: usize = 10_000;
/// Additional excluded reference text (Phase 6).
pub const MAX_REFERENCE_CHARS: usize = 20_000;

/// Current timestamp as RFC 3339 UTC millis, e.g. `2026-09-04T12:00:00.000Z`.
#[must_use]
pub fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[must_use]
pub fn new_id() -> String {
    // UUIDv7: time-ordered (better SQLite locality) while old UUIDv4 rows
    // coexist untouched — IDs are opaque text. Exposes creation time, so
    // never treat an ID as privacy-preserving.
    Uuid::now_v7().to_string()
}

/// Lifecycle of a teacher session. Locking/signing arrives in Phase 20.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Draft,
    Locked,
    Analyzed,
}

impl SessionStatus {
    /// Parse the status string stored in SQLite.
    pub fn parse(s: &str) -> Result<Self, CoreError> {
        match s {
            "draft" => Ok(Self::Draft),
            "locked" => Ok(Self::Locked),
            "analyzed" => Ok(Self::Analyzed),
            other => Err(CoreError::validation(format!(
                "unknown session status: {other}"
            ))),
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Locked => "locked",
            Self::Analyzed => "analyzed",
        }
    }
}

/// How submission content entered the app. Grows with input phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    PastedText,
    TxtFile,
    MarkdownFile,
    PdfDigital,
    DocxFile,
}

impl SourceType {
    pub fn parse(s: &str) -> Result<Self, CoreError> {
        match s {
            "pasted_text" => Ok(Self::PastedText),
            "txt_file" => Ok(Self::TxtFile),
            "markdown_file" => Ok(Self::MarkdownFile),
            "pdf_digital" => Ok(Self::PdfDigital),
            "docx_file" => Ok(Self::DocxFile),
            other => Err(CoreError::validation(format!(
                "unknown source type: {other}"
            ))),
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PastedText => "pasted_text",
            Self::TxtFile => "txt_file",
            Self::MarkdownFile => "markdown_file",
            Self::PdfDigital => "pdf_digital",
            Self::DocxFile => "docx_file",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubmissionStatus {
    Draft,
    Ready,
}

impl SubmissionStatus {
    pub fn parse(s: &str) -> Result<Self, CoreError> {
        match s {
            "draft" => Ok(Self::Draft),
            "ready" => Ok(Self::Ready),
            other => Err(CoreError::validation(format!(
                "unknown submission status: {other}"
            ))),
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Ready => "ready",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub subject: Option<String>,
    pub status: SessionStatus,
    /// Assignment question/instructions; matches excluded from scoring.
    pub assignment_prompt: Option<String>,
    /// Extra excluded reference text (declarations, boilerplate).
    pub excluded_reference_text: Option<String>,
    /// Whether session-frequency common text is excluded (default true).
    pub exclude_common_text: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Public lock metadata returned to the UI; private signing material is never
/// included in a session or lock response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionLockSummary {
    pub session_id: String,
    pub locked_at: String,
    pub manifest_sha256: String,
    pub signing_key_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewSession {
    pub name: String,
    pub subject: Option<String>,
}

impl NewSession {
    /// Trim + validate. Returns the cleaned value.
    pub fn validated(self) -> Result<ValidatedNewSession, CoreError> {
        let name = self.name.trim().to_string();
        if name.is_empty() {
            return Err(CoreError::validation("session name must not be empty"));
        }
        if name.chars().count() > MAX_NAME_LEN {
            return Err(CoreError::validation(format!(
                "session name must be at most {MAX_NAME_LEN} characters"
            )));
        }
        let subject = match self.subject {
            Some(s) => {
                let t = s.trim().to_string();
                if t.is_empty() {
                    None
                } else {
                    if t.chars().count() > MAX_SUBJECT_LEN {
                        return Err(CoreError::validation(format!(
                            "subject must be at most {MAX_SUBJECT_LEN} characters"
                        )));
                    }
                    Some(t)
                }
            }
            None => None,
        };
        Ok(ValidatedNewSession { name, subject })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedNewSession {
    pub name: String,
    pub subject: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionUpdate {
    pub name: Option<String>,
    pub subject: Option<Option<String>>,
    pub assignment_prompt: Option<Option<String>>,
    pub excluded_reference_text: Option<Option<String>>,
    pub exclude_common_text: Option<bool>,
}

fn clean_optional_text(
    raw: &str,
    max_chars: usize,
    field: &str,
) -> Result<Option<String>, CoreError> {
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > max_chars {
        return Err(CoreError::validation(format!(
            "{field} must be at most {max_chars} characters"
        )));
    }
    Ok(Some(trimmed))
}

impl SessionUpdate {
    pub fn validated_name(&self) -> Result<Option<String>, CoreError> {
        match &self.name {
            None => Ok(None),
            Some(raw) => {
                let name = raw.trim().to_string();
                if name.is_empty() {
                    return Err(CoreError::validation("session name must not be empty"));
                }
                if name.chars().count() > MAX_NAME_LEN {
                    return Err(CoreError::validation(format!(
                        "session name must be at most {MAX_NAME_LEN} characters"
                    )));
                }
                Ok(Some(name))
            }
        }
    }

    pub fn validated_subject(&self) -> Result<Option<Option<String>>, CoreError> {
        match &self.subject {
            None => Ok(None),
            Some(None) => Ok(Some(None)),
            Some(Some(raw)) => {
                let t = raw.trim().to_string();
                if t.is_empty() {
                    return Ok(Some(None));
                }
                if t.chars().count() > MAX_SUBJECT_LEN {
                    return Err(CoreError::validation(format!(
                        "subject must be at most {MAX_SUBJECT_LEN} characters"
                    )));
                }
                Ok(Some(Some(t)))
            }
        }
    }

    pub fn validated_prompt(&self) -> Result<Option<Option<String>>, CoreError> {
        match &self.assignment_prompt {
            None => Ok(None),
            Some(None) => Ok(Some(None)),
            Some(Some(raw)) => Ok(Some(clean_optional_text(
                raw,
                MAX_PROMPT_CHARS,
                "assignment question",
            )?)),
        }
    }

    pub fn validated_reference(&self) -> Result<Option<Option<String>>, CoreError> {
        match &self.excluded_reference_text {
            None => Ok(None),
            Some(None) => Ok(Some(None)),
            Some(Some(raw)) => Ok(Some(clean_optional_text(
                raw,
                MAX_REFERENCE_CHARS,
                "reference text",
            )?)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Student {
    pub id: String,
    pub session_id: String,
    pub display_name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewStudent {
    pub display_name: String,
}

impl NewStudent {
    pub fn validated_name(&self) -> Result<String, CoreError> {
        let name = self.display_name.trim().to_string();
        if name.is_empty() {
            return Err(CoreError::validation("student name must not be empty"));
        }
        if name.chars().count() > MAX_DISPLAY_NAME_LEN {
            return Err(CoreError::validation(format!(
                "student name must be at most {MAX_DISPLAY_NAME_LEN} characters"
            )));
        }
        Ok(name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Submission {
    pub id: String,
    pub student_id: String,
    pub session_id: String,
    pub source_type: SourceType,
    pub status: SubmissionStatus,
    /// LF-normalized text (empty until Phase 3 content is saved).
    pub original_text: String,
    pub source_filename: Option<String>,
    pub content_sha256: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A completed session snapshot used as immutable comparison material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceLibrary {
    pub id: String,
    pub name: String,
    pub source_session_id: String,
    pub source_session_name: String,
    pub created_at: String,
    pub fingerprint_version: u32,
    pub normalization_version: u32,
    pub modified_version: u32,
}

/// An anonymization-ready, read-only document captured in a reference library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceSubmission {
    pub id: String,
    pub library_id: String,
    pub source_label: String,
    pub source_filename: Option<String>,
    pub source_type: SourceType,
    pub original_text: String,
    pub content_sha256: String,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_status_round_trips() {
        for status in [
            SessionStatus::Draft,
            SessionStatus::Locked,
            SessionStatus::Analyzed,
        ] {
            assert_eq!(
                SessionStatus::parse(status.as_str()).expect("parse"),
                status
            );
        }
    }

    #[test]
    fn unknown_status_rejected() {
        assert!(SessionStatus::parse("archived").is_err());
    }

    #[test]
    fn source_and_submission_statuses_round_trip() {
        assert_eq!(
            SourceType::parse("pasted_text").expect("parse"),
            SourceType::PastedText
        );
        assert_eq!(
            SourceType::parse("pdf_digital").expect("parse"),
            SourceType::PdfDigital
        );
        assert_eq!(
            SourceType::parse("docx_file").expect("parse"),
            SourceType::DocxFile
        );
        assert!(SourceType::parse("nope").is_err());
        assert_eq!(
            SubmissionStatus::parse("ready").expect("parse"),
            SubmissionStatus::Ready
        );
        assert!(SubmissionStatus::parse("nope").is_err());
    }

    #[test]
    fn new_session_trims_and_clears_blank_subject() {
        let v = NewSession {
            name: "  Biology 1  ".to_string(),
            subject: Some("   ".to_string()),
        }
        .validated()
        .expect("valid");
        assert_eq!(v.name, "Biology 1");
        assert_eq!(v.subject, None);
    }

    #[test]
    fn new_session_rejects_empty_and_whitespace_names() {
        for bad in ["", "   ", "\t\n "] {
            assert!(
                NewSession {
                    name: bad.to_string(),
                    subject: None,
                }
                .validated()
                .is_err(),
                "must reject {bad:?}"
            );
        }
    }

    #[test]
    fn new_session_rejects_overlong_name_and_subject() {
        let long = "x".repeat(MAX_NAME_LEN + 1);
        assert!(NewSession {
            name: long,
            subject: None,
        }
        .validated()
        .is_err());
        let long_subject = "y".repeat(MAX_SUBJECT_LEN + 1);
        assert!(NewSession {
            name: "ok".to_string(),
            subject: Some(long_subject),
        }
        .validated()
        .is_err());
    }

    #[test]
    fn session_update_validation_trims_and_clears() {
        let u = SessionUpdate {
            name: Some("  New name ".to_string()),
            subject: Some(Some("  ".to_string())),
            assignment_prompt: Some(Some("  Discuss this.  ".to_string())),
            excluded_reference_text: None,
            exclude_common_text: None,
        };
        assert_eq!(
            u.validated_name().expect("name"),
            Some("New name".to_string())
        );
        assert_eq!(u.validated_subject().expect("subject"), Some(None));
        assert_eq!(
            u.validated_prompt().expect("prompt"),
            Some(Some("Discuss this.".to_string()))
        );
    }

    #[test]
    fn session_update_rejects_blank_name() {
        let u = SessionUpdate {
            name: Some("  ".to_string()),
            subject: None,
            assignment_prompt: None,
            excluded_reference_text: None,
            exclude_common_text: None,
        };
        assert!(u.validated_name().is_err());
    }

    #[test]
    fn session_update_rejects_oversize_prompt_and_reference() {
        let u = SessionUpdate {
            name: None,
            subject: None,
            assignment_prompt: Some(Some("q".repeat(MAX_PROMPT_CHARS + 1))),
            excluded_reference_text: Some(Some("r".repeat(MAX_REFERENCE_CHARS + 1))),
            exclude_common_text: Some(false),
        };
        assert!(u.validated_prompt().is_err());
        assert!(u.validated_reference().is_err());
    }

    #[test]
    fn student_name_must_be_meaningful_and_bounded() {
        assert!(NewStudent {
            display_name: "  ".to_string()
        }
        .validated_name()
        .is_err());
        assert!(NewStudent {
            display_name: "z".repeat(MAX_DISPLAY_NAME_LEN + 1)
        }
        .validated_name()
        .is_err());
        assert_eq!(
            NewStudent {
                display_name: "  Amit  ".to_string()
            }
            .validated_name()
            .expect("valid"),
            "Amit"
        );
    }

    #[test]
    fn ids_are_unique_and_timestamps_parse() {
        assert_ne!(new_id(), new_id());
        let ts = now_iso();
        assert!(chrono::DateTime::parse_from_rfc3339(&ts).is_ok());
    }

    #[test]
    fn new_ids_are_uuidv7_and_sortable() {
        // Version nibble is 7; time-ordering means later IDs sort after.
        let first = new_id();
        let parsed = Uuid::parse_str(&first).expect("new_id is a valid UUID");
        assert_eq!(parsed.get_version_num(), 7);
        let second = new_id();
        assert_ne!(first, second);
        assert!(first < second, "v7 time-ordering: {first} < {second}");
    }
}

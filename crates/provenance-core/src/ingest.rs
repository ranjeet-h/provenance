//! Phase 3: digital text ingestion — hashing, validation, newline normalization.
//!
//! Decisions:
//! - Stored text uses LF newlines (CRLF/CR normalized), so the same document
//!   hashes identically on Windows and Unix.
//! - `content_sha256` covers the normalized stored text bytes.
//! - Filenames are metadata only and never affect the hash.

use sha2::{Digest, Sha256};

use crate::error::CoreError;

/// Reject raw inputs larger than this (protects the local DB).
pub const MAX_SUBMISSION_BYTES: usize = 5_000_000;

/// Reject raw upload bytes larger than this BEFORE dispatching to any
/// parser (protects ZIP/PDF parsing from avoidable memory/CPU pressure).
/// Single uniform limit: uploads above it are rejected no matter the type.
pub const MAX_UPLOAD_BYTES: usize = 5_000_000;

/// Reject a DOCX whose `document.xml` expands beyond this (ZIP-bomb guard).
/// Checked while reading, before XML parsing begins.
pub const MAX_DOCX_XML_BYTES: usize = 10_000_000;

/// Reject stored filenames longer than this.
pub const MAX_FILENAME_CHARS: usize = 255;

/// Hex SHA-256 over bytes. Deterministic across platforms.
#[must_use]
pub fn content_sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Normalize all line endings to LF.
#[must_use]
pub fn normalize_newlines(text: &str) -> String {
    // CRLF first so lone-CR handling does not double-convert.
    text.replace("\r\n", "\n").replace('\r', "\n")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedText {
    /// LF-normalized, non-blank text ready to store.
    pub text: String,
    /// SHA-256 hex over `text` bytes.
    pub sha256: String,
}

/// Validate raw file/paste bytes into storable text.
pub fn validate_submission_bytes(bytes: &[u8]) -> Result<ValidatedText, CoreError> {
    if bytes.len() > MAX_SUBMISSION_BYTES {
        return Err(CoreError::validation(format!(
            "text is too large (max {} MB)",
            MAX_SUBMISSION_BYTES / 1_000_000
        )));
    }
    let raw = std::str::from_utf8(bytes)
        .map_err(|_| CoreError::validation("file is not valid UTF-8 text"))?;
    let text = normalize_newlines(raw);
    if text.trim().is_empty() {
        return Err(CoreError::validation("submission text must not be empty"));
    }
    let sha256 = content_sha256_hex(text.as_bytes());
    Ok(ValidatedText { text, sha256 })
}

/// Clean an optional upload filename into storable metadata.
pub fn clean_filename(raw: Option<&str>) -> Result<Option<String>, CoreError> {
    match raw {
        None => Ok(None),
        Some(name) => {
            let trimmed = name.trim().to_string();
            if trimmed.is_empty() {
                return Ok(None);
            }
            if trimmed.chars().count() > MAX_FILENAME_CHARS {
                return Err(CoreError::validation("filename is too long"));
            }
            Ok(Some(trimmed))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_sha256_vector() {
        assert_eq!(
            content_sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn same_bytes_same_hash_one_char_differs() {
        assert_eq!(
            content_sha256_hex(b"hello world"),
            content_sha256_hex(b"hello world")
        );
        assert_ne!(
            content_sha256_hex(b"hello world"),
            content_sha256_hex(b"hello worle")
        );
    }

    #[test]
    fn unicode_bytes_are_deterministic() {
        let a = content_sha256_hex("naïve café — “quotes”".as_bytes());
        let b = content_sha256_hex("naïve café — “quotes”".as_bytes());
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn crlf_and_lf_produce_identical_text_and_hash() {
        let crlf = validate_submission_bytes(b"line one\r\nline two\r\n").expect("crlf");
        let lf = validate_submission_bytes(b"line one\nline two\n").expect("lf");
        assert_eq!(crlf.text, "line one\nline two\n");
        assert_eq!(crlf, lf);
    }

    #[test]
    fn lone_cr_is_normalized() {
        let v = validate_submission_bytes(b"a\rb").expect("cr");
        assert_eq!(v.text, "a\nb");
    }

    #[test]
    fn empty_and_whitespace_only_rejected() {
        for bad in [&b""[..], b"   ", b"\n\r\n\t  "] {
            assert!(validate_submission_bytes(bad).is_err());
        }
    }

    #[test]
    fn invalid_utf8_rejected_as_binary() {
        assert!(validate_submission_bytes(&[0xFF, 0xFE, 0x00, 0x61]).is_err());
    }

    #[test]
    fn oversize_input_rejected() {
        let big = vec![b'a'; MAX_SUBMISSION_BYTES + 1];
        assert!(validate_submission_bytes(&big).is_err());
        let ok = vec![b'a'; 1024];
        assert!(validate_submission_bytes(&ok).is_ok());
    }

    #[test]
    fn filenames_cleaned_and_bounded() {
        assert_eq!(clean_filename(None).expect("none"), None);
        assert_eq!(clean_filename(Some("  ")).expect("blank"), None);
        assert_eq!(
            clean_filename(Some("  essay.txt  ")).expect("trim"),
            Some("essay.txt".to_string())
        );
        assert!(clean_filename(Some(&"n".repeat(MAX_FILENAME_CHARS + 1))).is_err());
    }
}

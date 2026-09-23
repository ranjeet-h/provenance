//! Detached Ed25519 certificates for reports and immutable session locks.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::report::{ReportMode, StudentReport};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignatureProof {
    pub algorithm: String,
    pub key_id: String,
    pub public_key_hex: String,
    pub signature_hex: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CertifiedStudentReport {
    pub schema_version: u32,
    pub report: StudentReport,
    pub lock_sha256: String,
    pub locked_at: String,
    pub certified_at: String,
    pub signature: SignatureProof,
}

/// Compact verification metadata encoded into the certified PDF QR code.
/// The companion signed JSON remains required to verify the complete report.
pub fn verification_qr_payload(certified: &CertifiedStudentReport) -> String {
    format!(
        "PROVENANCE1|{}|{}|{}|{}",
        certified.signature.key_id,
        certified.lock_sha256,
        certified.report.integrity.payload_sha256,
        certified.signature.signature_hex,
    )
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SignatureError {
    #[error("signature algorithm is unsupported")]
    UnsupportedAlgorithm,
    #[error("signature key ID does not match its public key")]
    KeyIdMismatch,
    #[error("signature public key is invalid")]
    InvalidPublicKey,
    #[error("signature encoding is invalid")]
    InvalidSignatureEncoding,
    #[error("signature verification failed")]
    InvalidSignature,
    #[error("report integrity hash is invalid")]
    InvalidReportIntegrity,
    #[error("only teacher-mode reports can be certified")]
    SelfCheckCannotBeCertified,
    #[error("session lock digest or timestamp is invalid")]
    InvalidLockMetadata,
    #[error("certified report could not be serialized")]
    Serialization,
}

const SIGNED_REPORT_CONTEXT: &[u8] = b"provenance/signed-student-report/v1\0";

#[derive(Serialize)]
struct CertifiedReportMessage<'a> {
    schema_version: u32,
    report: &'a StudentReport,
    lock_sha256: &'a str,
    locked_at: &'a str,
    certified_at: &'a str,
}

pub fn sign_bytes(domain: &[u8], payload: &[u8], key: &SigningKey) -> SignatureProof {
    let mut message = Vec::with_capacity(domain.len() + payload.len());
    message.extend_from_slice(domain);
    message.extend_from_slice(payload);
    let signature = key.sign(&message);
    let public_key = key.verifying_key().to_bytes();
    SignatureProof {
        algorithm: "Ed25519".into(),
        key_id: signing_key_id(key),
        public_key_hex: hex::encode(public_key),
        signature_hex: hex::encode(signature.to_bytes()),
    }
}

pub fn signing_key_id(key: &SigningKey) -> String {
    hex::encode(Sha256::digest(key.verifying_key().to_bytes()))
}

pub fn verify_bytes(
    domain: &[u8],
    payload: &[u8],
    proof: &SignatureProof,
) -> Result<(), SignatureError> {
    if proof.algorithm != "Ed25519" {
        return Err(SignatureError::UnsupportedAlgorithm);
    }
    let public_bytes =
        hex::decode(&proof.public_key_hex).map_err(|_| SignatureError::InvalidPublicKey)?;
    let public_array: [u8; 32] = public_bytes
        .try_into()
        .map_err(|_| SignatureError::InvalidPublicKey)?;
    if hex::encode(Sha256::digest(public_array)) != proof.key_id {
        return Err(SignatureError::KeyIdMismatch);
    }
    let public_key =
        VerifyingKey::from_bytes(&public_array).map_err(|_| SignatureError::InvalidPublicKey)?;
    let signature_bytes =
        hex::decode(&proof.signature_hex).map_err(|_| SignatureError::InvalidSignatureEncoding)?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| SignatureError::InvalidSignatureEncoding)?;
    let mut message = Vec::with_capacity(domain.len() + payload.len());
    message.extend_from_slice(domain);
    message.extend_from_slice(payload);
    public_key
        .verify_strict(&message, &signature)
        .map_err(|_| SignatureError::InvalidSignature)
}

pub fn certify_student_report(
    report: StudentReport,
    lock_sha256: &str,
    locked_at: &str,
    certified_at: &str,
    key: &SigningKey,
) -> Result<CertifiedStudentReport, SignatureError> {
    if !report.verify_integrity() {
        return Err(SignatureError::InvalidReportIntegrity);
    }
    if report.payload.report_mode != ReportMode::Teacher {
        return Err(SignatureError::SelfCheckCannotBeCertified);
    }
    if lock_sha256.len() != 64
        || !lock_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        || locked_at.trim().is_empty()
        || certified_at.trim().is_empty()
    {
        return Err(SignatureError::InvalidLockMetadata);
    }
    let mut certified = CertifiedStudentReport {
        schema_version: 1,
        report,
        lock_sha256: lock_sha256.to_ascii_lowercase(),
        locked_at: locked_at.to_string(),
        certified_at: certified_at.to_string(),
        signature: SignatureProof {
            algorithm: String::new(),
            key_id: String::new(),
            public_key_hex: String::new(),
            signature_hex: String::new(),
        },
    };
    let bytes = certified_message(&certified)?;
    certified.signature = sign_bytes(SIGNED_REPORT_CONTEXT, &bytes, key);
    Ok(certified)
}

pub fn verify_certified_student_report(
    certified: &CertifiedStudentReport,
) -> Result<(), SignatureError> {
    if certified.schema_version != 1 {
        return Err(SignatureError::Serialization);
    }
    if !certified.report.verify_integrity() {
        return Err(SignatureError::InvalidReportIntegrity);
    }
    if certified.report.payload.report_mode != ReportMode::Teacher {
        return Err(SignatureError::SelfCheckCannotBeCertified);
    }
    if certified.lock_sha256.len() != 64
        || !certified
            .lock_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || certified.locked_at.trim().is_empty()
        || certified.certified_at.trim().is_empty()
    {
        return Err(SignatureError::InvalidLockMetadata);
    }
    let bytes = certified_message(certified)?;
    verify_bytes(SIGNED_REPORT_CONTEXT, &bytes, &certified.signature)
}

fn certified_message(certified: &CertifiedStudentReport) -> Result<Vec<u8>, SignatureError> {
    serde_json::to_vec(&CertifiedReportMessage {
        schema_version: certified.schema_version,
        report: &certified.report,
        lock_sha256: &certified.lock_sha256,
        locked_at: &certified.locked_at,
        certified_at: &certified.certified_at,
    })
    .map_err(|_| SignatureError::Serialization)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{build_report, ReportInput, ReportMode};

    fn key(byte: u8) -> SigningKey {
        SigningKey::from_bytes(&[byte; 32])
    }

    fn report() -> StudentReport {
        build_report(ReportInput {
            session_id: "session".into(),
            session_name: "Seminar".into(),
            subject: None,
            target_student_id: "student".into(),
            target_student_name: "Avery".into(),
            target_source_id: "source".into(),
            generated_at: "2026-09-23T00:00:00Z".into(),
            report_mode: ReportMode::Teacher,
            anonymize: false,
            current_comparisons: Vec::new(),
            historical_libraries: Vec::new(),
            overall_coverage: None,
            exact_coverage: None,
            modified_coverage: None,
            matches: Vec::new(),
            exclusions: Vec::new(),
            sources: vec![crate::report::ReportSource {
                id: "source".into(),
                label: "Avery".into(),
                corpus: crate::report::SourceCorpus::Current,
                text: "An assessed passage.".into(),
                content_sha256: hex::encode(Sha256::digest(b"An assessed passage.")),
            }],
            engine_versions: crate::report::EngineVersions {
                fingerprint: 1,
                normalization: 1,
                common_text: 1,
                modified: 1,
            },
        })
        .unwrap()
    }

    fn certified() -> CertifiedStudentReport {
        certify_student_report(
            report(),
            &"a".repeat(64),
            "2026-09-23T00:00:00.000Z",
            "2026-09-23T00:01:00.000Z",
            &key(7),
        )
        .unwrap()
    }

    #[test]
    fn sign_then_verify_is_valid_and_json_round_trip_preserves_it() {
        let signed = certified();
        verify_certified_student_report(&signed).unwrap();
        let json = serde_json::to_vec(&signed).unwrap();
        let decoded: CertifiedStudentReport = serde_json::from_slice(&json).unwrap();
        verify_certified_student_report(&decoded).unwrap();
    }

    #[test]
    fn a_single_payload_byte_mutation_invalidates_report_integrity_and_signature() {
        let mut signed = certified();
        signed.report.payload.session_name.push('!');
        assert_eq!(
            verify_certified_student_report(&signed),
            Err(SignatureError::InvalidReportIntegrity)
        );
    }

    #[test]
    fn a_signature_from_a_different_key_cannot_verify() {
        let signed = certified();
        let message = certified_message(&signed).unwrap();
        let mut wrong_key_proof = signed.signature.clone();
        let wrong_public_key = key(9).verifying_key().to_bytes();
        wrong_key_proof.key_id = hex::encode(Sha256::digest(wrong_public_key));
        wrong_key_proof.public_key_hex = hex::encode(wrong_public_key);
        assert_eq!(
            verify_bytes(SIGNED_REPORT_CONTEXT, &message, &wrong_key_proof),
            Err(SignatureError::InvalidSignature)
        );
    }

    #[test]
    fn signature_covers_lock_digest_timestamps_and_required_footer() {
        let mut lock_change = certified();
        lock_change.lock_sha256.replace_range(..1, "b");
        assert_eq!(
            verify_certified_student_report(&lock_change),
            Err(SignatureError::InvalidSignature)
        );

        let mut footer_change = certified();
        footer_change.report.payload.reviewer_disclaimer.push('!');
        assert_eq!(
            verify_certified_student_report(&footer_change),
            Err(SignatureError::InvalidReportIntegrity)
        );
    }

    #[test]
    fn self_check_reports_are_never_certifiable() {
        let mut self_check = report();
        self_check.payload.report_mode = ReportMode::SelfCheck;
        self_check.integrity.payload_sha256 = hex::encode(Sha256::digest(
            serde_json::to_vec(&self_check.payload).unwrap(),
        ));
        assert_eq!(
            certify_student_report(
                self_check,
                &"a".repeat(64),
                "2026-09-23T00:00:00.000Z",
                "2026-09-23T00:01:00.000Z",
                &key(7),
            ),
            Err(SignatureError::SelfCheckCannotBeCertified)
        );
    }

    #[test]
    fn certified_pdf_contains_signature_metadata_and_ends_with_review_footer() {
        let signed = certified();
        let pdf = crate::report::render_certified_report_pdf(&signed).unwrap();
        let text = crate::report::extract_pdf_text(&pdf).unwrap();
        let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(normalized.contains("Teacher-certified session"));
        assert!(normalized.contains(&signed.signature.key_id));
        assert!(normalized.contains(&signed.signature.signature_hex));
        assert!(normalized.ends_with(crate::report::REVIEWER_DISCLAIMER));
        assert!(
            pdf.windows(b"/Image".len())
                .any(|window| window == b"/Image"),
            "image XObject not found; pdf length {}; excerpt: {}",
            pdf.len(),
            String::from_utf8_lossy(&pdf[..pdf.len().min(500)])
        );

        let mut altered = signed;
        altered.report.payload.session_name.push('!');
        assert!(crate::report::render_certified_report_pdf(&altered).is_err());
    }

    #[test]
    fn qr_payload_contains_lock_report_and_signature_fingerprints() {
        let signed = certified();
        assert_eq!(
            verification_qr_payload(&signed),
            format!(
                "PROVENANCE1|{}|{}|{}|{}",
                signed.signature.key_id,
                signed.lock_sha256,
                signed.report.integrity.payload_sha256,
                signed.signature.signature_hex,
            )
        );
    }
}

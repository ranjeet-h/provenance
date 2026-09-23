//! Versioned, anonymized, text-only portable Reference Library container.

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub const PLAGPACK_FORMAT_VERSION: u32 = 1;
pub const MAX_PLAGPACK_BYTES: usize = 50_000_000;
pub const MAX_PLAGPACK_EXPANDED_BYTES: usize = 100_000_000;
pub const MAX_PLAGPACK_DOCUMENTS: usize = 10_000;
pub const MAX_DOCUMENT_TEXT_BYTES: usize = 5_000_000;
const MANIFEST_ENTRY: &str = "manifest.json";
const DOCUMENTS_ENTRY: &str = "documents.json";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PlagPackError {
    #[error("invalid .plagpack: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackSourceDocument {
    pub source_type: String,
    pub original_text: String,
    pub content_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedPlagPack {
    pub library_name: String,
    pub engine_versions: PackEngineVersions,
    pub documents: Vec<PackDocument>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackEngineVersions {
    pub fingerprint: u32,
    pub normalization: u32,
    pub modified: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackDocument {
    pub source_label: String,
    pub source_type: String,
    pub canonical_text: String,
    pub original_text: String,
    pub content_sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlagPackManifest {
    plagpack_format_version: u32,
    library_name: String,
    exported_at: String,
    anonymized: bool,
    document_count: usize,
    documents_sha256: String,
    engine_versions: PackEngineVersions,
}

/// Serialize unique submission contents to a two-entry deflated ZIP package.
/// Student names, student IDs, and source filenames are intentionally not inputs.
pub fn export_plagpack(
    library_name: &str,
    engine_versions: PackEngineVersions,
    sources: &[PackSourceDocument],
) -> Result<Vec<u8>, PlagPackError> {
    let library_name = validate_library_name(library_name)?;
    if engine_versions.normalization != provenance_match::NORMALIZATION_VERSION {
        return Err(invalid(format!(
            "normalization engine version {} is unsupported for export",
            engine_versions.normalization
        )));
    }
    if sources.is_empty() {
        return Err(invalid(
            "a Reference Library must contain at least one document",
        ));
    }
    if sources.len() > MAX_PLAGPACK_DOCUMENTS {
        return Err(invalid(format!(
            "the library exceeds the {MAX_PLAGPACK_DOCUMENTS} document limit"
        )));
    }

    let mut documents = Vec::new();
    let mut seen_hashes: HashMap<String, String> = HashMap::new();
    let mut expanded_bytes = 0usize;
    for source in sources {
        validate_source_type(&source.source_type)?;
        validate_text(&source.original_text)?;
        let actual_hash = sha256_hex(source.original_text.as_bytes());
        if source.content_sha256 != actual_hash {
            return Err(invalid("a document hash does not match its text"));
        }
        expanded_bytes = expanded_bytes
            .checked_add(source.original_text.len())
            .ok_or_else(|| invalid("the library expanded size exceeds its limit"))?;
        if expanded_bytes > MAX_PLAGPACK_EXPANDED_BYTES {
            return Err(invalid("the library expanded size exceeds 100 MB"));
        }
        if let Some(previous_text) = seen_hashes.get(&actual_hash) {
            if previous_text != &source.original_text {
                return Err(invalid("two different documents share one content hash"));
            }
            continue;
        }
        seen_hashes.insert(actual_hash.clone(), source.original_text.clone());
        let canonical_text = provenance_match::canonicalize(&source.original_text).normalized_text;
        documents.push(PackDocument {
            source_label: format!("Document {}", documents.len() + 1),
            source_type: source.source_type.clone(),
            canonical_text,
            original_text: source.original_text.clone(),
            content_sha256: actual_hash,
        });
    }
    if documents.is_empty() {
        return Err(invalid("the library contains no unique text documents"));
    }
    let documents_json = serde_json::to_vec(&documents)
        .map_err(|_| invalid("the library documents could not be serialized"))?;
    if documents_json.len() > MAX_PLAGPACK_EXPANDED_BYTES {
        return Err(invalid("the library expanded size exceeds 100 MB"));
    }
    let manifest = PlagPackManifest {
        plagpack_format_version: PLAGPACK_FORMAT_VERSION,
        library_name,
        exported_at: chrono::Utc::now().to_rfc3339(),
        anonymized: true,
        document_count: documents.len(),
        documents_sha256: sha256_hex(&documents_json),
        engine_versions,
    };
    let manifest_json = serde_json::to_vec(&manifest)
        .map_err(|_| invalid("the package manifest could not be serialized"))?;

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    writer
        .start_file(MANIFEST_ENTRY, options)
        .map_err(|_| invalid("the package archive could not be written"))?;
    writer
        .write_all(&manifest_json)
        .map_err(|_| invalid("the package manifest could not be written"))?;
    writer
        .start_file(DOCUMENTS_ENTRY, options)
        .map_err(|_| invalid("the package archive could not be written"))?;
    writer
        .write_all(&documents_json)
        .map_err(|_| invalid("the package documents could not be written"))?;
    let bytes = writer
        .finish()
        .map_err(|_| invalid("the package archive could not be finalized"))?
        .into_inner();
    if bytes.len() > MAX_PLAGPACK_BYTES {
        return Err(invalid("the compressed library exceeds 50 MB"));
    }
    Ok(bytes)
}

/// Validate archive shape, size, manifest, payload hash, document hashes,
/// canonical text, source types, and strict anonymized labels before returning data.
pub fn import_plagpack(bytes: &[u8]) -> Result<ImportedPlagPack, PlagPackError> {
    if bytes.is_empty() || bytes.len() > MAX_PLAGPACK_BYTES {
        return Err(invalid("the package is empty or exceeds 50 MB"));
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| invalid("the archive is corrupt or is not a valid ZIP package"))?;
    if archive.len() != 2 {
        return Err(invalid(
            "the package must contain only manifest.json and documents.json",
        ));
    }

    let mut entries = std::collections::HashMap::new();
    let mut expanded_bytes = 0usize;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| invalid("the package contains a corrupt archive entry"))?;
        let name = entry.name().to_string();
        if entry.is_dir() || !matches!(name.as_str(), MANIFEST_ENTRY | DOCUMENTS_ENTRY) {
            return Err(invalid("the package contains an unsupported entry"));
        }
        if entries.contains_key(&name) {
            return Err(invalid("the package contains a duplicate entry"));
        }
        let declared_size = usize::try_from(entry.size())
            .map_err(|_| invalid("an archive entry exceeds the supported size"))?;
        expanded_bytes = expanded_bytes
            .checked_add(declared_size)
            .ok_or_else(|| invalid("the package expanded size exceeds its limit"))?;
        if expanded_bytes > MAX_PLAGPACK_EXPANDED_BYTES {
            return Err(invalid("the package expands beyond 100 MB"));
        }
        let mut content = Vec::with_capacity(declared_size);
        entry
            .by_ref()
            .take(MAX_PLAGPACK_EXPANDED_BYTES as u64 + 1)
            .read_to_end(&mut content)
            .map_err(|_| invalid("an archive entry could not be read or failed its checksum"))?;
        if content.len() != declared_size {
            return Err(invalid(
                "an archive entry size does not match its directory",
            ));
        }
        entries.insert(name, content);
    }
    let manifest_bytes = entries
        .remove(MANIFEST_ENTRY)
        .ok_or_else(|| invalid("manifest.json is missing"))?;
    let documents_bytes = entries
        .remove(DOCUMENTS_ENTRY)
        .ok_or_else(|| invalid("documents.json is missing"))?;
    if !entries.is_empty() {
        return Err(invalid("the package contains an unsupported entry"));
    }
    let manifest: PlagPackManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| invalid("manifest.json is malformed or contains unsupported fields"))?;
    if manifest.plagpack_format_version != PLAGPACK_FORMAT_VERSION {
        return Err(invalid(format!(
            "format version {} is unsupported; this app reads version {}",
            manifest.plagpack_format_version, PLAGPACK_FORMAT_VERSION
        )));
    }
    if !manifest.anonymized {
        return Err(invalid("the package must use anonymized student labels"));
    }
    if manifest.engine_versions.fingerprint == 0
        || manifest.engine_versions.normalization == 0
        || manifest.engine_versions.modified == 0
    {
        return Err(invalid("the manifest engine versions are invalid"));
    }
    if manifest.engine_versions.normalization != provenance_match::NORMALIZATION_VERSION {
        return Err(invalid(format!(
            "normalization engine version {} is unsupported; this app validates version {}",
            manifest.engine_versions.normalization,
            provenance_match::NORMALIZATION_VERSION
        )));
    }
    let library_name = validate_library_name(&manifest.library_name)?;
    if manifest.document_count == 0 || manifest.document_count > MAX_PLAGPACK_DOCUMENTS {
        return Err(invalid(
            "the manifest document count is outside the supported limit",
        ));
    }
    if sha256_hex(&documents_bytes) != manifest.documents_sha256 {
        return Err(invalid("documents.json does not match the manifest hash"));
    }
    let mut documents: Vec<PackDocument> = serde_json::from_slice(&documents_bytes)
        .map_err(|_| invalid("documents.json is malformed or contains unsupported fields"))?;
    if documents.len() != manifest.document_count {
        return Err(invalid("the document count does not match the manifest"));
    }

    let mut seen_hashes: HashMap<String, String> = HashMap::new();
    let mut deduplicated = Vec::with_capacity(documents.len());
    let mut text_bytes = 0usize;
    for (index, document) in documents.drain(..).enumerate() {
        if document.source_label != format!("Document {}", index + 1) {
            return Err(invalid("student labels must be anonymized as Document N"));
        }
        validate_source_type(&document.source_type)?;
        validate_text(&document.original_text)?;
        text_bytes = text_bytes
            .checked_add(document.original_text.len())
            .ok_or_else(|| invalid("the package text exceeds its size limit"))?;
        if text_bytes > MAX_PLAGPACK_EXPANDED_BYTES {
            return Err(invalid("the package text expands beyond 100 MB"));
        }
        let actual_hash = sha256_hex(document.original_text.as_bytes());
        if document.content_sha256 != actual_hash {
            return Err(invalid("a document hash does not match its text"));
        }
        let canonical = provenance_match::canonicalize(&document.original_text).normalized_text;
        if document.canonical_text != canonical {
            return Err(invalid(
                "a document canonical text does not match its original text",
            ));
        }
        if let Some(previous_text) = seen_hashes.get(&actual_hash) {
            if previous_text != &document.original_text {
                return Err(invalid("two different documents share one content hash"));
            }
            continue;
        }
        seen_hashes.insert(actual_hash, document.original_text.clone());
        deduplicated.push(document);
    }
    Ok(ImportedPlagPack {
        library_name,
        engine_versions: manifest.engine_versions,
        documents: deduplicated,
    })
}

fn validate_library_name(name: &str) -> Result<String, PlagPackError> {
    let cleaned = name.trim();
    if cleaned.is_empty() || cleaned.chars().count() > 200 {
        return Err(invalid("library name must contain 1 to 200 characters"));
    }
    Ok(cleaned.to_string())
}

fn validate_source_type(source_type: &str) -> Result<(), PlagPackError> {
    if matches!(
        source_type,
        "pasted_text" | "txt_file" | "markdown_file" | "pdf_digital" | "docx_file"
    ) {
        Ok(())
    } else {
        Err(invalid("a package document has an unsupported source type"))
    }
}

fn validate_text(text: &str) -> Result<(), PlagPackError> {
    if text.trim().is_empty() {
        return Err(invalid("a package document contains no text"));
    }
    if text.len() > MAX_DOCUMENT_TEXT_BYTES {
        return Err(invalid("a package document exceeds the 5 MB text limit"));
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn invalid(message: impl Into<String>) -> PlagPackError {
    PlagPackError::Invalid(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn versions() -> PackEngineVersions {
        PackEngineVersions {
            fingerprint: 1,
            normalization: 1,
            modified: 1,
        }
    }

    fn source(text: &str) -> PackSourceDocument {
        PackSourceDocument {
            source_type: "pasted_text".to_string(),
            original_text: text.to_string(),
            content_sha256: sha256_hex(text.as_bytes()),
        }
    }

    fn round_trip(sources: &[PackSourceDocument]) -> (Vec<u8>, ImportedPlagPack) {
        let bytes = export_plagpack("History", versions(), sources).expect("export");
        let imported = import_plagpack(&bytes).expect("import");
        (bytes, imported)
    }

    fn package_for_documents(version: u32, documents: &[PackDocument]) -> Vec<u8> {
        package_with_versions(version, documents, versions())
    }

    fn package_with_versions(
        version: u32,
        documents: &[PackDocument],
        engine_versions: PackEngineVersions,
    ) -> Vec<u8> {
        let documents_json = serde_json::to_vec(documents).expect("documents json");
        let manifest = PlagPackManifest {
            plagpack_format_version: version,
            library_name: "History".to_string(),
            exported_at: "2026-09-23T00:00:00Z".to_string(),
            anonymized: true,
            document_count: documents.len(),
            documents_sha256: sha256_hex(&documents_json),
            engine_versions,
        };
        let manifest_json = serde_json::to_vec(&manifest).expect("manifest json");
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer.start_file(MANIFEST_ENTRY, options).unwrap();
        writer.write_all(&manifest_json).unwrap();
        writer.start_file(DOCUMENTS_ENTRY, options).unwrap();
        writer.write_all(&documents_json).unwrap();
        writer.finish().unwrap().into_inner()
    }

    fn document(text: &str, label: &str) -> PackDocument {
        PackDocument {
            source_label: label.to_string(),
            source_type: "pasted_text".to_string(),
            canonical_text: provenance_match::canonicalize(text).normalized_text,
            original_text: text.to_string(),
            content_sha256: sha256_hex(text.as_bytes()),
        }
    }

    #[test]
    fn round_trip_preserves_original_and_canonical_text() {
        let text = "Café Notes: Unicode,  and   spacing.\nSecond line.";
        let (bytes, imported) = round_trip(&[source(text)]);
        assert!(bytes.starts_with(b"PK"));
        assert_eq!(imported.library_name, "History");
        assert_eq!(imported.documents.len(), 1);
        assert_eq!(imported.documents[0].original_text, text);
        assert_eq!(
            imported.documents[0].canonical_text,
            provenance_match::canonicalize(text).normalized_text
        );
        assert_eq!(imported.documents[0].source_label, "Document 1");
    }

    #[test]
    fn exports_anonymous_labels_and_never_serializes_names_or_filenames() {
        let mut item =
            source("Long enough text to be stored in a portable anonymized class archive.");
        // The public export contract has no student-name or filename field.
        item.source_type = "docx_file".to_string();
        let (bytes, imported) = round_trip(&[item]);
        let mut archive = ZipArchive::new(Cursor::new(bytes)).expect("zip");
        let mut document_entry = archive.by_name(DOCUMENTS_ENTRY).expect("documents");
        let mut serialized = String::new();
        document_entry
            .read_to_string(&mut serialized)
            .expect("read");
        assert!(!serialized.contains("student_name"));
        assert!(!serialized.contains("source_filename"));
        assert_eq!(imported.documents[0].source_label, "Document 1");
    }

    #[test]
    fn duplicate_text_is_deduplicated_by_content_hash() {
        let same = source("Repeated source content is included only one time in a package.");
        let (_, imported) = round_trip(&[same.clone(), same]);
        assert_eq!(imported.documents.len(), 1);
    }

    #[test]
    fn importer_deduplicates_duplicate_documents_in_a_valid_package() {
        let repeated = "Repeated source content is included only one time in a package.";
        let package = package_for_documents(
            PLAGPACK_FORMAT_VERSION,
            &[
                document(repeated, "Document 1"),
                document(repeated, "Document 2"),
            ],
        );
        let imported = import_plagpack(&package).expect("valid package");
        assert_eq!(imported.documents.len(), 1);
    }

    #[test]
    fn corrupt_non_zip_is_rejected() {
        assert!(import_plagpack(b"not a zip").is_err());
    }

    #[test]
    fn missing_manifest_and_additional_entries_are_rejected() {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(DOCUMENTS_ENTRY, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"[]").unwrap();
        let package = writer.finish().unwrap().into_inner();
        assert!(import_plagpack(&package).is_err());
    }

    #[test]
    fn unsupported_future_manifest_version_is_rejected() {
        let package = package_for_documents(
            PLAGPACK_FORMAT_VERSION + 1,
            &[document(
                "A sufficiently long text passage in the library document.",
                "Document 1",
            )],
        );
        let error = import_plagpack(&package).expect_err("future format rejected");
        assert!(error
            .to_string()
            .contains("format version 2 is unsupported"));
    }

    #[test]
    fn unsupported_normalization_metadata_is_rejected_explicitly() {
        let package = package_with_versions(
            PLAGPACK_FORMAT_VERSION,
            &[document(
                "A sufficiently long text passage in the library document.",
                "Document 1",
            )],
            PackEngineVersions {
                fingerprint: 1,
                normalization: provenance_match::NORMALIZATION_VERSION + 1,
                modified: 1,
            },
        );
        let error = import_plagpack(&package).expect_err("normalization is unsupported");
        assert!(error.to_string().contains("normalization engine version"));
    }

    #[test]
    fn content_hash_mismatch_is_rejected_on_export() {
        let mut invalid_source =
            source("Hash mismatch text is never silently repaired or imported.");
        invalid_source.content_sha256 = "0".repeat(64);
        assert!(export_plagpack("History", versions(), &[invalid_source]).is_err());
    }

    #[test]
    fn image_source_types_are_not_allowed() {
        let mut image = source("Images are outside the supported reference package contract.");
        image.source_type = "image_file".to_string();
        assert!(export_plagpack("History", versions(), &[image]).is_err());
    }

    #[test]
    fn tampered_document_hash_canonical_text_and_identity_label_are_rejected() {
        let text = "A package must verify canonical text, identity labels, and content bytes before import.";
        let mut bad_hash = document(text, "Document 1");
        bad_hash.content_sha256 = "0".repeat(64);
        let error = import_plagpack(&package_for_documents(PLAGPACK_FORMAT_VERSION, &[bad_hash]))
            .expect_err("hash mismatch");
        assert!(error.to_string().contains("hash does not match"));

        let mut bad_canonical = document(text, "Document 1");
        bad_canonical.canonical_text = "tampered tokens".to_string();
        let error = import_plagpack(&package_for_documents(
            PLAGPACK_FORMAT_VERSION,
            &[bad_canonical],
        ))
        .expect_err("canonical mismatch");
        assert!(error.to_string().contains("canonical text does not match"));

        let identity_label = document(text, "Sensitive Student Name");
        let error = import_plagpack(&package_for_documents(
            PLAGPACK_FORMAT_VERSION,
            &[identity_label],
        ))
        .expect_err("student label is not anonymized");
        assert!(error.to_string().contains("labels must be anonymized"));
    }
}

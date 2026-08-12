//! Repository metadata: `.kat/repository.toml` (stable configuration).
//!
//! This is deliberately distinct from the mutable repository head
//! (`refs/accepted`, see `ref_store`): dynamic semantic state and accepted
//! history must never live here.

use std::fs;
use std::path::Path;
use std::str::FromStr;

use crate::domain::identity::{RepositoryId, SoftwareId};

/// Supported repository format version (v0.1).
const SUPPORTED_FORMAT_VERSION: u32 = 1;

/// Stable repository configuration persisted at `.kat/repository.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryMetadata {
    /// Repository format version.
    pub format_version: u32,
    /// Stable identity of the repository.
    pub repository_id: RepositoryId,
    /// Stable identity of the software system described.
    pub software_id: SoftwareId,
    /// Canonical object encoding.
    pub object_encoding: ObjectEncoding,
    /// Content hash algorithm.
    pub hash_algorithm: HashAlgorithm,
}

/// Supported canonical object encodings (closed protocol vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectEncoding {
    /// Deterministic CBOR (RFC 8949 core deterministic profile).
    CborDeterministicV1,
}

/// Supported content hash algorithms (closed protocol vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlgorithm {
    /// SHA-256.
    Sha256,
}

impl ObjectEncoding {
    /// The TOML string for this encoding.
    pub const fn as_str(self) -> &'static str {
        match self {
            ObjectEncoding::CborDeterministicV1 => "cbor-deterministic-v1",
        }
    }

    /// Parses a TOML string, rejecting unsupported values.
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "cbor-deterministic-v1" => Some(ObjectEncoding::CborDeterministicV1),
            _ => None,
        }
    }
}

impl HashAlgorithm {
    /// The TOML string for this algorithm.
    pub const fn as_str(self) -> &'static str {
        match self {
            HashAlgorithm::Sha256 => "sha256",
        }
    }

    /// Parses a TOML string, rejecting unsupported values.
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "sha256" => Some(HashAlgorithm::Sha256),
            _ => None,
        }
    }
}

/// Error produced while reading or writing repository metadata.
#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    /// An underlying filesystem failure.
    #[error("repository metadata I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The file is not valid TOML.
    #[error("repository metadata is not valid TOML: {0}")]
    Parse(#[from] toml::de::Error),
    /// The file is valid TOML but violates the metadata contract.
    #[error("invalid repository metadata: {0}")]
    Invalid(String),
}

impl RepositoryMetadata {
    /// Reads and validates metadata from a TOML file.
    pub fn read(path: &Path) -> Result<Self, MetadataError> {
        let text = fs::read_to_string(path)?;
        let table: toml::Table = toml::from_str(&text)?;
        Self::from_table(&table)
    }

    /// Writes this metadata as TOML.
    ///
    /// The emitted values are TOML-safe (hyphenated UUIDs and fixed
    /// vocabulary strings), so the output is formatted literally rather than
    /// routed through a serializer.
    pub fn write(&self, path: &Path) -> Result<(), MetadataError> {
        let text = format!(
            "format_version = {}\n\
             repository_id = \"{}\"\n\
             software_id = \"{}\"\n\
             object_encoding = \"{}\"\n\
             hash_algorithm = \"{}\"\n",
            self.format_version,
            self.repository_id,
            self.software_id,
            self.object_encoding.as_str(),
            self.hash_algorithm.as_str(),
        );
        fs::write(path, text)?;
        Ok(())
    }

    fn from_table(table: &toml::Table) -> Result<Self, MetadataError> {
        let format_version = integer_field(table, "format_version")?;
        if format_version != i64::from(SUPPORTED_FORMAT_VERSION) {
            return Err(MetadataError::Invalid(format!(
                "unsupported format_version: {format_version}"
            )));
        }

        Ok(Self {
            format_version: format_version as u32,
            repository_id: uuid_field(table, "repository_id")?,
            software_id: uuid_field(table, "software_id")?,
            object_encoding: Some(text_field(table, "object_encoding")?)
                .and_then(ObjectEncoding::from_str)
                .ok_or_else(|| MetadataError::Invalid("unsupported object_encoding".to_string()))?,
            hash_algorithm: Some(text_field(table, "hash_algorithm")?)
                .and_then(HashAlgorithm::from_str)
                .ok_or_else(|| MetadataError::Invalid("unsupported hash_algorithm".to_string()))?,
        })
    }
}

/// Reads an integer field, rejecting a missing or non-integer value.
fn integer_field(table: &toml::Table, key: &str) -> Result<i64, MetadataError> {
    table
        .get(key)
        .and_then(toml::Value::as_integer)
        .ok_or_else(|| MetadataError::Invalid(format!("{key} must be an integer")))
}

/// Reads a text field, rejecting a missing or non-string value.
fn text_field<'a>(table: &'a toml::Table, key: &str) -> Result<&'a str, MetadataError> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .ok_or_else(|| MetadataError::Invalid(format!("{key} must be a string")))
}

/// Reads a UUID field via the semantic ID's `FromStr`, rejecting malformed
/// UUIDs.
fn uuid_field<T: FromStr>(table: &toml::Table, key: &str) -> Result<T, MetadataError> {
    let s = text_field(table, key)?;
    s.parse()
        .map_err(|_| MetadataError::Invalid(format!("malformed {key}: {s}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    const UUID_A: &str = "00000000-0000-0000-0000-000000000001";
    const UUID_B: &str = "00000000-0000-0000-0000-000000000002";

    fn metadata() -> RepositoryMetadata {
        RepositoryMetadata {
            format_version: 1,
            repository_id: RepositoryId::from_uuid(Uuid::parse_str(UUID_A).unwrap()),
            software_id: SoftwareId::from_uuid(Uuid::parse_str(UUID_B).unwrap()),
            object_encoding: ObjectEncoding::CborDeterministicV1,
            hash_algorithm: HashAlgorithm::Sha256,
        }
    }

    fn write_metadata(
        path: &Path,
        format_version: &str,
        repository_id: &str,
        software_id: &str,
        object_encoding: &str,
        hash_algorithm: &str,
    ) {
        let text = format!(
            "format_version = {format_version}\n\
             repository_id = \"{repository_id}\"\n\
             software_id = \"{software_id}\"\n\
             object_encoding = \"{object_encoding}\"\n\
             hash_algorithm = \"{hash_algorithm}\"\n"
        );
        fs::write(path, text).unwrap();
    }

    #[test]
    fn metadata_write_read_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("repository.toml");
        let meta = metadata();
        meta.write(&path).unwrap();
        assert_eq!(RepositoryMetadata::read(&path).unwrap(), meta);
    }

    #[test]
    fn unsupported_format_version_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("repository.toml");
        write_metadata(
            &path,
            "2",
            UUID_A,
            UUID_B,
            "cbor-deterministic-v1",
            "sha256",
        );
        let err = RepositoryMetadata::read(&path).unwrap_err();
        assert!(matches!(err, MetadataError::Invalid(_)));
    }

    #[test]
    fn unsupported_object_encoding_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("repository.toml");
        write_metadata(&path, "1", UUID_A, UUID_B, "bogus", "sha256");
        let err = RepositoryMetadata::read(&path).unwrap_err();
        assert!(matches!(err, MetadataError::Invalid(_)));
    }

    #[test]
    fn unsupported_hash_algorithm_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("repository.toml");
        write_metadata(&path, "1", UUID_A, UUID_B, "cbor-deterministic-v1", "md5");
        let err = RepositoryMetadata::read(&path).unwrap_err();
        assert!(matches!(err, MetadataError::Invalid(_)));
    }

    #[test]
    fn malformed_uuid_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("repository.toml");
        write_metadata(
            &path,
            "1",
            "not-a-uuid",
            UUID_B,
            "cbor-deterministic-v1",
            "sha256",
        );
        let err = RepositoryMetadata::read(&path).unwrap_err();
        assert!(matches!(err, MetadataError::Invalid(_)));
    }
}

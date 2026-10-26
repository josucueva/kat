//! Repository initialization (`kat init`).
//!
//! The semantic initializer stores metadata, canonical objects, and the
//! accepted semantic reference. The CLI's full workspace bootstrap is in
//! [`crate::repository::migration`].
//!
//! The core ontology contents come from `docs/ontology.md` and the canonical
//! type identifiers in `docs/canonical-format.md` — not from implementation
//! convenience.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::domain::identity::{ObjectId, OntologyId, RepositoryId, SoftwareId};
use crate::domain::ontology::{ElementTypeDefinition, OntologyVersion, RelationshipTypeDefinition};
use crate::domain::state::SemanticState;
use crate::encoding::canonical_bytes;
use crate::encoding::object::{CanonicalObject, CanonicalPayload};
use crate::repository::error::RepositoryError;
use crate::repository::metadata::{
    HashAlgorithm, LEGACY_FORMAT_VERSION, ObjectEncoding, RepositoryMetadata,
    SUPPORTED_FORMAT_VERSION,
};
use crate::repository::object_store::ObjectStore;
use crate::repository::ref_store::{AcceptedRef, FileRefStore};

/// Process-global counter for unique temporary metadata file names.
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);
const BOOTSTRAP_MARKER_VERSION: u32 = 1;

/// Outcome of a successful `kat init`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitResult {
    /// Stable identity of the repository.
    pub repository_id: RepositoryId,
    /// Stable identity of the software system described.
    pub software_id: SoftwareId,
    /// ObjectId of the persisted core OntologyVersion O1.
    pub ontology: ObjectId,
    /// ObjectId of the persisted empty SemanticState S0.
    pub state: ObjectId,
}

/// Initializes a KAT repository at `path`, creating `.kat/`.
///
/// Fail-closed: refuses to run over an existing `.kat/` (which also covers an
/// existing `repository.toml` or `refs/accepted`). The `accepted` ref is the
/// publication point — the repository is initialized only once
/// `refs/accepted = { S0, none }` exists; immutable O1/S0 objects left behind
/// by a failed init are harmless, as are any created directories.
pub fn init_repository(path: &Path) -> Result<InitResult, RepositoryError> {
    let kat_dir = path.join(".kat");
    if kat_dir.exists() {
        return Err(RepositoryError::AlreadyExists(kat_dir));
    }
    initialize_semantic_repository(
        path,
        &BootstrapMarker {
            marker_version: BOOTSTRAP_MARKER_VERSION,
            repository_id: RepositoryId::new(),
            software_id: SoftwareId::new(),
            ontology_id: OntologyId::new(),
        },
        SUPPORTED_FORMAT_VERSION,
        false,
    )
}

/// Creates the semantic portion of a new CLI repository in the legacy
/// in-progress format. `kat init` upgrades the marker only after root
/// revision and workspace bootstrap have been verified.
pub(crate) fn init_repository_for_bootstrap(path: &Path) -> Result<InitResult, RepositoryError> {
    let marker_path = bootstrap_marker_path(path);
    let marker = if marker_path.exists() {
        read_bootstrap_marker(&marker_path)?
    } else {
        let kat_dir = path.join(".kat");
        if kat_dir.exists() {
            return Err(RepositoryError::AlreadyExists(kat_dir));
        }
        remove_stale_bootstrap_temps(path)?;
        if marker_path.exists() {
            return Err(RepositoryError::Io(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "bootstrap marker already exists",
            )));
        }
        let marker = BootstrapMarker {
            marker_version: BOOTSTRAP_MARKER_VERSION,
            repository_id: RepositoryId::new(),
            software_id: SoftwareId::new(),
            ontology_id: OntologyId::new(),
        };
        write_bootstrap_marker(&marker_path, &marker)?;
        marker
    };
    let initialized = resume_bootstrap(path, &marker)?;
    fs::remove_file(marker_path)?;
    Ok(initialized)
}

/// Resumes fresh initialization if its atomic marker survived an interruption.
pub(crate) fn resume_pending_bootstrap(path: &Path) -> Result<Option<InitResult>, RepositoryError> {
    let marker_path = bootstrap_marker_path(path);
    if !marker_path.exists() {
        return Ok(None);
    }
    let marker = read_bootstrap_marker(&marker_path)?;
    let initialized = resume_bootstrap(path, &marker)?;
    fs::remove_file(marker_path)?;
    Ok(Some(initialized))
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BootstrapMarker {
    marker_version: u32,
    repository_id: RepositoryId,
    software_id: SoftwareId,
    ontology_id: OntologyId,
}

fn bootstrap_marker_path(root: &Path) -> std::path::PathBuf {
    root.join(".kat-bootstrap.json")
}

fn remove_stale_bootstrap_temps(root: &Path) -> Result<(), RepositoryError> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(".kat-bootstrap.tmp-")
        {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn read_bootstrap_marker(path: &Path) -> Result<BootstrapMarker, RepositoryError> {
    let bytes = fs::read(path)?;
    let marker: BootstrapMarker = serde_json::from_slice(&bytes)
        .map_err(|error| RepositoryError::Io(std::io::Error::other(error)))?;
    if marker.marker_version != BOOTSTRAP_MARKER_VERSION {
        return Err(RepositoryError::Io(std::io::Error::other(
            "unsupported KAT bootstrap marker version",
        )));
    }
    Ok(marker)
}

fn write_bootstrap_marker(path: &Path, marker: &BootstrapMarker) -> Result<(), RepositoryError> {
    let bytes = serde_json::to_vec(marker)
        .map_err(|error| RepositoryError::Io(std::io::Error::other(error)))?;
    let tmp = path.with_file_name(format!(
        ".kat-bootstrap.tmp-{}-{}",
        std::process::id(),
        NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::hard_link(&tmp, path)?;
        fs::remove_file(&tmp)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&tmp);
        return Err(RepositoryError::Io(error));
    }
    Ok(())
}

fn resume_bootstrap(path: &Path, marker: &BootstrapMarker) -> Result<InitResult, RepositoryError> {
    initialize_semantic_repository(path, marker, LEGACY_FORMAT_VERSION, true)
}

fn initialize_semantic_repository(
    path: &Path,
    marker: &BootstrapMarker,
    format_version: u32,
    resume: bool,
) -> Result<InitResult, RepositoryError> {
    let kat_dir = path.join(".kat");
    for sub in ["objects", "refs", "locks", "tmp"] {
        fs::create_dir_all(kat_dir.join(sub))?;
    }

    let metadata_path = kat_dir.join("repository.toml");
    if metadata_path.exists() {
        if !resume {
            return Err(RepositoryError::AlreadyExists(kat_dir));
        }
        let metadata = RepositoryMetadata::read_for_migration(&metadata_path)?;
        if metadata.format_version != format_version
            || metadata.repository_id != marker.repository_id
            || metadata.software_id != marker.software_id
        {
            return Err(RepositoryError::Io(std::io::Error::other(
                "incomplete initialization metadata does not match its bootstrap marker",
            )));
        }
    } else {
        let metadata = RepositoryMetadata {
            format_version,
            repository_id: marker.repository_id,
            software_id: marker.software_id,
            object_encoding: ObjectEncoding::CborDeterministicV1,
            hash_algorithm: HashAlgorithm::Sha256,
        };
        metadata.write_atomic(&metadata_path)?;
    }

    let o1_bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::OntologyVersion(initial_core_ontology(marker.ontology_id)),
    })?;
    let store = ObjectStore::new(&kat_dir);
    let ontology = store.put(&o1_bytes)?;
    let s0_bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::SemanticState(SemanticState {
            ontology_version: ontology,
            elements: vec![],
            relationships: vec![],
        }),
    })?;
    let state = store.put(&s0_bytes)?;
    let expected = AcceptedRef {
        state,
        change: None,
    };
    let refs = FileRefStore::new(&kat_dir);
    match refs.read_accepted() {
        Ok(actual) if actual == expected => {}
        Ok(_) if resume => {
            return Err(RepositoryError::Io(std::io::Error::other(
                "incomplete initialization has an unexpected accepted ref",
            )));
        }
        Ok(_) => {
            return Err(RepositoryError::Io(std::io::Error::other(
                "repository accepted ref already exists",
            )));
        }
        Err(crate::repository::ref_store::RefStoreError::NotFound) => {
            match refs.init_accepted(&expected) {
                Ok(()) => {}
                Err(crate::repository::ref_store::RefStoreError::Conflict)
                    if resume && refs.read_accepted()? == expected => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(error) => return Err(error.into()),
    }

    Ok(InitResult {
        repository_id: marker.repository_id,
        software_id: marker.software_id,
        ontology,
        state,
    })
}

/// The initial core ontology, defined by `docs/ontology.md` with the canonical
/// type identifiers from `docs/canonical-format.md`.
pub fn initial_core_ontology(id: OntologyId) -> OntologyVersion {
    OntologyVersion {
        ontology_id: id,
        element_types: vec![
            element_type("kat.core/artifact", "Artifact"),
            element_type("kat.core/constraint", "Constraint"),
            element_type("kat.core/design-decision", "Design Decision"),
            element_type("kat.core/implementation", "Implementation"),
            element_type("kat.core/intent", "Intent"),
            element_type("kat.core/requirement", "Requirement"),
            element_type("kat.core/validation", "Validation"),
        ],
        relationship_types: vec![
            relationship_type(
                "kat.core/addresses",
                "Addresses",
                &["kat.core/design-decision"],
                &["kat.core/requirement"],
            ),
            relationship_type(
                "kat.core/depends-on",
                "Depends On",
                &["kat.core/implementation"],
                &["kat.core/implementation"],
            ),
            relationship_type(
                "kat.core/derived-from",
                "Derived From",
                &["kat.core/artifact"],
                &[
                    "kat.core/constraint",
                    "kat.core/design-decision",
                    "kat.core/implementation",
                    "kat.core/requirement",
                ],
            ),
            relationship_type(
                "kat.core/guides",
                "Guides",
                &["kat.core/design-decision"],
                &["kat.core/implementation"],
            ),
            relationship_type(
                "kat.core/motivates",
                "Motivates",
                &["kat.core/intent"],
                &["kat.core/design-decision", "kat.core/requirement"],
            ),
            relationship_type(
                "kat.core/realizes",
                "Realizes",
                &["kat.core/implementation"],
                &["kat.core/requirement"],
            ),
            relationship_type(
                "kat.core/represents",
                "Represents",
                &["kat.core/artifact"],
                &["kat.core/implementation"],
            ),
            relationship_type(
                "kat.core/restricts",
                "Restricts",
                &["kat.core/constraint"],
                &[
                    "kat.core/design-decision",
                    "kat.core/implementation",
                    "kat.core/requirement",
                ],
            ),
            relationship_type(
                "kat.core/supersedes",
                "Supersedes",
                &["kat.core/design-decision"],
                &["kat.core/design-decision"],
            ),
            relationship_type(
                "kat.core/validates",
                "Validates",
                &["kat.core/validation"],
                &[
                    "kat.core/constraint",
                    "kat.core/implementation",
                    "kat.core/requirement",
                ],
            ),
        ],
    }
}

fn element_type(type_id: &str, name: &str) -> ElementTypeDefinition {
    ElementTypeDefinition {
        type_id: type_id.to_string(),
        name: name.to_string(),
    }
}

fn relationship_type(
    type_id: &str,
    name: &str,
    allowed_source_types: &[&str],
    allowed_target_types: &[&str],
) -> RelationshipTypeDefinition {
    RelationshipTypeDefinition {
        type_id: type_id.to_string(),
        name: name.to_string(),
        allowed_source_types: allowed_source_types.iter().map(|s| s.to_string()).collect(),
        allowed_target_types: allowed_target_types.iter().map(|s| s.to_string()).collect(),
    }
}

#[cfg(test)]
mod bootstrap_tests {
    use super::*;

    #[test]
    fn pending_marker_recovers_semantic_initialization_before_metadata_exists() {
        let dir = tempfile::tempdir().unwrap();
        let marker = BootstrapMarker {
            marker_version: BOOTSTRAP_MARKER_VERSION,
            repository_id: RepositoryId::new(),
            software_id: SoftwareId::new(),
            ontology_id: OntologyId::new(),
        };
        write_bootstrap_marker(&bootstrap_marker_path(dir.path()), &marker).unwrap();

        let initialized = resume_pending_bootstrap(dir.path()).unwrap().unwrap();
        assert_eq!(initialized.repository_id, marker.repository_id);
        assert!(dir.path().join(".kat/refs/accepted").is_file());
        assert!(!bootstrap_marker_path(dir.path()).exists());
        assert_eq!(
            RepositoryMetadata::read_for_migration(&dir.path().join(".kat/repository.toml"))
                .unwrap()
                .format_version,
            LEGACY_FORMAT_VERSION
        );
    }

    #[test]
    fn existing_marker_path_is_not_replaced_or_removed() {
        let dir = tempfile::tempdir().unwrap();
        let path = bootstrap_marker_path(dir.path());
        let contents = b"user data";
        fs::write(&path, contents).unwrap();

        assert!(init_repository_for_bootstrap(dir.path()).is_err());
        assert_eq!(fs::read(path).unwrap(), contents);
        assert!(!dir.path().join(".kat").exists());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::validate::CanonicalValidate;
    use uuid::Uuid;

    #[test]
    fn core_ontology_matches_spec() {
        let o1 = initial_core_ontology(OntologyId::from_uuid(Uuid::nil()));

        let element_ids: Vec<&str> = o1
            .element_types
            .iter()
            .map(|t| t.type_id.as_str())
            .collect();
        assert_eq!(
            element_ids,
            [
                "kat.core/artifact",
                "kat.core/constraint",
                "kat.core/design-decision",
                "kat.core/implementation",
                "kat.core/intent",
                "kat.core/requirement",
                "kat.core/validation",
            ]
        );

        let relationship_ids: Vec<&str> = o1
            .relationship_types
            .iter()
            .map(|t| t.type_id.as_str())
            .collect();
        assert_eq!(
            relationship_ids,
            [
                "kat.core/addresses",
                "kat.core/depends-on",
                "kat.core/derived-from",
                "kat.core/guides",
                "kat.core/motivates",
                "kat.core/realizes",
                "kat.core/represents",
                "kat.core/restricts",
                "kat.core/supersedes",
                "kat.core/validates",
            ]
        );

        // motivates: Intent -> {Requirement, Design Decision}
        let motivates = &o1.relationship_types[4];
        assert_eq!(
            motivates.allowed_source_types.as_slice(),
            &["kat.core/intent"][..]
        );
        assert_eq!(
            motivates.allowed_target_types.as_slice(),
            &["kat.core/design-decision", "kat.core/requirement"][..]
        );

        // derived-from: Artifact -> {Requirement, Constraint, Design Decision, Implementation}
        let derived_from = &o1.relationship_types[2];
        assert_eq!(
            derived_from.allowed_source_types.as_slice(),
            &["kat.core/artifact"][..]
        );
        assert_eq!(
            derived_from.allowed_target_types.as_slice(),
            &[
                "kat.core/constraint",
                "kat.core/design-decision",
                "kat.core/implementation",
                "kat.core/requirement",
            ][..]
        );

        // The ontology is structurally canonical (sorted, unique).
        o1.validate_canonical_structure().unwrap();
    }
}

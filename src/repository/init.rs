//! Repository initialization (`kat init`).
//!
//! The CLI is thin: it parses arguments and calls [`init_repository`], which
//! orchestrates metadata, the encoder, the ObjectStore, and the RefStore.
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
    HashAlgorithm, ObjectEncoding, RepositoryMetadata, SUPPORTED_FORMAT_VERSION,
};
use crate::repository::object_store::ObjectStore;
use crate::repository::ref_store::{AcceptedRef, FileRefStore};

/// Process-global counter for unique temporary metadata file names.
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

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

    let repository_id = RepositoryId::new();
    let software_id = SoftwareId::new();
    let ontology_id = OntologyId::new();

    // Create the canonical layout.
    for sub in ["objects", "refs", "locks", "tmp"] {
        fs::create_dir_all(kat_dir.join(sub))?;
    }

    // Write repository.toml atomically (fail-closed against partial writes).
    let metadata = RepositoryMetadata {
        format_version: SUPPORTED_FORMAT_VERSION,
        repository_id,
        software_id,
        object_encoding: ObjectEncoding::CborDeterministicV1,
        hash_algorithm: HashAlgorithm::Sha256,
    };
    write_metadata_atomic(&metadata, &kat_dir)?;

    // Persist the core ontology O1.
    let o1 = initial_core_ontology(ontology_id);
    let o1_bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::OntologyVersion(o1),
    })?;
    let store = ObjectStore::new(&kat_dir);
    let ontology = store.put(&o1_bytes)?;

    // Persist the empty SemanticState S0 referencing O1.
    let s0 = SemanticState {
        ontology_version: ontology,
        elements: vec![],
        relationships: vec![],
    };
    let s0_bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::SemanticState(s0),
    })?;
    let state = store.put(&s0_bytes)?;

    // Publish accepted = { S0, none }.
    let refs = FileRefStore::new(&kat_dir);
    refs.init_accepted(&AcceptedRef {
        state,
        change: None,
    })?;

    Ok(InitResult {
        repository_id,
        software_id,
        ontology,
        state,
    })
}

/// Writes `repository.toml` to a temporary file under `tmp/`, flushes it, then
/// atomically renames it into place.
fn write_metadata_atomic(
    metadata: &RepositoryMetadata,
    kat_dir: &Path,
) -> Result<(), RepositoryError> {
    let tmp = kat_dir.join("tmp").join(format!(
        "repository.toml-{}-{}",
        std::process::id(),
        NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(metadata.to_toml_string().as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&tmp, kat_dir.join("repository.toml"))?;
    Ok(())
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

//! Integration tests for `kat init` (step 0.9): end-to-end initialization,
//! layout, metadata, O1/S0 persistence, and the accepted ref.
//!
//! The stored O1/S0 are not semantically decoded (no decoder until 0.10); the
//! test retains/reconstructs the logical objects, encodes them, and asserts
//! their ObjectIds and stored bytes, then verifies metadata and refs
//! independently.

use std::fs;
use std::path::Path;

use kat::domain::state::SemanticState;
use kat::encoding::canonical_bytes;
use kat::encoding::object::{CanonicalObject, CanonicalPayload};
use kat::encoding::object_id;
use kat::repository::error::RepositoryError;
use kat::repository::init::init_repository;
use kat::repository::metadata::RepositoryMetadata;
use kat::repository::object_store::ObjectStore;
use kat::repository::ref_store::{AcceptedRef, FileRefStore, RefStore};

fn kat_dir(root: &Path) -> std::path::PathBuf {
    root.join(".kat")
}

#[test]
fn init_creates_layout_metadata_objects_and_accepted_ref() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let result = init_repository(root).unwrap();

    // Canonical layout.
    for sub in ["objects", "refs", "locks", "tmp"] {
        assert!(kat_dir(root).join(sub).is_dir(), "missing .kat/{sub}");
    }

    // repository.toml exists and parses with the generated identities.
    let metadata = RepositoryMetadata::read(&kat_dir(root).join("repository.toml")).unwrap();
    assert_eq!(metadata.repository_id, result.repository_id);
    assert_eq!(metadata.software_id, result.software_id);

    // O1 and S0 are persisted and their stored bytes hash to their ObjectIds.
    let store = ObjectStore::new(kat_dir(root));
    let o1_bytes = store.get(result.ontology).unwrap();
    let s0_bytes = store.get(result.state).unwrap();
    assert_eq!(object_id(&o1_bytes), result.ontology);
    assert_eq!(object_id(&s0_bytes), result.state);

    // S0 is the empty state referencing O1 (reconstructed logically).
    let s0 = SemanticState {
        ontology_version: result.ontology,
        elements: vec![],
        relationships: vec![],
    };
    let expected_s0 = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::SemanticState(s0),
    })
    .unwrap();
    assert_eq!(s0_bytes, expected_s0, "S0 must be empty and reference O1");

    // accepted = { S0, none }.
    let refs = FileRefStore::new(kat_dir(root));
    assert_eq!(
        refs.read_accepted().unwrap(),
        AcceptedRef {
            state: result.state,
            change: None,
        }
    );

    // Reconstructed low-level stores remain readable.
    assert_eq!(
        RepositoryMetadata::read(&kat_dir(root).join("repository.toml")).unwrap(),
        metadata
    );
    assert_eq!(store.get(result.state).unwrap(), s0_bytes);
    assert_eq!(refs.read_accepted().unwrap().state, result.state);
}

#[test]
fn init_twice_is_rejected_and_leaves_first_repository_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let first = init_repository(root).unwrap();

    let err = init_repository(root).unwrap_err();
    assert!(matches!(err, RepositoryError::AlreadyExists(_)));

    // The first repository is unchanged.
    let refs = FileRefStore::new(kat_dir(root));
    assert_eq!(refs.read_accepted().unwrap().state, first.state);
}

#[test]
fn init_inside_project_with_unrelated_files_leaves_them_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("src")).unwrap();
    let cargo_toml = b"[package]\nname = \"app\"\n";
    let main_rs = b"fn main() {}\n";
    fs::write(root.join("Cargo.toml"), cargo_toml).unwrap();
    fs::write(root.join("src").join("main.rs"), main_rs).unwrap();

    let _ = init_repository(root).unwrap();

    assert_eq!(fs::read(root.join("Cargo.toml")).unwrap(), cargo_toml);
    assert_eq!(fs::read(root.join("src").join("main.rs")).unwrap(), main_rs);
}

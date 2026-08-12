//! Integration tests for the Change Engine `prepare_change` (step 1.1).
//!
//! Step 1.1 produces only a [`ChangeContext`]: it resolves the accepted head
//! and loads the base SemanticState + its OntologyVersion. It performs **no**
//! mutation, persistence, or publication — these tests prove that invariant:
//! the object store and `refs/accepted` are unchanged after a prepare.
//!
//! Because `prepare_change` operates on an already-`open_repository`-validated
//! [`Repository`], integrity failures (missing / wrong-kind referenced
//! objects) are rejected by the `open` layer first. That is the intended
//! layered boundary: repository integrity is guaranteed before the engine can
//! run, and the engine's own kind checks are defense-in-depth. The last test
//! pins this boundary.

use std::fs;
use std::path::{Path, PathBuf};

use kat::repository::change::prepare_change;
use kat::repository::init::init_repository;
use kat::repository::open::open_repository;
use kat::repository::ref_store::AcceptedRef;

fn kat_dir(root: &Path) -> PathBuf {
    root.join(".kat")
}

fn object_ids(root: &Path) -> Vec<String> {
    let mut ids: Vec<String> = fs::read_dir(kat_dir(root).join("objects"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    ids.sort();
    ids
}

#[test]
fn prepare_change_resolves_loads_and_does_not_publish() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let init = init_repository(root).unwrap();
    let refs_before = fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap();
    let objects_before = object_ids(root);

    let repo = open_repository(root).unwrap();
    let context = prepare_change(&repo).unwrap();

    // Accepted head resolved exactly as initialized.
    assert_eq!(
        context.accepted,
        AcceptedRef {
            state: init.state,
            change: None,
        }
    );

    // Base SemanticState loaded: S0, empty, referencing O1.
    assert_eq!(context.base_state_id, init.state);
    assert_eq!(context.base_state.ontology_version, init.ontology);
    assert!(context.base_state.elements.is_empty());
    assert!(context.base_state.relationships.is_empty());

    // Base OntologyVersion loaded: the spec-derived core ontology.
    assert_eq!(context.ontology.element_types.len(), 7);
    assert_eq!(context.ontology.relationship_types.len(), 10);
    assert_eq!(
        context.ontology.element_types[5].type_id,
        "kat.core/requirement"
    );

    // No mutation: object store and accepted ref are byte-for-byte unchanged.
    assert_eq!(object_ids(root), objects_before);
    assert_eq!(
        fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap(),
        refs_before
    );
    // Still exactly the two immutable objects from init (O1, S0).
    assert_eq!(objects_before.len(), 2);
}

#[test]
fn prepared_base_matches_plan_acceptance() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    // Reopen like a fresh process; the context matches the plan's S0 base
    // with no accepted change head.
    let repo = open_repository(root).unwrap();
    let context = prepare_change(&repo).unwrap();
    let head = &context.accepted;
    assert_eq!(head.change, None, "fresh repository has no change head");
    assert_eq!(context.base_state_id, head.state, "base == accepted state");
}

#[test]
fn integrity_failures_rejected_at_open_boundary_before_prepare() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let init = init_repository(root).unwrap();

    // Removing the accepted SemanticState object makes the repository
    // integrity-invalid; the open layer rejects it, so the engine is never
    // reached on a broken repository.
    fs::remove_file(kat_dir(root).join("objects").join(init.state.to_string())).unwrap();

    let err = open_repository(root).unwrap_err();
    assert!(matches!(
        err,
        kat::repository::error::RepositoryError::ObjectStore(
            kat::repository::object_store::ObjectStoreError::NotFound(_)
        )
    ));

    // (prepare_change could not be invoked: there is no valid Repository to
    // pass to it, which is exactly the layered guarantee.)
}

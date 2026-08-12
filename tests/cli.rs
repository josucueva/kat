//! End-to-end CLI tests: spawn the real `kat` binary (via `CARGO_BIN_EXE_kat`,
//! no extra dependencies) against a temp repository.
//!
//! The CLI is thin parse + dispatch, so these tests assert the invocation
//! contract from `docs/cli.md` without re-testing library semantics.

use std::path::Path;
use std::process::Command;

use kat::domain::identity::{ChangeId, ElementId};
use kat::domain::property::PropertyValue;
use kat::repository::change::{
    CreateElementInput, apply_create_element, persist_prepared_change, prepare_change,
    prepare_change_revision, publish_persisted_change, validate_create_element_invariants,
    validate_create_element_ontology,
};
use kat::repository::init::init_repository;
use kat::repository::open::open_repository;
use uuid::Uuid;

fn run_kat(dir: &Path, args: &[&str]) -> (String, String, bool) {
    let output = Command::new(env!("CARGO_BIN_EXE_kat"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("kat binary runs");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.success(),
    )
}

/// The identities of a change published through the library.
struct Published {
    element_id: ElementId,
    version_id: String,
    state_id: String,
    change_revision_id: String,
}

/// Publishes one element through the library (the `kat create` CLI is wired
/// later, at Phase 1 closure), returning its identities.
fn publish_element(root: &Path, element_n: u128, change_n: u128) -> Published {
    let repo = open_repository(root).unwrap();
    let element_id = ElementId::from_uuid(Uuid::from_u128(element_n));
    let context = prepare_change(&repo).unwrap();
    let prepared = apply_create_element(
        context,
        CreateElementInput {
            element_id,
            type_id: "kat.core/requirement".into(),
            properties: vec![("title".into(), PropertyValue::Text("A requirement".into()))],
        },
    )
    .unwrap();
    let validated =
        validate_create_element_invariants(validate_create_element_ontology(prepared).unwrap())
            .unwrap();
    let revision = prepare_change_revision(
        validated,
        ChangeId::from_uuid(Uuid::from_u128(change_n)),
        None,
    )
    .unwrap();
    let version_id = revision.creation.element_version_id.to_string();
    let state_id = revision.state_id.to_string();
    let change_revision_id = revision.change_revision_id.to_string();
    let persisted = persist_prepared_change(&repo, revision).unwrap();
    publish_persisted_change(&repo, persisted).unwrap();
    Published {
        element_id,
        version_id,
        state_id,
        change_revision_id,
    }
}

#[test]
fn kat_show_prints_resolved_element() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // `kat init` from the CLI, like a real user.
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    // Publish a change through the library.
    let published = publish_element(root, 81, 181);

    // `kat show <element-id>` resolves and prints the accepted version.
    let (out, err, ok) = run_kat(root, &["show", &published.element_id.to_string()]);
    assert!(ok, "kat show failed: {err}");
    let expected = format!(
        "element_id: {}\n\
         version_id: {}\n\
         type: kat.core/requirement\n\
         lifecycle: active\n\
         title: A requirement\n",
        published.element_id, published.version_id
    );
    assert_eq!(out, expected);

    // An unknown element fails cleanly with a friendly message.
    let missing = ElementId::from_uuid(Uuid::from_u128(999));
    let (out, err, ok) = run_kat(root, &["show", &missing.to_string()]);
    assert!(!ok);
    assert!(out.is_empty());
    assert!(err.contains("not found"));
}

#[test]
fn kat_show_without_a_repository_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let element_id = ElementId::from_uuid(Uuid::from_u128(82));

    let (out, err, ok) = run_kat(root, &["show", &element_id.to_string()]);
    assert!(!ok);
    assert!(out.is_empty());
    assert!(err.contains("no KAT repository found"));
}

#[test]
fn kat_show_rejects_invalid_element_id() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let (out, err, ok) = run_kat(root, &["show", "not-a-uuid"]);
    assert!(!ok);
    assert!(out.is_empty());
    assert!(err.contains("invalid element ID"));
}

#[test]
fn kat_history_empty_on_fresh_repository() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    let (out, err, ok) = run_kat(root, &["history"]);
    assert!(ok, "kat history failed: {err}");
    assert!(out.is_empty(), "fresh repository has no history: {out}");
}

#[test]
fn kat_history_prints_accepted_chain() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    // The base state the change was prepared against.
    let s0 = open_repository(root).unwrap().accepted.state;

    let published = publish_element(root, 82, 182);

    let (out, err, ok) = run_kat(root, &["history"]);
    assert!(ok, "kat history failed: {err}");
    let change_id = ChangeId::from_uuid(Uuid::from_u128(182));
    let expected = format!(
        "revision_id: {}\nchange_id: {change_id}\nresult_state: {}\nbase_states:\n  {}\ndependencies:\n  none\noperations:\n  create_element {}\ndescription: none\n",
        published.change_revision_id, published.state_id, s0, published.version_id
    );
    assert_eq!(out, expected);
}

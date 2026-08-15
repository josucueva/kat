//! Unit tests for type-scoped unique-prefix ID resolution (Step 11.3).

use std::path::Path;

use kat::domain::identity::{ChangeId, ElementId, RelationshipId};
use kat::domain::property::PropertyValue;
use kat::repository::change::{
    CreateElementInput, LinkElementInput, apply_create_element, apply_link_element,
    persist_prepared_change, persist_prepared_link_change, prepare_change, prepare_change_revision,
    prepare_link_change_revision, publish_persisted_change, publish_persisted_link_change,
    validate_create_element_invariants, validate_create_element_ontology,
    validate_link_element_invariants, validate_link_element_ontology,
};
use kat::repository::init::init_repository;
use kat::repository::open::open_repository;
use kat::repository::resolve::{ResolveError, resolve_element_id, resolve_relationship_id};
use uuid::Uuid;

fn publish_test_element(root: &Path, uuid: Uuid, type_id: &str, title: &str) -> ElementId {
    let repo = open_repository(root).unwrap();
    let element_id = ElementId::from_uuid(uuid);
    let context = prepare_change(&repo).unwrap();
    let prepared = apply_create_element(
        context,
        CreateElementInput {
            element_id,
            type_id: type_id.to_string(),
            properties: vec![("title".into(), PropertyValue::Text(title.into()))],
        },
    )
    .unwrap();
    let validated =
        validate_create_element_invariants(validate_create_element_ontology(prepared).unwrap())
            .unwrap();
    let revision =
        prepare_change_revision(validated, ChangeId::from_uuid(Uuid::new_v4()), None).unwrap();
    let persisted = persist_prepared_change(&repo, revision).unwrap();
    publish_persisted_change(&repo, persisted).unwrap();
    element_id
}

fn publish_test_link(
    root: &Path,
    rel_uuid: Uuid,
    rel_type: &str,
    source: ElementId,
    target: ElementId,
) -> RelationshipId {
    let repo = open_repository(root).unwrap();
    let relationship_id = RelationshipId::from_uuid(rel_uuid);
    let context = prepare_change(&repo).unwrap();
    let prepared = apply_link_element(
        &repo,
        context,
        LinkElementInput {
            relationship_id,
            relationship_type_id: rel_type.to_string(),
            source_element_id: source,
            target_element_id: target,
            properties: vec![],
        },
    )
    .unwrap();
    let validated =
        validate_link_element_invariants(validate_link_element_ontology(prepared).unwrap())
            .unwrap();
    let revision =
        prepare_link_change_revision(validated, ChangeId::from_uuid(Uuid::new_v4()), None).unwrap();
    let persisted = persist_prepared_link_change(&repo, revision).unwrap();
    publish_persisted_link_change(&repo, persisted).unwrap();
    relationship_id
}

#[test]
fn resolve_full_element_id_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let uuid = Uuid::parse_str("7af83d1c-4102-4ee8-9ad0-90e21df3e027").unwrap();
    let id = publish_test_element(root, uuid, "kat.core/requirement", "Auth Req");

    let repo = open_repository(root).unwrap();
    let resolved = resolve_element_id(&repo, "7af83d1c-4102-4ee8-9ad0-90e21df3e027").unwrap();
    assert_eq!(resolved, id);
}

#[test]
fn resolve_unique_element_prefix_8_digits() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let uuid = Uuid::parse_str("7af83d1c-4102-4ee8-9ad0-90e21df3e027").unwrap();
    let id = publish_test_element(root, uuid, "kat.core/requirement", "Auth Req");

    let repo = open_repository(root).unwrap();
    let resolved = resolve_element_id(&repo, "7af83d1c").unwrap();
    assert_eq!(resolved, id);

    let resolved_with_hyphen = resolve_element_id(&repo, "7af83d1c-4102").unwrap();
    assert_eq!(resolved_with_hyphen, id);
}

#[test]
fn resolve_element_prefix_too_short_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let uuid = Uuid::parse_str("7af83d1c-4102-4ee8-9ad0-90e21df3e027").unwrap();
    publish_test_element(root, uuid, "kat.core/requirement", "Auth Req");

    let repo = open_repository(root).unwrap();
    let err = resolve_element_id(&repo, "7af83d1").unwrap_err();
    assert!(matches!(err, ResolveError::PrefixTooShort { .. }));
}

#[test]
fn resolve_element_prefix_not_found_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let uuid = Uuid::parse_str("7af83d1c-4102-4ee8-9ad0-90e21df3e027").unwrap();
    publish_test_element(root, uuid, "kat.core/requirement", "Auth Req");

    let repo = open_repository(root).unwrap();
    let err = resolve_element_id(&repo, "00000000").unwrap_err();
    assert!(matches!(err, ResolveError::NotFound { .. }));
}

#[test]
fn resolve_ambiguous_element_prefix_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let uuid1 = Uuid::parse_str("7af83d1c-4102-4ee8-9ad0-90e21df3e027").unwrap();
    let uuid2 = Uuid::parse_str("7af83d1c-9999-4ee8-9ad0-90e21df3e027").unwrap();
    publish_test_element(root, uuid1, "kat.core/requirement", "Auth Req 1");
    publish_test_element(root, uuid2, "kat.core/requirement", "Auth Req 2");

    let repo = open_repository(root).unwrap();
    let err = resolve_element_id(&repo, "7af83d1c").unwrap_err();
    if let ResolveError::Ambiguous { candidates, .. } = err {
        assert_eq!(candidates.len(), 2);
    } else {
        panic!("expected Ambiguous error, got {:?}", err);
    }
}

#[test]
fn resolve_relationship_id_and_domain_isolation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let e1 = publish_test_element(
        root,
        Uuid::parse_str("10000000-0000-0000-0000-000000000001").unwrap(),
        "kat.core/design-decision",
        "Dec",
    );
    let e2 = publish_test_element(
        root,
        Uuid::parse_str("20000000-0000-0000-0000-000000000002").unwrap(),
        "kat.core/requirement",
        "Req",
    );

    let rel_id = publish_test_link(
        root,
        Uuid::parse_str("7af83d1c-8888-4ee8-9ad0-90e21df3e027").unwrap(),
        "kat.core/addresses",
        e1,
        e2,
    );

    let repo = open_repository(root).unwrap();
    let resolved_rel = resolve_relationship_id(&repo, "7af83d1c").unwrap();
    assert_eq!(resolved_rel, rel_id);

    // Relationship resolution ignores ElementIds (and vice-versa)
    let elem_err = resolve_element_id(&repo, "7af83d1c").unwrap_err();
    assert!(matches!(elem_err, ResolveError::NotFound { .. }));
}

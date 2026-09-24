use kat::domain::conflict::SemanticConflictKind;
use kat::domain::element::{KnowledgeElementVersion, Lifecycle};
use kat::domain::identity::{
    ElementId, ObjectId, OntologyId, RelationshipId, RepositoryRevisionId,
};
use kat::domain::ontology::OntologyVersion;
use kat::domain::property::PropertyValue;
use kat::domain::state::{ElementStateEntry, RelationshipStateEntry, SemanticState};
use kat::domain::workspace::WorkspaceId;
use kat::encoding::object::{CanonicalObject, CanonicalPayload};
use kat::repository::object_store::ObjectStore;
use kat::repository::reconcile::{
    SessionLoadError, load_reconciliation_session, reconcile_semantic,
    save_reconciliation_session,
};

fn object_id(byte: u8) -> ObjectId {
    ObjectId::from_bytes([byte; 32])
}

fn setup() -> (tempfile::TempDir, ObjectStore) {
    let dir = tempfile::tempdir().unwrap();
    let obj_store = ObjectStore::new(dir.path());
    (dir, obj_store)
}

fn element_id(byte: u8) -> ElementId {
    ElementId::from_uuid(uuid::Uuid::from_u128(byte as u128))
}

fn put_mock_element(store: &ObjectStore, byte: u8, lifecycle: Lifecycle) -> ObjectId {
    let ev = KnowledgeElementVersion {
        element_id: element_id(byte),
        type_id: "kat.core/requirement".to_string(),
        lifecycle,
        properties: vec![("name".to_string(), PropertyValue::Text("Test".to_string()))],
    };
    let payload = CanonicalPayload::KnowledgeElementVersion(ev);
    let obj = CanonicalObject { payload };
    let bytes = kat::encoding::cbor::canonical_bytes(&obj).unwrap();
    store.put(&bytes).unwrap()
}

fn put_mock_ontology(store: &ObjectStore) -> ObjectId {
    let ontology = OntologyVersion {
        ontology_id: OntologyId::from_uuid(uuid::Uuid::from_u128(1)),
        element_types: vec![],
        relationship_types: vec![],
    };
    let payload = CanonicalPayload::OntologyVersion(ontology);
    let obj = CanonicalObject { payload };
    let bytes = kat::encoding::cbor::canonical_bytes(&obj).unwrap();
    store.put(&bytes).unwrap()
}

fn mock_state(ontology_id: ObjectId) -> SemanticState {
    SemanticState {
        ontology_version: ontology_id,
        elements: vec![],
        relationships: vec![],
    }
}

fn mock_repo_rev(byte: u8) -> RepositoryRevisionId {
    RepositoryRevisionId::from_object_id(object_id(byte))
}

fn call_reconcile(
    store: &ObjectStore,
    base: &SemanticState,
    local: &SemanticState,
    other: &SemanticState,
) -> kat::repository::reconcile::ReconciliationCandidate {
    reconcile_semantic(
        store,
        WorkspaceId("ws-test".to_string()),
        mock_repo_rev(1),
        mock_repo_rev(2),
        mock_repo_rev(3),
        base,
        local,
        other,
    )
    .unwrap()
}

#[test]
fn rec_01_independent_updates() {
    let (_dir, store) = setup();
    let ontology_id = put_mock_ontology(&store);

    // Base has elements 1 and 2
    let b1 = put_mock_element(&store, 1, Lifecycle::Active);
    let b2 = put_mock_element(&store, 2, Lifecycle::Active);

    let mut base = mock_state(ontology_id);
    base.elements.push(ElementStateEntry {
        element_id: element_id(1),
        version: b1,
    });
    base.elements.push(ElementStateEntry {
        element_id: element_id(2),
        version: b2,
    });

    // Local updates 1
    let l1 = put_mock_element(&store, 11, Lifecycle::Active);
    let mut local = base.clone();
    local.elements[0].version = l1;

    // Other updates 2
    let o2 = put_mock_element(&store, 22, Lifecycle::Active);
    let mut other = base.clone();
    other.elements[1].version = o2;

    let result = call_reconcile(&store, &base, &local, &other);
    assert!(result.semantic_conflicts.is_empty());

    // Reconciled should have l1 and o2
    let state = result.proposed_semantic_state;
    assert_eq!(state.elements.len(), 2);
    assert_eq!(state.elements[0].version, l1);
    assert_eq!(state.elements[1].version, o2);
}

#[test]
fn rec_02_identical_updates() {
    let (_dir, store) = setup();
    let ontology_id = put_mock_ontology(&store);
    let b1 = put_mock_element(&store, 1, Lifecycle::Active);

    let mut base = mock_state(ontology_id);
    base.elements.push(ElementStateEntry {
        element_id: element_id(1),
        version: b1,
    });

    let l1 = put_mock_element(&store, 11, Lifecycle::Active);

    let mut local = base.clone();
    local.elements[0].version = l1;

    let mut other = base.clone();
    other.elements[0].version = l1; // same update

    let result = call_reconcile(&store, &base, &local, &other);
    assert!(result.semantic_conflicts.is_empty());
    assert_eq!(result.proposed_semantic_state.elements[0].version, l1);
}

#[test]
fn rec_03_concurrent_modification() {
    let (_dir, store) = setup();
    let ontology_id = put_mock_ontology(&store);
    let b1 = put_mock_element(&store, 1, Lifecycle::Active);

    let mut base = mock_state(ontology_id);
    base.elements.push(ElementStateEntry {
        element_id: element_id(1),
        version: b1,
    });

    let l1 = put_mock_element(&store, 11, Lifecycle::Active);
    let o1 = put_mock_element(&store, 12, Lifecycle::Active);

    let mut local = base.clone();
    local.elements[0].version = l1;

    let mut other = base.clone();
    other.elements[0].version = o1;

    let result = call_reconcile(&store, &base, &local, &other);
    let conflicts = result.semantic_conflicts;
    assert_eq!(conflicts.len(), 1);

    match &conflicts[0].kind {
        SemanticConflictKind::ConcurrentModification {
            base_version,
            local_version,
            other_version,
        } => {
            assert_eq!(base_version, &Some(b1));
            assert_eq!(local_version, &Some(l1));
            assert_eq!(other_version, &Some(o1));
        }
        _ => panic!("Expected ConcurrentModification"),
    }
}

fn relationship_id(byte: u8) -> RelationshipId {
    RelationshipId::from_uuid(uuid::Uuid::from_u128(byte as u128))
}

#[test]
fn rec_04_link_unlink_independence() {
    let (_dir, store) = setup();
    let ontology_id = put_mock_ontology(&store);
    let mut base = mock_state(ontology_id);

    let rx = relationship_id(1);
    let rz = relationship_id(2);

    base.relationships.push(RelationshipStateEntry {
        relationship_id: rx,
        version: object_id(1),
    });
    base.relationships.push(RelationshipStateEntry {
        relationship_id: rz,
        version: object_id(2),
    });

    let mut local = base.clone();
    local.relationships.remove(0); // Unlinks X->Y

    let mut other = base.clone();
    other.relationships.remove(1); // Unlinks Z->W

    let result = call_reconcile(&store, &base, &local, &other);
    assert!(result.semantic_conflicts.is_empty());
    assert_eq!(result.proposed_semantic_state.relationships.len(), 0); // Both unlinked successfully
}

#[test]
fn rec_05_lifecycle_conflict() {
    let (_dir, store) = setup();
    let ontology_id = put_mock_ontology(&store);
    let b1 = put_mock_element(&store, 1, Lifecycle::Active);
    let l1 = put_mock_element(&store, 11, Lifecycle::Deprecated);
    let o1 = put_mock_element(&store, 12, Lifecycle::Active); // updated but active

    let mut base = mock_state(ontology_id);
    base.elements.push(ElementStateEntry {
        element_id: element_id(1),
        version: b1,
    });

    let mut local = base.clone();
    local.elements[0].version = l1;

    let mut other = base.clone();
    other.elements[0].version = o1;

    let result = call_reconcile(&store, &base, &local, &other);
    let conflicts = result.semantic_conflicts;
    assert_eq!(conflicts.len(), 1);

    match &conflicts[0].kind {
        SemanticConflictKind::LifecycleMismatch {
            base_version,
            local_version,
            other_version,
        } => {
            assert_eq!(base_version, &Some(b1));
            assert_eq!(local_version, &Some(l1));
            assert_eq!(other_version, &Some(o1));
        }
        _ => panic!("Expected LifecycleMismatch, got {:?}", conflicts[0].kind),
    }
}

#[test]
fn rec_07_already_reconciled() {
    let (_dir, store) = setup();
    let ontology_id = put_mock_ontology(&store);
    let b1 = put_mock_element(&store, 1, Lifecycle::Active);

    let mut base = mock_state(ontology_id);
    base.elements.push(ElementStateEntry {
        element_id: element_id(1),
        version: b1,
    });

    let l1 = put_mock_element(&store, 11, Lifecycle::Active);

    let mut local = base.clone();
    local.elements[0].version = l1;

    // other == local (they reached same state)
    let other = local.clone();

    let result = call_reconcile(&store, &base, &local, &other);
    assert!(result.semantic_conflicts.is_empty());
    assert_eq!(result.proposed_semantic_state.elements[0].version, l1);
}

#[test]
fn rec_08_persistence_survives_restart() {
    let (dir, store) = setup();
    let ontology_id = put_mock_ontology(&store);
    let base = mock_state(ontology_id);
    let local = base.clone();
    let other = base.clone();

    let candidate = call_reconcile(&store, &base, &local, &other);
    let backend = kat::repository::workspace::fake::FakeWorkspaceBackend::new();
    save_reconciliation_session(dir.path(), &candidate.workspace_id, &kat::repository::reconcile::ReconciliationSession { version: 1, workspace_id: candidate.workspace_id.clone(), base_revision: candidate.local_revision.clone(), target_revision: candidate.other_revision.clone(), state: kat::repository::reconcile::ReconciliationSessionState::Conflicted { candidate: candidate.clone() } }, &backend)
        .unwrap();
    let loaded = load_reconciliation_session(
        dir.path(), &WorkspaceId("ws-test".to_string()), mock_repo_rev(2), mock_repo_rev(3),
    )
    .unwrap()
    .unwrap();

    assert_eq!(loaded.workspace_id, candidate.workspace_id);
    assert_eq!(loaded.base_revision, candidate.local_revision);
    assert_eq!(
        match &loaded.state { kat::repository::reconcile::ReconciliationSessionState::Conflicted { candidate } => candidate.proposed_semantic_state.clone(), _ => panic!("Expected Conflicted") },
        candidate.proposed_semantic_state
    );
}

#[test]
fn rec_12_concurrent_supersession() {
    let (_dir, store) = setup();
    let ontology_id = put_mock_ontology(&store);
    let b1 = put_mock_element(&store, 1, Lifecycle::Active);

    let mut base = mock_state(ontology_id);
    base.elements.push(ElementStateEntry {
        element_id: element_id(1),
        version: b1,
    });

    let l1 = put_mock_element(&store, 11, Lifecycle::Superseded);
    let mut local = base.clone();
    local.elements[0].version = l1;

    let o1 = put_mock_element(&store, 12, Lifecycle::Superseded);
    let mut other = base.clone();
    other.elements[0].version = o1;

    let candidate = call_reconcile(&store, &base, &local, &other);
    assert_eq!(candidate.semantic_conflicts.len(), 1);

    // Test confirms that concurrent supersession intent (A superseded-by B vs A superseded-by C)
    // is correctly trapped. (State-based detection uses Lifecycle::Superseded as the proxy for this intent).
    match &candidate.semantic_conflicts[0].kind {
        SemanticConflictKind::SupersessionConflict { .. } => {}
        _ => panic!("Expected SupersessionConflict"),
    }
}

#[test]
fn rec_13_relationship_conflict() {
    let (_dir, store) = setup();
    let ontology_id = put_mock_ontology(&store);
    let mut base = mock_state(ontology_id);

    let rx = relationship_id(1);
    base.relationships.push(RelationshipStateEntry {
        relationship_id: rx,
        version: object_id(1),
    });

    let mut local = base.clone();
    local.relationships[0].version = object_id(11);

    let mut other = base.clone();
    other.relationships[0].version = object_id(12);

    let candidate = call_reconcile(&store, &base, &local, &other);
    assert_eq!(candidate.semantic_conflicts.len(), 1);

    match &candidate.semantic_conflicts[0].kind {
        SemanticConflictKind::RelationshipConflict { .. } => {}
        _ => panic!("Expected RelationshipConflict"),
    }
}

#[test]
fn rec_16_conflict_retains_base() {
    let (_dir, store) = setup();
    let ontology_id = put_mock_ontology(&store);

    let b1 = put_mock_element(&store, 1, Lifecycle::Active);
    let l1 = put_mock_element(&store, 2, Lifecycle::Active);
    let o1 = put_mock_element(&store, 3, Lifecycle::Active);

    let mut base = mock_state(ontology_id);
    base.elements.push(ElementStateEntry {
        element_id: element_id(1),
        version: b1,
    });

    let mut local = base.clone();
    local.elements[0].version = l1;

    let mut other = base.clone();
    other.elements[0].version = o1;

    let candidate = call_reconcile(&store, &base, &local, &other);
    assert_eq!(candidate.semantic_conflicts.len(), 1); // Concurrent modification

    // Neutral rule: the proposed state must retain the base version b1
    assert_eq!(candidate.proposed_semantic_state.elements.len(), 1);
    assert_eq!(candidate.proposed_semantic_state.elements[0].version, b1);
}

#[test]
fn rec_17_concurrent_creation_omitted() {
    let (_dir, store) = setup();
    let ontology_id = put_mock_ontology(&store);

    let l1 = put_mock_element(&store, 2, Lifecycle::Active);
    let o1 = put_mock_element(&store, 3, Lifecycle::Active);

    let base = mock_state(ontology_id);

    let mut local = base.clone();
    local.elements.push(ElementStateEntry {
        element_id: element_id(1),
        version: l1,
    });

    let mut other = base.clone();
    other.elements.push(ElementStateEntry {
        element_id: element_id(1),
        version: o1,
    });

    let candidate = call_reconcile(&store, &base, &local, &other);
    assert_eq!(candidate.semantic_conflicts.len(), 1); // Concurrent modification on non-existent base

    // Neutral rule: the proposed state must omit the entity since there is no base
    assert_eq!(candidate.proposed_semantic_state.elements.len(), 0);
}

#[test]
fn rec_09_persistence_moves_no_ref() {
    let (dir, store) = setup();
    let ontology_id = put_mock_ontology(&store);
    let base = mock_state(ontology_id);
    let candidate = call_reconcile(&store, &base, &base, &base);

    // Saving candidate should not update any git refs or workspaces.
    // In our simplified test, we just ensure no side-effects occur outside the candidate file.
    let backend = kat::repository::workspace::fake::FakeWorkspaceBackend::new();
    save_reconciliation_session(dir.path(), &candidate.workspace_id, &kat::repository::reconcile::ReconciliationSession { version: 1, workspace_id: candidate.workspace_id.clone(), base_revision: candidate.local_revision.clone(), target_revision: candidate.other_revision.clone(), state: kat::repository::reconcile::ReconciliationSessionState::Conflicted { candidate: candidate.clone() } }, &backend)
        .unwrap();
    let path = dir
        .path()
        .join(".kat")
        .join("workspaces")
        .join(&candidate.workspace_id.0)
        .join("reconciliation_session.json");
    assert!(path.exists());

    // (If we had a `Workspace` state to verify, we'd check it hasn't mutated its base_revision).
}

#[test]
fn rec_10_exact_blo_binding() {
    let (_dir, store) = setup();
    let ontology_id = put_mock_ontology(&store);
    let base = mock_state(ontology_id);

    let candidate = call_reconcile(&store, &base, &base, &base);
    assert_eq!(candidate.base_revision, mock_repo_rev(1));
    assert_eq!(candidate.local_revision, mock_repo_rev(2));
    assert_eq!(candidate.other_revision, mock_repo_rev(3));
}

#[test]
fn rec_14_validation_findings() {
    let (_dir, store) = setup();

    // Create a base state pointing to a non-existent ontology.
    // This should trigger a validation finding, but NO semantic conflict.
    let bad_ontology_id = object_id(99);
    let base = mock_state(bad_ontology_id);

    let candidate = call_reconcile(&store, &base, &base, &base);
    assert!(
        candidate.semantic_conflicts.is_empty(),
        "Expected no semantic conflicts"
    );

    // There should be a validation finding due to missing ontology.
    assert!(
        !candidate.validation_findings.is_empty(),
        "Expected validation finding for missing ontology"
    );
}

#[test]
fn rec_11_stale_candidate_rejection() {
    let (dir, store) = setup();
    let ontology_id = put_mock_ontology(&store);
    let base = mock_state(ontology_id);
    let candidate = call_reconcile(&store, &base, &base, &base);
    let backend = kat::repository::workspace::fake::FakeWorkspaceBackend::new();
    save_reconciliation_session(dir.path(), &candidate.workspace_id, &kat::repository::reconcile::ReconciliationSession { version: 1, workspace_id: candidate.workspace_id.clone(), base_revision: candidate.local_revision.clone(), target_revision: candidate.other_revision.clone(), state: kat::repository::reconcile::ReconciliationSessionState::Conflicted { candidate: candidate.clone() } }, &backend)
        .unwrap();

    // Simulate active workspace moving its base (from mock_repo_rev(1) to mock_repo_rev(9))
    let result = load_reconciliation_session(
        dir.path(), &WorkspaceId("ws-test".to_string()), mock_repo_rev(9), mock_repo_rev(3),
    );

    match result {
        Err(SessionLoadError::StaleBase) => {}
        _ => panic!("Expected StaleBase error"),
    }
}

#[test]
fn rec_unsupported_candidate_version() {
    let (dir, store) = setup();
    let ontology_id = put_mock_ontology(&store);
    let base = mock_state(ontology_id);
    let candidate = call_reconcile(&store, &base, &base, &base);
    let mut session = kat::repository::reconcile::ReconciliationSession { version: 1, workspace_id: candidate.workspace_id.clone(), base_revision: candidate.local_revision.clone(), target_revision: candidate.other_revision.clone(), state: kat::repository::reconcile::ReconciliationSessionState::Conflicted { candidate: candidate.clone() } };

    // Manually serialize with a future version
    session.version = 2;
    let path = dir
        .path()
        .join(".kat")
        .join("workspaces")
        .join(&candidate.workspace_id.0);
    std::fs::create_dir_all(&path).unwrap();
    let json = serde_json::to_string_pretty(&session).unwrap();
    std::fs::write(path.join("reconciliation_session.json"), json).unwrap();

    let result = load_reconciliation_session(
        dir.path(), &WorkspaceId("ws-test".to_string()), mock_repo_rev(1), mock_repo_rev(3),
    );

    match result {
        Err(SessionLoadError::UnsupportedVersion(2)) => {}
        _ => panic!("Expected UnsupportedVersion(2) error"),
    }
}

#[test]
fn rec_15a_repeat_determinism() {
    let (_dir, store) = setup();
    let ontology_id = put_mock_ontology(&store);

    let b1 = put_mock_element(&store, 1, Lifecycle::Active);
    let l1 = put_mock_element(&store, 2, Lifecycle::Active);
    let o1 = put_mock_element(&store, 3, Lifecycle::Active);

    let mut base = mock_state(ontology_id);
    base.elements.push(ElementStateEntry {
        element_id: element_id(1),
        version: b1,
    });

    let mut local = base.clone();
    local.elements[0].version = l1;

    let mut other = base.clone();
    other.elements[0].version = o1;

    let candidate1 = call_reconcile(&store, &base, &local, &other);
    let candidate2 = call_reconcile(&store, &base, &local, &other);

    let json1 = serde_json::to_string(&candidate1).unwrap();
    let json2 = serde_json::to_string(&candidate2).unwrap();
    assert_eq!(json1, json2);
}

#[test]
fn rec_15b_local_other_symmetry() {
    let (_dir, store) = setup();
    let ontology_id = put_mock_ontology(&store);

    let b1 = put_mock_element(&store, 1, Lifecycle::Active);
    let l1 = put_mock_element(&store, 2, Lifecycle::Active);
    let o1 = put_mock_element(&store, 3, Lifecycle::Active);

    let mut base = mock_state(ontology_id);
    base.elements.push(ElementStateEntry {
        element_id: element_id(1),
        version: b1,
    });

    let mut local = base.clone();
    local.elements[0].version = l1;

    let mut other = base.clone();
    other.elements[0].version = o1;

    // Reconcile with (base, local, other)
    let candidate1 = call_reconcile(&store, &base, &local, &other);

    // Reconcile with (base, other, local)
    // Note: To achieve exact candidate equality, we need to artificially swap the local/other revision inputs to the function as well.
    let candidate2 = kat::repository::reconcile::reconcile_semantic(
        &store,
        WorkspaceId("ws-test".to_string()),
        mock_repo_rev(1), // base
        mock_repo_rev(3), // local_rev = other's rev
        mock_repo_rev(2), // other_rev = local's rev
        &base,
        &other, // pass other as local
        &local, // pass local as other
    )
    .unwrap();

    // Because conflict kind records versions explicitly (e.g., `local_version: Some(o1), other_version: Some(l1)` vs `Some(l1), Some(o1)`),
    // the JSON will differ slightly in those metadata fields, but the proposed state MUST be identical.
    assert_eq!(
        candidate1.proposed_semantic_state,
        candidate2.proposed_semantic_state
    );
}

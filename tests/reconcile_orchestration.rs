use kat::domain::element::{KnowledgeElementVersion, Lifecycle};
use kat::domain::identity::{
    ElementId, ObjectId, OntologyId, RepositoryRevisionId, WorkspaceSnapshotId,
};
use kat::domain::ontology::OntologyVersion;
use kat::domain::state::{ElementStateEntry, SemanticState};
use kat::domain::workspace::{WorkspaceBackend, WorkspaceId};
use kat::encoding::decode::decode_canonical;
use kat::encoding::object::{CanonicalObject, CanonicalPayload};
use kat::repository::object_store::ObjectStore;
use kat::repository::reconcile::{ReconciliationSession, ReconciliationSessionState, reconcile};
use kat::repository::workspace::fake::FakeWorkspaceBackend;
use std::path::PathBuf;

fn setup() -> (
    ObjectStore,
    FakeWorkspaceBackend,
    tempfile::TempDir,
    ObjectId,
) {
    let dir = tempfile::tempdir().unwrap();
    let store = ObjectStore::new(dir.path());
    let backend = FakeWorkspaceBackend::with_root(dir.path());

    let ontology = OntologyVersion {
        ontology_id: OntologyId::new(),
        element_types: vec![],
        relationship_types: vec![],
    };
    let obj = CanonicalObject {
        payload: CanonicalPayload::OntologyVersion(ontology),
    };
    let bytes = kat::encoding::cbor::canonical_bytes(&obj).unwrap();
    let ontology_obj_id = store.put(&bytes).unwrap();

    (store, backend, dir, ontology_obj_id)
}

fn make_rev(i: u8) -> RepositoryRevisionId {
    let hash = [i; 32];
    RepositoryRevisionId::from_object_id(ObjectId::from_bytes(hash))
}

fn make_sem(ont_id: ObjectId) -> SemanticState {
    SemanticState {
        ontology_version: ont_id,
        elements: vec![],
        relationships: vec![],
    }
}

fn put_element(store: &ObjectStore, element_id: ElementId, prop_val: &str) -> ObjectId {
    let ev = KnowledgeElementVersion {
        element_id,
        type_id: "kat.core/requirement".to_string(),
        lifecycle: Lifecycle::Active,
        properties: vec![(
            "test_prop".to_string(),
            kat::domain::property::PropertyValue::Text(prop_val.to_string()),
        )],
    };
    let obj = CanonicalObject {
        payload: CanonicalPayload::KnowledgeElementVersion(ev),
    };
    let bytes = kat::encoding::cbor::canonical_bytes(&obj).unwrap();
    store.put(&bytes).unwrap()
}

fn make_snap(backend: &FakeWorkspaceBackend, files: &[(&str, &str)]) -> WorkspaceSnapshotId {
    let mut t = backend.working_tree.write().unwrap();
    t.clear();
    let mut paths = Vec::new();
    for (p, content) in files {
        paths.push(PathBuf::from(p));
        t.insert(
            PathBuf::from(p),
            kat::repository::workspace::fake::FakeEntry::File {
                content: content.as_bytes().to_vec(),
                executable: false,
            },
        );
    }
    drop(t);
    backend.create_snapshot(&paths).unwrap()
}

#[test]
fn rec_p01_clean_semantic_clean_physical() {
    let (store, backend, _dir, ont_id) = setup();

    let base = make_sem(ont_id);
    let local = make_sem(ont_id);
    let other = make_sem(ont_id);

    let snap_b = make_snap(&backend, &[("a.txt", "A")]);
    let snap_l = make_snap(&backend, &[("a.txt", "A"), ("b.txt", "B")]);
    let snap_o = make_snap(&backend, &[("a.txt", "A"), ("c.txt", "C")]);

    let res = reconcile(
        &store,
        &backend,
        WorkspaceId("w1".to_string()),
        make_rev(1),
        make_rev(2),
        make_rev(3),
        &snap_b,
        &snap_l,
        &snap_o,
        &base,
        &local,
        &other,
    )
    .unwrap();

    match res {
        ReconciliationSession { state: ReconciliationSessionState::PreparedClean { revision, .. }, .. } => {
            let rev_obj_bytes = store.get(revision.as_object_id()).unwrap();
            let canonical_obj = decode_canonical(&rev_obj_bytes).unwrap();
            let rev = match canonical_obj.payload {
                CanonicalPayload::RepositoryRevision(r) => r,
                _ => panic!(
                    "Expected Revision payload, got {:#?}",
                    canonical_obj.payload
                ),
            };
            assert_eq!(rev.parents.len(), 2);
            assert!(rev.semantic_change.is_none());
        }
        ReconciliationSession { state: ReconciliationSessionState::Conflicted { candidate: c }, .. } => panic!(
            "Expected clean, got conflicted. Sem: {:?}, Val: {:?}, Phys: {:?}",
            c.semantic_conflicts, c.validation_findings, c.materialization_conflicts
        ),
    }
}

#[test]
fn rec_p02_conflicted_semantic_clean_physical() {
    let (store, backend, _dir, ont_id) = setup();

    let snap_b = make_snap(&backend, &[("a.txt", "A")]);
    let snap_l = make_snap(&backend, &[("a.txt", "A"), ("b.txt", "B")]);
    let snap_o = make_snap(&backend, &[("a.txt", "A"), ("c.txt", "C")]);

    let base = make_sem(ont_id);
    let mut local = make_sem(ont_id);
    let shared_id = ElementId::new();
    let v_local = put_element(&store, shared_id, "local");
    local.elements.push(ElementStateEntry {
        element_id: shared_id,
        version: v_local,
    });
    let mut other = make_sem(ont_id);
    let v_other = put_element(&store, shared_id, "other");
    other.elements.push(ElementStateEntry {
        element_id: shared_id,
        version: v_other,
    });

    let res = reconcile(
        &store,
        &backend,
        WorkspaceId("w1".to_string()),
        make_rev(1),
        make_rev(2),
        make_rev(3),
        &snap_b,
        &snap_l,
        &snap_o,
        &base,
        &local,
        &other,
    )
    .unwrap();

    match res {
        ReconciliationSession { state: ReconciliationSessionState::Conflicted { candidate: candidate }, .. } => {
            assert!(
                !candidate.semantic_conflicts.is_empty(),
                "Expected semantic conflicts"
            );
            assert!(
                candidate.physical_candidate.is_none(),
                "Physical candidate should be None for clean physical"
            );
            assert!(
                candidate.materialization_conflicts.is_empty(),
                "No materialization conflicts expected"
            );
        }
        ReconciliationSession { state: ReconciliationSessionState::PreparedClean { .. }, .. } => panic!("Expected conflicted, got clean"),
    }
}

#[test]
fn rec_p03_clean_semantic_conflicted_physical() {
    let (store, backend, _dir, ont_id) = setup();

    let snap_b = make_snap(&backend, &[("a.txt", "A")]);
    let snap_l = make_snap(&backend, &[("a.txt", "A_L")]);
    let snap_o = make_snap(&backend, &[("a.txt", "A_O")]);

    let base = make_sem(ont_id);

    let res = reconcile(
        &store,
        &backend,
        WorkspaceId("w1".to_string()),
        make_rev(1),
        make_rev(2),
        make_rev(3),
        &snap_b,
        &snap_l,
        &snap_o,
        &base,
        &base,
        &base,
    )
    .unwrap();

    match res {
        ReconciliationSession { state: ReconciliationSessionState::Conflicted { candidate: candidate }, .. } => {
            assert!(
                candidate.semantic_conflicts.is_empty(),
                "No semantic conflicts expected"
            );
            assert!(
                candidate.physical_candidate.is_some(),
                "Physical candidate should be populated"
            );
            assert_eq!(candidate.materialization_conflicts.len(), 1);
        }
        ReconciliationSession { state: ReconciliationSessionState::PreparedClean { .. }, .. } => panic!("Expected conflicted, got clean"),
    }
}

#[test]
fn rec_p04_conflicted_semantic_conflicted_physical() {
    let (store, backend, _dir, ont_id) = setup();

    let snap_b = make_snap(&backend, &[("a.txt", "A")]);
    let snap_l = make_snap(&backend, &[("a.txt", "A_L")]);
    let snap_o = make_snap(&backend, &[("a.txt", "A_O")]);

    let base = make_sem(ont_id);
    let mut local = make_sem(ont_id);
    let shared_id = ElementId::new();
    let v_local = put_element(&store, shared_id, "local");
    local.elements.push(ElementStateEntry {
        element_id: shared_id,
        version: v_local,
    });
    let mut other = make_sem(ont_id);
    let v_other = put_element(&store, shared_id, "other");
    other.elements.push(ElementStateEntry {
        element_id: shared_id,
        version: v_other,
    });

    let res = reconcile(
        &store,
        &backend,
        WorkspaceId("w1".to_string()),
        make_rev(1),
        make_rev(2),
        make_rev(3),
        &snap_b,
        &snap_l,
        &snap_o,
        &base,
        &local,
        &other,
    )
    .unwrap();

    match res {
        ReconciliationSession { state: ReconciliationSessionState::Conflicted { candidate: candidate }, .. } => {
            assert!(
                !candidate.semantic_conflicts.is_empty(),
                "Expected semantic conflicts"
            );
            assert!(
                candidate.physical_candidate.is_some(),
                "Physical candidate should be populated"
            );
            assert_eq!(candidate.materialization_conflicts.len(), 1);
        }
        ReconciliationSession { state: ReconciliationSessionState::PreparedClean { .. }, .. } => panic!("Expected conflicted, got clean"),
    }
}

#[test]
fn rec_p05_validation_failure() {
    let (store, backend, _dir, _ont_id) = setup();

    let snap_b = make_snap(&backend, &[("a.txt", "A")]);
    let snap_l = make_snap(&backend, &[("a.txt", "A")]);
    let snap_o = make_snap(&backend, &[("a.txt", "A")]);

    // Use invalid ontology version [0; 32] which doesn't exist in store
    let base = SemanticState {
        ontology_version: ObjectId::from_bytes([0; 32]),
        elements: vec![],
        relationships: vec![],
    };

    let res = reconcile(
        &store,
        &backend,
        WorkspaceId("w1".to_string()),
        make_rev(1),
        make_rev(2),
        make_rev(3),
        &snap_b,
        &snap_l,
        &snap_o,
        &base,
        &base,
        &base,
    )
    .unwrap();

    match res {
        ReconciliationSession { state: ReconciliationSessionState::Conflicted { candidate: c }, .. } => {
            assert!(
                !c.validation_findings.is_empty(),
                "Expected validation finding"
            );
            assert!(c.semantic_conflicts.is_empty(), "No semantic conflicts");
            assert!(
                c.materialization_conflicts.is_empty(),
                "No physical conflicts"
            );
        }
        _ => panic!("Expected conflicted"),
    }
}

#[test]
fn rec_p07_canonical_parent_sorting() {
    let (store, backend, _dir, ont_id) = setup();

    let snap_b = make_snap(&backend, &[("a.txt", "A")]);
    let snap_l = make_snap(&backend, &[("a.txt", "A_L")]);
    let snap_o = make_snap(&backend, &[("a.txt", "A_O")]);

    let base = make_sem(ont_id);

    let res = reconcile(
        &store,
        &backend,
        WorkspaceId("w1".to_string()),
        make_rev(1), // base
        make_rev(2), // local
        make_rev(3), // other
        &snap_b,
        &snap_l,
        &snap_o,
        &base,
        &base,
        &base,
    )
    .unwrap();

    match res {
        ReconciliationSession { state: ReconciliationSessionState::Conflicted { candidate }, .. } => {
            // REC-P07: The candidate JSON should have the exact order
            assert_eq!(candidate.base_revision, make_rev(1));
            assert_eq!(candidate.local_revision, make_rev(2));
            assert_eq!(candidate.other_revision, make_rev(3));

            // Check that physical candidate retained the exact order of snapshots
            let phys = candidate.physical_candidate.unwrap();
            assert_eq!(phys.base, snap_b);
            assert_eq!(phys.local, snap_l);
            assert_eq!(phys.other, snap_o);
        }
        _ => panic!("Expected Conflicted merge"),
    }
}

#[test]
fn rec_p08_unified_restart_durability() {
    use kat::repository::reconcile::{
        load_reconciliation_session, save_reconciliation_session,
    };
    use kat::repository::workspace::git::GitWorkspaceBackend;
    use std::fs;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let store = ObjectStore::new(&root);
    let ontology_id = store
        .put(
            &kat::encoding::cbor::canonical_bytes(&CanonicalObject {
                payload: CanonicalPayload::OntologyVersion(OntologyVersion {
                    ontology_id: OntologyId::new(),
                    element_types: vec![],
                    relationship_types: vec![],
                }),
            })
            .unwrap(),
        )
        .unwrap();

    GitWorkspaceBackend::init(&root).unwrap();
    let git = GitWorkspaceBackend::open(&root).unwrap();
    let wid = WorkspaceId("test_ws".to_string());

    let create_git_snap = |file: &str, content: &str| {
        fs::write(root.join(file), content).unwrap();
        git.create_snapshot(&[PathBuf::from(file)]).unwrap()
    };

    let snap_b = create_git_snap("a.txt", "A");
    let snap_l = create_git_snap("a.txt", "A_L");
    let snap_o = create_git_snap("a.txt", "A_O");
    let base = make_sem(ontology_id);

    let res = reconcile(
        &store,
        &git,
        wid.clone(),
        make_rev(1),
        make_rev(2),
        make_rev(3),
        &snap_b,
        &snap_l,
        &snap_o,
        &base,
        &base,
        &base,
    )
    .unwrap();

    let cand = match res {
        ReconciliationSession { state: ReconciliationSessionState::Conflicted { candidate: c }, .. } => c,
        _ => panic!("Expected Conflicted merge"),
    };

    save_reconciliation_session(&root, &wid, &kat::repository::reconcile::ReconciliationSession { version: 1, workspace_id: cand.workspace_id.clone(), base_revision: cand.local_revision.clone(), target_revision: cand.other_revision.clone(), state: kat::repository::reconcile::ReconciliationSessionState::Conflicted { candidate: cand.clone() } }, &git).unwrap();
    drop(git);

    let git2 = GitWorkspaceBackend::open(&root).unwrap();
    let loaded = load_reconciliation_session(&root, &wid, make_rev(2), make_rev(3))
        .unwrap()
        .unwrap();

    assert!(match &loaded.state { kat::repository::reconcile::ReconciliationSessionState::Conflicted { candidate } => candidate.physical_candidate.clone(), _ => None }.is_some());
    let loaded_phys = match &loaded.state { kat::repository::reconcile::ReconciliationSessionState::Conflicted { candidate } => candidate.physical_candidate.clone(), _ => None }.unwrap();
    assert_eq!(
        loaded_phys.provisional,
        cand.physical_candidate.as_ref().unwrap().provisional
    );
    assert_eq!(
        loaded_phys.conflicts[0].kind,
        cand.physical_candidate.as_ref().unwrap().conflicts[0].kind
    );

    let loaded_git_state = git2
        .get_candidate_state(&wid, &loaded_phys.provisional)
        .unwrap();
    assert_eq!(
        loaded_git_state.base_snapshot,
        cand.physical_candidate.as_ref().unwrap().base.to_hex()
    );
    assert_eq!(
        loaded_git_state.local_snapshot,
        cand.physical_candidate.as_ref().unwrap().local.to_hex()
    );
    assert_eq!(
        loaded_git_state.other_snapshot,
        cand.physical_candidate.as_ref().unwrap().other.to_hex()
    );
}

#[test]
fn rec_p06_clean_preparation_non_mutation() {
    use kat::repository::workspace::git::GitWorkspaceBackend;
    use std::fs;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let store = ObjectStore::new(&root);
    let ontology_id = store
        .put(
            &kat::encoding::cbor::canonical_bytes(&CanonicalObject {
                payload: CanonicalPayload::OntologyVersion(OntologyVersion {
                    ontology_id: OntologyId::new(),
                    element_types: vec![],
                    relationship_types: vec![],
                }),
            })
            .unwrap(),
        )
        .unwrap();

    GitWorkspaceBackend::init(&root).unwrap();
    let git = GitWorkspaceBackend::open(&root).unwrap();
    let wid = WorkspaceId("test_ws".to_string());

    let create_git_snap = |file: &str, content: &str| {
        fs::write(root.join(file), content).unwrap();
        git.create_snapshot(&[PathBuf::from(file)]).unwrap()
    };

    let snap_b = create_git_snap("a.txt", "A");
    let snap_l = create_git_snap("a.txt", "A");
    let snap_o = create_git_snap("a.txt", "A");
    let base = make_sem(ontology_id);

    let kat_git_dir = root.join(".kat/physical/git");
    let repo = git2::Repository::open(&kat_git_dir).unwrap();
    let head_before = repo.head().unwrap().target().unwrap();

    // Simulate active_lineage
    let lineage_path = root
        .join(".kat/workspaces")
        .join(&wid.0)
        .join("active_lineage");
    fs::create_dir_all(lineage_path.parent().unwrap()).unwrap();
    fs::write(&lineage_path, head_before.to_string()).unwrap();

    let res = reconcile(
        &store,
        &git,
        wid.clone(),
        make_rev(1),
        make_rev(2),
        make_rev(3),
        &snap_b,
        &snap_l,
        &snap_o,
        &base,
        &base,
        &base,
    )
    .unwrap();

    match res {
        ReconciliationSession { state: ReconciliationSessionState::PreparedClean { revision: _ }, .. } => {
            // Verify HEAD unchanged
            let head_after = repo.head().unwrap().target().unwrap();
            assert_eq!(
                head_before, head_after,
                "Git HEAD should remain unchanged during clean merge preparation"
            );

            // Verify active_lineage unchanged
            let lineage_after = fs::read_to_string(&lineage_path).unwrap();
            assert_eq!(
                lineage_after,
                head_before.to_string(),
                "active_lineage should remain unchanged"
            );
        }
        _ => panic!("Expected Clean merge"),
    }
}

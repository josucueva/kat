use kat::domain::identity::ObjectId;
use kat::domain::identity::RepositoryRevisionId;
use kat::domain::identity::SemanticStateId;
use kat::domain::identity::WorkspaceSnapshotId;
use kat::domain::revision::RepositoryRevision;
use kat::encoding::cbor::canonical_bytes;
use kat::encoding::object::CanonicalObject;
use kat::encoding::object::CanonicalPayload;
use kat::repository::object_store::ObjectStore;
use kat::repository::topology::{
    DivergenceState, compare_ancestry, find_best_common_ancestors, find_graph_heads,
};

fn object_id(byte: u8) -> ObjectId {
    ObjectId::from_bytes([byte; 32])
}

fn put_mock_revision(
    obj_store: &ObjectStore,
    byte: u8,
    mut parents: Vec<RepositoryRevisionId>,
) -> RepositoryRevisionId {
    parents.sort_by_key(|id| id.to_string());
    
    let rev = RepositoryRevision {
        parents,
        semantic_state: SemanticStateId::from_object_id(object_id(byte)),
        workspace_snapshot: WorkspaceSnapshotId::new(vec![byte; 32]),
        semantic_change: None,
    };

    let obj = CanonicalObject {
        payload: CanonicalPayload::RepositoryRevision(rev),
    };

    let bytes = canonical_bytes(&obj).unwrap();
    RepositoryRevisionId::from_object_id(obj_store.put(&bytes).unwrap())
}

fn setup() -> (tempfile::TempDir, ObjectStore) {
    let dir = tempfile::tempdir().unwrap();
    let obj_store = ObjectStore::new(dir.path());
    (dir, obj_store)
}

#[test]
fn top_01_linear_ancestry() {
    let (_dir, store) = setup();
    let base = put_mock_revision(&store, 1, vec![]);
    let a = put_mock_revision(&store, 2, vec![base]);
    let b = put_mock_revision(&store, 3, vec![a]);

    assert_eq!(
        compare_ancestry(&store, b, a).unwrap(),
        DivergenceState::LocalAhead
    );
    assert_eq!(
        compare_ancestry(&store, a, b).unwrap(),
        DivergenceState::OtherAhead
    );
}

#[test]
fn top_02_same_revision() {
    let (_dir, store) = setup();
    let base = put_mock_revision(&store, 1, vec![]);

    assert_eq!(
        compare_ancestry(&store, base, base).unwrap(),
        DivergenceState::Same
    );
}

#[test]
fn top_03_simple_divergence() {
    let (_dir, store) = setup();
    let base = put_mock_revision(&store, 1, vec![]);
    let a = put_mock_revision(&store, 2, vec![base]);
    let b = put_mock_revision(&store, 3, vec![base]);

    assert_eq!(
        compare_ancestry(&store, a, b).unwrap(),
        DivergenceState::Diverged { common_base: base }
    );
}

#[test]
fn top_04_deep_divergence() {
    let (_dir, store) = setup();
    let base = put_mock_revision(&store, 1, vec![]);
    let a1 = put_mock_revision(&store, 2, vec![base]);
    let a2 = put_mock_revision(&store, 3, vec![a1]);
    let b1 = put_mock_revision(&store, 4, vec![base]);

    assert_eq!(
        compare_ancestry(&store, a2, b1).unwrap(),
        DivergenceState::Diverged { common_base: base }
    );
}

#[test]
fn top_05_criss_cross_merge_ambiguous_merge_base() {
    let (_dir, store) = setup();
    let base = put_mock_revision(&store, 1, vec![]);
    let a = put_mock_revision(&store, 2, vec![base]);
    let b = put_mock_revision(&store, 3, vec![base]);

    // Criss-cross merges
    let m1 = put_mock_revision(&store, 4, vec![a, b]);
    let m2 = put_mock_revision(&store, 5, vec![b, a]);

    let a2 = put_mock_revision(&store, 6, vec![m1]);
    let b2 = put_mock_revision(&store, 7, vec![m2]);

    let state = compare_ancestry(&store, a2, b2).unwrap();
    match state {
        DivergenceState::AmbiguousMergeBase { mut bases } => {
            let mut expected = vec![a, b];
            expected.sort_by_key(|id| id.to_string());
            bases.sort_by_key(|id| id.to_string());
            assert_eq!(bases, expected);
        }
        _ => panic!("Expected AmbiguousMergeBase, got {:?}", state),
    }
}

#[test]
fn top_06_disjoint_histories() {
    let (_dir, store) = setup();
    let a = put_mock_revision(&store, 1, vec![]);
    let b = put_mock_revision(&store, 2, vec![]);

    assert_eq!(
        compare_ancestry(&store, a, b).unwrap(),
        DivergenceState::Unrelated
    );
}

#[test]
fn top_07_find_graph_heads_filtering() {
    let (_dir, store) = setup();
    let base = put_mock_revision(&store, 1, vec![]);
    let a = put_mock_revision(&store, 2, vec![base]);
    let b = put_mock_revision(&store, 3, vec![base]);
    let c = put_mock_revision(&store, 4, vec![a]); // c succeeds a

    // Graph heads among [base, a, b, c] should be [b, c] because base is ancestor of a and b, and a is ancestor of c.
    let revs = vec![base, a, b, c];
    let mut heads = find_graph_heads(&store, &revs).unwrap();

    let mut expected = vec![b, c];
    heads.sort_by_key(|id| id.to_string());
    expected.sort_by_key(|id| id.to_string());

    assert_eq!(heads, expected);
}

#[test]
fn top_08_find_best_common_ancestors_multiple_common_ancestors_with_one_best() {
    let (_dir, store) = setup();
    let base = put_mock_revision(&store, 1, vec![]);
    let a1 = put_mock_revision(&store, 2, vec![base]);
    let a2 = put_mock_revision(&store, 3, vec![a1]);
    let b1 = put_mock_revision(&store, 4, vec![a1]);

    let best = find_best_common_ancestors(&store, a2, b1).unwrap();
    assert_eq!(best, vec![a1]);
}

#[test]
fn top_08_multi_parent_ancestry() {
    let (_dir, store) = setup();
    let base = put_mock_revision(&store, 1, vec![]);
    let a = put_mock_revision(&store, 2, vec![base]);
    let b = put_mock_revision(&store, 3, vec![base]);
    
    // M merges A and B
    let m = put_mock_revision(&store, 4, vec![a, b]);
    
    // Compare M with its parents
    assert_eq!(compare_ancestry(&store, m, a).unwrap(), DivergenceState::LocalAhead);
    assert_eq!(compare_ancestry(&store, m, b).unwrap(), DivergenceState::LocalAhead);
}

#[test]
fn top_09_already_reconciled_history() {
    let (_dir, store) = setup();
    let a = put_mock_revision(&store, 1, vec![]);
    let l = put_mock_revision(&store, 2, vec![a]);
    let r = put_mock_revision(&store, 3, vec![a]);
    let m = put_mock_revision(&store, 4, vec![l, r]);

    assert_eq!(compare_ancestry(&store, m, l).unwrap(), DivergenceState::LocalAhead);
    assert_eq!(compare_ancestry(&store, m, r).unwrap(), DivergenceState::LocalAhead);

    let revs = vec![l, r, m];
    let heads = find_graph_heads(&store, &revs).unwrap();
    assert_eq!(heads, vec![m]);
}

#[test]
fn top_10_missing_parent_object() {
    let (_dir, store) = setup();
    let r1 = RepositoryRevisionId::from_object_id(object_id(99)); // Absent
    let r2 = put_mock_revision(&store, 2, vec![r1]);
    let r3 = put_mock_revision(&store, 3, vec![]);

    // Should return MissingTopologyObject, not Unrelated
    let err = compare_ancestry(&store, r2, r3).unwrap_err();
    match err {
        kat::repository::query::QueryError::MissingTopologyObject(id) => {
            assert_eq!(id, object_id(99));
        }
        _ => panic!("Expected MissingTopologyObject, got {:?}", err),
    }
}

#[test]
fn top_11_workspace_reference_integration() {
    // This test proves that computing divergence from the DAG does not
    // implicitly move workspace bases or named references.
    let (dir, store) = setup();
    
    // Create RefStore and Workspace manually for the test
    let ref_store = kat::repository::ref_store::FileRefStore::new(dir.path());
    
    let base = put_mock_revision(&store, 1, vec![]);
    let r_local = put_mock_revision(&store, 2, vec![base]);
    let r_other = put_mock_revision(&store, 3, vec![base]);
    
    // Initialize Workspace pointing to r_local
    let ws = kat::domain::workspace::Workspace {
        id: kat::domain::workspace::WorkspaceId("default".to_string()),
        base_revision: r_local,
    };
    
    // Initialize NamedReference pointing to r_other
    ref_store.compare_and_swap_ref("local/main", None, r_other).unwrap();
    
    // Resolve IDs
    let local_id = ws.base_revision;
    let other_id = ref_store.read_ref("local/main").unwrap();
    
    // Compute divergence
    assert_eq!(
        compare_ancestry(&store, local_id, other_id).unwrap(),
        DivergenceState::Diverged { common_base: base }
    );
    
    // Pointers remain unchanged
    assert_eq!(ws.base_revision, r_local);
    assert_eq!(ref_store.read_ref("local/main").unwrap(), r_other);
}

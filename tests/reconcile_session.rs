pub mod tests {
    use kat::domain::identity::{ObjectId, RepositoryRevisionId};
    use kat::domain::workspace::WorkspaceId;
    use kat::repository::reconcile::{
        ReconciliationSession, ReconciliationSessionState, SessionLoadError,
        clear_reconciliation_session, load_reconciliation_session, save_reconciliation_session,
    };
    use kat::repository::workspace::fake::FakeWorkspaceBackend;

    pub fn session_path(
        repo_root: &std::path::Path,
        workspace_id: &WorkspaceId,
    ) -> std::path::PathBuf {
        repo_root
            .join(".kat")
            .join("workspaces")
            .join(&workspace_id.0)
            .join("reconciliation_session.json")
    }

    fn make_rev(i: u8) -> RepositoryRevisionId {
        let hash = [i; 32];
        RepositoryRevisionId::from_object_id(ObjectId::from_bytes(hash))
    }

    fn setup() -> (
        tempfile::TempDir,
        WorkspaceId,
        kat::domain::identity::RepositoryRevisionId,
        FakeWorkspaceBackend,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let backend = FakeWorkspaceBackend::with_root(root);
        let base_revision = make_rev(1);
        let ws_id = WorkspaceId("test_ws".to_string());

        // Initialize kat workspace structure
        std::fs::create_dir_all(root.join(".kat")).unwrap();

        (dir, ws_id, base_revision, backend)
    }

    #[test]
    fn session_01_02_08_save_load_prepared_clean_and_conflicted_atomic() {
        let (dir, ws_id, base_rev, backend) = setup();
        let root = dir.path();

        // Test PreparedClean
        let session = ReconciliationSession {
            version: 1,
            workspace_id: ws_id.clone(),
            base_revision: base_rev,
            target_revision: base_rev,
            state: ReconciliationSessionState::PreparedClean { revision: base_rev },
        };

        save_reconciliation_session(root, &ws_id, &session, &backend).unwrap();

        let loaded = load_reconciliation_session(root, &ws_id, base_rev)
            .unwrap()
            .unwrap();
        assert_eq!(loaded, session);

        // Test Conflicted
        let session_conflict = ReconciliationSession {
            version: 1,
            workspace_id: ws_id.clone(),
            base_revision: base_rev,
            target_revision: base_rev,
            state: ReconciliationSessionState::Conflicted {
                candidate: kat::repository::reconcile::ReconciliationCandidate {
                    version: 1,
                    base_revision: base_rev,
                    local_revision: base_rev,
                    other_revision: base_rev,
                    proposed_semantic_state: kat::domain::state::SemanticState {
                        ontology_version: kat::domain::identity::ObjectId::from_bytes([0; 32]),
                        elements: vec![],
                        relationships: vec![],
                    },
                    semantic_conflicts: vec![],
                    validation_findings: vec![],
                    physical_candidate: None,
                    materialization_conflicts: vec![],
                    workspace_id: ws_id.clone(),
                },
            },
        };

        save_reconciliation_session(root, &ws_id, &session_conflict, &backend).unwrap();
        let loaded = load_reconciliation_session(root, &ws_id, base_rev)
            .unwrap()
            .unwrap();
        assert_eq!(loaded, session_conflict);
    }

    #[test]
    fn session_03_unsupported_version() {
        let (dir, ws_id, base_rev, _backend) = setup();
        let root = dir.path();

        let session = ReconciliationSession {
            version: 999, // wrong version
            workspace_id: ws_id.clone(),
            base_revision: base_rev,
            target_revision: base_rev,
            state: ReconciliationSessionState::PreparedClean { revision: base_rev },
        };

        // Save under the right ws_id path manually to bypass normal serialization versioning
        let path = session_path(root, &ws_id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string(&session).unwrap()).unwrap();

        let err = load_reconciliation_session(root, &ws_id, base_rev).unwrap_err();
        assert!(matches!(err, SessionLoadError::UnsupportedVersion(999)));
    }

    #[test]
    fn session_04_workspace_mismatch() {
        let (dir, ws_id, base_rev, _backend) = setup();
        let root = dir.path();

        let session = ReconciliationSession {
            version: 1,
            workspace_id: WorkspaceId("wrong_ws".to_string()),
            base_revision: base_rev,
            target_revision: base_rev,
            state: ReconciliationSessionState::PreparedClean { revision: base_rev },
        };

        let path = session_path(root, &ws_id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string(&session).unwrap()).unwrap();

        let err = load_reconciliation_session(root, &ws_id, base_rev).unwrap_err();
        assert!(matches!(err, SessionLoadError::StaleWorkspace));
    }

    #[test]
    fn session_05_06_stale_base() {
        let (dir, ws_id, base_rev, backend) = setup();
        let root = dir.path();

        let session = ReconciliationSession {
            version: 1,
            workspace_id: ws_id.clone(),
            base_revision: base_rev,
            target_revision: base_rev,
            state: ReconciliationSessionState::PreparedClean { revision: base_rev },
        };

        save_reconciliation_session(root, &ws_id, &session, &backend).unwrap();

        let wrong_base = kat::domain::identity::RepositoryRevisionId::from_object_id(
            kat::domain::identity::ObjectId::from_bytes([9; 32]),
        );
        let err = load_reconciliation_session(root, &ws_id, wrong_base).unwrap_err();
        assert!(matches!(err, SessionLoadError::StaleBase));
    }

    #[test]
    fn session_09_clear_removes_session() {
        let (dir, ws_id, base_rev, backend) = setup();
        let root = dir.path();

        let session = ReconciliationSession {
            version: 1,
            workspace_id: ws_id.clone(),
            base_revision: base_rev,
            target_revision: base_rev,
            state: ReconciliationSessionState::PreparedClean { revision: base_rev },
        };

        save_reconciliation_session(root, &ws_id, &session, &backend).unwrap();
        assert!(
            load_reconciliation_session(root, &ws_id, base_rev)
                .unwrap()
                .is_some()
        );

        clear_reconciliation_session(root, &ws_id).unwrap();
        assert!(
            load_reconciliation_session(root, &ws_id, base_rev)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn session_10_draft_and_reconciliation_mutually_exclusive() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Fully init repo so open_repository works
        kat::repository::init::init_repository(root).unwrap();

        let ws_id = WorkspaceId("test_ws".to_string());
        let base_rev = make_rev(1);
        let backend = FakeWorkspaceBackend::with_root(root);

        let session = ReconciliationSession {
            version: 1,
            workspace_id: ws_id.clone(),
            base_revision: base_rev,
            target_revision: base_rev,
            state: ReconciliationSessionState::PreparedClean { revision: base_rev },
        };
        save_reconciliation_session(root, &ws_id, &session, &backend).unwrap();

        // Simulate an open draft session
        let kat_dir = root.join(".kat");
        let draft_session_dir = kat_dir.join("work").join("change");
        std::fs::create_dir_all(&draft_session_dir).unwrap();
        std::fs::write(draft_session_dir.join("session.json"), "{}").unwrap();

        let err =
            kat::repository::reconcile::reconcile_workspace(root, &ws_id, base_rev).unwrap_err();
        assert!(matches!(
            err,
            kat::repository::query::QueryError::WorkspaceConflict(_)
        ));
    }
}

use kat::domain::conflict::MaterializationConflictKind;
use kat::domain::workspace::{PhysicalReconciliationResult, WorkspaceBackend};
use kat::repository::workspace::fake::{FakeEntry, FakeWorkspaceBackend};
use std::path::PathBuf;

fn create_snap(
    backend: &FakeWorkspaceBackend,
    files: &[(&str, &str)],
) -> kat::domain::identity::WorkspaceSnapshotId {
    let mut tree = backend.working_tree.write().unwrap();
    tree.clear();
    let mut paths = Vec::new();
    for (path, content) in files {
        let p = PathBuf::from(path);
        paths.push(p.clone());
        tree.insert(
            p,
            FakeEntry::File {
                content: content.as_bytes().to_vec(),
                executable: false,
            },
        );
    }
    drop(tree);
    backend.create_snapshot(&paths).unwrap()
}

#[test]
fn phy_clean_local_changed_other_unchanged() {
    let backend = FakeWorkspaceBackend::new();
    let base = create_snap(&backend, &[("a.txt", "base")]);
    let local = create_snap(&backend, &[("a.txt", "local")]);
    let other = create_snap(&backend, &[("a.txt", "base")]);

    let res = backend.reconcile_physical(&base, &local, &other).unwrap();
    match res {
        PhysicalReconciliationResult::Clean { snapshot } => {
            let snaps = backend.snapshots.read().unwrap();
            let snap = snaps.get(&snapshot).unwrap();
            let entry = snap.get(&PathBuf::from("a.txt")).unwrap();
            assert_eq!(
                *entry,
                FakeEntry::File {
                    content: b"local".to_vec(),
                    executable: false
                }
            );
        }
        _ => panic!("Expected Clean merge"),
    }
}

#[test]
fn phy_clean_other_changed_local_unchanged() {
    let backend = FakeWorkspaceBackend::new();
    let base = create_snap(&backend, &[("a.txt", "base")]);
    let local = create_snap(&backend, &[("a.txt", "base")]);
    let other = create_snap(&backend, &[("a.txt", "other")]);

    let res = backend.reconcile_physical(&base, &local, &other).unwrap();
    match res {
        PhysicalReconciliationResult::Clean { snapshot } => {
            let snaps = backend.snapshots.read().unwrap();
            let snap = snaps.get(&snapshot).unwrap();
            let entry = snap.get(&PathBuf::from("a.txt")).unwrap();
            assert_eq!(
                *entry,
                FakeEntry::File {
                    content: b"other".to_vec(),
                    executable: false
                }
            );
        }
        _ => panic!("Expected Clean merge"),
    }
}

#[test]
fn phy_clean_both_changed_identically() {
    let backend = FakeWorkspaceBackend::new();
    let base = create_snap(&backend, &[("a.txt", "base")]);
    let local = create_snap(&backend, &[("a.txt", "same")]);
    let other = create_snap(&backend, &[("a.txt", "same")]);

    let res = backend.reconcile_physical(&base, &local, &other).unwrap();
    match res {
        PhysicalReconciliationResult::Clean { snapshot } => {
            let snaps = backend.snapshots.read().unwrap();
            let snap = snaps.get(&snapshot).unwrap();
            let entry = snap.get(&PathBuf::from("a.txt")).unwrap();
            assert_eq!(
                *entry,
                FakeEntry::File {
                    content: b"same".to_vec(),
                    executable: false
                }
            );
        }
        _ => panic!("Expected Clean merge"),
    }
}

#[test]
fn phy_clean_independent_additions() {
    let backend = FakeWorkspaceBackend::new();
    let base = create_snap(&backend, &[]);
    let local = create_snap(&backend, &[("local.txt", "l")]);
    let other = create_snap(&backend, &[("other.txt", "o")]);

    let res = backend.reconcile_physical(&base, &local, &other).unwrap();
    match res {
        PhysicalReconciliationResult::Clean { snapshot } => {
            let snaps = backend.snapshots.read().unwrap();
            let snap = snaps.get(&snapshot).unwrap();
            assert!(snap.contains_key(&PathBuf::from("local.txt")));
            assert!(snap.contains_key(&PathBuf::from("other.txt")));
        }
        _ => panic!("Expected Clean merge"),
    }
}

#[test]
fn phy_conflict_content() {
    let backend = FakeWorkspaceBackend::new();
    let base = create_snap(&backend, &[("a.txt", "base")]);
    let local = create_snap(&backend, &[("a.txt", "local")]);
    let other = create_snap(&backend, &[("a.txt", "other")]);

    let res = backend.reconcile_physical(&base, &local, &other).unwrap();
    match res {
        PhysicalReconciliationResult::Conflicted(candidate) => {
            assert_eq!(candidate.conflicts.len(), 1);
            assert_eq!(
                candidate.conflicts[0].kind,
                MaterializationConflictKind::Content
            );
            assert_eq!(candidate.conflicts[0].paths, vec![PathBuf::from("a.txt")]);

            // Provisional should retain Base
            let merges = backend.provisional_merges.read().unwrap();
            let prov = merges.get(&candidate.provisional).unwrap();
            assert_eq!(
                *prov.get(&PathBuf::from("a.txt")).unwrap(),
                FakeEntry::File {
                    content: b"base".to_vec(),
                    executable: false
                }
            );
        }
        _ => panic!("Expected Conflicted merge"),
    }
}

#[test]
fn phy_conflict_delete_modify() {
    let backend = FakeWorkspaceBackend::new();
    let base = create_snap(&backend, &[("a.txt", "base")]);
    let local = create_snap(&backend, &[]);
    let other = create_snap(&backend, &[("a.txt", "other")]);

    let res = backend.reconcile_physical(&base, &local, &other).unwrap();
    match res {
        PhysicalReconciliationResult::Conflicted(candidate) => {
            assert_eq!(candidate.conflicts.len(), 1);
            assert_eq!(
                candidate.conflicts[0].kind,
                MaterializationConflictKind::DeleteModify
            );
            assert_eq!(candidate.conflicts[0].paths, vec![PathBuf::from("a.txt")]);

            // Provisional should retain Base
            let merges = backend.provisional_merges.read().unwrap();
            let prov = merges.get(&candidate.provisional).unwrap();
            assert_eq!(
                *prov.get(&PathBuf::from("a.txt")).unwrap(),
                FakeEntry::File {
                    content: b"base".to_vec(),
                    executable: false
                }
            );
        }
        _ => panic!("Expected Conflicted merge"),
    }
}

#[test]
fn phy_conflict_path_collision() {
    let backend = FakeWorkspaceBackend::new();
    let base = create_snap(&backend, &[]);
    let local = create_snap(&backend, &[("foo", "file")]);
    let other = create_snap(&backend, &[("foo/bar", "file in dir")]);

    let res = backend.reconcile_physical(&base, &local, &other).unwrap();
    match res {
        PhysicalReconciliationResult::Conflicted(candidate) => {
            assert_eq!(candidate.conflicts.len(), 1);
            assert_eq!(
                candidate.conflicts[0].kind,
                MaterializationConflictKind::PathCollision
            );
            assert!(candidate.conflicts[0].paths.contains(&PathBuf::from("foo")));
            assert!(
                candidate.conflicts[0]
                    .paths
                    .contains(&PathBuf::from("foo/bar"))
            );

            // Provisional should omit since Base did not have them
            let merges = backend.provisional_merges.read().unwrap();
            let prov = merges.get(&candidate.provisional).unwrap();
            assert!(!prov.contains_key(&PathBuf::from("foo")));
            assert!(!prov.contains_key(&PathBuf::from("foo/bar")));
        }
        _ => panic!("Expected Conflicted merge"),
    }
}

#[test]
fn phy_conflict_partial_success() {
    let backend = FakeWorkspaceBackend::new();
    let base = create_snap(&backend, &[("a.txt", "A0"), ("b.txt", "B0")]);
    let local = create_snap(&backend, &[("a.txt", "A1"), ("b.txt", "B1")]);
    let other = create_snap(&backend, &[("a.txt", "A2"), ("b.txt", "B0")]);

    let res = backend.reconcile_physical(&base, &local, &other).unwrap();
    match res {
        PhysicalReconciliationResult::Conflicted(candidate) => {
            // `a.txt` is conflicted (Content)
            assert_eq!(candidate.conflicts.len(), 1);
            assert_eq!(
                candidate.conflicts[0].kind,
                MaterializationConflictKind::Content
            );
            assert_eq!(candidate.conflicts[0].paths, vec![PathBuf::from("a.txt")]);

            // Provisional should retain Base A0 for `a.txt` and safely compose B1 for `b.txt`
            let state = backend.get_candidate_state(&candidate.provisional).unwrap();
            let prov = state.provisional_tree;

            assert_eq!(
                *prov.get(&PathBuf::from("a.txt")).unwrap(),
                FakeEntry::File {
                    content: b"A0".to_vec(),
                    executable: false
                }
            );
            assert_eq!(
                *prov.get(&PathBuf::from("b.txt")).unwrap(),
                FakeEntry::File {
                    content: b"B1".to_vec(),
                    executable: false
                }
            );
        }
        _ => panic!("Expected Conflicted merge"),
    }
}

#[test]
fn phy_git_conflict_partial_success() {
    use kat::repository::workspace::git::GitWorkspaceBackend;
    use std::fs;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let root = dir.path();
    let backend = GitWorkspaceBackend::init(root).unwrap();

    let create_git_snap = |files: &[(&str, &str)]| {
        for entry in fs::read_dir(root).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name() != ".kat" {
                if entry.path().is_dir() {
                    fs::remove_dir_all(entry.path()).unwrap();
                } else {
                    fs::remove_file(entry.path()).unwrap();
                }
            }
        }
        let mut paths = Vec::new();
        for (p, content) in files {
            let full_p = root.join(p);
            if let Some(parent) = full_p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&full_p, content).unwrap();
            paths.push(PathBuf::from(p));
        }
        backend.create_snapshot(&paths).unwrap()
    };

    let base = create_git_snap(&[("a.txt", "A0"), ("b.txt", "B0")]);
    let local = create_git_snap(&[("a.txt", "A1"), ("b.txt", "B1")]);
    let other = create_git_snap(&[("a.txt", "A2"), ("b.txt", "B0")]);

    let res = backend.reconcile_physical(&base, &local, &other).unwrap();
    match res {
        PhysicalReconciliationResult::Conflicted(candidate) => {
            assert_eq!(candidate.conflicts.len(), 1);
            assert_eq!(
                candidate.conflicts[0].kind,
                MaterializationConflictKind::Content
            );
            assert_eq!(candidate.conflicts[0].paths, vec![PathBuf::from("a.txt")]);

            let wid = kat::domain::workspace::WorkspaceId("test_ws".to_string());
            backend
                .persist_physical_candidate(&wid, &candidate)
                .unwrap();
            let state = backend
                .get_candidate_state(&wid, &candidate.provisional)
                .unwrap();
            let prov_oid = state.provisional_tree;

            assert!(!prov_oid.is_empty());
        }
        _ => panic!("Expected Conflicted merge"),
    }
}

#[test]
fn phy_21_persistence_survives_restart() {
    use kat::repository::workspace::fake::FakeWorkspaceBackend;
    use std::path::PathBuf;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let root = dir.path();
    let backend = FakeWorkspaceBackend::with_root(root);

    let create_snap = |files: &[(&str, &str)]| {
        let mut tree = backend.working_tree.write().unwrap();
        tree.clear();
        let mut paths = Vec::new();
        for (p, content) in files {
            paths.push(PathBuf::from(p));
            tree.insert(
                PathBuf::from(p),
                kat::repository::workspace::fake::FakeEntry::File {
                    content: content.as_bytes().to_vec(),
                    executable: false,
                },
            );
        }
        drop(tree);
        backend.create_snapshot(&paths).unwrap()
    };

    let base = create_snap(&[("a.txt", "A0")]);
    let local = create_snap(&[("a.txt", "A1")]);
    let other = create_snap(&[("a.txt", "A2")]);

    let res = backend.reconcile_physical(&base, &local, &other).unwrap();

    let phys_id = match res {
        kat::domain::workspace::PhysicalReconciliationResult::Conflicted(c) => c.provisional,
        _ => panic!("Expected conflict"),
    };

    // Drop the backend to simulate process restart
    drop(backend);

    // Reopen backend at the same project root
    let new_backend = FakeWorkspaceBackend::with_root(root);

    // Attempt to load the candidate state
    let state = new_backend
        .get_candidate_state(&phys_id)
        .expect("Should load durable candidate state");

    assert_eq!(state.conflicts.len(), 1);
    assert_eq!(state.conflicts[0].path, PathBuf::from("a.txt"));

    // Ensure the neutral tree was restored correctly (base A0)
    let prov_entry = state.provisional_tree.get(&PathBuf::from("a.txt")).unwrap();
    if let kat::repository::workspace::fake::FakeEntry::File { content, .. } = prov_entry {
        assert_eq!(content, b"A0");
    } else {
        panic!("Expected file");
    }
}

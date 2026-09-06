use kat::domain::workspace::WorkspaceBackend;
use kat::repository::workspace::git::GitWorkspaceBackend;
use std::fs;
use std::path::PathBuf;

fn setup_temp_project(name: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let project_root = dir.path().join(name);
    fs::create_dir_all(&project_root).unwrap();
    (dir, project_root)
}

#[test]
fn adopt_01_import_failure_leaves_original_git_untouched() {
    let (_dir, root) = setup_temp_project("adopt_01");
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".git/config"), "fake git config").unwrap();

    // Simulate import failure by making `.kat/physical` readonly before adoption
    fs::create_dir_all(root.join(".kat/physical")).unwrap();
    let mut perms = fs::metadata(root.join(".kat/physical"))
        .unwrap()
        .permissions();
    perms.set_readonly(true);
    fs::set_permissions(root.join(".kat/physical"), perms.clone()).unwrap();

    let result = GitWorkspaceBackend::adopt(&root);
    assert!(result.is_err());

    // Restore permissions so cleanup works
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    fs::set_permissions(root.join(".kat/physical"), perms).unwrap();

    // The original `.git` must still be untouched
    assert!(root.join(".git/config").exists());
    assert!(!root.join(".kat/physical/git").exists());
}

#[test]
fn adopt_02_validation_failure_leaves_original_repo_usable() {
    let (_dir, root) = setup_temp_project("adopt_02");

    // Create an invalid .git directory that can be copied but not opened by git2
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".git/config"), "broken").unwrap();

    let result = GitWorkspaceBackend::adopt(&root);
    assert!(result.is_err());

    // The original `.git` must still be untouched
    assert!(root.join(".git/config").exists());
    // The partially imported .kat/physical/git should be cleaned up
    assert!(!root.join(".kat/physical/git").exists());
}

#[test]
fn adopt_04_successful_cutover() {
    let (_dir, root) = setup_temp_project("adopt_04");

    // Initialize a real Git repository
    git2::Repository::init(&root).unwrap();
    assert!(root.join(".git").exists());

    // Adopt it
    let _backend = GitWorkspaceBackend::adopt(&root).unwrap();

    // The original `.git` must be moved to backup
    assert!(!root.join(".git").exists());
    assert!(root.join(".kat/adoption-backup/original-git").exists());

    // The physical backend is active
    assert!(root.join(".kat/physical/git").exists());
    let _reopened = GitWorkspaceBackend::open(&root).unwrap();
}

#[test]
fn git_01_fresh_kat_project() {
    let (_dir, root) = setup_temp_project("git_01");
    let _backend = GitWorkspaceBackend::init(&root).unwrap();

    // `.kat/physical/git` should exist
    assert!(root.join(".kat/physical/git").exists());

    // Original `.git` shouldn't exist because we didn't adopt
    assert!(!root.join(".git").exists());

    let _backend = GitWorkspaceBackend::open(&root).unwrap();
}

#[test]
fn git_13_14_cross_backend_conformance() {
    let (_dir, root) = setup_temp_project("git_13_14");
    let git_backend = kat::repository::workspace::git::GitWorkspaceBackend::init(&root).unwrap();

    let paths = vec![
        std::path::PathBuf::from("file.txt"),
        std::path::PathBuf::from("symlink"),
        std::path::PathBuf::from("exe.sh"),
        std::path::PathBuf::from("subdir/nested.txt"),
    ];

    let fake_backend = kat::repository::workspace::fake::FakeWorkspaceBackend::new();
    {
        let mut t = fake_backend.working_tree.write().unwrap();
        t.insert(
            std::path::PathBuf::from("file.txt"),
            kat::repository::workspace::fake::FakeEntry::File {
                content: b"identical content".to_vec(),
                executable: false,
            },
        );

        #[cfg(unix)]
        t.insert(
            std::path::PathBuf::from("symlink"),
            kat::repository::workspace::fake::FakeEntry::Symlink {
                target: "file.txt".to_string(),
            },
        );

        t.insert(
            std::path::PathBuf::from("exe.sh"),
            kat::repository::workspace::fake::FakeEntry::File {
                content: b"#!/bin/sh\necho hello".to_vec(),
                executable: true,
            },
        );
        t.insert(
            std::path::PathBuf::from("subdir/nested.txt"),
            kat::repository::workspace::fake::FakeEntry::File {
                content: b"nested content".to_vec(),
                executable: false,
            },
        );
    }

    let fake_snapshot = fake_backend.create_snapshot(&paths).unwrap();

    std::fs::write(root.join("file.txt"), "identical content").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("file.txt", root.join("symlink")).unwrap();

    std::fs::write(root.join("exe.sh"), "#!/bin/sh\necho hello").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(root.join("exe.sh"))
            .unwrap()
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(root.join("exe.sh"), perms).unwrap();
    }

    std::fs::create_dir_all(root.join("subdir")).unwrap();
    std::fs::write(root.join("subdir/nested.txt"), "nested content").unwrap();

    let git_snapshot = git_backend.create_snapshot(&paths).unwrap();

    assert_eq!(fake_snapshot, git_snapshot);

    let fake_mat = fake_backend
        .resolve_materialization(&std::path::PathBuf::from("file.txt"), &fake_snapshot)
        .unwrap();
    let git_mat = git_backend
        .resolve_materialization(&std::path::PathBuf::from("file.txt"), &git_snapshot)
        .unwrap();
    assert_eq!(fake_mat, git_mat);
}

#[test]
fn git_20_reachability() {
    let (_dir, root) = setup_temp_project("git_20_reachability");
    let backend = kat::repository::workspace::git::GitWorkspaceBackend::init(&root).unwrap();

    std::fs::write(root.join("test.txt"), "content").unwrap();
    let id = backend
        .create_snapshot(&[std::path::PathBuf::from("test.txt")])
        .unwrap();

    let repo = git2::Repository::open(root.join(".kat/physical/git")).unwrap();

    let reps = kat::repository::workspace::git::GitWorkspaceBackend::get_snapshot_representations(
        &root, &id,
    );
    assert!(!reps.is_empty());

    let commit_oid = reps[0].commit;

    let id_hex = id
        .as_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    let ref_name = format!("refs/kat/lineage/{}/{}", id_hex, commit_oid);
    let reference = repo.find_reference(&ref_name).unwrap();
    assert_eq!(reference.target().unwrap(), commit_oid);
}

#[test]
fn git_22_gitlink_rejection() {
    let (_dir, root) = setup_temp_project("git_22_gitlink_rejection");
    let backend = kat::repository::workspace::git::GitWorkspaceBackend::init(&root).unwrap();

    let repo = git2::Repository::open(root.join(".kat/physical/git")).unwrap();
    repo.set_workdir(&root, false).unwrap();

    // Add custom isn't on git2::Index, so we will manually create a tree and commit.
    // We can use treebuilder
    let mut tb = repo.treebuilder(None).unwrap();
    let submodule_oid = git2::Oid::from_str("0123456789abcdef0123456789abcdef01234567").unwrap();
    tb.insert("submodule_dir", submodule_oid, 0o160000).unwrap();
    let tree_id = tb.write().unwrap();

    let sig = git2::Signature::now("KAT", "kat@example.com").unwrap();
    let tree = repo.find_tree(tree_id).unwrap();

    let commit_id = repo
        .commit(Some("HEAD"), &sig, &sig, "synthetic commit", &tree, &[])
        .unwrap();

    let snap_id = kat::domain::identity::WorkspaceSnapshotId::new(vec![0; 32]);

    let refs_dir = root.join(".kat/physical/active_lineage");
    std::fs::write(refs_dir, commit_id.to_string()).unwrap();

    let mut map = std::collections::HashMap::new();
    let snap_id_hex = snap_id
        .as_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    map.insert(snap_id_hex, vec![commit_id.to_string()]);

    let index_file = root.join(".kat/physical/snapshots.json");
    std::fs::create_dir_all(index_file.parent().unwrap()).unwrap();
    std::fs::write(&index_file, serde_json::to_string(&map).unwrap()).unwrap();

    let result =
        backend.resolve_materialization(&std::path::PathBuf::from("submodule_dir"), &snap_id);
    assert!(matches!(
        result,
        Err(kat::domain::workspace::WorkspaceBackendError::UnsupportedPhysicalEntryType)
    ));
}

#[test]
fn adopt_03_interruption_recovery() {
    let (_dir, root) = setup_temp_project("adopt_03_interruption");

    let repo = git2::Repository::init(&root).unwrap();
    let file_path = root.join("file.txt");
    std::fs::write(&file_path, "original content").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("file.txt")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let sig = git2::Signature::now("user", "user@example.com").unwrap();
    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        "init",
        &repo.find_tree(tree_id).unwrap(),
        &[],
    )
    .unwrap();

    std::fs::create_dir_all(root.join(".kat/physical/git")).unwrap();

    let _backend = kat::repository::workspace::git::GitWorkspaceBackend::adopt(&root).unwrap();

    assert!(root.join(".kat/adoption-backup/original-git").exists());
    assert!(root.join(".kat/physical/git/HEAD").exists());
}

#[test]
fn git_reopen_persistence() {
    let (_dir, root) = setup_temp_project("git_reopen_persistence");

    let id_v1;
    let id_v2;
    {
        // Process 1: create repository
        let backend = kat::repository::workspace::git::GitWorkspaceBackend::init(&root).unwrap();

        let file_path = root.join("file.txt");
        std::fs::write(&file_path, "content v1").unwrap();
        id_v1 = backend
            .create_snapshot(&[std::path::PathBuf::from("file.txt")])
            .unwrap();

        std::fs::write(&file_path, "content v2").unwrap();
        id_v2 = backend
            .create_snapshot(&[std::path::PathBuf::from("file.txt")])
            .unwrap();
    } // Backend instance dropped

    {
        // Process 2: reopen repository
        let backend = kat::repository::workspace::git::GitWorkspaceBackend::open(&root).unwrap();

        // Verify W1
        assert!(backend.verify_snapshot_integrity(&id_v1).unwrap());
        // Verify W2
        assert!(backend.verify_snapshot_integrity(&id_v2).unwrap());

        // Inspect working state
        let state = backend.inspect_working_state(&id_v2).unwrap();
        assert!(state.changes.is_clean());
        assert_eq!(
            state.backend_consistency,
            kat::domain::workspace::BackendConsistency::Consistent
        );

        // Materialize W1
        backend.materialize_snapshot(&id_v1).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("file.txt")).unwrap(),
            "content v1"
        );

        // Materialize W2
        backend.materialize_snapshot(&id_v2).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("file.txt")).unwrap(),
            "content v2"
        );
    }
}

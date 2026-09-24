use kat::domain::workspace::WorkspaceBackend;
use kat::repository::workspace::fake::FakeWorkspaceBackend;
use kat::repository::workspace::git::GitWorkspaceBackend;
use std::fs;
use std::path::PathBuf;

fn setup_temp_project(name: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let project_root = dir.path().join(name);
    fs::create_dir_all(&project_root).unwrap();
    (dir, project_root)
}

fn create_backends(root: &std::path::Path) -> (FakeWorkspaceBackend, GitWorkspaceBackend) {
    let fake_backend = FakeWorkspaceBackend::with_root(root);
    GitWorkspaceBackend::init(root).unwrap();
    let git_backend = GitWorkspaceBackend::open(root).unwrap();
    (fake_backend, git_backend)
}

fn create_parallel_snapshot(
    root: &std::path::Path,
    fake_backend: &FakeWorkspaceBackend,
    git_backend: &GitWorkspaceBackend,
    files: &[(&str, &str)],
) -> (
    kat::domain::identity::WorkspaceSnapshotId,
    kat::domain::identity::WorkspaceSnapshotId,
) {
    let mut paths = Vec::new();

    // Prepare Fake tree
    let mut t = fake_backend.working_tree.write().unwrap();
    t.clear();
    for (p, content) in files {
        t.insert(
            PathBuf::from(p),
            kat::repository::workspace::fake::FakeEntry::File {
                content: content.as_bytes().to_vec(),
                executable: false,
            },
        );
    }
    drop(t);

    // Prepare Git tree
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() != ".kat" {
            let _ = fs::remove_file(entry.path());
            let _ = fs::remove_dir_all(entry.path());
        }
    }
    for (p, content) in files {
        paths.push(PathBuf::from(p));
        let full_path = root.join(p);
        if let Some(parent) = full_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(full_path, content).unwrap();
    }

    let fake_id = fake_backend.create_snapshot(&paths).unwrap();
    let git_id = git_backend.create_snapshot(&paths).unwrap();

    assert_eq!(fake_id, git_id, "Snapshot IDs must match");
    (fake_id, git_id)
}

#[test]
fn phy_19a_clean_identical() {
    let (_dir, root) = setup_temp_project("phy_19a");
    let (fake, git) = create_backends(&root);

    let (fake_b, git_b) =
        create_parallel_snapshot(&root, &fake, &git, &[("a.txt", "A0"), ("b.txt", "B0")]);
    let (fake_l, git_l) =
        create_parallel_snapshot(&root, &fake, &git, &[("a.txt", "A1"), ("b.txt", "B0")]);
    let (fake_o, git_o) =
        create_parallel_snapshot(&root, &fake, &git, &[("a.txt", "A0"), ("b.txt", "B1")]);

    let fake_res = fake.reconcile_physical(&fake_b, &fake_l, &fake_o).unwrap();
    let git_res = git.reconcile_physical(&git_b, &git_l, &git_o).unwrap();

    match (fake_res, git_res) {
        (
            kat::domain::workspace::PhysicalReconciliationResult::Clean { snapshot: f_id },
            kat::domain::workspace::PhysicalReconciliationResult::Clean { snapshot: g_id },
        ) => {
            assert_eq!(f_id, g_id, "Clean result snapshot IDs must match exactly");
        }
        _ => panic!("Expected clean merges"),
    }
}

#[test]
fn phy_19b_content_conflict() {
    let (_dir, root) = setup_temp_project("phy_19b");
    let (fake, git) = create_backends(&root);

    let (fake_b, git_b) = create_parallel_snapshot(&root, &fake, &git, &[("a.txt", "A0")]);
    let (fake_l, git_l) = create_parallel_snapshot(&root, &fake, &git, &[("a.txt", "A1")]);
    let (fake_o, git_o) = create_parallel_snapshot(&root, &fake, &git, &[("a.txt", "A2")]);

    let fake_res = fake.reconcile_physical(&fake_b, &fake_l, &fake_o).unwrap();
    let git_res = git.reconcile_physical(&git_b, &git_l, &git_o).unwrap();

    match (fake_res, git_res) {
        (
            kat::domain::workspace::PhysicalReconciliationResult::Conflicted(f_c),
            kat::domain::workspace::PhysicalReconciliationResult::Conflicted(g_c),
        ) => {
            assert_eq!(
                f_c.conflicts, g_c.conflicts,
                "Conflict definitions must match exactly"
            );
        }
        _ => panic!("Expected conflicted merges"),
    }
}

#[test]
fn phy_19c_delete_modify() {
    let (_dir, root) = setup_temp_project("phy_19c");
    let (fake, git) = create_backends(&root);

    let (fake_b, git_b) = create_parallel_snapshot(&root, &fake, &git, &[("a.txt", "A0")]);
    let (fake_l, git_l) = create_parallel_snapshot(&root, &fake, &git, &[("a.txt", "A1")]); // modify
    let (fake_o, git_o) = create_parallel_snapshot(&root, &fake, &git, &[]); // delete

    let fake_res = fake.reconcile_physical(&fake_b, &fake_l, &fake_o).unwrap();
    let git_res = git.reconcile_physical(&git_b, &git_l, &git_o).unwrap();

    match (fake_res, git_res) {
        (
            kat::domain::workspace::PhysicalReconciliationResult::Conflicted(f_c),
            kat::domain::workspace::PhysicalReconciliationResult::Conflicted(g_c),
        ) => {
            assert_eq!(
                f_c.conflicts, g_c.conflicts,
                "Conflict definitions must match exactly"
            );
            assert_eq!(
                f_c.conflicts[0].kind,
                kat::domain::conflict::MaterializationConflictKind::DeleteModify
            );
        }
        _ => panic!("Expected conflicted merges"),
    }
}

#[test]
fn phy_19d_path_collision() {
    let (_dir, root) = setup_temp_project("phy_19d");
    let (fake, git) = create_backends(&root);

    let (fake_b, git_b) = create_parallel_snapshot(&root, &fake, &git, &[]);
    let (fake_l, git_l) = create_parallel_snapshot(&root, &fake, &git, &[("dir/file.txt", "L")]); // creates dir
    let (fake_o, git_o) = create_parallel_snapshot(&root, &fake, &git, &[("dir", "O")]); // creates file named dir

    let fake_res = fake.reconcile_physical(&fake_b, &fake_l, &fake_o).unwrap();
    let git_res = git.reconcile_physical(&git_b, &git_l, &git_o).unwrap();

    match (fake_res, git_res) {
        (
            kat::domain::workspace::PhysicalReconciliationResult::Conflicted(f_c),
            kat::domain::workspace::PhysicalReconciliationResult::Conflicted(g_c),
        ) => {
            println!("f_c: {:?}", f_c.conflicts);
            println!("g_c: {:?}", g_c.conflicts);
            assert_eq!(f_c.conflicts.len(), 1);
            assert_eq!(g_c.conflicts.len(), 1);
            assert_eq!(
                f_c.conflicts[0].kind,
                kat::domain::conflict::MaterializationConflictKind::PathCollision
            );
            assert_eq!(
                g_c.conflicts[0].kind,
                kat::domain::conflict::MaterializationConflictKind::PathCollision
            );
        }
        _ => panic!("Expected conflicted merges"),
    }
}

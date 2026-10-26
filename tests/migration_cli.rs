use std::path::Path;
use std::process::{Command, Output};

use kat::repository::init::init_repository;
use kat::repository::metadata::{
    LEGACY_FORMAT_VERSION, RepositoryMetadata, SUPPORTED_FORMAT_VERSION,
};
use kat::repository::open::open_repository;
use kat::repository::workspace::git::GitWorkspaceBackend;
use kat::repository::workspace::state::read_workspace_state;

fn kat(args: &[&str], cwd: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kat"))
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap()
}

#[test]
fn init_creates_a_complete_root_revision_in_a_real_cli_process() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "project\n").unwrap();

    let output = kat(&["init"], dir.path());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let repository = open_repository(dir.path()).unwrap();
    assert_eq!(repository.metadata.format_version, SUPPORTED_FORMAT_VERSION);
    let workspace = read_workspace_state(dir.path()).unwrap();
    let revision = repository.read_revision(workspace.base_revision).unwrap();

    assert!(revision.parents.is_empty());
    assert_eq!(
        revision.semantic_state.as_object_id(),
        repository.accepted.state
    );
    assert!(revision.semantic_change.is_none());
}

#[test]
fn init_inside_existing_git_project_captures_files_without_git_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(root.join(".gitignore"), "build/\n").unwrap();
    std::fs::create_dir_all(root.join("build")).unwrap();
    std::fs::write(root.join("build/generated.rs"), "ignored\n").unwrap();

    let git = git2::Repository::init(root).unwrap();
    let mut index = git.index().unwrap();
    index.add_path(std::path::Path::new("src/main.rs")).unwrap();
    index.add_path(std::path::Path::new(".gitignore")).unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = git.find_tree(tree_id).unwrap();
    let signature = git2::Signature::now("test", "test@example.com").unwrap();
    git.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
        .unwrap();
    drop(tree);
    drop(index);
    drop(git);

    let output = kat(&["init"], root);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let repository = open_repository(root).unwrap();
    let workspace = read_workspace_state(root).unwrap();
    let revision = repository.read_revision(workspace.base_revision).unwrap();
    let backend = git2::Repository::open(root.join(".kat/physical/git")).unwrap();
    let representation =
        GitWorkspaceBackend::get_snapshot_representations(root, &revision.workspace_snapshot)
            .unwrap()
            .remove(0);
    let commit = backend.find_commit(representation.commit).unwrap();
    let tree = commit.tree().unwrap();
    let main = tree.get_path(std::path::Path::new("src/main.rs")).unwrap();
    let blob = backend.find_blob(main.id()).unwrap();

    assert_eq!(blob.content(), b"fn main() {}\n");
    assert!(tree.get_path(std::path::Path::new(".kat")).is_err());
    assert!(tree.get_path(std::path::Path::new(".git")).is_err());
    assert!(
        tree.get_path(std::path::Path::new("build/generated.rs"))
            .is_err()
    );
    assert!(!root.join(".git").exists());
    assert!(root.join(".kat/adoption-backup/original-git").exists());
}

#[test]
fn migrate_is_explicit_and_repeated_migration_is_a_noop() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();
    init_repository(dir.path()).unwrap();

    let metadata_path = dir.path().join(".kat/repository.toml");
    let mut metadata = RepositoryMetadata::read(&metadata_path).unwrap();
    metadata.format_version = LEGACY_FORMAT_VERSION;
    metadata.write(&metadata_path).unwrap();

    let open = kat(&["status"], dir.path());
    assert!(!open.status.success());
    assert!(String::from_utf8_lossy(&open.stderr).contains("kat migrate"));

    let migrated = kat(&["migrate"], dir.path());
    assert!(
        migrated.status.success(),
        "{}",
        String::from_utf8_lossy(&migrated.stderr)
    );
    let base = read_workspace_state(dir.path()).unwrap().base_revision;

    let repeated = kat(&["migrate"], dir.path());
    assert!(
        repeated.status.success(),
        "{}",
        String::from_utf8_lossy(&repeated.stderr)
    );
    assert!(String::from_utf8_lossy(&repeated.stdout).contains("already migrated"));
    assert_eq!(
        read_workspace_state(dir.path()).unwrap().base_revision,
        base
    );
}

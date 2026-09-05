use std::fs;
use std::path::{Path, PathBuf};
use kat::domain::workspace::{WorkspaceBackend, WorkingState, BackendConsistency};
use kat::repository::workspace::git::GitWorkspaceBackend;

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
    let mut perms = fs::metadata(root.join(".kat/physical")).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(root.join(".kat/physical"), perms.clone()).unwrap();

    let result = GitWorkspaceBackend::adopt(&root);
    assert!(result.is_err());
    
    // Restore permissions so cleanup works
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
    let backend = GitWorkspaceBackend::init(&root).unwrap();
    
    // `.kat/physical/git` should exist
    assert!(root.join(".kat/physical/git").exists());
    
    // Original `.git` shouldn't exist because we didn't adopt
    assert!(!root.join(".git").exists());
    
    let _backend = GitWorkspaceBackend::open(&root).unwrap();
}

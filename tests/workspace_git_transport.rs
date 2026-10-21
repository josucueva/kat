#![cfg(unix)]

use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use git2::{Repository, Signature};
use kat::domain::identity::RepositoryId;
use kat::repository::workspace::git_transport::{
    ControlledGitBackend, GitWorkspaceBinding, WorkspaceBackendId,
};
use tempfile::TempDir;

struct GitDaemon {
    _dir: TempDir,
    child: Child,
    url: String,
}

impl Drop for GitDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start_git_daemon(root: TempDir) -> GitDaemon {
    let socket = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = socket.local_addr().unwrap().port();
    drop(socket);

    let child = Command::new("git")
        .args([
            "daemon",
            "--reuseaddr",
            "--export-all",
            "--enable=receive-pack",
            "--listen=127.0.0.1",
            &format!("--port={port}"),
            &format!("--base-path={}", root.path().display()),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("git daemon must be installed for the controlled-server tests");

    let mut server = GitDaemon {
        _dir: root,
        child,
        url: format!("git://127.0.0.1:{port}/workspace.git"),
    };
    for _ in 0..100 {
        if server.child.try_wait().unwrap().is_some() {
            panic!("git daemon exited during startup");
        }
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return server;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("git daemon did not start listening");
}

fn commit_pair(path: &Path) -> (Repository, git2::Oid, git2::Oid) {
    let repo = Repository::init_bare(path).unwrap();
    let blob = repo.blob(b"workspace snapshot\n").unwrap();
    let mut builder = repo.treebuilder(None).unwrap();
    builder.insert("file.txt", blob, 0o100644).unwrap();
    let tree_id = builder.write().unwrap();
    drop(builder);
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("KAT test", "kat@example.invalid").unwrap();
    let first = repo
        .commit(
            Some("refs/heads/main"),
            &signature,
            &signature,
            "first snapshot representation",
            &tree,
            &[],
        )
        .unwrap();
    let first_commit = repo.find_commit(first).unwrap();
    let second = repo
        .commit(
            Some("refs/heads/main"),
            &signature,
            &signature,
            "second snapshot representation",
            &tree,
            &[&first_commit],
        )
        .unwrap();
    drop(first_commit);
    drop(tree);
    (repo, first, second)
}

fn push(client_path: &Path, url: &str, refspec: &str) -> std::process::Output {
    Command::new("git")
        .args([
            "-C",
            &client_path.display().to_string(),
            "push",
            url,
            refspec,
        ])
        .output()
        .unwrap()
}

#[test]
fn controlled_git_server_binds_repository_and_rejects_rebinding() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace.git");
    let binding = GitWorkspaceBinding {
        repository_id: RepositoryId::new(),
        workspace_backend_id: WorkspaceBackendId::new(),
    };
    let backend = ControlledGitBackend::provision(&path, binding).unwrap();
    assert_eq!(backend.binding(), binding);
    assert!(backend.path().join("git-daemon-export-ok").exists());

    assert!(
        ControlledGitBackend::open(
            &path,
            GitWorkspaceBinding {
                repository_id: RepositoryId::new(),
                ..binding
            }
        )
        .is_err()
    );
    assert!(
        ControlledGitBackend::open(
            &path,
            GitWorkspaceBinding {
                workspace_backend_id: WorkspaceBackendId::new(),
                ..binding
            }
        )
        .is_err()
    );
}

#[test]
fn controlled_git_server_rejects_a_disabled_receive_hook() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace.git");
    let binding = GitWorkspaceBinding {
        repository_id: RepositoryId::new(),
        workspace_backend_id: WorkspaceBackendId::new(),
    };
    ControlledGitBackend::provision(&path, binding).unwrap();
    let hook_path = path.join("hooks/pre-receive");
    let mut permissions = std::fs::metadata(&hook_path).unwrap().permissions();
    permissions.set_mode(0o644);
    std::fs::set_permissions(&hook_path, permissions).unwrap();

    assert!(ControlledGitBackend::open(&path, binding).is_err());
}

#[test]
fn controlled_git_server_accepts_lineage_creation_but_denies_update_and_delete() {
    let root = tempfile::tempdir().unwrap();
    let server_repo = root.path().join("workspace.git");
    let binding = GitWorkspaceBinding {
        repository_id: RepositoryId::new(),
        workspace_backend_id: WorkspaceBackendId::new(),
    };
    ControlledGitBackend::provision(&server_repo, binding).unwrap();
    let server = start_git_daemon(root);

    let client_dir = tempfile::tempdir().unwrap();
    let client_path = client_dir.path().join("client.git");
    let (client, first, second) = commit_pair(&client_path);
    client
        .reference("refs/heads/main", first, true, "restore first test commit")
        .unwrap();
    let protected_ref = format!("refs/kat/lineage/{}/{}", "a".repeat(64), first);
    let create_refspec = format!("refs/heads/main:{protected_ref}");

    let created = push(&client_path, &server.url, &create_refspec);
    assert!(
        created.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&created.stderr)
    );
    assert!(
        !push(&client_path, &server.url, "refs/heads/main:refs/heads/main")
            .status
            .success()
    );

    client
        .reference(
            "refs/heads/main",
            second,
            true,
            "advance client test branch",
        )
        .unwrap();
    let update_refspec = format!("refs/heads/main:{protected_ref}");
    let updated = push(&client_path, &server.url, &update_refspec);
    assert!(!updated.status.success());

    let deleted = push(&client_path, &server.url, &format!(":{protected_ref}"));
    assert!(!deleted.status.success());

    let remote = Repository::open_bare(&server_repo).unwrap();
    assert_eq!(
        remote.find_reference(&protected_ref).unwrap().target(),
        Some(first)
    );
}

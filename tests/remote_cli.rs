//! Remote CLI process tests: configuration survives process restarts and runtime
//! credentials never enter repository configuration.

use std::fs;
use std::path::Path;
use std::process::Command;

use kat::domain::identity::{RepositoryRevisionId, SemanticStateId};
use kat::domain::revision::RepositoryRevision;
use kat::domain::workspace::WorkspaceBackend;
use kat::encoding::canonical_bytes;
use kat::encoding::object::{CanonicalObject, CanonicalPayload};
use kat::repository::init::init_repository;
use kat::repository::object_store::ObjectStore;
use kat::repository::ref_store::FileRefStore;
use kat::repository::workspace::git::GitWorkspaceBackend;
use kat::repository::workspace::git_transport::WorkspaceBackendId;
use kat::repository::workspace::init_workspace;

const RUNTIME_TOKEN: &str = "test-runtime-token-must-not-be-persisted-000000";

fn prepare_repository(root: &Path) -> WorkspaceBackendId {
    let initialized = init_repository(root).unwrap();
    let backend_id = WorkspaceBackendId::new();
    let backend = GitWorkspaceBackend::init(root).unwrap();
    let snapshot = backend.create_snapshot(&[]).unwrap();
    let store = ObjectStore::new(root.join(".kat"));
    let revision = RepositoryRevision {
        parents: vec![],
        semantic_state: SemanticStateId::from_object_id(initialized.state),
        workspace_snapshot: snapshot,
        semantic_change: None,
    };
    let revision_id = RepositoryRevisionId::from_object_id(
        store
            .put(
                &canonical_bytes(&CanonicalObject {
                    payload: CanonicalPayload::RepositoryRevision(revision),
                })
                .unwrap(),
            )
            .unwrap(),
    );
    FileRefStore::new(root.join(".kat"))
        .compare_and_swap_ref("local/main", None, revision_id)
        .unwrap();
    init_workspace(root, revision_id).unwrap();
    backend_id
}

fn run_kat(dir: &Path, args: &[&str], token: Option<&str>) -> (String, String, bool) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kat"));
    command
        .args(args)
        .current_dir(dir)
        .env_remove("KAT_REMOTE_CA_FILE");
    if let Some(token) = token {
        command.env("KAT_REMOTE_TOKEN", token);
    } else {
        command.env_remove("KAT_REMOTE_TOKEN");
    }
    let output = command.output().expect("kat CLI process starts");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.success(),
    )
}

#[test]
fn remote_configuration_persists_without_credentials_across_cli_processes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let backend_id = prepare_repository(root);
    let repository_id = open_repository_id(root);
    let hub_url = "https://hub.example.test";
    let git_url = format!("https://git.example.test/v1/repositories/{repository_id}/git");

    let (out, err, ok) = run_kat(
        root,
        &[
            "remote",
            "add",
            "origin",
            "--hub-url",
            hub_url,
            "--git-url",
            &git_url,
            "--workspace-backend-id",
            &backend_id.to_string(),
            "--json",
        ],
        Some(RUNTIME_TOKEN),
    );
    assert!(ok, "remote add failed: {err}\n{out}");
    let added: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(added["interface_schema_version"], 1);
    assert_eq!(added["data"]["name"], "origin");

    let remote_config = fs::read_to_string(root.join(".kat/remotes/origin.json")).unwrap();
    let workspace_config =
        fs::read_to_string(root.join(".kat/physical/workspace-remote.json")).unwrap();
    assert!(!remote_config.contains(RUNTIME_TOKEN));
    assert!(!workspace_config.contains(RUNTIME_TOKEN));
    assert!(remote_config.contains(hub_url));
    assert!(remote_config.contains(&repository_id.to_string()));
    assert!(remote_config.contains(&backend_id.to_string()));

    // A fresh process reads the saved config and still does not need a token.
    let (out, err, ok) = run_kat(root, &["remote", "list", "--json"], None);
    assert!(ok, "remote list failed: {err}\n{out}");
    let listed: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(listed["data"][0]["name"], "origin");
    assert_eq!(listed["data"][0]["hub_url"], hub_url);
    assert_eq!(listed["data"][0]["git_url"], git_url);
}

#[test]
fn remote_workflows_fail_closed_without_runtime_credentials_and_preserve_local_state() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let backend_id = prepare_repository(root);
    let repository_id = open_repository_id(root);
    let git_url = format!("https://git.example.test/v1/repositories/{repository_id}/git");
    let (_, err, ok) = run_kat(
        root,
        &[
            "remote",
            "add",
            "origin",
            "--hub-url",
            "https://hub.example.test",
            "--git-url",
            &git_url,
            "--workspace-backend-id",
            &backend_id.to_string(),
        ],
        None,
    );
    assert!(ok, "remote add failed: {err}");

    let accepted_before = fs::read(root.join(".kat/refs/accepted")).unwrap();
    let workspace_before = fs::read_dir(root.join(".kat/workspaces"))
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            fs::read(entry.path().join("state.json")).unwrap()
        })
        .collect::<Vec<_>>();
    let active_lineage_before = fs::read(root.join(".kat/physical/active_lineage")).unwrap();

    for args in [
        vec!["fetch", "origin", "--json"],
        vec!["push", "origin", "main", "--json"],
        vec!["sync", "origin", "--json"],
    ] {
        let (out, err, ok) = run_kat(root, &args, None);
        assert!(!ok, "command unexpectedly succeeded: {args:?}\n{out}");
        assert!(err.is_empty(), "JSON errors must stay on stdout: {err}");
        let result: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(result["success"], false, "{out}");
        assert_eq!(result["error"]["code"], "REMOTE_CREDENTIAL_MISSING");
        assert!(!out.contains(RUNTIME_TOKEN));
    }

    assert_eq!(
        fs::read(root.join(".kat/refs/accepted")).unwrap(),
        accepted_before
    );
    assert_eq!(
        fs::read_dir(root.join(".kat/workspaces"))
            .unwrap()
            .map(|entry| fs::read(entry.unwrap().path().join("state.json")).unwrap())
            .collect::<Vec<_>>(),
        workspace_before
    );
    assert_eq!(
        fs::read(root.join(".kat/physical/active_lineage")).unwrap(),
        active_lineage_before
    );
    assert!(
        FileRefStore::new(root.join(".kat"))
            .list_remote_refs()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn invalid_remote_urls_are_rejected_before_binding_configuration_is_written() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let backend_id = prepare_repository(root);
    let repository_id = open_repository_id(root);
    let git_url = format!("https://git.example.test/v1/repositories/{repository_id}/git");
    let (out, _, ok) = run_kat(
        root,
        &[
            "remote",
            "add",
            "origin",
            "--hub-url",
            "https://secret@hub.example.test?token=private",
            "--git-url",
            &git_url,
            "--workspace-backend-id",
            &backend_id.to_string(),
            "--json",
        ],
        None,
    );
    assert!(!ok);
    let result: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(result["success"], false);
    assert!(!root.join(".kat/remotes/origin.json").exists());
    assert!(!root.join(".kat/physical/workspace-remote.json").exists());
    assert!(!out.contains("private"));
}

fn open_repository_id(root: &Path) -> kat::domain::identity::RepositoryId {
    kat::repository::open::open_repository(root)
        .unwrap()
        .metadata
        .repository_id
}

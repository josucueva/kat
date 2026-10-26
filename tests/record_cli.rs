use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use kat::domain::identity::{RepositoryRevisionId, SemanticStateId, WorkspaceSnapshotId};
use kat::domain::revision::RepositoryRevision;
use kat::domain::workspace::WorkspaceBackend;
use kat::encoding::cbor::canonical_bytes;
use kat::encoding::hash::{hash_file_materialization, hash_workspace_snapshot, object_id};
use kat::encoding::object::{CanonicalObject, CanonicalPayload};
use kat::repository::open::open_repository;
use kat::repository::query::{
    AccountabilityContext, SemanticAccountability, analyze_artifact_accountability,
};
use kat::repository::ref_store::FileRefStore;
use kat::repository::workspace::git::{GitWorkspaceBackend, collect_git_tree};
use kat::repository::workspace::state::read_workspace_state;

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kat"))
        .args(args)
        .current_dir(root)
        .env_remove("KAT_REMOTE_TOKEN")
        .env_remove("KAT_REMOTE_CA_FILE")
        .output()
        .expect("kat process starts")
}

fn success(root: &Path, args: &[&str]) -> String {
    let output = run(root, args);
    assert!(
        output.status.success(),
        "kat {args:?} failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn failure(root: &Path, args: &[&str]) -> String {
    let output = run(root, args);
    assert!(
        !output.status.success(),
        "kat {args:?} unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn object_line(output: &str, key: &str) -> String {
    output
        .lines()
        .find_map(|line| {
            let (name, value) = line.trim().split_once(':')?;
            (name == key).then(|| value.trim().to_owned())
        })
        .unwrap_or_else(|| panic!("missing {key} in output:\n{output}"))
}

fn snapshot_paths(root: &Path, snapshot: &WorkspaceSnapshotId) -> BTreeSet<String> {
    let backend = GitWorkspaceBackend::open(root).unwrap();
    let reps = GitWorkspaceBackend::get_snapshot_representations(root, snapshot).unwrap();
    let git = git2::Repository::open(root.join(".kat/physical/git")).unwrap();
    let commit = git.find_commit(reps[0].commit).unwrap();
    let tree = commit.tree().unwrap();
    let mut entries = Vec::new();
    collect_git_tree(&git, &tree, Path::new(""), &mut entries).unwrap();
    assert!(backend.verify_snapshot_integrity(snapshot).unwrap());
    entries.into_iter().map(|entry| entry.0).collect()
}

fn revision(root: &Path, id: RepositoryRevisionId) -> RepositoryRevision {
    open_repository(root).unwrap().read_revision(id).unwrap()
}

#[test]
fn record_creates_semantic_physical_and_combined_revisions() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn value() -> u8 { 1 }\n").unwrap();
    success(root, &["init"]);

    let initial_workspace = read_workspace_state(root).unwrap();
    let initial_revision = revision(root, initial_workspace.base_revision);
    let refs = FileRefStore::new(root.join(".kat"));
    refs.compare_and_swap_ref("local/main", None, initial_workspace.base_revision)
        .unwrap();

    let create = success(
        root,
        &["create", "requirement", "--title", "Value is stable"],
    );
    let requirement = object_line(&create, "element_id");
    let before_record = open_repository(root).unwrap();
    let accepted_after_create = before_record.accepted.clone();
    let status = success(root, &["status"]);
    assert!(status.contains("accepted semantic revision: unrecorded"));

    let remote_add = success(
        root,
        &[
            "remote",
            "add",
            "origin",
            "--hub-url",
            "https://hub.example.test",
            "--git-url",
            "https://git.example.test/repository",
            "--workspace-backend-id",
            "00000000-0000-4000-8000-000000000001",
        ],
    );
    assert!(remote_add.contains("origin"));
    let push = Command::new(env!("CARGO_BIN_EXE_kat"))
        .args(["push", "origin", "main"])
        .current_dir(root)
        .env(
            "KAT_REMOTE_TOKEN",
            "test-runtime-token-must-not-be-persisted-000000",
        )
        .output()
        .unwrap();
    assert!(!push.status.success());
    assert!(
        String::from_utf8_lossy(&push.stderr).contains("finalize a repository revision"),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&push.stdout),
        String::from_utf8_lossy(&push.stderr)
    );
    assert_eq!(read_workspace_state(root).unwrap(), initial_workspace);
    assert_eq!(
        open_repository(root).unwrap().accepted,
        accepted_after_create
    );

    let semantic = success(root, &["record", "--json"]);
    let semantic_json: serde_json::Value = serde_json::from_str(&semantic).unwrap();
    let semantic_id: RepositoryRevisionId = semantic_json["data"]["revision_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let semantic_revision = revision(root, semantic_id);
    assert_eq!(
        semantic_revision.parents,
        vec![initial_workspace.base_revision]
    );
    assert_eq!(
        semantic_revision.semantic_state.as_object_id(),
        accepted_after_create.state
    );
    assert_eq!(
        semantic_revision.workspace_snapshot,
        initial_revision.workspace_snapshot
    );
    assert_eq!(
        semantic_revision.semantic_change,
        accepted_after_create
            .change
            .map(kat::domain::identity::ChangeRevisionId::from_object_id)
    );
    assert_eq!(
        refs.read_ref("local/main").unwrap(),
        initial_workspace.base_revision
    );
    assert_eq!(
        read_workspace_state(root).unwrap().base_revision,
        semantic_id
    );

    fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(root.join("ignored.txt"), "must stay outside the snapshot").unwrap();
    fs::write(root.join("src/new.rs"), "pub fn added() {}\n").unwrap();
    let physical = success(root, &["record", "src/new.rs"]);
    let physical_id: RepositoryRevisionId = object_line(&physical, "revision_id").parse().unwrap();
    let physical_revision = revision(root, physical_id);
    assert_eq!(physical_revision.parents, vec![semantic_id]);
    assert_eq!(
        physical_revision.semantic_state,
        semantic_revision.semantic_state
    );
    assert_eq!(physical_revision.semantic_change, None);
    assert_ne!(
        physical_revision.workspace_snapshot,
        semantic_revision.workspace_snapshot
    );
    let paths = snapshot_paths(root, &physical_revision.workspace_snapshot);
    assert!(paths.contains("src/lib.rs"));
    assert!(paths.contains("src/new.rs"));
    assert!(!paths.contains("ignored.txt"));
    assert!(!paths.contains(".gitignore"));

    success(
        root,
        &["update", &requirement, "--title", "Value remains stable"],
    );
    success(
        root,
        &[
            "update",
            &requirement,
            "--description",
            "Recorded after two Changes",
        ],
    );
    fs::write(root.join("src/lib.rs"), "pub fn value() -> u8 { 2 }\n").unwrap();
    let accepted_before_combined = open_repository(root).unwrap().accepted.clone();
    let combined = success(root, &["record", "src/lib.rs"]);
    let combined_id: RepositoryRevisionId = object_line(&combined, "revision_id").parse().unwrap();
    let combined_revision = revision(root, combined_id);
    assert_eq!(combined_revision.parents, vec![physical_id]);
    assert_eq!(
        combined_revision.semantic_state.as_object_id(),
        accepted_before_combined.state
    );
    assert_eq!(
        combined_revision.semantic_change,
        accepted_before_combined
            .change
            .map(kat::domain::identity::ChangeRevisionId::from_object_id)
    );
    assert_ne!(
        combined_revision.workspace_snapshot,
        physical_revision.workspace_snapshot
    );
    assert_eq!(
        refs.read_ref("local/main").unwrap(),
        initial_workspace.base_revision
    );
    assert_eq!(
        snapshot_paths(root, &combined_revision.workspace_snapshot),
        ["src/lib.rs", "src/new.rs"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    );
    assert!(success(root, &["history", "--oneline"]).lines().count() >= 3);
    assert!(failure(root, &["record"]).contains("no semantic or tracked physical changes"));
}

#[test]
fn record_rejects_invalid_paths_and_captures_tracked_deletions() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("tracked.txt"), "base\n").unwrap();
    success(root, &["init"]);
    let workspace = read_workspace_state(root).unwrap();
    let accepted = open_repository(root).unwrap().accepted;
    let lineage = GitWorkspaceBackend::read_active_lineage_for_test(root);

    fs::create_dir(root.join("folder")).unwrap();
    fs::write(root.join("folder/child.txt"), "not expanded\n").unwrap();
    fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(root.join("ignored.txt"), "ignored\n").unwrap();
    fs::write(root.join("new.txt"), "new\n").unwrap();

    for path in [
        "ignored.txt",
        "folder",
        "missing.txt",
        "../outside.txt",
        ".kat/repository.toml",
    ] {
        let error = failure(root, &["record", path]);
        assert!(
            error.contains("RECORD_INVALID_PATH") || error.contains("Invalid record path"),
            "{error}"
        );
        assert_eq!(read_workspace_state(root).unwrap(), workspace);
        assert_eq!(open_repository(root).unwrap().accepted, accepted);
        assert_eq!(
            GitWorkspaceBackend::read_active_lineage_for_test(root),
            lineage
        );
    }

    let no_changes = run(root, &["record", "--json"]);
    assert!(!no_changes.status.success());
    let no_changes_json: serde_json::Value = serde_json::from_slice(&no_changes.stdout).unwrap();
    assert_eq!(no_changes_json["error"]["code"], "RECORD_NO_CHANGES");

    let invalid_path = run(root, &["record", "ignored.txt", "--json"]);
    assert!(!invalid_path.status.success());
    let invalid_path_json: serde_json::Value =
        serde_json::from_slice(&invalid_path.stdout).unwrap();
    assert_eq!(invalid_path_json["error"]["code"], "RECORD_INVALID_PATH");

    assert!(failure(root, &["record"]).contains("no semantic or tracked physical changes"));
    let physical = success(root, &["record", "new.txt"]);
    let id: RepositoryRevisionId = object_line(&physical, "revision_id").parse().unwrap();
    let recorded = revision(root, id);
    let paths = snapshot_paths(root, &recorded.workspace_snapshot);
    assert!(paths.contains("tracked.txt"));
    assert!(paths.contains("new.txt"));
    assert!(!paths.contains("folder/child.txt"));
    assert!(!paths.contains("ignored.txt"));

    fs::remove_file(root.join("tracked.txt")).unwrap();
    let deletion = success(root, &["record"]);
    let deletion_id: RepositoryRevisionId = object_line(&deletion, "revision_id").parse().unwrap();
    let deletion_revision = revision(root, deletion_id);
    assert!(!snapshot_paths(root, &deletion_revision.workspace_snapshot).contains("tracked.txt"));
    assert!(snapshot_paths(root, &deletion_revision.workspace_snapshot).contains("new.txt"));
}

#[test]
fn record_failure_after_snapshot_persistence_preserves_authority_and_retry_recovers() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("base.txt"), "base\n").unwrap();
    success(root, &["init"]);
    let workspace = read_workspace_state(root).unwrap();
    let repository = open_repository(root).unwrap();
    let accepted = repository.accepted.clone();
    let refs = repository.ref_store();
    refs.compare_and_swap_ref("local/main", None, workspace.base_revision)
        .unwrap();
    let lineage = GitWorkspaceBackend::read_active_lineage_for_test(root);

    fs::write(root.join("new.txt"), "new physical state\n").unwrap();
    let candidate_snapshot = hash_workspace_snapshot(&[
        (
            "base.txt".into(),
            b'F',
            hash_file_materialization(false, b"base\n"),
        ),
        (
            "new.txt".into(),
            b'F',
            hash_file_materialization(false, b"new physical state\n"),
        ),
    ]);
    let candidate_revision = RepositoryRevision {
        parents: vec![workspace.base_revision],
        semantic_state: SemanticStateId::from_object_id(accepted.state),
        workspace_snapshot: candidate_snapshot.clone(),
        semantic_change: None,
    };
    let candidate_bytes = canonical_bytes(&CanonicalObject {
        payload: CanonicalPayload::RepositoryRevision(candidate_revision),
    })
    .unwrap();
    let candidate_id = object_id(&candidate_bytes);
    fs::write(
        root.join(".kat/objects").join(candidate_id.to_string()),
        b"corrupt destination",
    )
    .unwrap();

    let error = failure(root, &["record", "new.txt"]);
    assert!(error.contains("integrity mismatch"), "{error}");
    assert_eq!(read_workspace_state(root).unwrap(), workspace);
    assert_eq!(open_repository(root).unwrap().accepted, accepted);
    assert_eq!(
        FileRefStore::new(root.join(".kat"))
            .read_ref("local/main")
            .unwrap(),
        workspace.base_revision
    );
    assert_eq!(
        GitWorkspaceBackend::read_active_lineage_for_test(root),
        lineage
    );
    assert_eq!(fs::read(root.join("base.txt")).unwrap(), b"base\n");
    assert_eq!(
        fs::read(root.join("new.txt")).unwrap(),
        b"new physical state\n"
    );
    assert!(!root.join(".kat/work/record/pending.json").exists());

    fs::remove_file(root.join(".kat/objects").join(candidate_id.to_string())).unwrap();
    let retried = success(root, &["record", "new.txt"]);
    let revision_id: RepositoryRevisionId = object_line(&retried, "revision_id").parse().unwrap();
    let recorded = revision(root, revision_id);
    assert_eq!(recorded.workspace_snapshot, candidate_snapshot);
    assert_eq!(recorded.parents, vec![workspace.base_revision]);
    assert_eq!(recorded.semantic_change, None);
    assert_eq!(
        FileRefStore::new(root.join(".kat"))
            .read_ref("local/main")
            .unwrap(),
        workspace.base_revision
    );
    assert_ne!(
        GitWorkspaceBackend::read_active_lineage_for_test(root),
        lineage
    );
}

#[cfg(unix)]
#[test]
fn record_captures_symlink_without_following_its_target() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("target.txt"), "target\n").unwrap();
    success(root, &["init"]);
    symlink("target.txt", root.join("alias.txt")).unwrap();
    let output = success(root, &["record", "alias.txt"]);
    let revision_id: RepositoryRevisionId = object_line(&output, "revision_id").parse().unwrap();
    let recorded = revision(root, revision_id);
    let reps =
        GitWorkspaceBackend::get_snapshot_representations(root, &recorded.workspace_snapshot)
            .unwrap();
    let git = git2::Repository::open(root.join(".kat/physical/git")).unwrap();
    let tree = git.find_commit(reps[0].commit).unwrap().tree().unwrap();
    assert_eq!(
        tree.get_path(Path::new("alias.txt")).unwrap().filemode(),
        0o120000
    );
}

#[cfg(unix)]
#[test]
fn record_rejects_a_tracked_file_reached_through_a_symlinked_parent() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/tracked.txt"), "workspace version\n").unwrap();
    success(root, &["init"]);
    let workspace = read_workspace_state(root).unwrap();
    let lineage = GitWorkspaceBackend::read_active_lineage_for_test(root);

    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("tracked.txt"), "outside secret\n").unwrap();
    fs::remove_dir_all(root.join("src")).unwrap();
    symlink(&outside, root.join("src")).unwrap();

    let error = failure(root, &["record"]);
    assert!(error.contains("parent components must be workspace directories"));
    assert_eq!(read_workspace_state(root).unwrap(), workspace);
    assert_eq!(
        GitWorkspaceBackend::read_active_lineage_for_test(root),
        lineage
    );
    assert_eq!(
        fs::read(root.join("src/tracked.txt")).unwrap(),
        b"outside secret\n"
    );
}

#[test]
fn recorded_revisions_keep_their_own_artifact_accountability_history() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    success(root, &["init"]);

    let requirement = success(
        root,
        &["create", "requirement", "--title", "Stable behavior"],
    );
    let requirement_id = object_line(&requirement, "element_id");
    let implementation = success(root, &["create", "implementation", "--title", "Handler v1"]);
    let implementation_id = object_line(&implementation, "element_id");
    let artifact = success(root, &["create", "artifact", "--title", "handler.ts"]);
    let artifact_id = object_line(&artifact, "element_id");
    success(
        root,
        &["link", "realizes", &implementation_id, &requirement_id],
    );
    success(
        root,
        &["link", "represents", &artifact_id, &implementation_id],
    );
    let initial = success(root, &["record"]);
    let initial_id: RepositoryRevisionId = object_line(&initial, "revision_id").parse().unwrap();

    success(
        root,
        &["update", &implementation_id, "--title", "Handler v2"],
    );
    assert!(failure(root, &["artifacts"]).contains("stale"));
    let stale = success(root, &["record"]);
    let stale_id: RepositoryRevisionId = object_line(&stale, "revision_id").parse().unwrap();

    success(
        root,
        &["change", "begin", "--description", "Update and re-account"],
    );
    success(
        root,
        &["update", &implementation_id, "--title", "Handler v3"],
    );
    success(root, &["account", &artifact_id]);
    success(root, &["commit"]);
    assert!(success(root, &["artifacts"]).contains("current"));
    let accounted = success(root, &["record"]);
    let accounted_id: RepositoryRevisionId =
        object_line(&accounted, "revision_id").parse().unwrap();

    let repository = open_repository(root).unwrap();
    let backend = GitWorkspaceBackend::open(root).unwrap();
    let report_at = |revision_id| {
        let revision = repository.read_revision(revision_id).unwrap();
        analyze_artifact_accountability(
            &repository,
            Some(AccountabilityContext::Revision(&backend, &revision)),
        )
        .unwrap()
        .artifacts[0]
            .semantic
    };
    assert_eq!(
        report_at(initial_id),
        SemanticAccountability::Current,
        "a later re-account must not alter the initial revision's baseline"
    );
    assert_eq!(report_at(stale_id), SemanticAccountability::Stale);
    assert_eq!(report_at(accounted_id), SemanticAccountability::Current);
}

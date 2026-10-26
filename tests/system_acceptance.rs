use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use kat::domain::identity::RepositoryRevisionId;
use kat::domain::revision::RepositoryRevision;
use kat::repository::open::open_repository;
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

fn revision(root: &Path, id: RepositoryRevisionId) -> RepositoryRevision {
    open_repository(root).unwrap().read_revision(id).unwrap()
}

fn tree_entry(
    root: &Path,
    snapshot: &kat::domain::identity::WorkspaceSnapshotId,
    path: &str,
) -> (u32, Vec<u8>) {
    let representations =
        GitWorkspaceBackend::get_snapshot_representations(root, snapshot).unwrap();
    let git = git2::Repository::open(root.join(".kat/physical/git")).unwrap();
    let commit = git.find_commit(representations[0].commit).unwrap();
    let tree = commit.tree().unwrap();
    let entry = tree.get_path(Path::new(path)).unwrap();
    let blob = git.find_blob(entry.id()).unwrap();
    (entry.filemode() as u32, blob.content().to_vec())
}

fn snapshot_paths(
    root: &Path,
    snapshot: &kat::domain::identity::WorkspaceSnapshotId,
) -> BTreeSet<String> {
    let representations =
        GitWorkspaceBackend::get_snapshot_representations(root, snapshot).unwrap();
    let git = git2::Repository::open(root.join(".kat/physical/git")).unwrap();
    let commit = git.find_commit(representations[0].commit).unwrap();
    let tree = commit.tree().unwrap();
    let mut entries = Vec::new();
    collect_git_tree(&git, &tree, Path::new(""), &mut entries).unwrap();
    entries.into_iter().map(|(path, _, _)| path).collect()
}

fn object_names(root: &Path) -> BTreeSet<String> {
    fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn sys_01_build_and_reopen_through_separate_cli_processes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/main.rs"),
        "fn main() { println!(\"v1\"); }\n",
    )
    .unwrap();
    fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(root.join("ignored.txt"), "excluded\n").unwrap();

    success(root, &["init"]);
    let initial_workspace = read_workspace_state(root).unwrap();
    let initial_revision = revision(root, initial_workspace.base_revision);
    assert!(initial_revision.parents.is_empty());
    assert_eq!(
        tree_entry(root, &initial_revision.workspace_snapshot, "src/main.rs").1,
        b"fn main() { println!(\"v1\"); }\n"
    );
    let initial_paths = snapshot_paths(root, &initial_revision.workspace_snapshot);
    assert!(initial_paths.contains("src/main.rs"));
    assert!(!initial_paths.contains("ignored.txt"));
    assert!(!initial_paths.iter().any(|path| path.starts_with(".kat/")));
    assert!(
        GitWorkspaceBackend::get_snapshot_representations(
            root,
            &initial_revision.workspace_snapshot
        )
        .is_ok()
    );
    let local_refs = FileRefStore::new(root.join(".kat"));
    local_refs
        .compare_and_swap_ref("local/main", None, initial_workspace.base_revision)
        .unwrap();

    success(
        root,
        &["change", "begin", "--description", "Initial project model"],
    );
    let intent = object_line(
        &success(root, &["create", "intent", "--title", "Keep values stable"]),
        "element_id",
    );
    let requirement = object_line(
        &success(
            root,
            &["create", "requirement", "--title", "Values are stable"],
        ),
        "element_id",
    );
    let obsolete_decision = object_line(
        &success(
            root,
            &["create", "design-decision", "--title", "Legacy decision"],
        ),
        "element_id",
    );
    let constraint = object_line(
        &success(
            root,
            &["create", "constraint", "--title", "Preserve input order"],
        ),
        "element_id",
    );
    let decision = object_line(
        &success(
            root,
            &["create", "design-decision", "--title", "Use stable sorting"],
        ),
        "element_id",
    );
    let implementation = object_line(
        &success(
            root,
            &["create", "implementation", "--title", "Value handler v1"],
        ),
        "element_id",
    );
    let artifact = object_line(
        &success(
            root,
            &[
                "create",
                "artifact",
                "--title",
                "main source",
                "--locator",
                "src/main.rs",
            ],
        ),
        "element_id",
    );
    let validation = object_line(
        &success(
            root,
            &["create", "validation", "--title", "Value ordering test"],
        ),
        "element_id",
    );
    success(root, &["link", "motivates", &intent, &requirement]);
    success(root, &["link", "addresses", &decision, &requirement]);
    success(root, &["link", "restricts", &constraint, &decision]);
    success(root, &["link", "realizes", &implementation, &requirement]);
    let validation_link = success(root, &["link", "validates", &validation, &requirement]);
    let validation_relationship = object_line(&validation_link, "relationship_id");
    success(root, &["link", "represents", &artifact, &implementation]);
    success(root, &["commit"]);

    let status = success(root, &["status"]);
    assert!(status.contains("accepted semantic revision: unrecorded"));
    success(
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
    let push = Command::new(env!("CARGO_BIN_EXE_kat"))
        .args(["push", "origin", "main"])
        .current_dir(root)
        .env(
            "KAT_REMOTE_TOKEN",
            "runtime-token-that-must-not-be-stored-000001",
        )
        .env_remove("KAT_REMOTE_CA_FILE")
        .output()
        .unwrap();
    assert!(!push.status.success());
    assert!(String::from_utf8_lossy(&push.stderr).contains("finalize a repository revision"));
    assert_eq!(read_workspace_state(root).unwrap(), initial_workspace);
    assert_eq!(
        local_refs.read_ref("local/main").unwrap(),
        initial_workspace.base_revision
    );

    success(root, &["check"]);
    success(root, &["validate"]);
    let semantic_record = success(root, &["record", "--json"]);
    let semantic_json: serde_json::Value = serde_json::from_str(&semantic_record).unwrap();
    let semantic_revision_id: RepositoryRevisionId = semantic_json["data"]["revision_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let semantic_revision = revision(root, semantic_revision_id);
    assert_eq!(
        semantic_revision.parents,
        vec![initial_workspace.base_revision]
    );
    assert_eq!(
        semantic_revision.workspace_snapshot,
        initial_revision.workspace_snapshot
    );
    assert_eq!(
        semantic_revision.semantic_state.as_object_id(),
        open_repository(root).unwrap().accepted.state
    );
    assert_ne!(semantic_revision.semantic_change, None);
    assert_eq!(
        semantic_revision
            .semantic_change
            .map(|id| id.as_object_id()),
        open_repository(root).unwrap().accepted.change
    );
    assert_eq!(
        local_refs.read_ref("local/main").unwrap(),
        initial_workspace.base_revision,
        "record must not move named references"
    );
    success(
        root,
        &["update", &requirement, "--title", "Values remain stable"],
    );
    let supersede = success(
        root,
        &[
            "supersede",
            &obsolete_decision,
            "design-decision",
            "--title",
            "Current requirement",
        ],
    );
    assert!(supersede.contains("replacement_element_id:"));
    success(root, &["unlink", &validation_relationship]);
    success(root, &["deprecate", &validation]);
    success(
        root,
        &["update", &implementation, "--title", "Value handler v2"],
    );
    assert!(failure(root, &["artifacts"]).contains("stale"));
    success(root, &["account", &artifact]);
    assert!(success(root, &["artifacts"]).contains("current"));
    let semantic_only = success(root, &["record"]);
    let semantic_only_id: RepositoryRevisionId =
        object_line(&semantic_only, "revision_id").parse().unwrap();
    let semantic_only_revision = revision(root, semantic_only_id);
    assert_eq!(semantic_only_revision.parents, vec![semantic_revision_id]);
    assert_eq!(
        semantic_only_revision.workspace_snapshot,
        initial_revision.workspace_snapshot
    );
    assert!(success(root, &["history", "--oneline"]).lines().count() >= 7);

    fs::create_dir_all(root.join("scripts")).unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn value() -> u8 { 2 }\n").unwrap();
    fs::write(root.join("scripts/check.sh"), "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(root.join("scripts/check.sh"))
            .unwrap()
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(root.join("scripts/check.sh"), permissions).unwrap();
    }
    let physical = success(root, &["record", "src/lib.rs", "scripts/check.sh"]);
    let physical_id: RepositoryRevisionId = object_line(&physical, "revision_id").parse().unwrap();
    let physical_revision = revision(root, physical_id);
    assert_eq!(physical_revision.parents, vec![semantic_only_id]);
    assert_eq!(
        physical_revision.semantic_state,
        semantic_only_revision.semantic_state
    );
    assert_eq!(physical_revision.semantic_change, None);
    assert_eq!(
        tree_entry(root, &physical_revision.workspace_snapshot, "src/lib.rs").1,
        b"pub fn value() -> u8 { 2 }\n"
    );
    #[cfg(unix)]
    assert_eq!(
        tree_entry(
            root,
            &physical_revision.workspace_snapshot,
            "scripts/check.sh"
        )
        .0,
        0o100755
    );
    let physical_paths = snapshot_paths(root, &physical_revision.workspace_snapshot);
    assert!(!physical_paths.contains("ignored.txt"));
    assert_eq!(
        local_refs.read_ref("local/main").unwrap(),
        initial_workspace.base_revision
    );

    success(
        root,
        &[
            "change",
            "begin",
            "--description",
            "Update implementation and source",
        ],
    );
    success(
        root,
        &["update", &implementation, "--title", "Value handler v3"],
    );
    fs::write(
        root.join("src/main.rs"),
        "fn main() { println!(\"v3\"); }\n",
    )
    .unwrap();
    success(root, &["account", &artifact]);
    success(root, &["check"]);
    success(root, &["validate"]);
    success(root, &["commit"]);
    assert!(success(root, &["status"]).contains("accepted semantic revision: unrecorded"));
    let combined = success(root, &["record", "src/main.rs"]);
    let combined_id: RepositoryRevisionId = object_line(&combined, "revision_id").parse().unwrap();
    let combined_revision = revision(root, combined_id);
    assert_eq!(combined_revision.parents, vec![physical_id]);
    assert_ne!(
        combined_revision.semantic_state,
        physical_revision.semantic_state
    );
    assert_ne!(
        combined_revision.workspace_snapshot,
        physical_revision.workspace_snapshot
    );
    assert_ne!(combined_revision.semantic_change, None);
    assert_eq!(
        tree_entry(root, &combined_revision.workspace_snapshot, "src/main.rs").1,
        b"fn main() { println!(\"v3\"); }\n"
    );
    assert_eq!(
        local_refs.read_ref("local/main").unwrap(),
        initial_workspace.base_revision
    );

    let repository = open_repository(root).unwrap();
    let accepted_before_queries = repository.ref_store().read_accepted().unwrap();
    let workspace_before_queries = read_workspace_state(root).unwrap();
    let objects_before_queries = object_names(root);
    let lineage_before_queries = GitWorkspaceBackend::read_active_lineage_for_test(root);
    assert!(success(root, &["status"]).contains("recorded"));
    assert!(success(root, &["show", &implementation]).contains("Value handler v3"));
    assert!(success(root, &["history", "--oneline"]).lines().count() >= 8);
    assert!(success(root, &["trace", &artifact]).contains("Value handler v3"));
    assert!(success(root, &["impact", &requirement]).contains("Values remain stable"));
    assert!(success(root, &["artifacts"]).contains("current"));
    success(root, &["validate"]);
    let reopened = open_repository(root).unwrap();
    assert_eq!(
        reopened.ref_store().read_accepted().unwrap(),
        accepted_before_queries
    );
    assert_eq!(
        read_workspace_state(root).unwrap(),
        workspace_before_queries
    );
    assert_eq!(object_names(root), objects_before_queries);
    assert_eq!(
        GitWorkspaceBackend::read_active_lineage_for_test(root),
        lineage_before_queries
    );
    assert_eq!(
        read_workspace_state(root).unwrap().base_revision,
        combined_id,
        "the final complete revision survives reopening"
    );
    assert_eq!(
        local_refs.read_ref("local/main").unwrap(),
        initial_workspace.base_revision
    );
    assert_eq!(
        tree_entry(root, &combined_revision.workspace_snapshot, "src/main.rs").1,
        fs::read(root.join("src/main.rs")).unwrap()
    );
}

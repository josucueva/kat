use std::path::PathBuf;
use std::process::{Command, Output};

use kat::domain::element::{KnowledgeElementVersion, Lifecycle};
use kat::domain::identity::{
    ElementId, ObjectId, RepositoryRevisionId, SemanticStateId, WorkspaceSnapshotId,
};
use kat::domain::property::PropertyValue;
use kat::domain::revision::RepositoryRevision;
use kat::domain::state::{ElementStateEntry, SemanticState};
use kat::domain::workspace::WorkspaceBackend;
use kat::encoding::cbor::canonical_bytes;
use kat::encoding::object::{CanonicalObject, CanonicalPayload};
use kat::repository::reconcile::{
    ReconciliationSession, ReconciliationSessionState, load_reconciliation_session,
};
use kat::repository::workspace::git::GitWorkspaceBackend;
use serde_json::Value;

struct Project {
    _temp: tempfile::TempDir,
    root: PathBuf,
    ontology: ObjectId,
}

impl Project {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        kat::repository::init::init_repository(&root).unwrap();
        GitWorkspaceBackend::init(&root).unwrap();
        let repo = kat::repository::open::open_repository(&root).unwrap();
        let bytes = repo.object_store().get(repo.accepted.state).unwrap();
        let ontology = match kat::encoding::decode_canonical(&bytes).unwrap().payload {
            CanonicalPayload::SemanticState(state) => state.ontology_version,
            _ => panic!("expected semantic state"),
        };
        Self {
            _temp: temp,
            root,
            ontology,
        }
    }

    fn snapshot(&self, content: Option<&str>) -> WorkspaceSnapshotId {
        let path = self.root.join("a.txt");
        let tracked = if let Some(content) = content {
            std::fs::write(path, content).unwrap();
            vec![PathBuf::from("a.txt")]
        } else {
            vec![]
        };
        GitWorkspaceBackend::open(&self.root)
            .unwrap()
            .create_snapshot(&tracked)
            .unwrap()
    }

    fn revision(
        &self,
        parents: Vec<RepositoryRevisionId>,
        elements: &[(u8, &str)],
        snapshot: WorkspaceSnapshotId,
    ) -> RepositoryRevisionId {
        let repo = kat::repository::open::open_repository(&self.root).unwrap();
        let store = repo.object_store();
        let mut entries = Vec::new();
        for (byte, title) in elements {
            let element_id = ElementId::from_uuid(uuid::Uuid::from_bytes([*byte; 16]));
            let element = KnowledgeElementVersion {
                element_id,
                type_id: "kat.core/requirement".into(),
                lifecycle: Lifecycle::Active,
                properties: vec![("title".into(), PropertyValue::Text((*title).into()))],
            };
            let bytes = canonical_bytes(&CanonicalObject {
                payload: CanonicalPayload::KnowledgeElementVersion(element),
            })
            .unwrap();
            entries.push(ElementStateEntry {
                element_id,
                version: store.put(&bytes).unwrap(),
            });
        }
        entries.sort_by_key(|entry| entry.element_id);
        let state = SemanticState {
            ontology_version: self.ontology,
            elements: entries,
            relationships: vec![],
        };
        let bytes = canonical_bytes(&CanonicalObject {
            payload: CanonicalPayload::SemanticState(state),
        })
        .unwrap();
        let semantic_state = SemanticStateId::from_object_id(store.put(&bytes).unwrap());
        let mut parents = parents;
        parents.sort();
        let bytes = canonical_bytes(&CanonicalObject {
            payload: CanonicalPayload::RepositoryRevision(RepositoryRevision {
                parents,
                semantic_state,
                semantic_change: None,
                workspace_snapshot: snapshot,
            }),
        })
        .unwrap();
        RepositoryRevisionId::from_object_id(store.put(&bytes).unwrap())
    }

    fn workspace(&self, revision: RepositoryRevisionId) {
        kat::repository::workspace::init_workspace(&self.root, revision).unwrap();
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_kat"))
            .current_dir(&self.root)
            .args(args)
            .output()
            .unwrap()
    }

    fn json(&self, args: &[&str], code: i32) -> Value {
        let output = self.run(args);
        assert_eq!(
            output.status.code(),
            Some(code),
            "{}: stdout={} stderr={}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
            panic!(
                "not one JSON envelope: {}",
                String::from_utf8_lossy(&output.stdout)
            )
        });
        assert_eq!(value["interface_schema_version"], 1);
        assert_eq!(value["success"], code == 0);
        assert_eq!(value["data"].is_null(), code != 0);
        assert_eq!(value["error"].is_null(), code == 0);
        value
    }

    fn session(&self) -> ReconciliationSession {
        let backend = GitWorkspaceBackend::open(&self.root).unwrap();
        let workspace = kat::repository::workspace::open_workspace(&self.root, &backend).unwrap();
        load_reconciliation_session(&self.root, &workspace.id, workspace.base_revision)
            .unwrap()
            .unwrap()
    }

    fn base(&self) -> RepositoryRevisionId {
        kat::repository::workspace::open_workspace(
            &self.root,
            &GitWorkspaceBackend::open(&self.root).unwrap(),
        )
        .unwrap()
        .base_revision
    }

    fn revision_data(&self, id: RepositoryRevisionId) -> RepositoryRevision {
        kat::repository::open::open_repository(&self.root)
            .unwrap()
            .read_revision(id)
            .unwrap()
    }

    fn assert_snapshot_file(&self, revision: RepositoryRevisionId, content: &str) {
        let snapshot = self.revision_data(revision).workspace_snapshot;
        let materialization = GitWorkspaceBackend::open(&self.root)
            .unwrap()
            .resolve_materialization(std::path::Path::new("a.txt"), &snapshot)
            .unwrap();
        assert_eq!(
            materialization,
            kat::domain::workspace::MaterializationResolution::File(
                kat::encoding::hash::hash_file_materialization(false, content.as_bytes()),
            )
        );
    }

    fn local_refs(&self) -> Vec<kat::domain::reference::NamedReference> {
        kat::repository::open::open_repository(&self.root)
            .unwrap()
            .ref_store()
            .list_local_refs()
            .unwrap()
    }

    fn session_path_exists(&self) -> bool {
        let backend = GitWorkspaceBackend::open(&self.root).unwrap();
        let workspace = kat::repository::workspace::open_workspace(&self.root, &backend).unwrap();
        kat::repository::reconcile::session_path(&self.root, &workspace.id).exists()
    }

    fn candidate_resources(&self) -> (PathBuf, String) {
        let session = self.session();
        let candidate = match &session.state {
            ReconciliationSessionState::Conflicted { candidate }
            | ReconciliationSessionState::ConflictedMaterialized { candidate } => candidate,
            _ => panic!("expected candidate"),
        };
        let id = candidate.physical_candidate.as_ref().unwrap().provisional;
        let path = self
            .root
            .join(".kat/workspaces")
            .join(&session.workspace_id.0)
            .join("physical-reconciliation")
            .join(id.to_string())
            .join("git.json");
        (path, format!("refs/kat/candidates/{id}"))
    }

    fn git_candidate_ref(&self, name: &str) -> bool {
        git2::Repository::open(self.root.join(".kat/physical/git"))
            .unwrap()
            .find_reference(name)
            .is_ok()
    }
}

fn physical_conflict_project() -> (Project, RepositoryRevisionId, RepositoryRevisionId) {
    let project = Project::new();
    let base_snapshot = project.snapshot(Some("base\n"));
    let local_snapshot = project.snapshot(Some("local\n"));
    let other_snapshot = project.snapshot(Some("other\n"));
    let base = project.revision(vec![], &[], base_snapshot);
    let local = project.revision(vec![base], &[], local_snapshot.clone());
    let other = project.revision(vec![base], &[], other_snapshot);
    project.workspace(local);
    GitWorkspaceBackend::open(&project.root)
        .unwrap()
        .materialize_snapshot(&local_snapshot)
        .unwrap();
    (project, local, other)
}

fn physical_conflict_project_with_semantics()
-> (Project, RepositoryRevisionId, RepositoryRevisionId) {
    let project = Project::new();
    let base_snapshot = project.snapshot(Some("base\n"));
    let local_snapshot = project.snapshot(Some("local\n"));
    let other_snapshot = project.snapshot(Some("other\n"));
    let base = project.revision(vec![], &[(1, "base")], base_snapshot);
    let local = project.revision(vec![base], &[(1, "local")], local_snapshot.clone());
    let other = project.revision(vec![base], &[(1, "other")], other_snapshot);
    project.workspace(local);
    GitWorkspaceBackend::open(&project.root)
        .unwrap()
        .materialize_snapshot(&local_snapshot)
        .unwrap();
    (project, local, other)
}

#[test]
fn status_and_usage_errors_follow_v1_envelope() {
    let project = Project::new();
    let first = project.json(&["status", "--json"], 0);
    let second = project.json(&["status", "--json"], 0);
    assert_eq!(first, second);
    assert!(first["data"]["workspace"].is_null());

    let usage = project.json(&["reconcile", "--json"], 2);
    assert_eq!(usage["error"]["code"], "INVALID_ARGUMENTS");
    let nested_usage = project.json(&["resolve", "semantic", "--json"], 2);
    assert_eq!(nested_usage["error"]["code"], "INVALID_ARGUMENTS");
    assert_eq!(
        project.json(&["--json", "reconcile"], 2)["error"]["code"],
        "INVALID_ARGUMENTS"
    );

    let human = project.run(&["reconcile"]);
    assert_eq!(human.status.code(), Some(2));
    assert!(human.stdout.is_empty());
}

#[test]
fn draft_abort_retains_existing_machine_payload() {
    let project = Project::new();
    assert!(project.run(&["change", "begin"]).status.success());
    let abort = project.json(&["abort", "--json"], 0);
    assert_eq!(
        abort["data"],
        serde_json::json!({ "aborted": true, "type": "draft" })
    );
}

#[test]
fn every_collaboration_command_uses_error_envelope_outside_repository() {
    let temp = tempfile::tempdir().unwrap();
    let commands: &[&[&str]] = &[
        &["status", "--json"],
        &[
            "reconcile",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "--json",
        ],
        &["conflicts", "--json"],
        &["materialize", "--json"],
        &["resolve", "semantic", "id", "--accept", "local", "--json"],
        &["abort", "--json"],
        &["finalize", "--json"],
        &[
            "advance",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "--json",
        ],
        &[
            "switch",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "--json",
        ],
    ];
    for args in commands {
        let output = Command::new(env!("CARGO_BIN_EXE_kat"))
            .current_dir(temp.path())
            .args(*args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{}", args.join(" "));
        let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(envelope["interface_schema_version"], 1);
        assert_eq!(envelope["success"], false);
        assert!(envelope["data"].is_null());
        assert!(envelope["error"]["code"].is_string());
    }
}

#[test]
fn reconcile_clean_conflicts_materialize_and_finalize_json() {
    let project = Project::new();
    let snapshot = project.snapshot(None);
    let base = project.revision(vec![], &[], snapshot.clone());
    let local = project.revision(vec![base], &[(1, "local")], snapshot.clone());
    let other = project.revision(vec![base], &[(2, "other")], snapshot);
    project.workspace(local);

    let target = other.to_string();
    let prepared = project.json(&["reconcile", &target, "--json"], 0);
    assert_eq!(prepared["data"]["status"], "prepared_clean");
    assert_eq!(prepared["data"]["base_revision"], local.to_string());
    assert_eq!(prepared["data"]["target_revision"], target);
    let merged = prepared["data"]["prepared_revision"].as_str().unwrap();
    assert_eq!(merged.len(), 64);

    let conflicts = project.json(&["conflicts", "--json"], 0);
    assert_eq!(
        conflicts["data"]["semantic_conflicts"],
        serde_json::json!([])
    );
    assert_eq!(
        conflicts["data"]["physical_conflicts"],
        serde_json::json!([])
    );
    assert_eq!(
        conflicts["data"]["validation_findings"],
        serde_json::json!([])
    );
    let materialize = project.json(&["materialize", "--json"], 0);
    assert_eq!(materialize["data"]["status"], "prepared_clean");

    let finalized = project.json(&["finalize", "--json"], 0);
    assert_eq!(finalized["data"]["from_revision"], local.to_string());
    assert_eq!(finalized["data"]["to_revision"], merged);
    assert_eq!(
        project.json(&["status", "--json"], 0)["data"]["workspace"]["base_revision"],
        merged
    );
}

#[test]
fn semantic_conflict_ids_round_trip_and_abort_json() {
    let project = Project::new();
    let snapshot = project.snapshot(None);
    let base = project.revision(vec![], &[(1, "base")], snapshot.clone());
    let local = project.revision(vec![base], &[(1, "local")], snapshot.clone());
    let other = project.revision(vec![base], &[(1, "other")], snapshot);
    project.workspace(local);

    let target = other.to_string();
    let reconciled = project.json(&["reconcile", &target, "--json"], 0);
    assert_eq!(reconciled["data"]["status"], "conflicted");
    assert_eq!(reconciled["data"]["semantic_conflicts"], 1);
    assert_eq!(reconciled["data"]["physical_conflicts"], 0);
    assert_eq!(
        project.json(&["finalize", "--json"], 1)["error"]["code"],
        "FinalizeFailed"
    );

    let conflicts = project.json(&["conflicts", "--json"], 0);
    assert_eq!(conflicts, project.json(&["conflicts", "--json"], 0));
    let id = conflicts["data"]["semantic_conflicts"][0]["conflict_id"]
        .as_str()
        .unwrap();
    assert_eq!(id.len(), 36);
    assert_eq!(
        project.json(&["materialize", "--json"], 0)["data"]["status"],
        "materialized"
    );
    let resolved = project.json(
        &["resolve", "semantic", id, "--accept", "local", "--json"],
        0,
    );
    assert_eq!(resolved["data"]["conflict_id"], id);
    assert_eq!(resolved["data"]["status"], "prepared_clean");
    assert!(resolved["data"]["prepared_revision"].is_string());

    let aborted = project.json(&["abort", "--json"], 0);
    assert_eq!(aborted["data"]["type"], "reconciliation");
    assert_eq!(aborted["data"]["aborted"], true);
    assert_eq!(aborted["data"]["base_revision"], local.to_string());
    assert_eq!(
        project.json(&["conflicts", "--json"], 1)["error"]["code"],
        "NoActiveReconciliation"
    );
    assert_eq!(
        project.json(&["materialize", "--json"], 1)["error"]["code"],
        "NoActiveReconciliation"
    );
}

#[test]
fn conflicts_are_sorted_by_persisted_id() {
    let project = Project::new();
    let snapshot = project.snapshot(None);
    let base = project.revision(
        vec![],
        &[(1, "base one"), (2, "base two")],
        snapshot.clone(),
    );
    let local = project.revision(
        vec![base],
        &[(1, "local one"), (2, "local two")],
        snapshot.clone(),
    );
    let other = project.revision(vec![base], &[(1, "other one"), (2, "other two")], snapshot);
    project.workspace(local);
    assert_eq!(
        project.json(&["reconcile", &other.to_string(), "--json"], 0)["data"]["semantic_conflicts"],
        2
    );
    let conflicts = project.json(&["conflicts", "--json"], 0);
    let entries = conflicts["data"]["semantic_conflicts"].as_array().unwrap();
    let ids: Vec<_> = entries
        .iter()
        .map(|entry| entry["conflict_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 2);
    assert!(ids[0] < ids[1]);
    assert_eq!(conflicts, project.json(&["conflicts", "--json"], 0));
}

#[test]
fn physical_conflict_materialize_and_resolve_json() {
    let project = Project::new();
    let base_snapshot = project.snapshot(Some("base\n"));
    let local_snapshot = project.snapshot(Some("local\n"));
    let other_snapshot = project.snapshot(Some("other\n"));
    let base = project.revision(vec![], &[], base_snapshot);
    let local = project.revision(vec![base], &[], local_snapshot.clone());
    let other = project.revision(vec![base], &[], other_snapshot);
    project.workspace(local);
    GitWorkspaceBackend::open(&project.root)
        .unwrap()
        .materialize_snapshot(&local_snapshot)
        .unwrap();

    let result = project.json(&["reconcile", &other.to_string(), "--json"], 0);
    assert_eq!(result["data"]["status"], "conflicted");
    assert_eq!(result["data"]["physical_conflicts"], 1);
    let conflicts = project.json(&["conflicts", "--json"], 0);
    let id = conflicts["data"]["physical_conflicts"][0]["conflict_id"]
        .as_str()
        .unwrap();
    assert_eq!(id.len(), 36);
    assert_eq!(
        conflicts["data"]["semantic_conflicts"],
        serde_json::json!([])
    );
    assert_eq!(
        conflicts["data"]["physical_conflicts"][0]["paths"],
        serde_json::json!(["a.txt"])
    );

    assert_eq!(
        project.json(&["materialize", "--json"], 0)["data"]["status"],
        "materialized"
    );
    assert_eq!(
        project.json(&["materialize", "--json"], 0)["data"]["status"],
        "already_materialized"
    );
    std::fs::write(project.root.join("a.txt"), "resolved\n").unwrap();
    let resolved = project.json(
        &["resolve", "physical", id, "--accept", "working", "--json"],
        0,
    );
    assert_eq!(resolved["data"]["conflict_id"], id);
    assert_eq!(resolved["data"]["accepted"], "working");
    assert_eq!(resolved["data"]["status"], "prepared_clean");
}

#[test]
fn advance_and_switch_report_before_and_after() {
    let project = Project::new();
    let snapshot = project.snapshot(None);
    let base = project.revision(vec![], &[], snapshot.clone());
    let child = project.revision(vec![base], &[(1, "child")], snapshot.clone());
    let unrelated = project.revision(vec![], &[(2, "unrelated")], snapshot);
    project.workspace(base);

    let child_id = child.to_string();
    assert_eq!(
        project.json(&["reconcile", "not-a-revision", "--json"], 1)["error"]["code"],
        "InvalidTarget"
    );
    assert_eq!(
        project.json(&["reconcile", &child_id, "--json"], 1)["error"]["code"],
        "OtherAhead"
    );
    let advanced = project.json(&["advance", &child_id, "--json"], 0);
    assert_eq!(advanced["data"]["advanced"], true);
    assert_eq!(advanced["data"]["from_revision"], base.to_string());
    assert_eq!(advanced["data"]["to_revision"], child_id);
    let same = project.json(&["advance", &child_id, "--json"], 0);
    assert_eq!(same["data"]["advanced"], false);
    assert_eq!(same["data"]["from_revision"], same["data"]["to_revision"]);

    let switched = project.json(&["switch", &unrelated.to_string(), "--json"], 0);
    assert_eq!(switched["data"]["switched"], true);
    assert_eq!(switched["data"]["from_revision"], child_id);
    assert_eq!(switched["data"]["to_revision"], unrelated.to_string());
    let same = project.json(&["switch", &unrelated.to_string(), "--json"], 0);
    assert_eq!(same["data"]["switched"], false);
    assert_eq!(same["data"]["from_revision"], same["data"]["to_revision"]);
}

#[test]
fn e2e_01_clean_divergence_finalizes_merged_state_and_tree() {
    let project = Project::new();
    let base_snapshot = project.snapshot(Some("base\n"));
    let local_snapshot = project.snapshot(Some("local\n"));
    let base = project.revision(vec![], &[], base_snapshot);
    let local = project.revision(vec![base], &[(1, "local")], local_snapshot.clone());
    let other = project.revision(vec![base], &[(2, "other")], local_snapshot.clone());
    project.workspace(local);
    GitWorkspaceBackend::open(&project.root)
        .unwrap()
        .materialize_snapshot(&local_snapshot)
        .unwrap();
    let result = project.json(&["reconcile", &other.to_string(), "--json"], 0);
    let merged: RepositoryRevisionId = result["data"]["prepared_revision"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(project.base(), local);
    project.json(&["finalize", "--json"], 0);
    assert_eq!(project.base(), merged);
    assert_eq!(
        std::fs::read(project.root.join("a.txt")).unwrap(),
        b"local\n"
    );
    project.assert_snapshot_file(merged, "local\n");
    assert!(!project.session_path_exists());
    let merged_state = project.revision_data(merged).semantic_state;
    let bytes = kat::repository::open::open_repository(&project.root)
        .unwrap()
        .object_store()
        .get(merged_state.as_object_id())
        .unwrap();
    let CanonicalPayload::SemanticState(state) =
        kat::encoding::decode_canonical(&bytes).unwrap().payload
    else {
        panic!("not a semantic state")
    };
    assert_eq!(state.elements.len(), 2);
}

#[test]
fn e2e_02_semantic_resolution_needs_no_materialization() {
    let project = Project::new();
    let base_snapshot = project.snapshot(Some("base\n"));
    let other_snapshot = project.snapshot(Some("other clean\n"));
    let base = project.revision(vec![], &[(1, "base")], base_snapshot.clone());
    let local = project.revision(vec![base], &[(1, "local")], base_snapshot.clone());
    let other = project.revision(vec![base], &[(1, "other")], other_snapshot);
    project.workspace(local);
    GitWorkspaceBackend::open(&project.root)
        .unwrap()
        .materialize_snapshot(&base_snapshot)
        .unwrap();
    project.json(&["reconcile", &other.to_string(), "--json"], 0);
    let expected_snapshot = match &project.session().state {
        ReconciliationSessionState::Conflicted { candidate } => {
            candidate.clean_physical_snapshot.clone().unwrap()
        }
        _ => panic!("expected conflict"),
    };
    let conflict = project.json(&["conflicts", "--json"], 0);
    let id = conflict["data"]["semantic_conflicts"][0]["conflict_id"]
        .as_str()
        .unwrap();
    let result = project.json(
        &["resolve", "semantic", id, "--accept", "local", "--json"],
        0,
    );
    assert_eq!(result["data"]["status"], "prepared_clean");
    assert_eq!(
        std::fs::read(project.root.join("a.txt")).unwrap(),
        b"base\n"
    );
    let merged = match project.session().state {
        ReconciliationSessionState::PreparedClean { revision } => revision,
        _ => panic!("not prepared"),
    };
    assert_eq!(
        project.revision_data(merged).workspace_snapshot,
        expected_snapshot
    );
    project.assert_snapshot_file(merged, "other clean\n");
    project.json(&["finalize", "--json"], 0);
    assert_eq!(project.base(), merged);
    assert_eq!(
        std::fs::read(project.root.join("a.txt")).unwrap(),
        b"other clean\n"
    );
}

#[test]
fn old_v1_session_recovers_clean_snapshot_from_immutable_revisions() {
    let project = Project::new();
    let base_snapshot = project.snapshot(Some("base\n"));
    let other_snapshot = project.snapshot(Some("other\n"));
    let base = project.revision(vec![], &[(1, "base")], base_snapshot.clone());
    let local = project.revision(vec![base], &[(1, "local")], base_snapshot.clone());
    let other = project.revision(vec![base], &[(1, "other")], other_snapshot);
    project.workspace(local);
    GitWorkspaceBackend::open(&project.root)
        .unwrap()
        .materialize_snapshot(&base_snapshot)
        .unwrap();
    project.json(&["reconcile", &other.to_string(), "--json"], 0);
    let session = project.session();
    let expected = match &session.state {
        ReconciliationSessionState::Conflicted { candidate } => {
            candidate.clean_physical_snapshot.clone().unwrap()
        }
        _ => panic!("not conflicted"),
    };
    let path = kat::repository::reconcile::session_path(&project.root, &session.workspace_id);
    let mut json: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    json["state"]["Conflicted"]["candidate"]
        .as_object_mut()
        .unwrap()
        .remove("clean_physical_snapshot");
    std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    std::fs::write(project.root.join("a.txt"), "mutable work\n").unwrap();
    let id =
        project.json(&["conflicts", "--json"], 0)["data"]["semantic_conflicts"][0]["conflict_id"]
            .as_str()
            .unwrap()
            .to_owned();
    project.json(
        &["resolve", "semantic", &id, "--accept", "local", "--json"],
        0,
    );
    let merged = match project.session().state {
        ReconciliationSessionState::PreparedClean { revision } => revision,
        _ => panic!("not prepared"),
    };
    assert_eq!(project.revision_data(merged).workspace_snapshot, expected);
    assert_eq!(
        std::fs::read(project.root.join("a.txt")).unwrap(),
        b"mutable work\n"
    );
}

#[test]
fn human_semantic_workflow_has_same_repository_outcome() {
    let project = Project::new();
    let snapshot = project.snapshot(None);
    let base = project.revision(vec![], &[(1, "base")], snapshot.clone());
    let local = project.revision(vec![base], &[(1, "local")], snapshot.clone());
    let other = project.revision(vec![base], &[(1, "other")], snapshot);
    project.workspace(local);
    assert!(
        project
            .run(&["reconcile", &other.to_string()])
            .status
            .success()
    );
    let conflicts = project.run(&["conflicts"]);
    assert!(conflicts.status.success());
    let id = match &project.session().state {
        ReconciliationSessionState::Conflicted { candidate } => {
            candidate.semantic_conflicts[0].id.clone()
        }
        _ => panic!("not conflicted"),
    };
    assert!(
        project
            .run(&["resolve", "semantic", &id, "--accept", "local"])
            .status
            .success()
    );
    let merged = match project.session().state {
        ReconciliationSessionState::PreparedClean { revision } => revision,
        _ => panic!("not prepared"),
    };
    assert!(project.run(&["finalize"]).status.success());
    assert_eq!(project.base(), merged);
}

#[test]
fn e2e_03_physical_resolution_preserves_edit_and_cleans_candidate() {
    let (project, local, other) = physical_conflict_project();
    project.json(&["reconcile", &other.to_string(), "--json"], 0);
    let (candidate_path, ref_name) = project.candidate_resources();
    assert!(candidate_path.exists());
    assert!(project.git_candidate_ref(&ref_name));
    let id =
        project.json(&["conflicts", "--json"], 0)["data"]["physical_conflicts"][0]["conflict_id"]
            .as_str()
            .unwrap()
            .to_owned();
    project.json(&["materialize", "--json"], 0);
    std::fs::write(project.root.join("a.txt"), "resolved\n").unwrap();
    project.json(
        &["resolve", "physical", &id, "--accept", "working", "--json"],
        0,
    );
    let merged = match project.session().state {
        ReconciliationSessionState::PreparedClean { revision } => revision,
        _ => panic!("not prepared"),
    };
    project.assert_snapshot_file(merged, "resolved\n");
    assert_eq!(project.base(), local);
    project.json(&["finalize", "--json"], 0);
    assert_eq!(project.base(), merged);
    assert_eq!(
        std::fs::read(project.root.join("a.txt")).unwrap(),
        b"resolved\n"
    );
    assert!(!candidate_path.exists());
    assert!(!project.git_candidate_ref(&ref_name));
}

#[test]
fn e2e_04_mixed_conflicts_resolve_independently() {
    let (project, local, other) = physical_conflict_project_with_semantics();
    project.json(&["reconcile", &other.to_string(), "--json"], 0);
    let conflicts = project.json(&["conflicts", "--json"], 0);
    let semantic = conflicts["data"]["semantic_conflicts"][0]["conflict_id"]
        .as_str()
        .unwrap();
    let physical = conflicts["data"]["physical_conflicts"][0]["conflict_id"]
        .as_str()
        .unwrap();
    project.json(
        &[
            "resolve", "semantic", semantic, "--accept", "other", "--json",
        ],
        0,
    );
    assert!(matches!(
        project.session().state,
        ReconciliationSessionState::Conflicted { .. }
    ));
    project.json(&["finalize", "--json"], 1);
    assert_eq!(project.base(), local);
    project.json(&["materialize", "--json"], 0);
    std::fs::write(project.root.join("a.txt"), "mixed resolved\n").unwrap();
    project.json(
        &[
            "resolve", "physical", physical, "--accept", "working", "--json",
        ],
        0,
    );
    project.json(&["finalize", "--json"], 0);
    assert_ne!(project.base(), local);
    assert_eq!(
        std::fs::read(project.root.join("a.txt")).unwrap(),
        b"mixed resolved\n"
    );
}

#[test]
fn e2e_05_abort_restores_exact_tree_and_removes_candidate() {
    let (project, local, other) = physical_conflict_project();
    kat::repository::open::open_repository(&project.root)
        .unwrap()
        .ref_store()
        .compare_and_swap_ref("local/anchor", None, local)
        .unwrap();
    let named_ref = std::fs::read(project.root.join(".kat/refs/local/anchor")).unwrap();
    let before = std::fs::read(project.root.join("a.txt")).unwrap();
    project.json(&["reconcile", &other.to_string(), "--json"], 0);
    let (candidate_path, ref_name) = project.candidate_resources();
    project.json(&["materialize", "--json"], 0);
    std::fs::write(project.root.join("a.txt"), "temporary edit\n").unwrap();
    project.json(&["abort", "--json"], 0);
    assert_eq!(project.base(), local);
    assert_eq!(std::fs::read(project.root.join("a.txt")).unwrap(), before);
    assert!(!project.session_path_exists());
    assert!(!candidate_path.exists());
    assert!(!project.git_candidate_ref(&ref_name));
    assert_eq!(
        std::fs::read(project.root.join(".kat/refs/local/anchor")).unwrap(),
        named_ref
    );
}

#[test]
fn e2e_06_other_ahead_advances_physical_tree() {
    let project = Project::new();
    let base_snapshot = project.snapshot(Some("base\n"));
    let child_snapshot = project.snapshot(Some("child\n"));
    let base = project.revision(vec![], &[], base_snapshot.clone());
    let child = project.revision(vec![base], &[], child_snapshot);
    project.workspace(base);
    kat::repository::open::open_repository(&project.root)
        .unwrap()
        .ref_store()
        .compare_and_swap_ref("local/anchor", None, base)
        .unwrap();
    let named_ref = std::fs::read(project.root.join(".kat/refs/local/anchor")).unwrap();
    GitWorkspaceBackend::open(&project.root)
        .unwrap()
        .materialize_snapshot(&base_snapshot)
        .unwrap();
    project.json(&["reconcile", &child.to_string(), "--json"], 1);
    project.json(&["advance", &child.to_string(), "--json"], 0);
    assert_eq!(project.base(), child);
    assert_eq!(
        std::fs::read(project.root.join("a.txt")).unwrap(),
        b"child\n"
    );
    assert!(!project.session_path_exists());
    assert_eq!(
        std::fs::read(project.root.join(".kat/refs/local/anchor")).unwrap(),
        named_ref
    );
}

#[test]
fn e2e_07_switch_between_unrelated_trees() {
    let project = Project::new();
    let first_snapshot = project.snapshot(Some("first\n"));
    let second_snapshot = project.snapshot(Some("second\n"));
    let first = project.revision(vec![], &[], first_snapshot.clone());
    let second = project.revision(vec![], &[], second_snapshot);
    project.workspace(first);
    GitWorkspaceBackend::open(&project.root)
        .unwrap()
        .materialize_snapshot(&first_snapshot)
        .unwrap();
    project.json(&["switch", &second.to_string(), "--json"], 0);
    assert_eq!(project.base(), second);
    assert_eq!(
        std::fs::read(project.root.join("a.txt")).unwrap(),
        b"second\n"
    );
    project.json(&["switch", &first.to_string(), "--json"], 0);
    assert_eq!(project.base(), first);
    assert_eq!(
        std::fs::read(project.root.join("a.txt")).unwrap(),
        b"first\n"
    );
}

#[test]
fn e2e_08_materialized_candidate_survives_process_restart() {
    let (project, _, other) = physical_conflict_project();
    project.json(&["reconcile", &other.to_string(), "--json"], 0);
    let (candidate_path, ref_name) = project.candidate_resources();
    let id =
        project.json(&["conflicts", "--json"], 0)["data"]["physical_conflicts"][0]["conflict_id"]
            .as_str()
            .unwrap()
            .to_owned();
    project.json(&["materialize", "--json"], 0);
    assert!(candidate_path.exists());
    assert!(project.git_candidate_ref(&ref_name));
    let session = project.session();
    let ReconciliationSessionState::ConflictedMaterialized { candidate } = session.state else {
        panic!("not materialized")
    };
    let physical = candidate.physical_candidate.unwrap();
    let state = GitWorkspaceBackend::open(&project.root)
        .unwrap()
        .get_candidate_state(&session.workspace_id, &physical.provisional)
        .unwrap();
    assert_eq!(state.version, 1);
    assert_eq!(state.local_snapshot, physical.local.to_hex());
    assert_eq!(
        project.json(&["conflicts", "--json"], 0)["data"]["physical_conflicts"][0]["conflict_id"],
        id
    );
    std::fs::write(project.root.join("a.txt"), "after restart\n").unwrap();
    project.json(
        &["resolve", "physical", &id, "--accept", "working", "--json"],
        0,
    );
    let merged = match project.session().state {
        ReconciliationSessionState::PreparedClean { revision } => revision,
        _ => panic!("not prepared"),
    };
    project.assert_snapshot_file(merged, "after restart\n");
}

#[test]
fn e2e_09_prepared_session_finalizes_after_restart() {
    let project = Project::new();
    let snapshot = project.snapshot(None);
    let base = project.revision(vec![], &[], snapshot.clone());
    let local = project.revision(vec![base], &[(1, "local")], snapshot.clone());
    let other = project.revision(vec![base], &[(2, "other")], snapshot);
    project.workspace(local);
    project.json(&["reconcile", &other.to_string(), "--json"], 0);
    let prepared = match project.session().state {
        ReconciliationSessionState::PreparedClean { revision } => revision,
        _ => panic!("not prepared"),
    };
    project.json(&["finalize", "--json"], 0);
    assert_eq!(project.base(), prepared);
    assert!(!project.session_path_exists());
}

#[test]
fn e2e_10_named_references_do_not_move_with_collaboration() {
    let (project, local, other) = physical_conflict_project_with_semantics();
    let refs = kat::repository::open::open_repository(&project.root).unwrap();
    refs.ref_store()
        .compare_and_swap_ref("local/anchor", None, local)
        .unwrap();
    let before = std::fs::read(project.root.join(".kat/refs/local/anchor")).unwrap();
    project.json(&["reconcile", &other.to_string(), "--json"], 0);
    let conflicts = project.json(&["conflicts", "--json"], 0);
    let semantic = conflicts["data"]["semantic_conflicts"][0]["conflict_id"]
        .as_str()
        .unwrap();
    let physical = conflicts["data"]["physical_conflicts"][0]["conflict_id"]
        .as_str()
        .unwrap();
    project.json(
        &[
            "resolve", "semantic", semantic, "--accept", "local", "--json",
        ],
        0,
    );
    project.json(&["materialize", "--json"], 0);
    std::fs::write(project.root.join("a.txt"), "resolved\n").unwrap();
    project.json(
        &[
            "resolve", "physical", physical, "--accept", "working", "--json",
        ],
        0,
    );
    project.json(&["finalize", "--json"], 0);
    project.json(&["switch", &local.to_string(), "--json"], 0);
    project.json(&["advance", &project.base().to_string(), "--json"], 0);
    assert_eq!(project.local_refs().len(), 1);
    assert_eq!(
        std::fs::read(project.root.join(".kat/refs/local/anchor")).unwrap(),
        before
    );
}

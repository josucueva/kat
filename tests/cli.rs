//! End-to-end CLI tests: spawn the real `kat` binary (via `CARGO_BIN_EXE_kat`,
//! no extra dependencies) against a temp repository.
//!
//! The CLI is thin parse + dispatch, so these tests assert the invocation
//! contract from `docs/cli.md` without re-testing library semantics.

use std::path::Path;
use std::process::Command;
use std::str::FromStr;

use kat::domain::identity::{ChangeId, ElementId, ObjectId, RelationshipId};
use kat::domain::operation::Operation;
use kat::domain::property::PropertyValue;
use kat::encoding::decode::decode_canonical;
use kat::encoding::object::CanonicalPayload;
use kat::repository::change::{
    CreateElementInput, apply_create_element, persist_prepared_change, prepare_change,
    prepare_change_revision, publish_persisted_change, validate_create_element_invariants,
    validate_create_element_ontology,
};
use kat::repository::init::init_repository;
use kat::repository::open::open_repository;
use kat::repository::query::history;
use uuid::Uuid;

fn run_kat(dir: &Path, args: &[&str]) -> (String, String, bool) {
    let output = Command::new(env!("CARGO_BIN_EXE_kat"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("kat binary runs");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.success(),
    )
}

/// Extracts the value of a `key: value` line from CLI stdout.
fn id_line<'a>(out: &'a str, key: &str) -> &'a str {
    let prefix = format!("{key}: ");
    out.lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("output must contain a '{key}: ...' line:\n{out}"))
}

/// The identities of a change published through the library.
struct Published {
    element_id: ElementId,
    version_id: String,
    state_id: String,
    change_revision_id: String,
}

/// Publishes one element through the library (the `kat create` CLI is wired
/// later, at Phase 1 closure), returning its identities.
fn publish_element(root: &Path, element_n: u128, change_n: u128) -> Published {
    let repo = open_repository(root).unwrap();
    let element_id = ElementId::from_uuid(Uuid::from_u128(element_n));
    let context = prepare_change(&repo).unwrap();
    let prepared = apply_create_element(
        context,
        CreateElementInput {
            element_id,
            type_id: "kat.core/requirement".into(),
            properties: vec![("title".into(), PropertyValue::Text("A requirement".into()))],
        },
    )
    .unwrap();
    let validated =
        validate_create_element_invariants(validate_create_element_ontology(prepared).unwrap())
            .unwrap();
    let revision = prepare_change_revision(
        validated,
        ChangeId::from_uuid(Uuid::from_u128(change_n)),
        None,
    )
    .unwrap();
    let version_id = revision.creation.element_version_id.to_string();
    let state_id = revision.state_id.to_string();
    let change_revision_id = revision.change_revision_id.to_string();
    let persisted = persist_prepared_change(&repo, revision).unwrap();
    publish_persisted_change(&repo, persisted).unwrap();
    Published {
        element_id,
        version_id,
        state_id,
        change_revision_id,
    }
}

#[test]
fn kat_show_prints_resolved_element() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // `kat init` from the CLI, like a real user.
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    // Publish a change through the library.
    let published = publish_element(root, 81, 181);

    // `kat show <element-id>` resolves and prints the accepted version.
    let (out, err, ok) = run_kat(root, &["show", &published.element_id.to_string()]);
    assert!(ok, "kat show failed: {err}");
    let expected = format!(
        "element_id: {}\n\
         version_id: {}\n\
         type: kat.core/requirement\n\
         lifecycle: active\n\
         title: A requirement\n",
        published.element_id, published.version_id
    );
    assert_eq!(out, expected);

    // An unknown element fails cleanly with a friendly message.
    let missing = ElementId::from_uuid(Uuid::from_u128(999));
    let (out, err, ok) = run_kat(root, &["show", &missing.to_string()]);
    assert!(!ok);
    assert!(out.is_empty());
    assert!(err.contains("not found"));
}

#[test]
fn kat_show_without_a_repository_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let element_id = ElementId::from_uuid(Uuid::from_u128(82));

    let (out, err, ok) = run_kat(root, &["show", &element_id.to_string()]);
    assert!(!ok);
    assert!(out.is_empty());
    assert!(err.contains("no KAT repository found"));
}

#[test]
fn kat_show_rejects_invalid_element_id() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();

    let (out, err, ok) = run_kat(root, &["show", "not-a-uuid"]);
    assert!(!ok);
    assert!(out.is_empty());
    assert!(err.contains("invalid element ID"));
}

#[test]
fn kat_history_empty_on_fresh_repository() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    let (out, err, ok) = run_kat(root, &["history"]);
    assert!(ok, "kat history failed: {err}");
    assert!(out.is_empty(), "fresh repository has no history: {out}");
}

#[test]
fn kat_history_prints_accepted_chain() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    // The base state the change was prepared against.
    let s0 = open_repository(root).unwrap().accepted.state;

    let published = publish_element(root, 82, 182);

    let (out, err, ok) = run_kat(root, &["history"]);
    assert!(ok, "kat history failed: {err}");
    let change_id = ChangeId::from_uuid(Uuid::from_u128(182));
    let expected = format!(
        "revision_id: {}\nchange_id: {change_id}\nresult_state: {}\nbase_states:\n  {}\ndependencies:\n  none\noperations:\n  create_element {}\ndescription: none\n",
        published.change_revision_id, published.state_id, s0, published.version_id
    );
    assert_eq!(out, expected);
}

// ---------------------------------------------------------------------------
// kat create — Phase 1 closure
// ---------------------------------------------------------------------------

/// The Phase 1 acceptance scenario end to end as a black-box CLI flow (per
/// `docs/implementation-plan.md`): init -> create -> capture IDs -> fresh
/// reopen -> verify accepted / C1 / S1 -> kat show -> kat history.
#[test]
fn phase1_acceptance_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // 1. kat init (CLI).
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");
    let s0 = open_repository(root).unwrap().accepted.state;

    // 2-3. kat create requirement --title "User authentication"; capture IDs.
    let (create_out, create_err, ok) = run_kat(
        root,
        &["create", "requirement", "--title", "User authentication"],
    );
    assert!(ok, "kat create failed: {create_err}\n{create_out}");
    let element_id = ElementId::from_str(id_line(&create_out, "element_id")).unwrap();
    let version_id = ObjectId::from_str(id_line(&create_out, "version_id")).unwrap();
    let state_id = ObjectId::from_str(id_line(&create_out, "state_id")).unwrap();
    let change_id = ChangeId::from_str(id_line(&create_out, "change_id")).unwrap();
    let change_revision_id =
        ObjectId::from_str(id_line(&create_out, "change_revision_id")).unwrap();

    // 4. Fresh process reopen.
    let repo = open_repository(root).unwrap();

    // 5. Accepted head: { state: S1, change: C1 }.
    assert_eq!(repo.accepted.state, state_id);
    assert_eq!(repo.accepted.change, Some(change_revision_id));

    // 6. C1: result_state == S1, base_states == [S0], CreateElement(V1).
    let entries = history(&repo).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].revision_id, change_revision_id);
    assert_eq!(entries[0].change.change_id, change_id);
    assert_eq!(entries[0].change.result_state, state_id);
    assert_eq!(entries[0].change.base_states, vec![s0]);
    assert_eq!(
        entries[0].change.operations,
        vec![Operation::CreateElement {
            new_version: version_id,
        }]
    );

    // 7. S1 maps E1 -> V1.
    let context = prepare_change(&repo).unwrap();
    assert_eq!(context.base_state_id, state_id);
    assert_eq!(context.base_state.elements.len(), 1);
    assert_eq!(context.base_state.elements[0].element_id, element_id);
    assert_eq!(context.base_state.elements[0].version, version_id);

    // 8. kat show E1: requirement, active, title present.
    let (out, err, ok) = run_kat(root, &["show", &element_id.to_string()]);
    assert!(ok, "kat show failed: {err}");
    assert!(out.contains("type: kat.core/requirement"));
    assert!(out.contains("lifecycle: active"));
    assert!(out.contains("title: User authentication"));

    // 9. kat history: C1 shown, S1 shown, CreateElement(V1) shown.
    let (out, err, ok) = run_kat(root, &["history"]);
    assert!(ok, "kat history failed: {err}");
    assert!(out.contains(&change_revision_id.to_string()));
    assert!(out.contains(&state_id.to_string()));
    assert!(out.contains(&format!("create_element {version_id}")));
}

#[test]
fn kat_create_outside_repository_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let (out, err, ok) = run_kat(root, &["create", "requirement", "--title", "x"]);
    assert!(!ok);
    assert!(out.is_empty());
    assert!(err.contains("no KAT repository found"));
}

#[test]
fn kat_create_unknown_short_type_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    let (out, err, ok) = run_kat(root, &["create", "bogus", "--title", "x"]);
    assert!(!ok);
    assert!(out.is_empty());
    assert!(err.contains("unknown element type 'bogus'"));

    // Nothing was published.
    assert_eq!(open_repository(root).unwrap().accepted.change, None);
}

#[test]
fn kat_create_unknown_qualified_type_is_ontology_failure() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    // A fully-qualified ID passes through the CLI and is rejected by the
    // engine's ontology conformance stage.
    let (out, err, ok) = run_kat(root, &["create", "kat.core/nope", "--title", "x"]);
    assert!(!ok);
    assert!(out.is_empty());
    assert!(err.contains("ontology conformance"));
    assert!(err.contains("kat.core/nope"));

    assert_eq!(open_repository(root).unwrap().accepted.change, None);
}

#[test]
fn kat_create_missing_title_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    let (out, err, ok) = run_kat(root, &["create", "requirement"]);
    assert!(!ok);
    assert!(out.is_empty());
    assert!(err.contains("--title is required"));
    assert!(err.contains("usage: kat create"));
}

#[test]
fn kat_create_rejects_malformed_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    // Duplicate --title.
    let (out, err, ok) = run_kat(
        root,
        &["create", "requirement", "--title", "a", "--title", "b"],
    );
    assert!(!ok);
    assert!(out.is_empty());
    assert!(err.contains("duplicate --title"));

    // Unknown option.
    let (out, err, ok) = run_kat(
        root,
        &["create", "requirement", "--title", "a", "--bogus", "x"],
    );
    assert!(!ok);
    assert!(out.is_empty());
    assert!(err.contains("unknown option '--bogus'"));

    // Missing flag value.
    let (out, err, ok) = run_kat(root, &["create", "requirement", "--title"]);
    assert!(!ok);
    assert!(out.is_empty());
    assert!(err.contains("missing value for --title"));

    // Nothing was published by any of the failures.
    assert_eq!(open_repository(root).unwrap().accepted.change, None);
}

#[test]
fn kat_create_supports_optional_description() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    let (create_out, create_err, ok) = run_kat(
        root,
        &[
            "create",
            "requirement",
            "--title",
            "T",
            "--description",
            "D",
        ],
    );
    assert!(ok, "kat create failed: {create_err}\n{create_out}");
    let element_id = ElementId::from_str(id_line(&create_out, "element_id")).unwrap();

    // Both flags become text element properties.
    let (out, err, ok) = run_kat(root, &["show", &element_id.to_string()]);
    assert!(ok, "kat show failed: {err}");
    assert!(out.contains("title: T"));
    assert!(out.contains("description: D"));
}

#[test]
fn kat_create_twice_produces_linear_history() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    let (c1_out, c1_err, ok) = run_kat(root, &["create", "requirement", "--title", "First"]);
    assert!(ok, "first kat create failed: {c1_err}\n{c1_out}");
    let (c2_out, c2_err, ok) = run_kat(root, &["create", "requirement", "--title", "Second"]);
    assert!(ok, "second kat create failed: {c2_err}\n{c2_out}");

    // Each create is a fresh process, so the second prepares against S1: the
    // history is a linear C2 -> C1 chain (dependencies = accepted head).
    let repo = open_repository(root).unwrap();
    let entries = history(&repo).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].change.dependencies, vec![entries[1].revision_id]);
    assert!(entries[1].change.dependencies.is_empty());

    // Both elements are in the accepted state.
    let context = prepare_change(&repo).unwrap();
    assert_eq!(context.base_state.elements.len(), 2);
}

// ---------------------------------------------------------------------------
// kat update — Phase 2 closure
// ---------------------------------------------------------------------------

/// The Phase 2 acceptance scenario end to end as a black-box CLI flow (per
/// `docs/implementation-plan-phase2.md`): init -> create requirement --title "A"
/// -> update <E1> --title "B" -> fresh reopen -> verify accepted {S2, C2} ->
/// C2 operations = [UpdateElement(E1, V1, V2)], base_states = [S1], result_state = S2 ->
/// kat show E1 -> title "B" (resolves V2) -> kat history -> [C2, C1] -> V1 still present in objects/.
#[test]
fn phase2_acceptance_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // 1. kat init
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    // 2. kat create requirement --title "A" -> E1, V1, S1, C1
    let (create_out, create_err, ok) = run_kat(root, &["create", "requirement", "--title", "A"]);
    assert!(ok, "kat create failed: {create_err}\n{create_out}");
    let element_id = ElementId::from_str(id_line(&create_out, "element_id")).unwrap();
    let v1_id = ObjectId::from_str(id_line(&create_out, "version_id")).unwrap();
    let s1_id = ObjectId::from_str(id_line(&create_out, "state_id")).unwrap();
    let c1_change_id = ChangeId::from_str(id_line(&create_out, "change_id")).unwrap();
    let c1_rev_id = ObjectId::from_str(id_line(&create_out, "change_revision_id")).unwrap();

    let repo = open_repository(root).unwrap();
    let v1_bytes = repo.object_store().get(v1_id).unwrap();

    // 3. kat update <E1> --title "B" -> V2, S2, C2
    let (update_out, update_err, ok) =
        run_kat(root, &["update", &element_id.to_string(), "--title", "B"]);
    assert!(ok, "kat update failed: {update_err}\n{update_out}");
    let out_prev_v = ObjectId::from_str(id_line(&update_out, "previous_version_id")).unwrap();
    let v2_id = ObjectId::from_str(id_line(&update_out, "version_id")).unwrap();
    let s2_id = ObjectId::from_str(id_line(&update_out, "state_id")).unwrap();
    let c2_change_id = ChangeId::from_str(id_line(&update_out, "change_id")).unwrap();
    let c2_rev_id = ObjectId::from_str(id_line(&update_out, "change_revision_id")).unwrap();

    assert_eq!(out_prev_v, v1_id);
    assert_ne!(v2_id, v1_id);

    // 4. Reopen (fresh process) -> accepted.state == S2, accepted.change == C2
    let reopened = open_repository(root).unwrap();
    assert_eq!(reopened.accepted.state, s2_id);
    assert_eq!(reopened.accepted.change, Some(c2_rev_id));

    // 5. S2 maps E1 -> V2
    let context = prepare_change(&reopened).unwrap();
    assert_eq!(context.base_state_id, s2_id);
    assert_eq!(context.base_state.elements.len(), 1);
    assert_eq!(context.base_state.elements[0].element_id, element_id);
    assert_eq!(context.base_state.elements[0].version, v2_id);

    // 6. C2: base_states == [S1], result_state == S2, operations = [UpdateElement(E1, V1, V2)]
    let entries = history(&reopened).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].revision_id, c2_rev_id);
    assert_eq!(entries[0].change.change_id, c2_change_id);
    assert_eq!(entries[0].change.result_state, s2_id);
    assert_eq!(entries[0].change.base_states, vec![s1_id]);
    assert_eq!(entries[0].change.dependencies, vec![c1_rev_id]);
    assert_eq!(
        entries[0].change.operations,
        vec![Operation::UpdateElement {
            element_id,
            expected_version: v1_id,
            new_version: v2_id,
        }]
    );

    // 7. C1: result_state == S1
    assert_eq!(entries[1].revision_id, c1_rev_id);
    assert_eq!(entries[1].change.change_id, c1_change_id);
    assert_eq!(entries[1].change.result_state, s1_id);

    // 8. kat show E1 -> title "B" (resolves V2)
    let (show_out, show_err, ok) = run_kat(root, &["show", &element_id.to_string()]);
    assert!(ok, "kat show failed: {show_err}");
    assert!(show_out.contains(&format!("version_id: {v2_id}")));
    assert!(show_out.contains("title: B"));

    // 9. kat history -> [C2, C1] (newest first)
    let (hist_out, hist_err, ok) = run_kat(root, &["history"]);
    assert!(ok, "kat history failed: {hist_err}");
    let c2_pos = hist_out.find(&c2_rev_id.to_string()).unwrap();
    let c1_pos = hist_out.find(&c1_rev_id.to_string()).unwrap();
    assert!(c2_pos < c1_pos, "history must list C2 before C1");

    // 10. V1 still present in objects/ (previous state traceable)
    assert_eq!(reopened.object_store().get(v1_id).unwrap(), v1_bytes);
}

#[test]
fn kat_update_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // 1. kat init
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    // 2. kat create
    let (create_out, create_err, ok) = run_kat(
        root,
        &[
            "create",
            "requirement",
            "--title",
            "Initial title",
            "--description",
            "Initial desc",
        ],
    );
    assert!(ok, "kat create failed: {create_err}\n{create_out}");
    let element_id = ElementId::from_str(id_line(&create_out, "element_id")).unwrap();
    let v1_id = ObjectId::from_str(id_line(&create_out, "version_id")).unwrap();
    let _c1_rev_id = ObjectId::from_str(id_line(&create_out, "change_revision_id")).unwrap();

    let repo = open_repository(root).unwrap();
    let v1_bytes = repo.object_store().get(v1_id).unwrap();

    // 3. kat update element_id --title "Updated title"
    let (update_out, update_err, ok) = run_kat(
        root,
        &[
            "update",
            &element_id.to_string(),
            "--title",
            "Updated title",
        ],
    );
    assert!(ok, "kat update failed: {update_err}\n{update_out}");

    let out_element_id = ElementId::from_str(id_line(&update_out, "element_id")).unwrap();
    let out_prev_v_id = ObjectId::from_str(id_line(&update_out, "previous_version_id")).unwrap();
    let v2_id = ObjectId::from_str(id_line(&update_out, "version_id")).unwrap();
    let _s2_id = ObjectId::from_str(id_line(&update_out, "state_id")).unwrap();
    let _c2_change_id = ChangeId::from_str(id_line(&update_out, "change_id")).unwrap();
    let c2_rev_id = ObjectId::from_str(id_line(&update_out, "change_revision_id")).unwrap();

    assert_eq!(out_element_id, element_id);
    assert_eq!(out_prev_v_id, v1_id);
    assert_ne!(v2_id, v1_id);

    // 4. kat show E1 resolves V2, updated title, preserved description
    let (show_out, show_err, ok) = run_kat(root, &["show", &element_id.to_string()]);
    assert!(ok, "kat show failed: {show_err}");
    assert!(show_out.contains(&format!("version_id: {v2_id}")));
    assert!(show_out.contains("title: Updated title"));
    assert!(show_out.contains("description: Initial desc"));

    // 5. kat history shows C2 before C1 with update_element E1 V1 V2
    let (hist_out, hist_err, ok) = run_kat(root, &["history"]);
    assert!(ok, "kat history failed: {hist_err}");
    assert!(hist_out.contains(&c2_rev_id.to_string()));
    assert!(hist_out.contains(&format!("update_element {element_id} {v1_id} {v2_id}")));

    // 6. V1 still exists byte-for-byte in ObjectStore
    let reopened = open_repository(root).unwrap();
    assert_eq!(reopened.object_store().get(v1_id).unwrap(), v1_bytes);
}

#[test]
fn kat_update_supports_optional_description_patch() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    let (create_out, create_err, ok) = run_kat(root, &["create", "requirement", "--title", "T"]);
    assert!(ok, "kat create failed: {create_err}\n{create_out}");
    let element_id = ElementId::from_str(id_line(&create_out, "element_id")).unwrap();

    let (update_out, update_err, ok) = run_kat(
        root,
        &[
            "update",
            &element_id.to_string(),
            "--description",
            "Added desc",
        ],
    );
    assert!(ok, "kat update failed: {update_err}\n{update_out}");

    let (show_out, show_err, ok) = run_kat(root, &["show", &element_id.to_string()]);
    assert!(ok, "kat show failed: {show_err}");
    assert!(show_out.contains("title: T"));
    assert!(show_out.contains("description: Added desc"));
}

#[test]
fn kat_update_outside_repository_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let missing_id = ElementId::from_uuid(Uuid::from_u128(991));

    let (out, err, ok) = run_kat(root, &["update", &missing_id.to_string(), "--title", "x"]);
    assert!(!ok);
    assert!(out.is_empty());
    assert!(err.contains("no KAT repository found"));
}

#[test]
fn kat_update_unknown_element_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    let missing_id = ElementId::from_uuid(Uuid::from_u128(992));
    let (out, err, ok) = run_kat(root, &["update", &missing_id.to_string(), "--title", "x"]);
    assert!(!ok);
    assert!(out.is_empty());
    assert!(err.contains("not found in the base state"));
}

#[test]
fn kat_update_malformed_arguments_and_no_effective_change_fail() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    let (create_out, create_err, ok) = run_kat(root, &["create", "requirement", "--title", "T"]);
    assert!(ok, "kat create failed: {create_err}\n{create_out}");
    let element_id = ElementId::from_str(id_line(&create_out, "element_id")).unwrap();

    // Invalid element ID
    let (_out, err, ok) = run_kat(root, &["update", "not-a-uuid", "--title", "x"]);
    assert!(!ok);
    assert!(err.contains("invalid element ID"));

    // No property flags
    let (_out, err, ok) = run_kat(root, &["update", &element_id.to_string()]);
    assert!(!ok);
    assert!(err.contains("at least one property flag"));

    // Duplicate flag
    let (_out, err, ok) = run_kat(
        root,
        &[
            "update",
            &element_id.to_string(),
            "--title",
            "a",
            "--title",
            "b",
        ],
    );
    assert!(!ok);
    assert!(err.contains("duplicate --title"));

    // Unknown option
    let (_out, err, ok) = run_kat(root, &["update", &element_id.to_string(), "--bogus", "x"]);
    assert!(!ok);
    assert!(err.contains("unknown option '--bogus'"));

    // Missing flag value
    let (_out, err, ok) = run_kat(root, &["update", &element_id.to_string(), "--title"]);
    assert!(!ok);
    assert!(err.contains("missing value for --title"));

    // No effective change (updating title to the exact same value "T")
    let (_out, err, ok) = run_kat(root, &["update", &element_id.to_string(), "--title", "T"]);
    assert!(!ok);
    assert!(err.contains("no effective change"));
}

#[test]
fn kat_deprecate_outside_repository_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let element_id = ElementId::from_uuid(Uuid::from_u128(993));
    let (out, err, ok) = run_kat(root, &["deprecate", &element_id.to_string()]);
    assert!(!ok);
    assert!(out.is_empty());
    assert!(err.contains("no KAT repository found"));
}

#[test]
fn kat_deprecate_unknown_element_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    let missing_id = ElementId::from_uuid(Uuid::from_u128(994));
    let (out, err, ok) = run_kat(root, &["deprecate", &missing_id.to_string()]);
    assert!(!ok);
    assert!(out.is_empty());
    assert!(err.contains("not found in the base state"));
}

#[test]
fn kat_deprecate_malformed_arguments_fail() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    // Invalid element ID
    let (_out, err, ok) = run_kat(root, &["deprecate", "not-a-uuid"]);
    assert!(!ok);
    assert!(err.contains("invalid element ID"));

    // Missing element ID
    let (_out, err, ok) = run_kat(root, &["deprecate"]);
    assert!(!ok);
    assert!(err.contains("expected <element-id>"));

    // Extra arguments
    let (_out, err, ok) = run_kat(root, &["deprecate", "arg1", "arg2"]);
    assert!(!ok);
    assert!(err.contains("expected <element-id>"));
}

#[test]
fn kat_deprecate_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    // 1. Create requirement -> V1, S1, C1
    let (create_out, create_err, ok) =
        run_kat(root, &["create", "requirement", "--title", "Req 1"]);
    assert!(ok, "kat create failed: {create_err}\n{create_out}");
    let element_id = ElementId::from_str(id_line(&create_out, "element_id")).unwrap();
    let v1_id = ObjectId::from_str(id_line(&create_out, "version_id")).unwrap();
    let s1_id = ObjectId::from_str(id_line(&create_out, "state_id")).unwrap();
    let c1_rev_id = ObjectId::from_str(id_line(&create_out, "change_revision_id")).unwrap();

    // 2. Deprecate requirement -> V2, S2, C2
    let (deprecate_out, deprecate_err, ok) = run_kat(root, &["deprecate", &element_id.to_string()]);
    assert!(ok, "kat deprecate failed: {deprecate_err}\n{deprecate_out}");
    let prev_v_id = ObjectId::from_str(id_line(&deprecate_out, "previous_version_id")).unwrap();
    let v2_id = ObjectId::from_str(id_line(&deprecate_out, "version_id")).unwrap();
    let s2_id = ObjectId::from_str(id_line(&deprecate_out, "state_id")).unwrap();
    let c2_change_id = ChangeId::from_str(id_line(&deprecate_out, "change_id")).unwrap();
    let c2_rev_id = ObjectId::from_str(id_line(&deprecate_out, "change_revision_id")).unwrap();

    assert_eq!(prev_v_id, v1_id);
    assert_ne!(v2_id, v1_id);

    // 3. Reopen repository -> accepted head is { S2, C2 }
    let reopened = open_repository(root).unwrap();
    assert_eq!(reopened.accepted.state, s2_id);
    assert_eq!(reopened.accepted.change, Some(c2_rev_id));

    // 4. History lists C2 -> C1
    let entries = history(&reopened).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].revision_id, c2_rev_id);
    assert_eq!(entries[0].change.change_id, c2_change_id);
    assert_eq!(entries[0].change.result_state, s2_id);
    assert_eq!(entries[0].change.base_states, vec![s1_id]);
    assert_eq!(entries[0].change.dependencies, vec![c1_rev_id]);
    assert_eq!(
        entries[0].change.operations,
        vec![Operation::DeprecateElement {
            element_id,
            expected_version: v1_id,
            new_version: v2_id,
        }]
    );

    // 5. kat show resolves V2 with lifecycle Deprecated
    let (show_out, show_err, ok) = run_kat(root, &["show", &element_id.to_string()]);
    assert!(ok, "kat show failed: {show_err}\n{show_out}");
    assert!(show_out.contains("lifecycle: deprecated"));

    // 6. Deprecating an already deprecated element fails with 'not active'
    let (deprecate_again_out, deprecate_again_err, ok) =
        run_kat(root, &["deprecate", &element_id.to_string()]);
    assert!(!ok);
    assert!(deprecate_again_out.is_empty());
    assert!(deprecate_again_err.contains("is not active in the base state"));
}

#[test]
fn phase3_acceptance_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // 1. kat init
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    // 2. kat create requirement --title "Architecture" --description "Core"
    let (create_out, create_err, ok) = run_kat(
        root,
        &[
            "create",
            "requirement",
            "--title",
            "Architecture",
            "--description",
            "Core",
        ],
    );
    assert!(ok, "kat create failed: {create_err}\n{create_out}");
    let element_id = ElementId::from_str(id_line(&create_out, "element_id")).unwrap();
    let v1_id = ObjectId::from_str(id_line(&create_out, "version_id")).unwrap();
    let s1_id = ObjectId::from_str(id_line(&create_out, "state_id")).unwrap();
    let c1_change_id = ChangeId::from_str(id_line(&create_out, "change_id")).unwrap();
    let c1_rev_id = ObjectId::from_str(id_line(&create_out, "change_revision_id")).unwrap();

    let repo = open_repository(root).unwrap();
    let v1_bytes = repo.object_store().get(v1_id).unwrap();

    // 3. kat deprecate element_id
    let (dep_out, dep_err, ok) = run_kat(root, &["deprecate", &element_id.to_string()]);
    assert!(ok, "kat deprecate failed: {dep_err}\n{dep_out}");

    let out_element_id = ElementId::from_str(id_line(&dep_out, "element_id")).unwrap();
    let out_prev_v_id = ObjectId::from_str(id_line(&dep_out, "previous_version_id")).unwrap();
    let v2_id = ObjectId::from_str(id_line(&dep_out, "version_id")).unwrap();
    let s2_id = ObjectId::from_str(id_line(&dep_out, "state_id")).unwrap();
    let c2_change_id = ChangeId::from_str(id_line(&dep_out, "change_id")).unwrap();
    let c2_rev_id = ObjectId::from_str(id_line(&dep_out, "change_revision_id")).unwrap();

    assert_eq!(out_element_id, element_id);
    assert_eq!(out_prev_v_id, v1_id);
    assert_ne!(v2_id, v1_id);

    // 4. Reopen repository (fresh process) -> accepted state == S2, accepted change == C2
    let reopened = open_repository(root).unwrap();
    assert_eq!(reopened.accepted.state, s2_id);
    assert_eq!(reopened.accepted.change, Some(c2_rev_id));

    // 5. S2 maps E1 -> V2
    let context = prepare_change(&reopened).unwrap();
    assert_eq!(context.base_state_id, s2_id);
    assert_eq!(context.base_state.elements.len(), 1);
    assert_eq!(context.base_state.elements[0].element_id, element_id);
    assert_eq!(context.base_state.elements[0].version, v2_id);

    // 6. C2: base_states == [S1], result_state == S2, operations = [DeprecateElement(E1, V1, V2)]
    let entries = history(&reopened).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].revision_id, c2_rev_id);
    assert_eq!(entries[0].change.change_id, c2_change_id);
    assert_eq!(entries[0].change.result_state, s2_id);
    assert_eq!(entries[0].change.base_states, vec![s1_id]);
    assert_eq!(entries[0].change.dependencies, vec![c1_rev_id]);
    assert_eq!(
        entries[0].change.operations,
        vec![Operation::DeprecateElement {
            element_id,
            expected_version: v1_id,
            new_version: v2_id,
        }]
    );

    // 7. C1: result_state == S1
    assert_eq!(entries[1].revision_id, c1_rev_id);
    assert_eq!(entries[1].change.change_id, c1_change_id);
    assert_eq!(entries[1].change.result_state, s1_id);

    // 8. kat show E1 -> lifecycle deprecated
    let (show_out, show_err, ok) = run_kat(root, &["show", &element_id.to_string()]);
    assert!(ok, "kat show failed: {show_err}");
    assert!(show_out.contains(&format!("version_id: {v2_id}")));
    assert!(show_out.contains("lifecycle: deprecated"));

    // 9. V1 still present in objects/ byte-for-byte unchanged
    assert_eq!(reopened.object_store().get(v1_id).unwrap(), v1_bytes);
}

// ---------------------------------------------------------------------------
// kat supersede CLI tests
// ---------------------------------------------------------------------------

#[test]
fn kat_supersede_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // 1. kat init
    let (out, err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {err}\n{out}");

    // 2. kat create design-decision --title "Old Decision"
    let (create_out, create_err, ok) = run_kat(
        root,
        &["create", "design-decision", "--title", "Old Decision"],
    );
    assert!(ok, "kat create failed: {create_err}\n{create_out}");
    let e1 = id_line(&create_out, "element_id");
    let v1 = id_line(&create_out, "version_id");

    // 3. kat supersede <E1> design-decision --title "New Decision" --description "Replaces old"
    let (sup_out, sup_err, ok) = run_kat(
        root,
        &[
            "supersede",
            e1,
            "design-decision",
            "--title",
            "New Decision",
            "--description",
            "Replaces old",
        ],
    );
    assert!(ok, "kat supersede failed: {sup_err}\n{sup_out}");

    assert!(sup_out.contains(&format!("existing_element_id: {e1}")));
    assert!(sup_out.contains(&format!("previous_version_id: {v1}")));

    let v1_next = id_line(&sup_out, "superseded_version_id");
    let e2 = id_line(&sup_out, "replacement_element_id");
    let v2 = id_line(&sup_out, "replacement_version_id");
    let r1 = id_line(&sup_out, "relationship_id");
    let r1v = id_line(&sup_out, "relationship_version_id");
    let snext = id_line(&sup_out, "state_id");
    let cnext = id_line(&sup_out, "change_revision_id");

    assert_ne!(v1, v1_next);
    assert_ne!(e1, e2);
    assert!(!r1.is_empty());
    assert!(!r1v.is_empty());
    assert!(!snext.is_empty());
    assert!(!cnext.is_empty());

    // 4. kat show E1 -> lifecycle: superseded
    let (show1_out, show1_err, ok) = run_kat(root, &["show", e1]);
    assert!(ok, "kat show E1 failed: {show1_err}");
    assert!(show1_out.contains(&format!("version_id: {v1_next}")));
    assert!(show1_out.contains("lifecycle: superseded"));

    // 5. kat show E2 -> lifecycle: active
    let (show2_out, show2_err, ok) = run_kat(root, &["show", e2]);
    assert!(ok, "kat show E2 failed: {show2_err}");
    assert!(show2_out.contains(&format!("version_id: {v2}")));
    assert!(show2_out.contains("lifecycle: active"));
    assert!(show2_out.contains("title: New Decision"));

    // 6. kat history -> contains supersede operation
    let (hist_out, hist_err, ok) = run_kat(root, &["history"]);
    assert!(ok, "kat history failed: {hist_err}");
    assert!(hist_out.contains(&format!("supersede {e1} {v1} {e2} {v2} {r1v}")));

    // 7. kat update E1 -> fails (not active)
    let (up1_out, up1_err, ok) = run_kat(root, &["update", e1, "--title", "Tampered E1"]);
    assert!(!ok, "kat update E1 should fail but succeeded:\n{up1_out}");
    assert!(up1_err.contains("is not active in the base state"));

    // 8. kat update E2 -> succeeds
    let (up2_out, up2_err, ok) = run_kat(root, &["update", e2, "--title", "Updated Replacement"]);
    assert!(ok, "kat update E2 failed: {up2_err}\n{up2_out}");
}

#[test]
fn kat_supersede_outside_repository_fails() {
    let dir = tempfile::tempdir().unwrap();
    let (_out, err, ok) = run_kat(
        dir.path(),
        &[
            "supersede",
            "00000000-0000-0000-0000-000000000001",
            "design-decision",
            "--title",
            "New",
        ],
    );
    assert!(!ok);
    assert!(err.contains("no KAT repository found"));
}

#[test]
fn kat_supersede_unknown_existing_element_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run_kat(root, &["init"]);
    let (_out, err, ok) = run_kat(
        root,
        &[
            "supersede",
            "00000000-0000-0000-0000-000000000099",
            "design-decision",
            "--title",
            "New",
        ],
    );
    assert!(!ok);
    assert!(err.contains("not found in the base state"));
}

#[test]
fn kat_supersede_invalid_element_id_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run_kat(root, &["init"]);
    let (_out, err, ok) = run_kat(
        root,
        &[
            "supersede",
            "not-a-uuid",
            "design-decision",
            "--title",
            "New",
        ],
    );
    assert!(!ok);
    assert!(err.contains("invalid element ID"));
}

#[test]
fn kat_supersede_unknown_replacement_type_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run_kat(root, &["init"]);
    let (create_out, _, _) = run_kat(root, &["create", "design-decision", "--title", "Old"]);
    let e1 = id_line(&create_out, "element_id");

    let (_out, err, ok) = run_kat(root, &["supersede", e1, "unknown-type", "--title", "New"]);
    assert!(!ok);
    assert!(err.contains("unknown element type"));
}

#[test]
fn kat_supersede_forbidden_ontology_type_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run_kat(root, &["init"]);
    let (create_out, _, _) = run_kat(root, &["create", "requirement", "--title", "Req Old"]);
    let e1 = id_line(&create_out, "element_id");

    // Attempting to supersede a requirement fails ontology validation
    let (_out, err, ok) = run_kat(
        root,
        &["supersede", e1, "requirement", "--title", "Req New"],
    );
    assert!(!ok);
    assert!(err.contains("does not allow source element type"));
}

#[test]
fn kat_supersede_missing_title_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run_kat(root, &["init"]);
    let (_out, err, ok) = run_kat(
        root,
        &[
            "supersede",
            "00000000-0000-0000-0000-000000000001",
            "design-decision",
        ],
    );
    assert!(!ok);
    assert!(err.contains("--title is required"));
}

#[test]
fn phase4_acceptance_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // 1. kat init (Object count = 2: O1, S0)
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    let repo0 = open_repository(root).unwrap();
    let s0_id = repo0.accepted.state;
    let initial_objects = std::fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .count();
    assert_eq!(initial_objects, 2);

    // 2. kat create design-decision --title "Original decision"
    let (create_out, create_err, ok) = run_kat(
        root,
        &["create", "design-decision", "--title", "Original decision"],
    );
    assert!(ok, "kat create failed: {create_err}\n{create_out}");
    let e1 = ElementId::from_str(id_line(&create_out, "element_id")).unwrap();
    let v1_id = ObjectId::from_str(id_line(&create_out, "version_id")).unwrap();
    let s1_id = ObjectId::from_str(id_line(&create_out, "state_id")).unwrap();
    let c1_change_id = ChangeId::from_str(id_line(&create_out, "change_id")).unwrap();
    let c1_rev_id = ObjectId::from_str(id_line(&create_out, "change_revision_id")).unwrap();

    let repo1 = open_repository(root).unwrap();
    let v1_bytes = repo1.object_store().get(v1_id).unwrap();
    let after_create_objects = std::fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .count();
    assert_eq!(after_create_objects, 5); // + V1, S1, C1

    // 3. kat supersede E1 design-decision --title "Replacement decision"
    let (sup_out, sup_err, ok) = run_kat(
        root,
        &[
            "supersede",
            &e1.to_string(),
            "design-decision",
            "--title",
            "Replacement decision",
        ],
    );
    assert!(ok, "kat supersede failed: {sup_err}\n{sup_out}");

    let out_e1 = ElementId::from_str(id_line(&sup_out, "existing_element_id")).unwrap();
    let out_prev_v1 = ObjectId::from_str(id_line(&sup_out, "previous_version_id")).unwrap();
    let v1_next_id = ObjectId::from_str(id_line(&sup_out, "superseded_version_id")).unwrap();

    let e2 = ElementId::from_str(id_line(&sup_out, "replacement_element_id")).unwrap();
    let v2_id = ObjectId::from_str(id_line(&sup_out, "replacement_version_id")).unwrap();

    let r1 = RelationshipId::from_str(id_line(&sup_out, "relationship_id")).unwrap();
    let r1v_id = ObjectId::from_str(id_line(&sup_out, "relationship_version_id")).unwrap();

    let s2_id = ObjectId::from_str(id_line(&sup_out, "state_id")).unwrap();
    let c2_change_id = ChangeId::from_str(id_line(&sup_out, "change_id")).unwrap();
    let c2_rev_id = ObjectId::from_str(id_line(&sup_out, "change_revision_id")).unwrap();

    assert_eq!(out_e1, e1);
    assert_eq!(out_prev_v1, v1_id);
    assert_ne!(v1_next_id, v1_id);
    assert_ne!(e2, e1);

    let after_supersede_objects = std::fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .count();
    assert_eq!(after_supersede_objects, 10); // + V1_next, V2, R1V, S2, C2

    // 4. Reopen repository (fresh process) -> accepted ref == { S2, C2 }
    let reopened = open_repository(root).unwrap();
    assert_eq!(reopened.accepted.state, s2_id);
    assert_eq!(reopened.accepted.change, Some(c2_rev_id));

    // 5. S2 contains E1 -> V1_next (Superseded), E2 -> V2 (Active), R1 -> R1V
    let context = prepare_change(&reopened).unwrap();
    assert_eq!(context.base_state_id, s2_id);
    assert_eq!(context.base_state.elements.len(), 2);

    let elem_e1 = context
        .base_state
        .elements
        .iter()
        .find(|e| e.element_id == e1)
        .unwrap();
    assert_eq!(elem_e1.version, v1_next_id);

    let elem_e2 = context
        .base_state
        .elements
        .iter()
        .find(|e| e.element_id == e2)
        .unwrap();
    assert_eq!(elem_e2.version, v2_id);

    assert_eq!(context.base_state.relationships.len(), 1);
    let rel_r1 = &context.base_state.relationships[0];
    assert_eq!(rel_r1.relationship_id, r1);
    assert_eq!(rel_r1.version, r1v_id);

    // Verify R1V relationship contents
    let r1v_bytes = reopened.object_store().get(r1v_id).unwrap();
    let r1v_obj = decode_canonical(&r1v_bytes).unwrap();
    let rel_v_obj = match r1v_obj.payload {
        CanonicalPayload::RelationshipVersion(v) => v,
        _ => panic!("expected RelationshipVersion payload"),
    };
    assert_eq!(rel_v_obj.relationship_id, r1);
    assert_eq!(rel_v_obj.relationship_type, "kat.core/supersedes");
    assert_eq!(rel_v_obj.source_element_id, e2);
    assert_eq!(rel_v_obj.target_element_id, e1);

    // 6. C2 history assertions
    let entries = history(&reopened).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].revision_id, c2_rev_id);
    assert_eq!(entries[0].change.change_id, c2_change_id);
    assert_eq!(entries[0].change.result_state, s2_id);
    assert_eq!(entries[0].change.base_states, vec![s1_id]);
    assert_eq!(entries[0].change.dependencies, vec![c1_rev_id]);
    assert_eq!(
        entries[0].change.operations,
        vec![Operation::Supersede {
            existing_element: e1,
            expected_existing_version: v1_id,
            replacement_element: e2,
            replacement_version: v2_id,
            superseding_relationship: r1v_id,
        }]
    );

    assert_eq!(entries[1].revision_id, c1_rev_id);
    assert_eq!(entries[1].change.change_id, c1_change_id);
    assert_eq!(entries[1].change.result_state, s1_id);
    assert_eq!(entries[1].change.base_states, vec![s0_id]);

    // 7. CLI queries
    let (show1_out, show1_err, ok) = run_kat(root, &["show", &e1.to_string()]);
    assert!(ok, "kat show E1 failed: {show1_err}");
    assert!(show1_out.contains(&format!("version_id: {v1_next_id}")));
    assert!(show1_out.contains("lifecycle: superseded"));

    let (show2_out, show2_err, ok) = run_kat(root, &["show", &e2.to_string()]);
    assert!(ok, "kat show E2 failed: {show2_err}");
    assert!(show2_out.contains(&format!("version_id: {v2_id}")));
    assert!(show2_out.contains("lifecycle: active"));
    assert!(show2_out.contains("title: Replacement decision"));

    let (hist_out, hist_err, ok) = run_kat(root, &["history"]);
    assert!(ok, "kat history failed: {hist_err}");
    assert!(hist_out.contains(&format!("supersede {e1} {v1_id} {e2} {v2_id} {r1v_id}")));

    // 8. Update behavior
    let (up1_out, up1_err, ok) =
        run_kat(root, &["update", &e1.to_string(), "--title", "must fail"]);
    assert!(!ok, "kat update E1 should fail:\n{up1_out}");
    assert!(up1_err.contains("is not active in the base state"));

    let (up2_out, up2_err, ok) = run_kat(
        root,
        &["update", &e2.to_string(), "--title", "still active"],
    );
    assert!(ok, "kat update E2 failed: {up2_err}\n{up2_out}");

    // 9. V1 preserved in ObjectStore byte-for-byte unchanged
    assert_eq!(reopened.object_store().get(v1_id).unwrap(), v1_bytes);
}

// ---------------------------------------------------------------------------
// Step 5.7 — CLI `kat link` tests
// ---------------------------------------------------------------------------

#[test]
fn kat_link_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // 1. kat init
    let (out, err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {err}\n{out}");

    // 2. kat create design-decision --title "Decision 1"
    let (c1_out, c1_err, ok) = run_kat(
        root,
        &["create", "design-decision", "--title", "Decision 1"],
    );
    assert!(ok, "kat create E1 failed: {c1_err}\n{c1_out}");
    let e1 = id_line(&c1_out, "element_id");

    // 3. kat create requirement --title "Requirement 1"
    let (c2_out, c2_err, ok) =
        run_kat(root, &["create", "requirement", "--title", "Requirement 1"]);
    assert!(ok, "kat create E2 failed: {c2_err}\n{c2_out}");
    let e2 = id_line(&c2_out, "element_id");

    // 4. kat link addresses <E1> <E2> --description "Link decision to requirement"
    let (link_out, link_err, ok) = run_kat(
        root,
        &[
            "link",
            "addresses",
            e1,
            e2,
            "--description",
            "Link decision to requirement",
        ],
    );
    assert!(ok, "kat link failed: {link_err}\n{link_out}");

    let r1 = id_line(&link_out, "relationship_id");
    let r1v = id_line(&link_out, "relationship_version_id");
    let snext = id_line(&link_out, "state_id");
    let cnext = id_line(&link_out, "change_revision_id");

    assert!(!r1.is_empty());
    assert!(!r1v.is_empty());
    assert!(!snext.is_empty());
    assert!(!cnext.is_empty());
    assert!(link_out.contains(&format!("source_element_id: {e1}")));
    assert!(link_out.contains(&format!("target_element_id: {e2}")));

    // 5. kat show E1 and kat show E2 remain active & unchanged
    let (show1_out, _, ok1) = run_kat(root, &["show", e1]);
    assert!(ok1);
    assert!(show1_out.contains("lifecycle: active"));

    let (show2_out, _, ok2) = run_kat(root, &["show", e2]);
    assert!(ok2);
    assert!(show2_out.contains("lifecycle: active"));

    // 6. kat history contains exactly one `link <R1V>` operation
    let (hist_out, hist_err, ok) = run_kat(root, &["history"]);
    assert!(ok, "kat history failed: {hist_err}");
    assert!(hist_out.contains(&format!("link {r1v}")));

    // 7. Duplicate link fails
    let (dup_out, dup_err, ok) = run_kat(root, &["link", "addresses", e1, e2]);
    assert!(!ok, "duplicate link should fail but passed:\n{dup_out}");
    assert!(dup_err.contains("already exists between source element"));
}

#[test]
fn kat_link_short_and_qualified_relationship_types() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);
    let (c1_out, _, _) = run_kat(root, &["create", "design-decision", "--title", "D1"]);
    let (c2_out, _, _) = run_kat(root, &["create", "requirement", "--title", "R1"]);
    let e1 = id_line(&c1_out, "element_id");
    let e2 = id_line(&c2_out, "element_id");

    // Qualified relationship type
    let (out, err, ok) = run_kat(root, &["link", "kat.core/addresses", e1, e2]);
    assert!(ok, "qualified link failed: {err}\n{out}");
}

#[test]
fn kat_link_unknown_relationship_type_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);
    let (c1_out, _, _) = run_kat(root, &["create", "design-decision", "--title", "D1"]);
    let (c2_out, _, _) = run_kat(root, &["create", "requirement", "--title", "R1"]);
    let e1 = id_line(&c1_out, "element_id");
    let e2 = id_line(&c2_out, "element_id");

    let (out, err, ok) = run_kat(root, &["link", "nonexistent", e1, e2]);
    assert!(!ok, "unknown link type should fail:\n{out}");
    assert!(err.contains("unknown relationship type 'nonexistent'"));
}

#[test]
fn kat_link_forbidden_ontology_types_fail() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);
    let (c1_out, _, _) = run_kat(root, &["create", "design-decision", "--title", "D1"]);
    let (c2_out, _, _) = run_kat(root, &["create", "requirement", "--title", "R1"]);
    let e1 = id_line(&c1_out, "element_id");
    let e2 = id_line(&c2_out, "element_id");

    // addresses expects source: design-decision, target: requirement.
    // Reversing endpoints should fail ontology validation.
    let (out, err, ok) = run_kat(root, &["link", "addresses", e2, e1]);
    assert!(!ok, "forbidden direction should fail:\n{out}");
    assert!(err.contains("does not allow source element type"));
}

#[test]
fn kat_link_missing_endpoints_fail() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);
    let (c1_out, _, _) = run_kat(root, &["create", "design-decision", "--title", "D1"]);
    let e1 = id_line(&c1_out, "element_id");
    let missing_id = "00000000-0000-0000-0000-000000000000";

    let (_out1, err1, ok1) = run_kat(root, &["link", "addresses", missing_id, e1]);
    assert!(!ok1);
    assert!(err1.contains("not found in the base state"));

    let (_out2, err2, ok2) = run_kat(root, &["link", "addresses", e1, missing_id]);
    assert!(!ok2);
    assert!(err2.contains("not found in the base state"));
}

#[test]
fn kat_link_non_active_source_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);
    let (c1_out, _, _) = run_kat(root, &["create", "design-decision", "--title", "D1"]);
    let (c2_out, _, _) = run_kat(root, &["create", "requirement", "--title", "R1"]);
    let e1 = id_line(&c1_out, "element_id");
    let e2 = id_line(&c2_out, "element_id");

    run_kat(root, &["deprecate", e1]);

    let (out, err, ok) = run_kat(root, &["link", "addresses", e1, e2]);
    assert!(!ok, "deprecated source link should fail:\n{out}");
    assert!(err.contains("is not active in the base state"));
}

#[test]
fn kat_link_to_deprecated_target_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);
    let (c1_out, _, _) = run_kat(root, &["create", "design-decision", "--title", "D1"]);
    let (c2_out, _, _) = run_kat(root, &["create", "requirement", "--title", "R1"]);
    let e1 = id_line(&c1_out, "element_id");
    let e2 = id_line(&c2_out, "element_id");

    // Deprecate target requirement
    run_kat(root, &["deprecate", e2]);

    // Linking active source to deprecated target succeeds!
    let (out, err, ok) = run_kat(root, &["link", "addresses", e1, e2]);
    assert!(ok, "link to deprecated target failed: {err}\n{out}");
}

#[test]
fn kat_link_malformed_arguments_fail() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);

    let (_out1, err1, ok1) = run_kat(root, &["link", "addresses", "not-a-uuid", "not-a-uuid"]);
    assert!(!ok1);
    assert!(err1.contains("invalid element ID"));

    let (_out2, err2, ok2) = run_kat(root, &["link", "addresses"]);
    assert!(!ok2);
    assert!(err2.contains("expected <relationship-type>"));

    let (_out3, err3, ok3) = run_kat(
        root,
        &[
            "link",
            "addresses",
            "00000000-0000-0000-0000-000000000001",
            "00000000-0000-0000-0000-000000000002",
            "--unknown",
            "val",
        ],
    );
    assert!(!ok3);
    assert!(err3.contains("unknown option"));
}

#[test]
fn kat_link_outside_repository_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let (_out, err, ok) = run_kat(
        root,
        &[
            "link",
            "addresses",
            "00000000-0000-0000-0000-000000000001",
            "00000000-0000-0000-0000-000000000002",
        ],
    );
    assert!(!ok);
    assert!(err.contains("no KAT repository found"));
}

// ---------------------------------------------------------------------------
// Step 5.8 — Acceptance Verification & Phase 5 Closure
// ---------------------------------------------------------------------------

#[test]
fn phase5_acceptance_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // 1. kat init
    let (out, err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {err}\n{out}");

    let count0 = std::fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .count();
    assert_eq!(
        count0, 2,
        "init must materialize exactly 2 objects (O1, S0)"
    );

    // 2. kat create requirement --title "User must authenticate"
    let (c1_out, c1_err, ok) = run_kat(
        root,
        &["create", "requirement", "--title", "User must authenticate"],
    );
    assert!(ok, "kat create requirement failed: {c1_err}\n{c1_out}");
    let e_req = ElementId::from_str(id_line(&c1_out, "element_id")).unwrap();
    let v_req_id = ObjectId::from_str(id_line(&c1_out, "version_id")).unwrap();
    let s1_id = ObjectId::from_str(id_line(&c1_out, "state_id")).unwrap();
    let c1_change_id = ChangeId::from_str(id_line(&c1_out, "change_id")).unwrap();
    let c1_rev_id = ObjectId::from_str(id_line(&c1_out, "change_revision_id")).unwrap();

    let count1 = std::fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .count();
    assert_eq!(
        count1, 5,
        "create requirement must increase objects by 3 (V1, S1, C1)"
    );

    // 3. kat create design-decision --title "Use OAuth2"
    let (c2_out, c2_err, ok) = run_kat(
        root,
        &["create", "design-decision", "--title", "Use OAuth2"],
    );
    assert!(ok, "kat create design-decision failed: {c2_err}\n{c2_out}");
    let e_dec = ElementId::from_str(id_line(&c2_out, "element_id")).unwrap();
    let v_dec_id = ObjectId::from_str(id_line(&c2_out, "version_id")).unwrap();
    let s2_id = ObjectId::from_str(id_line(&c2_out, "state_id")).unwrap();
    let c2_change_id = ChangeId::from_str(id_line(&c2_out, "change_id")).unwrap();
    let c2_rev_id = ObjectId::from_str(id_line(&c2_out, "change_revision_id")).unwrap();

    let count2 = std::fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .count();
    assert_eq!(
        count2, 8,
        "create design-decision must increase objects by 3 (V2, S2, C2)"
    );

    // Save endpoint version bytes prior to link
    let repo0 = open_repository(root).unwrap();
    let v_req_bytes = repo0.object_store().get(v_req_id).unwrap();
    let v_dec_bytes = repo0.object_store().get(v_dec_id).unwrap();

    // 4. kat link addresses <e_dec> <e_req>
    let (link_out, link_err, ok) = run_kat(
        root,
        &[
            "link",
            "addresses",
            &e_dec.to_string(),
            &e_req.to_string(),
            "--description",
            "OAuth2 addresses user authentication",
        ],
    );
    assert!(ok, "kat link failed: {link_err}\n{link_out}");

    let r1 = RelationshipId::from_str(id_line(&link_out, "relationship_id")).unwrap();
    let r1v_id = ObjectId::from_str(id_line(&link_out, "relationship_version_id")).unwrap();
    let s3_id = ObjectId::from_str(id_line(&link_out, "state_id")).unwrap();
    let c3_change_id = ChangeId::from_str(id_line(&link_out, "change_id")).unwrap();
    let c3_rev_id = ObjectId::from_str(id_line(&link_out, "change_revision_id")).unwrap();

    let count3 = std::fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .count();
    assert_eq!(
        count3, 11,
        "link must increase objects by EXACTLY 3 (R1V, S3, C3)"
    );

    // 5. Fresh process reopen verification
    let reopened = open_repository(root).unwrap();
    assert_eq!(reopened.accepted.state, s3_id);
    assert_eq!(reopened.accepted.change, Some(c3_rev_id));

    let context = prepare_change(&reopened).unwrap();
    assert_eq!(context.base_state_id, s3_id);

    // Verify elements in S3 are byte-for-byte identical to S2
    assert_eq!(context.base_state.elements.len(), 2);
    let req_entry = context
        .base_state
        .elements
        .iter()
        .find(|e| e.element_id == e_req)
        .unwrap();
    let dec_entry = context
        .base_state
        .elements
        .iter()
        .find(|e| e.element_id == e_dec)
        .unwrap();
    assert_eq!(req_entry.version, v_req_id);
    assert_eq!(dec_entry.version, v_dec_id);

    // Verify endpoint bytes in ObjectStore remain unchanged
    assert_eq!(reopened.object_store().get(v_req_id).unwrap(), v_req_bytes);
    assert_eq!(reopened.object_store().get(v_dec_id).unwrap(), v_dec_bytes);

    // Verify relationship in S3
    assert_eq!(context.base_state.relationships.len(), 1);
    let rel_r1 = &context.base_state.relationships[0];
    assert_eq!(rel_r1.relationship_id, r1);
    assert_eq!(rel_r1.version, r1v_id);

    // Verify R1V relationship payload object
    let r1v_bytes = reopened.object_store().get(r1v_id).unwrap();
    let r1v_obj = decode_canonical(&r1v_bytes).unwrap();
    let rel_v_payload = match r1v_obj.payload {
        CanonicalPayload::RelationshipVersion(v) => v,
        _ => panic!("expected RelationshipVersion payload"),
    };
    assert_eq!(rel_v_payload.relationship_id, r1);
    assert_eq!(rel_v_payload.relationship_type, "kat.core/addresses");
    assert_eq!(rel_v_payload.source_element_id, e_dec);
    assert_eq!(rel_v_payload.target_element_id, e_req);

    // 6. History verification (C3 -> C2 -> C1)
    let entries = history(&reopened).unwrap();
    assert_eq!(entries.len(), 3);

    // C3: Link
    assert_eq!(entries[0].revision_id, c3_rev_id);
    assert_eq!(entries[0].change.change_id, c3_change_id);
    assert_eq!(entries[0].change.result_state, s3_id);
    assert_eq!(entries[0].change.base_states, vec![s2_id]);
    assert_eq!(entries[0].change.dependencies, vec![c2_rev_id]);
    assert_eq!(
        entries[0].change.operations,
        vec![Operation::Link {
            new_relationship_version: r1v_id,
        }]
    );

    // C2: Create decision
    assert_eq!(entries[1].revision_id, c2_rev_id);
    assert_eq!(entries[1].change.change_id, c2_change_id);
    assert_eq!(entries[1].change.result_state, s2_id);
    assert_eq!(entries[1].change.base_states, vec![s1_id]);
    assert_eq!(entries[1].change.dependencies, vec![c1_rev_id]);

    // C1: Create requirement
    assert_eq!(entries[2].revision_id, c1_rev_id);
    assert_eq!(entries[2].change.change_id, c1_change_id);
    assert_eq!(entries[2].change.result_state, s1_id);
    assert_eq!(entries[2].change.dependencies, vec![]);

    // 7. CLI output queries
    let (show_req_out, _, ok_req) = run_kat(root, &["show", &e_req.to_string()]);
    assert!(ok_req);
    assert!(show_req_out.contains(&format!("version_id: {v_req_id}")));
    assert!(show_req_out.contains("lifecycle: active"));

    let (show_dec_out, _, ok_dec) = run_kat(root, &["show", &e_dec.to_string()]);
    assert!(ok_dec);
    assert!(show_dec_out.contains(&format!("version_id: {v_dec_id}")));
    assert!(show_dec_out.contains("lifecycle: active"));

    let (hist_out, _, ok_hist) = run_kat(root, &["history"]);
    assert!(ok_hist);
    assert!(hist_out.contains(&format!("link {r1v_id}")));

    // 8. Duplicate link attempt MUST fail
    let (dup_out, dup_err, ok_dup) = run_kat(
        root,
        &["link", "addresses", &e_dec.to_string(), &e_req.to_string()],
    );
    assert!(!ok_dup, "duplicate link should fail:\n{dup_out}");
    assert!(dup_err.contains("already exists between source element"));

    // 9. Reversed link direction MUST fail ontology validation
    let (rev_out, rev_err, ok_rev) = run_kat(
        root,
        &["link", "addresses", &e_req.to_string(), &e_dec.to_string()],
    );
    assert!(!ok_rev, "reversed direction link should fail:\n{rev_out}");
    assert!(rev_err.contains("does not allow source element type"));
}

// ---------------------------------------------------------------------------
// Step 6.6 — kat unlink CLI tests
// ---------------------------------------------------------------------------

#[test]
fn kat_unlink_outside_repository_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let missing_rel = "00000000-0000-0000-0000-000000000001";
    let (_out, err, ok) = run_kat(root, &["unlink", missing_rel]);
    assert!(!ok);
    assert!(err.contains("no KAT repository found"));
}

#[test]
fn kat_unlink_malformed_arguments_fail() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run_kat(root, &["init"]);

    let (_out1, err1, ok1) = run_kat(root, &["unlink"]);
    assert!(!ok1);
    assert!(err1.contains("expected relationship ID"));

    let (_out2, err2, ok2) = run_kat(root, &["unlink", "not-a-uuid"]);
    assert!(!ok2);
    assert!(err2.contains("invalid relationship ID"));

    let (_out3, err3, ok3) = run_kat(
        root,
        &[
            "unlink",
            "00000000-0000-0000-0000-000000000001",
            "--unknown",
        ],
    );
    assert!(!ok3);
    assert!(err3.contains("unexpected option"));
}

#[test]
fn kat_unlink_unknown_relationship_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run_kat(root, &["init"]);

    let missing_rel = "00000000-0000-0000-0000-000000000001";
    let (_out, err, ok) = run_kat(root, &["unlink", missing_rel]);
    assert!(!ok);
    assert!(err.contains("not found in the accepted state"));
}

#[test]
fn phase6_acceptance_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // 1. kat init (Object count = 2: O1, S0)
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    let initial_objects = std::fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .count();
    assert_eq!(initial_objects, 2);

    // 2. kat create requirement --title "User must authenticate"
    let (c1_out, c1_err, ok) = run_kat(
        root,
        &["create", "requirement", "--title", "User must authenticate"],
    );
    assert!(ok, "kat create requirement failed: {c1_err}\n{c1_out}");
    let e_req = ElementId::from_str(id_line(&c1_out, "element_id")).unwrap();
    let v_req_id = ObjectId::from_str(id_line(&c1_out, "version_id")).unwrap();
    let c1_rev_id = ObjectId::from_str(id_line(&c1_out, "change_revision_id")).unwrap();

    let after_req_objects = std::fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .count();
    assert_eq!(after_req_objects, 5); // + V_req, S1, C1

    // 3. kat create design-decision --title "Use OAuth2"
    let (c2_out, c2_err, ok) = run_kat(
        root,
        &["create", "design-decision", "--title", "Use OAuth2"],
    );
    assert!(ok, "kat create design-decision failed: {c2_err}\n{c2_out}");
    let e_dec = ElementId::from_str(id_line(&c2_out, "element_id")).unwrap();
    let v_dec_id = ObjectId::from_str(id_line(&c2_out, "version_id")).unwrap();
    let c2_rev_id = ObjectId::from_str(id_line(&c2_out, "change_revision_id")).unwrap();

    let after_dec_objects = std::fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .count();
    assert_eq!(after_dec_objects, 8); // + V_dec, S2, C2

    // 4. kat link addresses <design-decision-id> <requirement-id>
    let (link_out, link_err, ok) = run_kat(
        root,
        &["link", "addresses", &e_dec.to_string(), &e_req.to_string()],
    );
    assert!(ok, "kat link failed: {link_err}\n{link_out}");
    let r1_id = RelationshipId::from_str(id_line(&link_out, "relationship_id")).unwrap();
    let r1v_id = ObjectId::from_str(id_line(&link_out, "relationship_version_id")).unwrap();
    let s3_id = ObjectId::from_str(id_line(&link_out, "state_id")).unwrap();
    let c3_rev_id = ObjectId::from_str(id_line(&link_out, "change_revision_id")).unwrap();

    let after_link_objects = std::fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .count();
    assert_eq!(after_link_objects, 11); // + R1V, S3, C3

    // 5. kat unlink <relationship-id>
    let (unlink_out, unlink_err, ok) = run_kat(root, &["unlink", &r1_id.to_string()]);
    assert!(ok, "kat unlink failed: {unlink_err}\n{unlink_out}");
    let unlinked_r1_id = RelationshipId::from_str(id_line(&unlink_out, "relationship_id")).unwrap();
    let s4_id = ObjectId::from_str(id_line(&unlink_out, "state_id")).unwrap();
    let c4_rev_id = ObjectId::from_str(id_line(&unlink_out, "change_revision_id")).unwrap();

    assert_eq!(unlinked_r1_id, r1_id);

    // Objects count becomes 12 because S4 (relationships empty) is identical to S2 (deduplicated by CAS), so only C4 is added.
    let after_unlink_objects = std::fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .count();
    assert_eq!(after_unlink_objects, 12); // + C4 (S4 deduplicated = S2)

    // 6. Fresh reopen verification
    let reopened = open_repository(root).unwrap();
    assert_eq!(reopened.accepted.state, s4_id);
    assert_eq!(reopened.accepted.change, Some(c4_rev_id));

    // S4 maps zero relationships, elements unchanged
    let context = prepare_change(&reopened).unwrap();
    assert_eq!(context.base_state_id, s4_id);
    assert_eq!(context.base_state.relationships.len(), 0);
    assert_eq!(context.base_state.elements.len(), 2);
    let elem_dec = context
        .base_state
        .elements
        .iter()
        .find(|e| e.element_id == e_dec)
        .unwrap();
    assert_eq!(elem_dec.version, v_dec_id);
    let elem_req = context
        .base_state
        .elements
        .iter()
        .find(|e| e.element_id == e_req)
        .unwrap();
    assert_eq!(elem_req.version, v_req_id);

    // History chain: C4 -> C3 -> C2 -> C1
    let entries = history(&reopened).unwrap();
    assert_eq!(entries.len(), 4);

    // C4: Unlink R1
    assert_eq!(entries[0].revision_id, c4_rev_id);
    assert_eq!(entries[0].change.result_state, s4_id);
    assert_eq!(entries[0].change.base_states, vec![s3_id]);
    assert_eq!(entries[0].change.dependencies, vec![c3_rev_id]);
    assert_eq!(
        entries[0].change.operations,
        vec![Operation::Unlink {
            relationship_id: r1_id,
            expected_version: r1v_id,
        }]
    );

    // C3: Link R1
    assert_eq!(entries[1].revision_id, c3_rev_id);
    assert_eq!(entries[1].change.result_state, s3_id);

    // C2: Create design decision
    assert_eq!(entries[2].revision_id, c2_rev_id);

    // C1: Create requirement
    assert_eq!(entries[3].revision_id, c1_rev_id);

    // 7. Duplicate unlink attempt MUST fail with relationship not found
    let (dup_out, dup_err, ok_dup) = run_kat(root, &["unlink", &r1_id.to_string()]);
    assert!(!ok_dup, "second unlink should fail:\n{dup_out}");
    assert!(dup_err.contains("not found in the accepted state"));
}

// ---------------------------------------------------------------------------
// Phase 7 CLI Acceptance Tests (Trace Origin)
// ---------------------------------------------------------------------------

#[test]
fn kat_trace_outside_repository_fails() {
    let dir = tempfile::tempdir().unwrap();
    let (_out, err, ok) = run_kat(
        dir.path(),
        &["trace", "00000000-0000-0000-0000-000000000001"],
    );
    assert!(!ok);
    assert!(err.contains("no KAT repository found"));
}

#[test]
fn kat_trace_unknown_element_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run_kat(root, &["init"]);
    let (_out, err, ok) = run_kat(root, &["trace", "00000000-0000-0000-0000-000000000099"]);
    assert!(!ok);
    assert!(err.contains("not found in the accepted state"));
}

#[test]
fn kat_trace_malformed_arguments_fail() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run_kat(root, &["init"]);

    let (_out, err1, ok1) = run_kat(root, &["trace"]);
    assert!(!ok1);
    assert!(err1.contains("expected exactly one argument"));

    let (_out, err2, ok2) = run_kat(root, &["trace", "invalid-uuid"]);
    assert!(!ok2);
    assert!(err2.contains("invalid element ID"));
}

#[test]
fn phase7_acceptance_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // 1. kat init
    let (_out, err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {err}");

    // 2. Create Intent (I1)
    let (i1_out, i1_err, ok) = run_kat(
        root,
        &["create", "intent", "--title", "AuthX Identity Service"],
    );
    assert!(ok, "create intent failed: {i1_err}");
    let e_intent = id_line(&i1_out, "element_id");

    // 3. Create Requirement (R1)
    let (r1_out, r1_err, ok) = run_kat(
        root,
        &["create", "requirement", "--title", "OAuth2 Authentication"],
    );
    assert!(ok, "create requirement failed: {r1_err}");
    let e_req = id_line(&r1_out, "element_id");

    // 4. Create Implementation (M1)
    let (m1_out, m1_err, ok) = run_kat(
        root,
        &["create", "implementation", "--title", "OAuth Core Module"],
    );
    assert!(ok, "create implementation failed: {m1_err}");
    let e_impl = id_line(&m1_out, "element_id");

    // 5. Create Artifact (A1)
    let (a1_out, a1_err, ok) = run_kat(root, &["create", "artifact", "--title", "authx-core.jar"]);
    assert!(ok, "create artifact failed: {a1_err}");
    let e_art = id_line(&a1_out, "element_id");

    // 6. Create Validation (V1)
    let (v1_out, v1_err, ok) = run_kat(
        root,
        &["create", "validation", "--title", "OAuth Integration Suite"],
    );
    assert!(ok, "create validation failed: {v1_err}");
    let e_val = id_line(&v1_out, "element_id");

    // 7. Link Intent (motivates) -> Requirement
    let (_out, err, ok) = run_kat(root, &["link", "motivates", e_intent, e_req]);
    assert!(ok, "link motivates failed: {err}");

    // 8. Link Implementation (realizes) -> Requirement
    let (_out, err, ok) = run_kat(root, &["link", "realizes", e_impl, e_req]);
    assert!(ok, "link realizes failed: {err}");

    // 9. Link Artifact (represents) -> Implementation
    let (_out, err, ok) = run_kat(root, &["link", "represents", e_art, e_impl]);
    assert!(ok, "link represents failed: {err}");

    // 10. Link Validation (validates) -> Requirement
    let (_out, err, ok) = run_kat(root, &["link", "validates", e_val, e_req]);
    assert!(ok, "link validates failed: {err}");

    // 11. kat trace Artifact A1
    let (art_trace_out, art_trace_err, ok) = run_kat(root, &["trace", e_art]);
    assert!(
        ok,
        "kat trace artifact failed: {art_trace_err}\n{art_trace_out}"
    );
    assert!(art_trace_out.contains(&format!("element_id: {e_art}")));
    assert!(art_trace_out.contains("type: kat.core/artifact"));
    assert!(art_trace_out.contains("title: authx-core.jar"));
    assert!(art_trace_out.contains("via kat.core/represents (forward)"));
    assert!(art_trace_out.contains("via kat.core/realizes (forward)"));
    assert!(art_trace_out.contains("via kat.core/motivates (backward)"));
    assert!(art_trace_out.contains(e_intent));

    // 12. kat trace Validation V1
    let (val_trace_out, val_trace_err, ok) = run_kat(root, &["trace", e_val]);
    assert!(
        ok,
        "kat trace validation failed: {val_trace_err}\n{val_trace_out}"
    );
    assert!(val_trace_out.contains(&format!("element_id: {e_val}")));
    assert!(val_trace_out.contains("type: kat.core/validation"));
    assert!(val_trace_out.contains("via kat.core/validates (forward)"));
    assert!(val_trace_out.contains("via kat.core/motivates (backward)"));
    assert!(val_trace_out.contains(e_intent));

    // 13. kat trace Intent I1 (origin root -> no origin paths)
    let (int_trace_out, int_trace_err, ok) = run_kat(root, &["trace", e_intent]);
    assert!(
        ok,
        "kat trace intent failed: {int_trace_err}\n{int_trace_out}"
    );
    assert!(int_trace_out.contains(&format!("element_id: {e_intent}")));
    assert!(int_trace_out.contains("origin: none"));

    // 14. Non-mutation verification
    let objects_before = std::fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .count();
    let refs_before = std::fs::read_to_string(root.join(".kat/refs/accepted")).unwrap();

    let (_out, _err, ok) = run_kat(root, &["trace", e_art]);
    assert!(ok);

    let objects_after = std::fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .count();
    let refs_after = std::fs::read_to_string(root.join(".kat/refs/accepted")).unwrap();

    assert_eq!(objects_after, objects_before);
    assert_eq!(refs_after, refs_before);
}

#[test]
fn kat_impact_malformed_arguments_fail() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run_kat(root, &["init"]);

    let (_out, err, ok) = run_kat(root, &["impact"]);
    assert!(!ok);
    assert!(err.contains("expected exactly one argument"));

    let (_out, err, ok) = run_kat(root, &["impact", "not-a-uuid"]);
    assert!(!ok);
    assert!(err.contains("invalid element ID"));
}

#[test]
fn kat_impact_outside_repository_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let uuid_str = Uuid::from_u128(1001).to_string();

    let (_out, err, ok) = run_kat(root, &["impact", &uuid_str]);
    assert!(!ok);
    assert!(err.contains("no KAT repository found"));
}

#[test]
fn kat_impact_unknown_element_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run_kat(root, &["init"]);
    let uuid_str = Uuid::from_u128(9999).to_string();

    let (_out, err, ok) = run_kat(root, &["impact", &uuid_str]);
    assert!(!ok);
    assert!(err.contains("not found in the accepted state"));
}

#[test]
fn phase8_acceptance_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // 1. kat init
    let (_out, err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {err}");

    // 2. Create Intent (I1)
    let (i1_out, i1_err, ok) = run_kat(
        root,
        &["create", "intent", "--title", "AuthX Identity Service"],
    );
    assert!(ok, "create intent failed: {i1_err}");
    let e_intent = id_line(&i1_out, "element_id");

    // 3. Create Requirement (R1)
    let (r1_out, r1_err, ok) = run_kat(
        root,
        &["create", "requirement", "--title", "OAuth2 Authentication"],
    );
    assert!(ok, "create requirement failed: {r1_err}");
    let e_req = id_line(&r1_out, "element_id");

    // 4. Create Decision (D1)
    let (d1_out, d1_err, ok) = run_kat(
        root,
        &["create", "design-decision", "--title", "Use PASETO Tokens"],
    );
    assert!(ok, "create decision failed: {d1_err}");
    let e_dec = id_line(&d1_out, "element_id");

    // 5. Create Implementation (M1)
    let (m1_out, m1_err, ok) = run_kat(
        root,
        &["create", "implementation", "--title", "OAuth Core Module"],
    );
    assert!(ok, "create implementation M1 failed: {m1_err}");
    let e_impl1 = id_line(&m1_out, "element_id");

    // 6. Create Implementation (M2)
    let (m2_out, m2_err, ok) = run_kat(
        root,
        &["create", "implementation", "--title", "OAuth Gateway Proxy"],
    );
    assert!(ok, "create implementation M2 failed: {m2_err}");
    let e_impl2 = id_line(&m2_out, "element_id");

    // 7. Create Artifact (A1)
    let (a1_out, a1_err, ok) = run_kat(root, &["create", "artifact", "--title", "authx-core.jar"]);
    assert!(ok, "create artifact failed: {a1_err}");
    let e_art = id_line(&a1_out, "element_id");

    // 8. Create Validation (V1)
    let (v1_out, v1_err, ok) = run_kat(
        root,
        &["create", "validation", "--title", "OAuth Integration Suite"],
    );
    assert!(ok, "create validation failed: {v1_err}");
    let e_val = id_line(&v1_out, "element_id");

    // 9. Link Intent (motivates) -> Requirement
    let (_out, err, ok) = run_kat(root, &["link", "motivates", e_intent, e_req]);
    assert!(ok, "link motivates failed: {err}");

    // 10. Link Decision (addresses) -> Requirement
    let (_out, err, ok) = run_kat(root, &["link", "addresses", e_dec, e_req]);
    assert!(ok, "link addresses failed: {err}");

    // 11. Link Implementation M1 (realizes) -> Requirement
    let (_out, err, ok) = run_kat(root, &["link", "realizes", e_impl1, e_req]);
    assert!(ok, "link realizes failed: {err}");

    // 12. Link Implementation M2 (depends-on) -> Implementation M1
    let (_out, err, ok) = run_kat(root, &["link", "depends-on", e_impl2, e_impl1]);
    assert!(ok, "link depends-on failed: {err}");

    // 13. Link Artifact A1 (represents) -> Implementation M1
    let (_out, err, ok) = run_kat(root, &["link", "represents", e_art, e_impl1]);
    assert!(ok, "link represents failed: {err}");

    // 14. Link Validation V1 (validates) -> Requirement R1
    let (_out, err, ok) = run_kat(root, &["link", "validates", e_val, e_req]);
    assert!(ok, "link validates failed: {err}");

    // 15. kat impact Requirement R1
    let (impact_out, impact_err, ok) = run_kat(root, &["impact", e_req]);
    assert!(ok, "kat impact failed: {impact_err}\n{impact_out}");

    // Verify Directly Changed
    assert!(impact_out.contains("directly_changed:"));
    assert!(impact_out.contains(e_req));

    // Verify Semantically Affected
    assert!(impact_out.contains("semantically_affected:"));
    assert!(impact_out.contains(e_dec));
    assert!(impact_out.contains(e_impl1));
    assert!(impact_out.contains(e_impl2));
    assert!(impact_out.contains(e_val));

    // Verify Affected Artifacts
    assert!(impact_out.contains("affected_artifacts:"));
    assert!(impact_out.contains(e_art));
    assert!(impact_out.contains("via kat.core/represents (backward)"));

    // 16. Non-mutation verification
    let objects_before = std::fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .count();
    let refs_before = std::fs::read_to_string(root.join(".kat/refs/accepted")).unwrap();

    let (_out, _err, ok) = run_kat(root, &["impact", e_req]);
    assert!(ok);

    let objects_after = std::fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .count();
    let refs_after = std::fs::read_to_string(root.join(".kat/refs/accepted")).unwrap();

    assert_eq!(objects_after, objects_before);
    assert_eq!(refs_after, refs_before);
}

#[test]
fn kat_validate_outside_repository_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let (_out, err, ok) = run_kat(root, &["validate"]);
    assert!(!ok);
    assert!(err.contains("no KAT repository found"));
}

#[test]
fn phase9_acceptance_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // 1. kat init
    let (_out, err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {err}");

    // 2. Create Constraint C1
    let (c1_out, c1_err, ok) = run_kat(
        root,
        &[
            "create",
            "constraint",
            "--title",
            "TLS 1.3 Encryption Required",
        ],
    );
    assert!(ok, "create constraint failed: {c1_err}");
    let e_con = id_line(&c1_out, "element_id");

    // 3. Create Decision D1
    let (d1_out, d1_err, ok) = run_kat(
        root,
        &["create", "design-decision", "--title", "Use PASETO Tokens"],
    );
    assert!(ok, "create decision failed: {d1_err}");
    let e_dec = id_line(&d1_out, "element_id");

    // 4. Link C1 (restricts) -> D1
    let (_out, err, ok) = run_kat(root, &["link", "restricts", e_con, e_dec]);
    assert!(ok, "link restricts failed: {err}");

    // 5. kat validate
    let (val_out, val_err, ok) = run_kat(root, &["validate"]);
    assert!(ok, "kat validate failed: {val_err}\n{val_out}");

    assert!(val_out.contains("semantic consistency: no violations detected"));
    assert!(val_out.contains("unverified_constraints:"));
    assert!(val_out.contains(e_con));
    assert!(val_out.contains("TLS 1.3 Encryption Required"));
    assert!(val_out.contains("[reason: no executable validation rule]"));
    assert!(val_out.contains("constrained_elements:"));
    assert!(val_out.contains(e_dec));

    // 6. Non-mutation verification
    let objects_before = std::fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .count();
    let refs_before = std::fs::read_to_string(root.join(".kat/refs/accepted")).unwrap();

    let (_out, _err, ok) = run_kat(root, &["validate"]);
    assert!(ok);

    let objects_after = std::fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .count();
    let refs_after = std::fs::read_to_string(root.join(".kat/refs/accepted")).unwrap();

    assert_eq!(objects_after, objects_before);
    assert_eq!(refs_after, refs_before);
}

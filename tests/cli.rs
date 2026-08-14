//! End-to-end CLI tests: spawn the real `kat` binary (via `CARGO_BIN_EXE_kat`,
//! no extra dependencies) against a temp repository.
//!
//! The CLI is thin parse + dispatch, so these tests assert the invocation
//! contract from `docs/cli.md` without re-testing library semantics.

use std::path::Path;
use std::process::Command;
use std::str::FromStr;

use kat::domain::identity::{ChangeId, ElementId, ObjectId};
use kat::domain::operation::Operation;
use kat::domain::property::PropertyValue;
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

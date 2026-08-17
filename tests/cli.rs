//! End-to-end CLI tests: spawn the real `kat` binary (via `CARGO_BIN_EXE_kat`,
//! no extra dependencies) against a temp repository.
//!
//! The CLI is thin parse + dispatch, so these tests assert the invocation
//! contract from `docs/cli.md` without re-testing library semantics.

use std::fs;
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
    let prefix = format!("{key}:");
    out.lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with(&prefix) {
                Some(trimmed[prefix.len()..].trim())
            } else {
                None
            }
        })
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
    assert!(out.contains(&format!("Element {}", published.element_id)));
    assert!(out.contains(&format!(
        "version:     {}",
        &published.version_id.to_string()[..12]
    )));
    assert!(out.contains("type:        kat.core/requirement"));
    assert!(out.contains("lifecycle:   active"));
    assert!(out.contains("title:       A requirement"));

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
    assert!(out.contains("Accepted change history (0 revisions)"));
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
    assert!(out.contains("Accepted change history (1 revision)"));
    assert!(out.contains(&format!(
        "Revision {}",
        &published.change_revision_id.to_string()[..12]
    )));
    assert!(out.contains(&format!("change:        {change_id}")));
    assert!(out.contains(&format!(
        "result_state:  {}",
        &published.state_id.to_string()[..12]
    )));
    assert!(out.contains(&format!("  {}", &s0.to_string()[..12])));
    assert!(out.contains("create element"));
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
    assert!(out.contains("type:        kat.core/requirement"));
    assert!(out.contains("lifecycle:   active"));
    assert!(out.contains("title:       User authentication"));

    // 9. kat history: C1 shown, S1 shown, CreateElement(V1) shown.
    let (out, err, ok) = run_kat(root, &["history"]);
    assert!(ok, "kat history failed: {err}");
    assert!(out.contains(&change_revision_id.to_string()[..12]));
    assert!(out.contains(&state_id.to_string()[..12]));
    assert!(out.contains("create element"));
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
    assert!(err.contains("required") || err.contains("--title"));
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
    assert!(err.contains("cannot be used multiple times") || err.contains("duplicate"));

    // Unknown option.
    let (out, err, ok) = run_kat(
        root,
        &["create", "requirement", "--title", "a", "--bogus", "x"],
    );
    assert!(!ok);
    assert!(out.is_empty());
    assert!(err.contains("unexpected argument '--bogus'") || err.contains("unknown option"));

    // Missing flag value.
    let (out, err, ok) = run_kat(root, &["create", "requirement", "--title"]);
    assert!(!ok);
    assert!(out.is_empty());
    assert!(
        err.contains("requires a value")
            || err.contains("a value is required")
            || err.contains("missing value")
    );

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
    assert!(out.contains("title:       T"));
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
    let _c1_change_id = ChangeId::from_str(id_line(&create_out, "change_id")).unwrap();
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
    assert_eq!(entries[1].change.result_state, s1_id);

    // 8. kat show E1 -> title "B" (resolves V2)
    let (show_out, show_err, ok) = run_kat(root, &["show", &element_id.to_string()]);
    assert!(ok, "kat show failed: {show_err}");
    assert!(show_out.contains(&format!("version:     {}", &v2_id.to_string()[..12])));
    assert!(show_out.contains("title:       B"));

    // 9. kat history -> [C2, C1] (newest first)
    let (hist_out, hist_err, ok) = run_kat(root, &["history"]);
    assert!(ok, "kat history failed: {hist_err}");
    let c2_pos = hist_out.find(&c2_rev_id.to_string()[..12]).unwrap();
    let c1_pos = hist_out.find(&c1_rev_id.to_string()[..12]).unwrap();
    assert!(c2_pos < c1_pos, "history must list C2 before C1");

    // 10. V1 still present in objects/ (previous state traceable)
    assert_eq!(reopened.object_store().get(v1_id).unwrap(), v1_bytes);
}

#[test]
fn kat_update_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (init_out, init_err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {init_err}\n{init_out}");

    // 1. Create element with title and description
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

    let reopened = open_repository(root).unwrap();
    let v1_bytes = reopened.object_store().get(v1_id).unwrap();

    // 2. Update title only
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
    let c2_rev_id = ObjectId::from_str(id_line(&update_out, "change_revision_id")).unwrap();

    assert_eq!(out_element_id, element_id);
    assert_eq!(out_prev_v_id, v1_id);
    assert_ne!(v2_id, v1_id);

    // 4. kat show E1 resolves V2, updated title, preserved description
    let (show_out, show_err, ok) = run_kat(root, &["show", &element_id.to_string()]);
    assert!(ok, "kat show failed: {show_err}");
    assert!(show_out.contains(&format!("version:     {}", &v2_id.to_string()[..12])));
    assert!(show_out.contains("title:       Updated title"));
    assert!(show_out.contains("description: Initial desc"));

    // 5. kat history shows C2 before C1 with update_element E1 V1 V2
    let (hist_out, hist_err, ok) = run_kat(root, &["history"]);
    assert!(ok, "kat history failed: {hist_err}");
    assert!(hist_out.contains(&c2_rev_id.to_string()[..12]));
    assert!(hist_out.contains("update element"));

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
    assert!(show_out.contains("title:       T"));
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
    assert!(err.contains("not found in the accepted state"));
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
    assert!(err.contains("cannot be used multiple times") || err.contains("duplicate"));

    // Unknown option
    let (_out, err, ok) = run_kat(root, &["update", &element_id.to_string(), "--bogus", "x"]);
    assert!(!ok);
    assert!(err.contains("unexpected argument '--bogus'") || err.contains("unknown option"));

    // Missing flag value
    let (_out, err, ok) = run_kat(root, &["update", &element_id.to_string(), "--title"]);
    assert!(!ok);
    assert!(
        err.contains("requires a value")
            || err.contains("a value is required")
            || err.contains("missing value")
    );

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
    assert!(err.contains("not found in the accepted state"));
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
    assert!(err.contains("required") || err.contains("expected <element-id>"));

    // Extra arguments
    let (_out, err, ok) = run_kat(root, &["deprecate", "arg1", "arg2"]);
    assert!(!ok);
    assert!(err.contains("unexpected argument") || err.contains("expected <element-id>"));
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
    assert!(show_out.contains("lifecycle:   deprecated"));

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
    assert!(show_out.contains(&format!("version:     {}", &v2_id.to_string()[..12])));
    assert!(show_out.contains("lifecycle:   deprecated"));

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
    assert!(show1_out.contains(&format!("version:     {}", &v1_next[..12])));
    assert!(show1_out.contains("lifecycle:   superseded"));

    // 5. kat show E2 -> lifecycle: active
    let (show2_out, show2_err, ok) = run_kat(root, &["show", e2]);
    assert!(ok, "kat show E2 failed: {show2_err}");
    assert!(show2_out.contains(&format!("version:     {}", &v2[..12])));
    assert!(show2_out.contains("lifecycle:   active"));
    assert!(show2_out.contains("title:       New Decision"));

    // 6. kat history -> contains supersede operation
    let (hist_out, hist_err, ok) = run_kat(root, &["history"]);
    assert!(ok, "kat history failed: {hist_err}");
    assert!(hist_out.contains("supersede element"));

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
    assert!(err.contains("not found in the accepted state"));
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
    assert!(err.contains("required") || err.contains("--title"));
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
    assert!(show1_out.contains(&format!("version:     {}", &v1_next_id.to_string()[..12])));
    assert!(show1_out.contains("lifecycle:   superseded"));

    let (show2_out, show2_err, ok) = run_kat(root, &["show", &e2.to_string()]);
    assert!(ok, "kat show E2 failed: {show2_err}");
    assert!(show2_out.contains(&format!("version:     {}", &v2_id.to_string()[..12])));
    assert!(show2_out.contains("lifecycle:   active"));
    assert!(show2_out.contains("title:       Replacement decision"));

    let (hist_out, hist_err, ok) = run_kat(root, &["history"]);
    assert!(ok, "kat history failed: {hist_err}");
    assert!(hist_out.contains("supersede element"));

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
    assert!(show1_out.contains("lifecycle:   active"));

    let (show2_out, _, ok2) = run_kat(root, &["show", e2]);
    assert!(ok2);
    assert!(show2_out.contains("lifecycle:   active"));

    // 6. kat history contains link operation
    let (hist_out, hist_err, ok) = run_kat(root, &["history"]);
    assert!(ok, "kat history failed: {hist_err}");
    assert!(hist_out.contains("link"));

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
    assert!(err1.contains("not found in the accepted state"));

    let (_out2, err2, ok2) = run_kat(root, &["link", "addresses", e1, missing_id]);
    assert!(!ok2);
    assert!(err2.contains("not found in the accepted state"));
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
    assert!(err1.contains("invalid source element ID") || err1.contains("invalid element ID"));

    let (_out2, err2, ok2) = run_kat(root, &["link", "addresses"]);
    assert!(!ok2);
    assert!(err2.contains("required") || err2.contains("expected <relationship-type>"));

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
    assert!(err3.contains("unexpected argument") || err3.contains("unknown option"));
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
    assert!(show_req_out.contains(&format!("version:     {}", &v_req_id.to_string()[..12])));
    assert!(show_req_out.contains("lifecycle:   active"));

    let (show_dec_out, _, ok_dec) = run_kat(root, &["show", &e_dec.to_string()]);
    assert!(ok_dec);
    assert!(show_dec_out.contains(&format!("version:     {}", &v_dec_id.to_string()[..12])));
    assert!(show_dec_out.contains("lifecycle:   active"));

    let (hist_out, _, ok_hist) = run_kat(root, &["history"]);
    assert!(ok_hist);
    assert!(hist_out.contains("link"));

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
    assert!(err1.contains("required") || err1.contains("expected relationship ID"));

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
    assert!(err3.contains("unexpected argument") || err3.contains("unexpected option"));
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
    assert!(err1.contains("required") || err1.contains("expected exactly one argument"));

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
    assert!(art_trace_out.contains(&format!("Trace origin for element {e_art}")));
    assert!(art_trace_out.contains("type:        kat.core/artifact"));
    assert!(art_trace_out.contains("title:       \"authx-core.jar\""));
    assert!(art_trace_out.contains("kat.core/represents"));
    assert!(art_trace_out.contains("kat.core/realizes"));
    assert!(art_trace_out.contains("kat.core/motivates"));
    assert!(art_trace_out.contains(e_intent));

    // 12. kat trace Validation V1
    let (val_trace_out, val_trace_err, ok) = run_kat(root, &["trace", e_val]);
    assert!(
        ok,
        "kat trace validation failed: {val_trace_err}\n{val_trace_out}"
    );
    assert!(val_trace_out.contains(&format!("Trace origin for element {e_val}")));
    assert!(val_trace_out.contains("type:        kat.core/validation"));
    assert!(val_trace_out.contains("kat.core/validates"));
    assert!(val_trace_out.contains("kat.core/motivates"));
    assert!(val_trace_out.contains(e_intent));

    // 13. kat trace Intent I1 (origin root -> no origin paths)
    let (int_trace_out, int_trace_err, ok) = run_kat(root, &["trace", e_intent]);
    assert!(
        ok,
        "kat trace intent failed: {int_trace_err}\n{int_trace_out}"
    );
    assert!(int_trace_out.contains(&format!("Trace origin for element {e_intent}")));
    assert!(int_trace_out.contains("none"));

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
    assert!(err.contains("required") || err.contains("expected exactly one argument"));

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
    assert!(impact_out.contains("Directly changed"));
    assert!(impact_out.contains(e_req));

    // Verify Semantically Affected
    assert!(impact_out.contains("Semantically affected elements"));
    assert!(impact_out.contains(e_dec));
    assert!(impact_out.contains(e_impl1));
    assert!(impact_out.contains(e_impl2));
    assert!(impact_out.contains(e_val));

    // Verify Affected Artifacts
    assert!(impact_out.contains("Affected artifacts"));
    assert!(impact_out.contains(e_art));
    assert!(impact_out.contains("via kat.core/represents (backward <-)"));

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

    assert!(val_out.contains("VALIDATION SUMMARY"));
    assert!(val_out.contains("MECHANICALLY UNVERIFIED CONSTRAINTS"));
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

#[test]
fn kat_artifacts_outside_repository_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let (_out, err, ok) = run_kat(root, &["artifacts"]);
    assert!(!ok);
    assert!(err.contains("no KAT repository found"));
}

#[test]
fn phase10_acceptance_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // 1. kat init
    let (_out, err, ok) = run_kat(root, &["init"]);
    assert!(ok, "kat init failed: {err}");

    // 2. Create Implementation M1
    let (m1_out, m1_err, ok) = run_kat(
        root,
        &[
            "create",
            "implementation",
            "--title",
            "AuthX Service Core Module",
        ],
    );
    assert!(ok, "create implementation failed: {m1_err}");
    let e_imp = id_line(&m1_out, "element_id");

    // 3. Create Artifact A1
    let (a1_out, a1_err, ok) = run_kat(
        root,
        &["create", "artifact", "--title", "authx-core-v1.jar"],
    );
    assert!(ok, "create artifact failed: {a1_err}");
    let e_art = id_line(&a1_out, "element_id");

    // 4. Check initial artifacts status -> UNACCOUNTED
    let (art_out1, _art_err1, ok1) = run_kat(root, &["artifacts"]);
    assert!(!ok1, "expected exit 1 for unaccounted artifact");
    assert!(art_out1.contains(e_art));
    assert!(art_out1.contains("status:      unaccounted"));

    // 5. Link A1 (represents) -> M1
    let (l1_out, l1_err, ok) = run_kat(root, &["link", "represents", e_art, e_imp]);
    assert!(ok, "link represents failed: {l1_err}");
    let r1_id = id_line(&l1_out, "relationship_id");

    // 6. Check status -> CURRENT
    let (art_out2, art_err2, ok2) = run_kat(root, &["artifacts"]);
    assert!(ok2, "kat artifacts failed: {art_err2}\n{art_out2}");
    assert!(art_out2.contains("status:      current"));
    assert!(art_out2.contains(e_imp));

    // 7. Update M1 -> advances Implementation version
    let (_out, err, ok) = run_kat(
        root,
        &["update", e_imp, "--title", "AuthX Service Core Module v2"],
    );
    assert!(ok, "update implementation failed: {err}");

    // 8. Check status -> STALE
    let (art_out3, _err, ok3) = run_kat(root, &["artifacts"]);
    assert!(!ok3, "expected exit 1 for stale artifact");
    assert!(art_out3.contains("status:      stale"));

    // 9. Re-account: Unlink r1 and Link r2 (A1 represents M1)
    let (_out, err, ok) = run_kat(root, &["unlink", r1_id]);
    assert!(ok, "unlink failed: {err}");

    let (_out, err, ok) = run_kat(root, &["link", "represents", e_art, e_imp]);
    assert!(ok, "re-link failed: {err}");

    // 10. Check status -> CURRENT restored
    let (art_out4, art_err4, ok4) = run_kat(root, &["artifacts"]);
    assert!(ok4, "kat artifacts failed: {art_err4}\n{art_out4}");
    assert!(art_out4.contains("status:      current"));
    assert!(art_out4.contains("  current:      1"));

    // 11. Non-mutation verification
    let objects_before = std::fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .count();
    let refs_before = std::fs::read_to_string(root.join(".kat/refs/accepted")).unwrap();

    let (_out, _err, ok) = run_kat(root, &["artifacts"]);
    assert!(ok);

    let objects_after = std::fs::read_dir(root.join(".kat/objects"))
        .unwrap()
        .count();
    let refs_after = std::fs::read_to_string(root.join(".kat/refs/accepted")).unwrap();

    assert_eq!(objects_after, objects_before);
    assert_eq!(refs_after, refs_before);
}

#[test]
fn kat_status_without_repository_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let (out, err, ok) = run_kat(root, &["status"]);
    assert!(!ok);
    assert!(out.is_empty());
    assert!(err.contains("no KAT repository found"));
}

#[test]
fn kat_status_fresh_repository_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);

    let (out, err, ok) = run_kat(root, &["status"]);
    assert!(ok, "kat status failed: {err}\n{out}");
    assert!(out.contains("KAT repository"));
    assert!(out.contains("Repository"));
    assert!(out.contains("change:      none"));
    assert!(out.contains("Knowledge"));
    assert!(out.contains("elements:       0"));
    assert!(out.contains("active:        0"));
    assert!(out.contains("relationships:  0"));
    assert!(out.contains("Consistency"));
    assert!(out.contains("violations:             0"));
    assert!(out.contains("unverified constraints: 0"));
    assert!(out.contains("Accountability"));
    assert!(out.contains("current:      0"));
}

#[test]
fn kat_status_evolved_repository_displays_counts_and_latest_change() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);

    // Create a requirement element
    let (create_out, create_err, ok) = run_kat(
        root,
        &[
            "create",
            "requirement",
            "--title",
            "Authentication Requirement",
        ],
    );
    assert!(ok, "kat create failed: {create_err}\n{create_out}");

    let (out, err, ok) = run_kat(root, &["status"]);
    assert!(ok, "kat status failed: {err}\n{out}");

    assert!(out.contains("KAT repository"));
    assert!(out.contains("Latest change"));
    assert!(out.contains("operation:   create element"));
    assert!(out.contains("elements:       1"));
    assert!(out.contains("active:        1"));
}

// ---------------------------------------------------------------------------
// kat list CLI tests (Phase 11 Step 11.2)
// ---------------------------------------------------------------------------

#[test]
fn kat_list_outside_repository_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let (out, err, ok) = run_kat(root, &["list"]);
    assert!(!ok);
    assert!(err.contains("kat list:"));
    assert!(out.is_empty());
}

#[test]
fn kat_list_empty_repository_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);

    let (out, err, ok) = run_kat(root, &["list"]);
    assert!(ok, "kat list failed: {err}\n{out}");
    assert_eq!(out.trim(), "none");
}

#[test]
fn kat_list_all_elements_table_output() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);

    run_kat(
        root,
        &["create", "requirement", "--title", "User authentication"],
    );
    run_kat(
        root,
        &["create", "design-decision", "--title", "Use WebAuthn"],
    );

    let (out, err, ok) = run_kat(root, &["list"]);
    assert!(ok, "kat list failed: {err}\n{out}");

    assert!(out.contains("ID         TYPE             STATE       TITLE"));
    assert!(out.contains("requirement      active      User authentication"));
    assert!(out.contains("design-decision  active      Use WebAuthn"));
}

#[test]
fn kat_list_type_filter_positional_and_flag() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);

    run_kat(root, &["create", "requirement", "--title", "Req 1"]);
    run_kat(root, &["create", "design-decision", "--title", "Dec 1"]);

    // Positional shorthand
    let (out_pos, _err, ok) = run_kat(root, &["list", "requirement"]);
    assert!(ok);
    assert!(out_pos.contains("requirement"));
    assert!(!out_pos.contains("design-decision"));
    assert!(out_pos.contains("Req 1"));

    // Long flag
    let (out_flag, _err, ok) = run_kat(root, &["list", "--type", "requirement"]);
    assert!(ok);
    assert_eq!(out_pos, out_flag);
}

#[test]
fn kat_list_lifecycle_filter() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);

    let (create_out, _, ok) = run_kat(root, &["create", "requirement", "--title", "Req 1"]);
    assert!(ok);
    let req_id = id_line(&create_out, "element_id");

    run_kat(root, &["deprecate", req_id]);

    let (out_active, _, ok) = run_kat(root, &["list", "--lifecycle", "active"]);
    assert!(ok);
    assert_eq!(out_active.trim(), "none");

    let (out_dep, _, ok) = run_kat(root, &["list", "--lifecycle", "deprecated"]);
    assert!(ok);
    assert!(out_dep.contains("requirement      deprecated  Req 1"));
}

#[test]
fn kat_list_conflicting_type_arguments_fail() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);

    let (_out, err, ok) = run_kat(root, &["list", "requirement", "--type", "design-decision"]);
    assert!(!ok);
    assert!(err.contains("conflicting type arguments"));
}

#[test]
fn kat_list_unknown_type_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);

    let (_out, err, ok) = run_kat(root, &["list", "nonexistent-type"]);
    assert!(!ok);
    assert!(err.contains("unknown element type"));
}

#[test]
fn kat_list_invalid_lifecycle_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);

    let (_out, err, ok) = run_kat(root, &["list", "--lifecycle", "invalid-state"]);
    assert!(!ok);
    assert!(err.contains("invalid lifecycle state"));
}

// ---------------------------------------------------------------------------
// Unique-prefix ID resolution CLI tests (Phase 11 Step 11.3)
// ---------------------------------------------------------------------------

#[test]
fn kat_cli_prefix_resolution_acceptance_workflow() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);

    let (_create_out, _, ok) = run_kat(
        root,
        &["create", "requirement", "--title", "User authentication"],
    );
    assert!(ok);

    // 1. Get list output and extract the displayed 8-character ID prefix
    let (list_out, _, ok) = run_kat(root, &["list"]);
    assert!(ok);
    let short_id = list_out
        .lines()
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    assert_eq!(short_id.len(), 8);

    // 2. Pass the 8-character short ID directly to kat show
    let (show_out, _, ok) = run_kat(root, &["show", short_id]);
    assert!(ok, "kat show with prefix {short_id} failed");
    assert!(show_out.contains("User authentication"));

    // 3. Pass the 8-character short ID directly to kat impact
    let (impact_out, _, ok) = run_kat(root, &["impact", short_id]);
    assert!(ok, "kat impact with prefix {short_id} failed");
    assert!(impact_out.contains("User authentication"));

    // 4. Pass the 8-character short ID directly to kat update
    let (update_out, _, ok) = run_kat(
        root,
        &["update", short_id, "--title", "User authentication v2"],
    );
    assert!(ok, "kat update with prefix {short_id} failed");
    assert!(update_out.contains("version_id:"));
}

#[test]
fn kat_cli_prefix_too_short_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);
    run_kat(
        root,
        &["create", "requirement", "--title", "User authentication"],
    );

    let (_, err, ok) = run_kat(root, &["show", "7af83d1"]);
    assert!(!ok);
    assert!(err.contains("identifier prefix '7af83d1' is too short"));
}

#[test]
fn kat_cli_prefix_ambiguous_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);
    run_kat(root, &["create", "requirement", "--title", "Req A"]);
    run_kat(root, &["create", "requirement", "--title", "Req B"]);

    // Query non-matching prefix
    let (_, err, ok) = run_kat(root, &["show", "00000000"]);
    assert!(!ok);
    assert!(err.contains("not found in the accepted state"));
}

// ---------------------------------------------------------------------------
// Phase 11 Discovery acceptance flow (zero external ID bookkeeping)
// ---------------------------------------------------------------------------

#[test]
fn phase11_acceptance_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // 1. Initialize repository
    run_kat(root, &["init"]);

    // 2. Create requirement & decision
    run_kat(
        root,
        &["create", "requirement", "--title", "User authentication"],
    );
    run_kat(
        root,
        &["create", "design-decision", "--title", "Use WebAuthn"],
    );

    // 3. Enumerate elements via kat list (no external IDs known)
    let (list_out, _, ok) = run_kat(root, &["list"]);
    assert!(ok);
    let lines: Vec<&str> = list_out.lines().collect();
    assert_eq!(lines.len(), 3); // Header + 2 element rows

    // Extract short prefixes from table output
    let id1 = lines[1].split_whitespace().next().unwrap();
    let id2 = lines[2].split_whitespace().next().unwrap();

    // Determine requirement vs decision IDs from list
    let (req_short_id, dec_short_id) = if lines[1].contains("requirement") {
        (id1, id2)
    } else {
        (id2, id1)
    };

    // 4. Link decision -> requirement using only visible 8-character prefixes
    let (link_out, _, ok) = run_kat(root, &["link", "addresses", dec_short_id, req_short_id]);
    assert!(ok, "linking with 8-char prefixes failed");
    assert!(link_out.contains("relationship_id:"));

    // 5. Inspect requirement via kat show <req-short-id> and verify incoming relationship
    let (show_req_out, _, ok) = run_kat(root, &["show", req_short_id]);
    assert!(ok);
    assert!(show_req_out.contains("User authentication"));
    assert!(show_req_out.contains("Relationships"));
    assert!(show_req_out.contains("in "));
    assert!(show_req_out.contains("addresses"));
    assert!(show_req_out.contains("Use WebAuthn"));

    // Extract visible 8-character relationship ID from show output table row
    let show_req_lines: Vec<&str> = show_req_out.lines().collect();
    let rel_row = show_req_lines
        .iter()
        .find(|l| l.contains("in "))
        .expect("incoming relationship row present");
    let rel_short_id = rel_row.split_whitespace().nth(1).unwrap();
    assert_eq!(rel_short_id.len(), 8);

    // 6. Unlink relationship using only visible 8-character relationship ID prefix
    let (unlink_out, _, ok) = run_kat(root, &["unlink", rel_short_id]);
    assert!(ok, "unlinking with 8-char relationship prefix failed");
    assert!(unlink_out.contains("relationship_id:"));

    // 7. Verify relationship table returns to 'none'
    let (show_after_out, _, ok) = run_kat(root, &["show", req_short_id]);
    assert!(ok);
    assert!(show_after_out.contains("Relationships"));
    assert!(show_after_out.contains("none"));
}

// ---------------------------------------------------------------------------
// Phase 12 Output Modes tests (--compact, history --oneline/--limit/--element)
// ---------------------------------------------------------------------------

#[test]
fn kat_history_oneline_limit_element_flags() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);
    let (c1, _, _) = run_kat(root, &["create", "requirement", "--title", "Req 1"]);
    let (c2, _, _) = run_kat(root, &["create", "design-decision", "--title", "Dec 1"]);
    let e1 = id_line(&c1, "element_id");
    let e2 = id_line(&c2, "element_id");

    run_kat(root, &["link", "addresses", e2, e1]);

    // Test kat history --oneline
    let (h_oneline, _, ok) = run_kat(root, &["history", "--oneline"]);
    assert!(ok);
    let lines: Vec<&str> = h_oneline.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(h_oneline.contains("link"));
    assert!(h_oneline.contains("create element"));

    // Test kat history --limit 1
    let (h_limit, _, ok) = run_kat(root, &["history", "--limit", "1"]);
    assert!(ok);
    assert!(h_limit.contains("Accepted change history (1 revision)"));

    // Test kat history --limit 0 fails
    let (_, err, ok) = run_kat(root, &["history", "--limit", "0"]);
    assert!(!ok);
    assert!(err.contains("--limit must be at least 1"));

    // Test kat history --element <e1-prefix>
    let e1_short = &e1[..8];
    let (h_elem, _, ok) = run_kat(root, &["history", "--oneline", "--element", e1_short]);
    assert!(ok);
    let elem_lines: Vec<&str> = h_elem.lines().collect();
    assert_eq!(elem_lines.len(), 2); // link + create req1
}

#[test]
fn kat_compact_read_commands() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run_kat(root, &["init"]);
    let (c1, _, _) = run_kat(root, &["create", "requirement", "--title", "Req 1"]);
    let e1 = id_line(&c1, "element_id");
    let e1_short = &e1[..8];

    // status --compact
    let (st_out, _, ok) = run_kat(root, &["status", "--compact"]);
    assert!(ok);
    assert!(st_out.contains("1 elements · 0 relationships · 0 violations · 0 stale artifacts"));

    // show --compact <prefix>
    let (sh_out, _, ok) = run_kat(root, &["show", e1_short, "--compact"]);
    assert!(ok);
    assert!(sh_out.contains("requirement"));
    assert!(sh_out.contains("active"));
    assert!(sh_out.contains("Req 1"));

    // validate --compact
    let (val_out, _, ok) = run_kat(root, &["validate", "--compact"]);
    assert!(ok);
    assert!(val_out.contains("0 violations, 0 unverified constraints"));

    // artifacts --compact
    let (art_out, _, ok) = run_kat(root, &["artifacts", "--compact"]);
    assert!(ok);
    assert!(art_out.contains("STATUS       ARTIFACT"));
}

#[test]
fn phase12_acceptance_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // 1. Init
    run_kat(root, &["init"]);

    // 2. Multi-revision creation
    let (c1, _, _) = run_kat(
        root,
        &[
            "create",
            "requirement",
            "--title",
            "User must authenticate",
            "--description",
            "MFA required",
        ],
    );
    let (c2, _, _) = run_kat(
        root,
        &["create", "design-decision", "--title", "Use WebAuthn"],
    );
    let e1 = id_line(&c1, "element_id");
    let e2 = id_line(&c2, "element_id");
    let e1_short = &e1[..8];
    let e2_short = &e2[..8];

    run_kat(root, &["link", "addresses", e2_short, e1_short]);

    // 3. Verify history --oneline --limit 2 --element
    let (h_out, _, ok) = run_kat(
        root,
        &[
            "history",
            "--oneline",
            "--limit",
            "2",
            "--element",
            e1_short,
        ],
    );
    assert!(ok);
    let h_lines: Vec<&str> = h_out.lines().collect();
    assert_eq!(h_lines.len(), 2);

    // 4. Verify compact output across all read commands
    let (status_c, _, ok1) = run_kat(root, &["status", "--compact"]);
    assert!(ok1);
    assert!(status_c.contains("2 elements · 1 relationships"));

    let (show_c, _, ok2) = run_kat(root, &["show", e1_short, "--compact"]);
    assert!(ok2);
    assert!(show_c.contains("User must authenticate"));

    let (trace_c, _, ok3) = run_kat(root, &["trace", e2_short, "--compact"]);
    assert!(ok3);
    assert!(trace_c.contains("User must authenticate"));

    let (impact_c, _, ok4) = run_kat(root, &["impact", e1_short, "--compact"]);
    assert!(ok4);
    assert!(impact_c.contains("CATEGORY  TYPE             ID        TITLE"));

    let (validate_c, _, ok5) = run_kat(root, &["validate", "--compact"]);
    assert!(ok5);
    assert!(validate_c.contains("0 violations"));

    let (artifacts_c, _, ok6) = run_kat(root, &["artifacts", "--compact"]);
    assert!(ok6);
    assert!(artifacts_c.contains("STATUS       ARTIFACT"));
}

#[test]
fn phase14_acceptance_cli_flow_end_to_end() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    // 1. Initialize repository
    let (_, _, ok) = run_kat(root, &["init"]);
    assert!(ok);

    // 2. Open multi-operation change transaction
    let (begin_out, _, ok) = run_kat(
        root,
        &[
            "change",
            "begin",
            "--description",
            "Multi-operation change transaction",
        ],
    );
    assert!(ok);
    assert!(begin_out.contains("opened draft change transaction"));

    // 3. Stage 2 operations onto open draft
    let (c1_out, c1_err, ok1) = run_kat(
        root,
        &["create", "requirement", "--title", "User authentication"],
    );
    assert!(ok1, "c1_out: {c1_out}, c1_err: {c1_err}");
    assert!(c1_out.contains("staged create element"));
    assert!(c1_out.contains("change operations: 1"));

    let (c2_out, _, ok2) = run_kat(
        root,
        &["create", "design-decision", "--title", "Use WebAuthn"],
    );
    assert!(ok2);
    assert!(c2_out.contains("staged create element"));
    assert!(c2_out.contains("change operations: 2"));

    // 4. Inspect status of draft change transaction
    let (status_out, _, ok_st) = run_kat(root, &["change", "status"]);
    assert!(ok_st);
    assert!(status_out.contains("Draft Change Transaction"));
    assert!(status_out.contains("status:       open"));
    assert!(status_out.contains("operations:   2"));
    assert!(status_out.contains("1. CreateElement") || status_out.contains("1. create element"));
    assert!(status_out.contains("2. CreateElement") || status_out.contains("2. create element"));

    let (status_compact, _, ok_c) = run_kat(root, &["change", "status", "--compact"]);
    assert!(ok_c);
    assert!(status_compact.contains("draft status: open"));
    assert!(status_compact.contains("operations: 2"));

    // 5. Commit open draft transaction
    let (commit_out, _, ok_cm) = run_kat(root, &["change", "commit"]);
    assert!(ok_cm);
    assert!(commit_out.contains("committed change transaction"));
    assert!(commit_out.contains("operations:         2"));

    // 6. Verify single ChangeRevision in history containing both operations
    let (history_out, _, ok_h) = run_kat(root, &["history", "--oneline"]);
    assert!(ok_h);
    let history_lines: Vec<&str> = history_out.lines().collect();
    assert_eq!(history_lines.len(), 1);
    assert!(history_lines[0].contains("2 operations"));

    // 7. Verify elements in accepted state
    let (list_out, _, ok_l) = run_kat(root, &["list"]);
    assert!(ok_l);
    assert!(list_out.contains("User authentication"));
    assert!(list_out.contains("Use WebAuthn"));

    // 8. Test kat change abort
    let (_, _, ok_b2) = run_kat(root, &["change", "begin"]);
    assert!(ok_b2);
    let (_, _, ok_stg) = run_kat(
        root,
        &["create", "constraint", "--title", "Must complete in 1s"],
    );
    assert!(ok_stg);

    let (abort_out, _, ok_ab) = run_kat(root, &["change", "abort"]);
    assert!(ok_ab);
    assert!(abort_out.contains("aborted draft change transaction"));

    // Verify aborted staged operation was discarded
    let (list_after, _, _) = run_kat(root, &["list"]);
    assert!(!list_after.contains("Must complete in 1s"));
}

/// Extracts the value of a key from staged CLI stdout lines (ignoring leading whitespace).
fn staged_id_line<'a>(out: &'a str, key: &str) -> &'a str {
    out.lines()
        .find_map(|line| {
            let trimmed = line.trim();
            let prefix = format!("{key}:");
            if trimmed.starts_with(&prefix) {
                Some(trimmed[prefix.len()..].trim())
            } else {
                None
            }
        })
        .unwrap_or_else(|| panic!("staged output must contain a '{key}: ...' line:\n{out}"))
}

#[test]
fn kat_change_read_isolation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    run_kat(root, &["init"]);

    // Publish base element to S1
    let (c1, _, ok1) = run_kat(
        root,
        &[
            "create",
            "requirement",
            "--title",
            "Accepted Base Requirement",
        ],
    );
    assert!(ok1);
    let base_id = id_line(&c1, "element_id");
    let base_short = &base_id[..8];

    // Begin draft session and stage a new requirement
    run_kat(root, &["change", "begin"]);
    let (c2, _, ok2) = run_kat(
        root,
        &[
            "create",
            "requirement",
            "--title",
            "Staged Draft Requirement",
        ],
    );
    assert!(ok2);
    let staged_id = staged_id_line(&c2, "element_id");
    let staged_short = &staged_id[..8];

    // 1. Normal read commands inspect accepted state S1 ONLY
    let (list_out, _, _) = run_kat(root, &["list"]);
    assert!(list_out.contains("Accepted Base Requirement"));
    assert!(!list_out.contains("Staged Draft Requirement"));

    let (status_out, _, _) = run_kat(root, &["status"]);
    assert!(status_out.contains("active:"), "status_out: {status_out}");

    let (_show_staged, show_staged_err, show_staged_ok) = run_kat(root, &["show", staged_short]);
    assert!(!show_staged_ok);
    assert!(show_staged_err.contains("not found"));

    let (show_base, _, show_base_ok) = run_kat(root, &["show", base_short]);
    assert!(show_base_ok);
    assert!(show_base.contains("Accepted Base Requirement"));

    // 2. kat change status inspects working draft candidate state S_working
    let (change_status, _, _) = run_kat(root, &["change", "status"]);
    assert!(change_status.contains("status:       open"));
    assert!(change_status.contains("operations:   1"));
    assert!(change_status.contains("elements:      2"));
}

#[test]
fn kat_change_sequential_composition_dependency_chain() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    run_kat(root, &["init"]);

    run_kat(
        root,
        &[
            "change",
            "begin",
            "--description",
            "Sequential multi-operation dependency chain",
        ],
    );

    // O1: Create Requirement R
    let (c1, _, ok1) = run_kat(root, &["create", "requirement", "--title", "Requirement R"]);
    assert!(ok1);
    let r_id = staged_id_line(&c1, "element_id");
    let r_short = &r_id[..8];

    // O2: Create Design Decision D
    let (c2, _, ok2) = run_kat(
        root,
        &["create", "design-decision", "--title", "Design Decision D"],
    );
    assert!(ok2);
    let d_id = staged_id_line(&c2, "element_id");
    let d_short = &d_id[..8];

    // O3: Link D --addresses--> R (references newly staged elements D and R)
    let (link_out, link_err, ok3) = run_kat(root, &["link", "addresses", d_short, r_short]);
    assert!(ok3, "link failed: out={link_out}, err={link_err}");

    // O4: Update R (modifies newly staged element R)
    let (up_out, _, ok4) = run_kat(
        root,
        &["update", r_short, "--title", "Requirement R Updated"],
    );
    assert!(ok4, "update failed: {up_out}");

    // O5: Deprecate R (lifecycle transition on newly staged element R)
    let (dep_out, _, ok5) = run_kat(root, &["deprecate", r_short]);
    assert!(ok5, "deprecate failed: {dep_out}");

    // Verify 5 operations staged
    let (status_out, _, _) = run_kat(root, &["change", "status"]);
    assert!(status_out.contains("operations:   5"));

    // Commit single atomic revision
    let (commit_out, _, ok_cm) = run_kat(root, &["change", "commit"]);
    assert!(ok_cm, "commit failed: {commit_out}");
    assert!(commit_out.contains("operations:         5"));

    // Verify history contains exactly 1 revision with 5 operations
    let (history_out, _, _) = run_kat(root, &["history", "--oneline"]);
    let h_lines: Vec<&str> = history_out.lines().collect();
    assert_eq!(h_lines.len(), 1);
    assert!(h_lines[0].contains("5 operations"));

    // Inspect R in accepted state
    let (show_r, _, ok_r) = run_kat(root, &["show", r_short]);
    assert!(ok_r);
    assert!(show_r.contains("deprecated"));
    assert!(show_r.contains("Requirement R Updated"));
}

#[test]
fn kat_change_cas_conflict_stale_session() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    run_kat(root, &["init"]);

    // 1. Begin draft session at S0
    run_kat(root, &["change", "begin"]);
    run_kat(
        root,
        &["create", "requirement", "--title", "Staged in Session"],
    );

    // 2. Simulate concurrent publisher moving accepted head from S0 to S1 directly in engine
    let repository = open_repository(root).unwrap();
    let context = prepare_change(&repository).unwrap();
    let input = CreateElementInput {
        element_id: ElementId::new(),
        type_id: "kat.core/requirement".to_string(),
        properties: vec![(
            "title".to_string(),
            PropertyValue::Text("Concurrent Publication".to_string()),
        )],
    };
    let prep = apply_create_element(context, input).unwrap();
    let ont = validate_create_element_ontology(prep).unwrap();
    let val = validate_create_element_invariants(ont).unwrap();
    let rev = prepare_change_revision(val, ChangeId::new(), None).unwrap();
    let pers = persist_prepared_change(&repository, rev).unwrap();
    publish_persisted_change(&repository, pers).unwrap();

    // 3. Attempt committing session -> fails due to CAS conflict
    let (commit_out, commit_err, ok_cm) = run_kat(root, &["change", "commit"]);
    assert!(!ok_cm);
    assert!(
        commit_err.contains("accepted ref changed concurrently")
            || commit_err.contains("accepted repository state changed")
            || commit_out.contains("accepted repository state changed"),
        "commit_err: {commit_err}, commit_out: {commit_out}"
    );

    // 4. Inspect session status -> marked stale
    let (status_out, _, ok_st) = run_kat(root, &["change", "status"]);
    assert!(ok_st);
    assert!(status_out.contains("status:       stale"));

    // 5. Verify abort cleans up stale session
    let (abort_out, _, ok_ab) = run_kat(root, &["change", "abort"]);
    assert!(ok_ab);
    assert!(abort_out.contains("aborted draft change transaction"));

    let (status_after, _, _) = run_kat(root, &["change", "status"]);
    assert!(status_after.contains("no open draft change transaction found"));
}

#[test]
fn phase15_acceptance_cli_flow_end_to_end() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    // 1. kat init
    run_kat(root, &["init"]);

    // 2. Create artifact and requirement
    let (art_out, _, ok1) = run_kat(root, &["create", "artifact", "--title", "styles.css"]);
    assert!(ok1);
    let art_id = id_line(&art_out, "element_id");
    let art_short = &art_id[..8];

    let (req_out, _, ok2) = run_kat(
        root,
        &["create", "requirement", "--title", "Layout Spec v1"],
    );
    assert!(ok2);
    let req_id = id_line(&req_out, "element_id");
    let req_short = &req_id[..8];

    // 3. Link artifact --derived-from--> requirement
    let (link_out, link_err, ok3) = run_kat(root, &["link", "derived-from", art_short, req_short]);
    assert!(ok3, "link failed: out={link_out}, err={link_err}");

    // 4. Verify kat artifacts shows current
    let (arts_1, _, ok_arts1) = run_kat(root, &["artifacts"]);
    assert!(ok_arts1);
    assert!(arts_1.contains("current"));
    assert!(arts_1.contains("styles.css"));

    // 5. Update requirement -> kat artifacts becomes stale (and exits with non-zero status for CI)
    let (up_out, up_err, ok4) = run_kat(root, &["update", req_short, "--title", "Layout Spec v2"]);
    assert!(ok4, "update failed: out={up_out}, err={up_err}");

    let (arts_2, _, ok_arts2) = run_kat(root, &["artifacts"]);
    assert!(!ok_arts2); // kat artifacts fails when stale artifacts exist
    assert!(arts_2.contains("stale"));

    // 6. kat account <artifact> (first-class re-accountability operation)
    let (acc_out, _, ok5) = run_kat(
        root,
        &[
            "account",
            art_short,
            "--description",
            "Reconciled styles.css against Layout Spec v2",
        ],
    );
    assert!(ok5, "account failed: {acc_out}");

    // 7. Verify kat artifacts is current again (without unlink/link ceremony!)
    let (arts_3, _, ok_arts3) = run_kat(root, &["artifacts"]);
    assert!(ok_arts3);
    assert!(arts_3.contains("current:      1"));
    assert!(arts_3.contains("stale:        0"));

    // 8. Verify kat history --oneline records account artifact operation
    let (hist_out, _, ok_hist) = run_kat(root, &["history", "--oneline"]);
    assert!(ok_hist);
    assert!(hist_out.contains("account artifact"));

    // 9. Verify kat validate returns clean
    let (val_out, _, ok_val) = run_kat(root, &["validate"]);
    assert!(ok_val);
    assert!(val_out.contains("Mechanical Violations:                 0"));
}

#[test]
fn phase15_staged_multi_op_account_composition() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    run_kat(root, &["init"]);

    let (art_out, _, ok1) = run_kat(root, &["create", "artifact", "--title", "component.tsx"]);
    assert!(ok1);
    let art_short = &id_line(&art_out, "element_id")[..8];

    let (req_out, _, ok2) = run_kat(root, &["create", "requirement", "--title", "Spec v1"]);
    assert!(ok2);
    let req_short = &id_line(&req_out, "element_id")[..8];

    run_kat(root, &["link", "derived-from", art_short, req_short]);

    // Open multi-op change transaction
    let (_, _, ok_begin) = run_kat(
        root,
        &[
            "change",
            "begin",
            "--description",
            "Evolve spec and reconcile component in one change",
        ],
    );
    assert!(ok_begin);

    // Stage 1: update requirement in candidate state
    let (up_out, _, ok_up) = run_kat(root, &["update", req_short, "--title", "Spec v2"]);
    assert!(ok_up, "staged update failed: {up_out}");
    assert!(up_out.contains("staged update"));

    // Stage 2: account artifact (observes updated requirement version in S_working!)
    let (acc_out, _, ok_acc) = run_kat(root, &["account", art_short]);
    assert!(ok_acc, "staged account failed: {acc_out}");
    assert!(acc_out.contains("staged account artifact"));

    // Commit draft transaction
    let (cm_out, _, ok_cm) = run_kat(root, &["change", "commit"]);
    assert!(ok_cm, "commit failed: {cm_out}");

    // Verify artifacts is current
    let (arts_out, _, ok_arts) = run_kat(root, &["artifacts"]);
    assert!(ok_arts);
    assert!(arts_out.contains("current:      1"));

    // Verify single revision in history containing both operations
    let (hist_out, _, ok_hist) = run_kat(root, &["history"]);
    assert!(ok_hist);
    assert!(
        hist_out.contains("update element") || hist_out.contains("update"),
        "hist_out: {hist_out}"
    );
    assert!(
        hist_out.contains("account artifact"),
        "hist_out: {hist_out}"
    );
}

#[test]
fn phase15_multiple_accountability_edges_reconciliation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    run_kat(root, &["init"]);

    let (art_out, _, _) = run_kat(root, &["create", "artifact", "--title", "app.ts"]);
    let art_short = &id_line(&art_out, "element_id")[..8];

    let (req_out, _, _) = run_kat(root, &["create", "requirement", "--title", "Req 1"]);
    let req_short = &id_line(&req_out, "element_id")[..8];

    let (imp_out, _, _) = run_kat(root, &["create", "implementation", "--title", "Impl 1"]);
    let imp_short = &id_line(&imp_out, "element_id")[..8];

    run_kat(root, &["link", "derived-from", art_short, req_short]);
    run_kat(root, &["link", "derived-from", art_short, imp_short]);

    let (arts_init, _, _) = run_kat(root, &["artifacts"]);
    assert!(arts_init.contains("current:      1"));

    // Update both upstream elements
    run_kat(root, &["update", req_short, "--title", "Req 2"]);
    run_kat(root, &["update", imp_short, "--title", "Impl 2"]);

    let (arts_stale, _, ok_stale) = run_kat(root, &["artifacts"]);
    assert!(!ok_stale); // non-zero exit when stale
    assert!(arts_stale.contains("stale:        1"));

    // kat account app.ts reconciles BOTH accountability relationships
    let (acc_out, _, ok_acc) = run_kat(root, &["account", art_short]);
    assert!(ok_acc, "account failed: {acc_out}");

    let (arts_curr, _, ok_curr) = run_kat(root, &["artifacts"]);
    assert!(ok_curr);
    assert!(arts_curr.contains("current:      1"));
    assert!(arts_curr.contains("stale:        0"));
}

#[test]
fn phase17_acceptance_cli_flow_end_to_end() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    run_kat(root, &["init"]);

    let kat_dir = |p: &Path| p.join(".kat");
    let object_ids = |p: &Path| {
        let mut ids = Vec::new();
        let objects_dir = kat_dir(p).join("objects");
        if objects_dir.is_dir() {
            for entry in fs::read_dir(objects_dir).unwrap() {
                let entry = entry.unwrap();
                ids.push(entry.file_name().to_string_lossy().to_string());
            }
        }
        ids.sort();
        ids
    };

    let objects_before = object_ids(root);
    let accepted_before = fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap();

    // 1. kat ontology (default summary)
    let (out_summary, _, ok_sum) = run_kat(root, &["ontology"]);
    assert!(ok_sum, "kat ontology failed: {out_summary}");
    assert!(out_summary.contains("ONTOLOGY"));
    assert!(out_summary.contains("ELEMENT TYPES (7)"));
    assert!(out_summary.contains("RELATIONSHIP TYPES (10)"));
    assert!(out_summary.contains("kat.core/requirement"));
    assert!(out_summary.contains("kat.core/realizes"));
    assert!(out_summary.contains("NAME")); // verifies NAME column present

    // 2. kat ontology --compact (compact summary)
    let (out_compact_sum, _, ok_csum) = run_kat(root, &["ontology", "--compact"]);
    assert!(ok_csum, "kat ontology --compact failed: {out_compact_sum}");
    assert!(out_compact_sum.contains("ELEMENT TYPES"));
    assert!(out_compact_sum.contains("  requirement"));
    assert!(out_compact_sum.contains("  realizes"));

    // 3. kat ontology show requirement (default detail element view)
    let (out_show_req, _, ok_sreq) = run_kat(root, &["ontology", "show", "requirement"]);
    assert!(
        ok_sreq,
        "kat ontology show requirement failed: {out_show_req}"
    );
    assert!(out_show_req.contains("kat.core/requirement"));
    assert!(out_show_req.contains("Kind:\n  element"));
    assert!(out_show_req.contains("Name:\n  Requirement"));
    assert!(out_show_req.contains("kat.core/realizes"));

    // 4. kat ontology show requirement --compact (compact detail element view)
    let (out_cshow_req, _, ok_csreq) =
        run_kat(root, &["ontology", "show", "requirement", "--compact"]);
    assert!(
        ok_csreq,
        "kat ontology show requirement --compact failed: {out_cshow_req}"
    );
    assert!(out_cshow_req.contains("requirement"));
    assert!(out_cshow_req.contains("kind: element"));
    assert!(out_cshow_req.contains("realizes <- implementation"));

    // 5. kat ontology show kat.core/realizes (default detail relationship view)
    let (out_show_rel, _, ok_srel) = run_kat(root, &["ontology", "show", "kat.core/realizes"]);
    assert!(
        ok_srel,
        "kat ontology show kat.core/realizes failed: {out_show_rel}"
    );
    assert!(out_show_rel.contains("kat.core/realizes"));
    assert!(out_show_rel.contains("Kind:\n  relationship"));
    assert!(out_show_rel.contains("Name:\n  Realizes"));
    assert!(out_show_rel.contains("kat.core/implementation"));
    assert!(out_show_rel.contains("kat.core/requirement"));

    // 6. kat ontology show realizes --compact (compact detail relationship view)
    let (out_cshow_rel, _, ok_csrel) =
        run_kat(root, &["ontology", "show", "realizes", "--compact"]);
    assert!(
        ok_csrel,
        "kat ontology show realizes --compact failed: {out_cshow_rel}"
    );
    assert!(out_cshow_rel.contains("realizes"));
    assert!(out_cshow_rel.contains("kind: relationship"));
    assert!(out_cshow_rel.contains("implementation"));
    assert!(out_cshow_rel.contains("requirement"));

    // 7. kat ontology show does-not-exist (unknown type error)
    let (out_err, err_stream, ok_err) = run_kat(root, &["ontology", "show", "does-not-exist"]);
    assert!(!ok_err, "expected failure for unknown type");
    assert!(
        err_stream.contains("unknown ontology type 'does-not-exist'")
            || out_err.contains("unknown ontology type")
    );

    // 8. Open draft session and verify ontology query works and leaves draft intact
    run_kat(root, &["change", "begin", "--description", "Test draft"]);
    let (out_draft_ont, _, ok_dont) = run_kat(root, &["ontology"]);
    assert!(
        ok_dont,
        "ontology query during draft session failed: {out_draft_ont}"
    );
    assert!(
        ok_dont,
        "ontology query during draft session failed: {out_draft_ont}"
    );

    // Verify repository objects and accepted ref unchanged
    assert_eq!(object_ids(root), objects_before);
    assert_eq!(
        fs::read_to_string(kat_dir(root).join("refs").join("accepted")).unwrap(),
        accepted_before
    );
}

#[test]
fn phase18_acceptance_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // 1. Initialize KAT repository
    let (out_init, _, ok_init) = run_kat(root, &["init"]);
    assert!(ok_init, "kat init failed: {out_init}");

    // Create multi-hop graph in one draft session:
    // Intent I1 <-motivates- Requirement R1 <-realizes- Implementation M1 <-represents- Artifact A1
    run_kat(
        root,
        &[
            "change",
            "begin",
            "--description",
            "Create multi-hop trace graph",
        ],
    );

    let (i1_out, _, ok_i1) = run_kat(root, &["create", "intent", "--title", "Intent I1"]);
    assert!(ok_i1, "create intent failed: {i1_out}");
    let e_intent = id_line(&i1_out, "element_id");

    let (r1_out, _, ok_r1) = run_kat(
        root,
        &["create", "requirement", "--title", "Requirement R1"],
    );
    assert!(ok_r1, "create requirement failed: {r1_out}");
    let e_req = id_line(&r1_out, "element_id");

    let (m1_out, _, ok_m1) = run_kat(
        root,
        &["create", "implementation", "--title", "Implementation M1"],
    );
    assert!(ok_m1, "create implementation failed: {m1_out}");
    let e_impl = id_line(&m1_out, "element_id");

    let (a1_out, _, ok_a1) = run_kat(root, &["create", "artifact", "--title", "Artifact A1"]);
    assert!(ok_a1, "create artifact failed: {a1_out}");
    let e_art = id_line(&a1_out, "element_id");

    let (_, err_l1, ok_l1) = run_kat(root, &["link", "motivates", e_intent, e_req]);
    assert!(ok_l1, "link motivates failed: {err_l1}");

    let (_, err_l2, ok_l2) = run_kat(root, &["link", "realizes", e_impl, e_req]);
    assert!(ok_l2, "link realizes failed: {err_l2}");

    let (_, err_l3, ok_l3) = run_kat(root, &["link", "represents", e_art, e_impl]);
    assert!(ok_l3, "link represents failed: {err_l3}");

    let (out_commit, err_commit, ok_commit) = run_kat(root, &["change", "commit"]);
    assert!(
        ok_commit,
        "commit failed: out={out_commit}, err={err_commit}"
    );

    // 2. kat trace <artifact-id> (default tree output view)
    let (out_tree, err_tree, ok_tree) = run_kat(root, &["trace", e_art]);
    assert!(ok_tree, "kat trace failed: out={out_tree}, err={err_tree}");
    assert!(out_tree.contains(&format!("Trace origin for element {e_art}")));
    assert!(out_tree.contains("Origin tree"));
    assert!(out_tree.contains("└── via kat.core/represents (forward ->)"));
    assert!(out_tree.contains("Implementation M1"));

    // 3. kat trace <artifact-id> --paths (explicit path list rendering)
    let (out_paths, _, ok_paths) = run_kat(root, &["trace", e_art, "--paths"]);
    assert!(ok_paths, "kat trace --paths failed: {out_paths}");
    assert!(out_paths.contains("Path 1"));
    assert!(out_paths.contains("Step 1"));
    assert!(out_paths.contains("Step 2"));

    // 4. kat trace <artifact-id> --max-depth 1 (depth-bounded trace)
    let (out_md1, _, ok_md1) = run_kat(root, &["trace", e_art, "--max-depth", "1"]);
    assert!(ok_md1, "kat trace --max-depth 1 failed: {out_md1}");
    assert!(out_md1.contains("Implementation M1"));
    assert!(!out_md1.contains("Requirement R1"));

    // 5. kat trace <artifact-id> --max-depth 0 (invalid max-depth error)
    let (out_md0, err_md0, ok_md0) = run_kat(root, &["trace", e_art, "--max-depth", "0"]);
    assert!(!ok_md0, "expected failure for max-depth 0");
    assert!(
        err_md0.contains("max depth must be greater than 0, got 0")
            || out_md0.contains("max depth must be greater than 0")
    );

    // 6. kat impact <req-id> --max-depth 1 (depth-bounded impact)
    let (out_imp1, _, ok_imp1) = run_kat(root, &["impact", e_req, "--max-depth", "1"]);
    assert!(ok_imp1, "kat impact --max-depth 1 failed: {out_imp1}");
    assert!(out_imp1.contains("Implementation M1"));
    assert!(!out_imp1.contains("Artifact A1"));

    // 7. kat impact <req-id> --max-depth 0 (invalid max-depth error)
    let (out_imp0, err_imp0, ok_imp0) = run_kat(root, &["impact", e_req, "--max-depth", "0"]);
    assert!(!ok_imp0, "expected failure for max-depth 0");
    assert!(
        err_imp0.contains("max depth must be greater than 0, got 0")
            || out_imp0.contains("max depth must be greater than 0")
    );

    // 8. kat trace <artifact-id> --compact
    let (out_comp, _, ok_comp) = run_kat(root, &["trace", e_art, "--compact"]);
    assert!(ok_comp, "kat trace --compact failed: {out_comp}");
    assert!(out_comp.contains("Artifact A1 -> Implementation M1 -> Requirement R1 -> Intent I1"));
}

#[test]
fn phase19_acceptance_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // 1. Initialize KAT repository
    let (out_init, _, ok_init) = run_kat(root, &["init"]);
    assert!(ok_init, "kat init failed: {out_init}");

    // 2. Create elements & relationships in a draft session
    run_kat(
        root,
        &[
            "change",
            "begin",
            "--description",
            "Setup validation coverage fixture",
        ],
    );

    let (c1_out, _, ok_c1) = run_kat(
        root,
        &["create", "constraint", "--title", "Password Min 12 Chars"],
    );
    assert!(ok_c1, "create constraint failed: {c1_out}");
    let e_con = id_line(&c1_out, "element_id");

    let (r1_out, _, ok_r1) = run_kat(
        root,
        &["create", "requirement", "--title", "Secure User Auth"],
    );
    assert!(ok_r1, "create requirement failed: {r1_out}");
    let e_req = id_line(&r1_out, "element_id");

    let (v1_out, _, ok_v1) = run_kat(
        root,
        &[
            "create",
            "validation",
            "--title",
            "Password Policy Unit Test",
        ],
    );
    assert!(ok_v1, "create validation failed: {v1_out}");
    let e_val = id_line(&v1_out, "element_id");

    let (_, err_l1, ok_l1) = run_kat(root, &["link", "restricts", e_con, e_req]);
    assert!(ok_l1, "link restricts failed: {err_l1}");

    let (_, err_l2, ok_l2) = run_kat(root, &["link", "validates", e_val, e_con]);
    assert!(ok_l2, "link validates failed: {err_l2}");

    let (out_commit, err_commit, ok_commit) = run_kat(root, &["change", "commit"]);
    assert!(
        ok_commit,
        "commit failed: out={out_commit}, err={err_commit}"
    );

    // 3. kat validate (default view)
    let (out_val, err_val, ok_val) = run_kat(root, &["validate"]);
    assert!(ok_val, "kat validate failed: out={out_val}, err={err_val}");
    assert!(out_val.contains("VALIDATION SUMMARY"));
    assert!(out_val.contains("Mechanical Violations:                 0"));
    assert!(out_val.contains("Mechanically Unverified Constraints:   1"));
    assert!(out_val.contains(
        "Validation Evidence Coverage:          1 / 1 constraints evidence-backed (100.0%)"
    ));
    assert!(out_val.contains("MECHANICALLY UNVERIFIED CONSTRAINTS (1)"));
    assert!(out_val.contains(e_con));
    assert!(out_val.contains("Password Min 12 Chars"));
    assert!(out_val.contains("Validation Evidence: 1 validation(s):"));
    assert!(out_val.contains(e_val));
    assert!(out_val.contains("Password Policy Unit Test"));
    assert!(out_val.contains("> Note: Evidence-backed constraints remain mechanically unverified by KAT (no executable rule engine)."));

    // 4. kat validate --coverage
    let (out_cov, err_cov, ok_cov) = run_kat(root, &["validate", "--coverage"]);
    assert!(
        ok_cov,
        "kat validate --coverage failed: out={out_cov}, err={err_cov}"
    );
    assert!(out_cov.contains("EVIDENCE COVERAGE BREAKDOWN"));
    assert!(out_cov.contains("kat.core/constraint"));
    assert!(out_cov.contains("kat.core/requirement"));
    assert!(out_cov.contains("UNCOVERED KNOWLEDGE ELEMENTS (1)"));
    assert!(out_cov.contains(e_req));
    assert!(out_cov.contains("Secure User Auth"));

    // 5. kat validate --compact
    let (out_comp, _, ok_comp) = run_kat(root, &["validate", "--compact"]);
    assert!(ok_comp, "kat validate --compact failed: {out_comp}");
    assert!(
        out_comp
            .contains("0 violations, 1 unverified constraints, constraint_coverage: 1/1 (100.0%)")
    );

    // 6. kat validate --coverage --compact
    let (out_cov_comp, _, ok_cov_comp) = run_kat(root, &["validate", "--coverage", "--compact"]);
    assert!(
        ok_cov_comp,
        "kat validate --coverage --compact failed: {out_cov_comp}"
    );
    assert!(out_cov_comp.contains("category_coverage:"));
    assert!(out_cov_comp.contains("kat.core/constraint: 1/1 (100.0%)"));
    assert!(out_cov_comp.contains("uncovered: 1 elements"));

    // 7. Draft isolation invariant: open draft session does not alter accepted validation results
    run_kat(
        root,
        &["change", "begin", "--description", "Draft in progress"],
    );
    run_kat(
        root,
        &["create", "requirement", "--title", "Uncommitted Draft Req"],
    );
    let (out_draft_val, _, ok_draft_val) = run_kat(root, &["validate"]);
    assert!(ok_draft_val, "kat validate during draft session failed");
    assert_eq!(
        out_draft_val, out_val,
        "validate results must be point-in-time isolated to accepted state Sn"
    );
}

#[test]
fn phase20_acceptance_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // 1. Initialize KAT repository
    let (out_init, _, ok_init) = run_kat(root, &["init"]);
    assert!(ok_init, "kat init failed: {out_init}");

    // 2. kat change status on clean repository
    let (out_clean_st, _, ok_clean_st) = run_kat(root, &["change", "status"]);
    assert!(ok_clean_st);
    assert!(out_clean_st.contains("no open draft change transaction found"));

    let (out_clean_comp, _, ok_clean_comp) = run_kat(root, &["change", "status", "--compact"]);
    assert!(ok_clean_comp);
    assert!(out_clean_comp.contains("draft status: none"));

    // 3. Open draft change session
    let (out_begin, _, ok_begin) = run_kat(
        root,
        &[
            "change",
            "begin",
            "--description",
            "Introduce JSON persistence",
        ],
    );
    assert!(ok_begin, "kat change begin failed: {out_begin}");

    // 4. Stage multi-operation transaction
    let (c1_out, _, ok_c1) = run_kat(
        root,
        &[
            "create",
            "design-decision",
            "--title",
            "Persist data to JSON file",
        ],
    );
    assert!(ok_c1, "create design-decision failed: {c1_out}");
    let e_dec = id_line(&c1_out, "element_id");

    let (c2_out, _, ok_c2) = run_kat(
        root,
        &["create", "implementation", "--title", "JSON-file store"],
    );
    assert!(ok_c2, "create implementation failed: {c2_out}");
    let e_imp = id_line(&c2_out, "element_id");

    let (_, err_l1, ok_l1) = run_kat(root, &["link", "guides", e_dec, e_imp]);
    assert!(ok_l1, "link guides failed: {err_l1}");

    let (c3_out, _, ok_c3) = run_kat(root, &["create", "artifact", "--title", "src/store.js"]);
    assert!(ok_c3, "create artifact failed: {c3_out}");
    let e_art = id_line(&c3_out, "element_id");

    let (_, err_l2, ok_l2) = run_kat(root, &["link", "represents", e_art, e_imp]);
    assert!(ok_l2, "link represents failed: {err_l2}");

    // 5. Inspect kat change status
    let (out_st, err_st, ok_st) = run_kat(root, &["change", "status"]);
    assert!(
        ok_st,
        "kat change status failed: out={out_st}, err={err_st}"
    );
    assert!(out_st.contains("Draft Change Transaction"));
    assert!(out_st.contains("status:       open"));
    assert!(out_st.contains("description:  Introduce JSON persistence"));
    assert!(out_st.contains("operations:   5"));

    assert!(out_st.contains("STAGED OPERATIONS (5)"));
    assert!(out_st.contains("CreateElement"));
    assert!(out_st.contains("Persist data to JSON file"));
    assert!(out_st.contains("JSON-file store"));
    assert!(out_st.contains("LinkKnowledgeElements"));
    assert!(out_st.contains("src/store.js"));

    assert!(out_st.contains("CANDIDATE EFFECT"));
    assert!(out_st.contains("elements:      3"));
    assert!(out_st.contains("+3 created"));
    assert!(out_st.contains("relationships: 2"));
    assert!(out_st.contains("+2 created"));

    assert!(out_st.contains("ARTIFACT ACCOUNTABILITY PREVIEW"));
    assert!(out_st.contains("total:      1"));

    assert!(out_st.contains("CANDIDATE VALIDATION"));
    assert!(out_st.contains("status: Valid (0 violations"));

    // 6. kat change status --compact
    let (out_st_comp, _, ok_st_comp) = run_kat(root, &["change", "status", "--compact"]);
    assert!(ok_st_comp);
    assert!(out_st_comp.contains("draft status: open"));
    assert!(out_st_comp.contains("operations: 5"));

    // 7. Verify draft isolation (accepted state validation remains clean)
    let (out_val, _, ok_val) = run_kat(root, &["validate"]);
    assert!(ok_val);
    assert!(out_val.contains("Mechanical Violations:                 0"));

    // 8. Commit open draft transaction
    let (out_commit, err_commit, ok_commit) = run_kat(root, &["change", "commit"]);
    assert!(
        ok_commit,
        "commit failed: out={out_commit}, err={err_commit}"
    );

    // 9. Verify change status returns clean after commit
    let (out_post, _, ok_post) = run_kat(root, &["change", "status"]);
    assert!(ok_post);
    assert!(out_post.contains("no open draft change transaction found"));
}

#[test]
fn phase21_acceptance_cli_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // 1. Initialize KAT repository
    let (out_init, _, ok_init) = run_kat(root, &["init"]);
    assert!(ok_init, "kat init failed: {out_init}");

    // 2. Create knowledge elements and accountability relationship
    let (c1_out, _, ok_c1) = run_kat(
        root,
        &["create", "requirement", "--title", "Secure Auth Endpoint"],
    );
    assert!(ok_c1);
    let e_req = id_line(&c1_out, "element_id");

    let (c2_out, _, ok_c2) = run_kat(
        root,
        &["create", "implementation", "--title", "Auth Route Handler"],
    );
    assert!(ok_c2);
    let e_imp = id_line(&c2_out, "element_id");

    let (c3_out, _, ok_c3) = run_kat(root, &["create", "artifact", "--title", "src/auth.js"]);
    assert!(ok_c3);
    let e_art = id_line(&c3_out, "element_id");

    run_kat(root, &["link", "implements", e_imp, e_req]);
    run_kat(root, &["link", "represents", e_art, e_imp]);

    // 3. Inspect kat artifacts on clean repository (status current)
    let (out_art1, _, ok_art1) = run_kat(root, &["artifacts"]);
    assert!(ok_art1);
    assert!(out_art1.contains("Artifacts (1)"));
    assert!(out_art1.contains("current"));
    assert!(out_art1.contains("src/auth.js"));

    // kat artifacts --stale returns 0 stale artifacts
    let (out_stale0, _, ok_stale0) = run_kat(root, &["artifacts", "--stale"]);
    assert!(ok_stale0);
    assert!(out_stale0.contains("Artifacts (0)"));

    // 4. Update implementation element e_imp (creating a new version in accepted state)
    let (u_out, _, ok_u) = run_kat(root, &["update", e_imp, "--title", "Auth Route Handler v2"]);
    assert!(ok_u, "kat update implementation failed: {u_out}");

    // 5. Inspect kat artifacts --stale (status stale)
    let (out_stale1, _, ok_stale1) = run_kat(root, &["artifacts", "--stale"]);
    assert!(
        !ok_stale1,
        "kat artifacts --stale should exit with failure code when stale artifacts exist"
    );
    assert!(out_stale1.contains("Artifacts (1)"));
    assert!(out_stale1.contains("stale"));
    assert!(out_stale1.contains("src/auth.js"));

    // 6. Inspect per-artifact detail: kat artifacts <artifact-id>
    let (out_det, _, ok_det) = run_kat(root, &["artifacts", e_art]);
    assert!(
        !ok_det,
        "kat artifacts <id> exits failure code when artifact is STALE"
    );
    assert!(out_det.contains("ARTIFACT ACCOUNTABILITY DETAIL"));
    assert!(out_det.contains(e_art));
    assert!(out_det.contains("src/auth.js"));
    assert!(out_det.contains("status:      stale"));
    assert!(out_det.contains("ACCOUNTABILITY BASELINES (1)"));
    assert!(out_det.contains("kat.core/represents"));
    assert!(out_det.contains("[STALE]"));

    // 7. Compact stale list: kat artifacts --stale --compact
    let (out_comp_stale, _, ok_comp_stale) = run_kat(root, &["artifacts", "--stale", "--compact"]);
    assert!(!ok_comp_stale);
    assert!(out_comp_stale.contains("stale        src/auth.js"));

    // 8. Re-baseline artifact using kat account <artifact-id>
    let (out_acc, err_acc, ok_acc) = run_kat(
        root,
        &[
            "account",
            e_art,
            "--description",
            "Re-baseline auth.js against v2 handler",
        ],
    );
    assert!(ok_acc, "kat account failed: out={out_acc}, err={err_acc}");

    // 9. Verify artifact status is current after re-baselining
    let (out_stale2, _, ok_stale2) = run_kat(root, &["artifacts", "--stale"]);
    assert!(ok_stale2);
    assert!(out_stale2.contains("Artifacts (0)"));

    let (out_art2, _, ok_art2) = run_kat(root, &["artifacts"]);
    assert!(ok_art2);
    assert!(out_art2.contains("current"));
}

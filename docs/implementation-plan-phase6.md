# Phase 6 Implementation Plan: `UnlinkKnowledgeElements`

Phase 6 implements **`UnlinkKnowledgeElements`**, removing an active relationship mapping ($R_1 \to R_1V$) from the accepted `SemanticState` while preserving its historical existence in `ObjectStore` and prior change history.

---

## Architecture & Data Flow

```text
kat unlink <relationship-id> [--description "..."]
    ↓
prepare_change                           (load accepted S_n, C_n, O_n)
    ↓
apply_unlink_element                     (validate R1 presence & expected version; remove R1 -> R1V from S_n -> candidate S_n+1)
    ↓
validate_unlink_element_ontology         (pass-through; no new types introduced)
    ↓
validate_unlink_element_invariants       (verify exact 1-relationship removal delta, elements unchanged, ontology unchanged)
    ↓
prepare_unlink_change_revision           (construct Cn+1 with Operation::Unlink { relationship_id, expected_version }; derive Sn+1, Cn+1 ObjectIds)
    ↓
persist_prepared_unlink_change           (materialize 2 objects: S_n+1, C_n+1; leave refs/accepted untouched)
    ↓
publish_persisted_unlink_change          (atomic CAS on refs/accepted -> { S_n+1, C_n+1 })
```

---

## Detailed Step Breakdown

### Step 6.1 — Input Struct & Candidate State Application (`apply_unlink_element`)
- [ ] **6.1 — Input struct & candidate application.**
      Define `UnlinkElementInput { relationship_id: RelationshipId, expected_version: ObjectId }` and `PreparedElementUnlinked` struct in `src/repository/change.rs`. Implement `apply_unlink_element(context: ChangeContext, input: UnlinkElementInput) -> Result<PreparedElementUnlinked, ChangeError>`.
      Preconditions:
      1. Relationship $R_1$ exists in base state $S_n.relationships$. Fail with `PreconditionError::RelationshipNotFound(relationship_id)` if missing.
      2. Base state's mapped version for $R_1$ matches `input.expected_version`. Fail with `PreconditionError::RelationshipVersionMismatch { expected, actual }` if tampered.
      Endpoint lifecycle: Unlink is allowed regardless of whether source or target element is `Active`, `Deprecated`, or `Superseded`.
      Candidate state: $S_{\text{candidate}}.relationships = S_n.relationships \setminus \{ R_1 \to R_1V \}$.
      Re-export in `repository/mod.rs`. Unit tests in `tests/change.rs`.

### Step 6.2 — Ontology Validation (`validate_unlink_element_ontology`)
- [ ] **6.2 — Ontology validation.**
      Implement `validate_unlink_element_ontology(prepared: PreparedElementUnlinked) -> Result<PreparedElementUnlinked, ChangeError>` in `src/repository/change.rs`. Pass-through validation since removing a relationship introduces no new types. Re-export in `repository/mod.rs`. Unit tests in `tests/change.rs`.

### Step 6.3 — Invariant Validation (`validate_unlink_element_invariants`)
- [ ] **6.3 — Invariant validation.**
      Define `ValidatedElementUnlinked` typestate guard in `src/repository/change.rs`. Implement `validate_unlink_element_invariants(prepared: PreparedElementUnlinked) -> Result<ValidatedElementUnlinked, ChangeError>` in `src/repository/validation/invariant.rs` & `src/repository/change.rs`.
      Invariants enforced:
      1. Candidate structural canonicality (`validate_canonical_structure`).
      2. Base ontology version reference preserved (`OntologyVersionChanged`).
      3. Candidate elements array byte-for-byte identical to base (`UnexpectedElementMutation`).
      4. Exact 1-relationship removal delta ($S_{\text{base}}.relationships \setminus \{R_1\} == S_{\text{candidate}}.relationships$, `UnexpectedRelationshipMutation`).
      5. $R_1$ absent from candidate state (`UnlinkRelationshipNotRemoved`).
      Re-export in `repository/mod.rs`. Unit tests in `tests/change.rs`.

### Step 6.4 — Construct `ChangeRevision Cn+1` (`prepare_unlink_change_revision`)
- [ ] **6.4 — Construct `ChangeRevision Cn+1`.**
      Define `PreparedUnlinkChangeRevision` struct in `src/repository/change.rs`. Implement `prepare_unlink_change_revision(validated: ValidatedElementUnlinked, change_id: ChangeId, description: Option<String>) -> Result<PreparedUnlinkChangeRevision, ChangeError>`. Accepts only `ValidatedElementUnlinked`. Constructs `ChangeRevision` with single operation `Operation::Unlink { relationship_id, expected_version }`. Derives $S_{n+1}$ and $C_{n+1}$ ObjectIds via canonical encoding. Re-export in `repository/mod.rs`. Unit tests in `tests/change.rs`.

### Step 6.5 — Persist Before Publication (`persist_prepared_unlink_change`)
- [ ] **6.5 — Persist before publication.**
      Define `PersistedUnlinkChange` struct in `src/repository/change.rs`. Implement `persist_prepared_unlink_change(repository: &Repository, prepared: PreparedUnlinkChangeRevision) -> Result<PersistedUnlinkChange, ChangeError>`. Materializes **2** objects in order: (1) $S_{n+1}$ (`SemanticState`), (2) $C_{n+1}$ (`ChangeRevision`). Zero element-version or relationship-version objects written. Leaves `refs/accepted` untouched (`{ Sn, Cn }`). Re-export in `repository/mod.rs`. Unit tests in `tests/change.rs`.

### Step 6.6 — CAS Publication (`publish_persisted_unlink_change`)
- [ ] **6.6 — CAS publication.**
      Define `PublishedUnlinkChange` struct in `src/repository/change.rs`. Implement `publish_persisted_unlink_change(repository: &Repository, persisted: PersistedUnlinkChange) -> Result<PublishedUnlinkChange, ChangeError>`. Pre-CAS check (`change.result_state == state_id`). Single atomic CAS on `refs/accepted`. Re-export in `repository/mod.rs`. Integration tests in `tests/change.rs`.

### Step 6.7 — CLI `kat unlink` Wiring
- [ ] **6.7 — CLI `kat unlink` wiring.**
      Wire `kat unlink <relationship-id> [--description "..."]` in `src/main.rs`. Resolves `expected_version` ($R_1V$) from base accepted state `S_n.relationships`. Integration tests in `tests/cli.rs`.

### Step 6.8 — Acceptance Verification & Phase 6 Closure
- [ ] **6.8 — Acceptance verification & Phase 6 closure.**
      Add `phase6_acceptance_cli_flow_end_to_end` in `tests/cli.rs`. Verify physical object count progression (init=2 $\to$ create req=5 $\to$ create dec=8 $\to$ link=11 $\to$ unlink=13), fresh reopen accepted ref $\{S_4, C_4\}$, $S_4$ relationship removal, history $C_4 \to C_3 \to C_2 \to C_1$, unlinking non-existent relationship failure, and unlink on deprecated endpoint success. Update `docs/cli.md` and freeze Phase 6.

---

## Verification Plan

### Automated Tests
- `cargo test` (all unit & integration tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`

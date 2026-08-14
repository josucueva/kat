# Phase 6 Implementation Plan: `UnlinkKnowledgeElements`

Phase 6 implements **`UnlinkKnowledgeElements`**, removing an active relationship mapping ($R_1 \to R_1V$) from the accepted `SemanticState` while preserving its historical existence in `ObjectStore` and prior change history.

---

## Architecture & Data Flow

```text
kat unlink <relationship-id> [--description "..."]
    ↓
prepare_change                           (load accepted S_n, C_n, O_n)
    ↓
apply_unlink_element                     (validate R1 presence & expected version; load R1V; remove R1 -> R1V from S_n -> candidate S_n+1)
    ↓
validate_unlink_element_invariants       (verify exact 1-relationship removal delta, elements unchanged, ontology unchanged, R1V derivation)
    ↓
prepare_unlink_change_revision           (construct Cn+1 with Operation::Unlink { relationship_id, expected_version }; derive Sn+1, Cn+1 ObjectIds)
    ↓
persist_prepared_unlink_change           (materialize 2 objects: S_n+1, C_n+1; leave refs/accepted untouched)
    ↓
publish_persisted_unlink_change          (atomic CAS on refs/accepted -> { S_n+1, C_n+1 })
```

> **Note on Ontology Validation**: `UnlinkKnowledgeElements` intentionally performs **no ontology conformance validation**. The operation removes an already accepted relationship and must remain possible even if the current ontology no longer permits that relationship.

---

## Detailed Step Breakdown

### Step 6.1 — Input Struct & Candidate State Application (`apply_unlink_element`)
- [x] **6.1 — Input struct & candidate application.**
      Notes: `src/repository/change.rs` — defined `UnlinkElementInput` and `PreparedElementUnlinked` (carrying `previous_relationship: RelationshipVersion` and `previous_relationship_version_id: ObjectId`). Implemented `apply_unlink_element(repository: &Repository, context: ChangeContext, input: UnlinkElementInput) -> Result<PreparedElementUnlinked, ChangeError>`. Enforces preconditions (relationship presence in $S_n$, version matching `expected_version`, object store load/decoding as `RelationshipVersion`, defensive ID check). Candidate state removes $R_1$ mapping from $S_n.relationships$. Endpoint lifecycle loading is intentionally omitted. Re-exported in `repository/mod.rs`. 4 unit tests in `tests/change.rs` (valid unlink candidate, missing relationship error, version mismatch error, unlink on deprecated endpoint success). `cargo test` 293 pass, fmt/clippy clean.

### Step 6.2 — Invariant Validation (`validate_unlink_element_invariants`)
- [x] **6.2 — Invariant validation.**
      Notes: `src/repository/validation/invariant.rs` & `src/repository/change.rs` — defined `ValidatedElementUnlinked` typestate guard and implemented `validate_unlink_element_invariants`. Enforces structural canonicality, ontology reference preservation, element array immutability, exact 1-relationship removal delta ($S_{\text{candidate}}.relationships == S_{\text{base}}.relationships \setminus \{ R_1 \to R_1V \}$), $R_1$ absence in candidate, and $R_1V$ identity derivation check. Re-exported in `repository/mod.rs`. 2 unit tests in `tests/change.rs` (valid unlink invariants pass, tampering checks fail). `cargo test` 295 pass, fmt/clippy clean.

### Step 6.3 — Construct `ChangeRevision Cn+1` (`prepare_unlink_change_revision`)
- [x] **6.3 — Construct `ChangeRevision Cn+1`.**
      Notes: `src/repository/change.rs` — defined `PreparedUnlinkChangeRevision` and implemented `prepare_unlink_change_revision(validated: ValidatedElementUnlinked, change_id: ChangeId, description: Option<String>)`. Constructs `ChangeRevision` with single operation `Operation::Unlink { relationship_id, expected_version }`. Derives canonical ObjectIds for $S_{n+1}$ and $C_{n+1}$. Purely preparatory; ObjectStore and accepted ref remain untouched. Re-exported in `repository/mod.rs`. 2 unit tests in `tests/change.rs` (end-to-end preparation, description handling). `cargo test` 297 pass, fmt/clippy clean.

### Step 6.4 — Persist Before Publication (`persist_prepared_unlink_change`)
- [x] **6.4 — Persist before publication.**
      Notes: `src/repository/change.rs` — defined `PersistedUnlinkChange` and implemented `persist_prepared_unlink_change(repository: &Repository, prepared: PreparedUnlinkChangeRevision)`. Materializes objects $S_{n+1}$ and $C_{n+1}$ in reference dependency order into ObjectStore. Leaves `refs/accepted` untouched. Note: If $S_{n+1}$ is identical to an earlier state (e.g. $S_2$ before linking), CAS deduplication reuses the existing state object and 1 new object ($C_{n+1}$) is added to ObjectStore. Re-exported in `repository/mod.rs`. Unit test in `tests/change.rs`. `cargo test` 298 pass, fmt/clippy clean.

### Step 6.5 — CAS Publication (`publish_persisted_unlink_change`)
- [ ] **6.5 — CAS publication.**
      Define `PublishedUnlinkChange` struct in `src/repository/change.rs`. Implement `publish_persisted_unlink_change(repository: &Repository, persisted: PersistedUnlinkChange) -> Result<PublishedUnlinkChange, ChangeError>`. Pre-CAS check (`change.result_state == state_id`). Single atomic CAS on `refs/accepted`. Re-export in `repository/mod.rs`. Integration tests in `tests/cli.rs`.

### Step 6.6 — CLI `kat unlink` Wiring
- [ ] **6.6 — CLI `kat unlink` wiring.**
      Wire `kat unlink <relationship-id> [--description "..."]` in `src/main.rs`. Resolves `expected_version` ($R_1V$) from base accepted state `S_n.relationships`. Integration tests in `tests/cli.rs`.

### Step 6.7 — Acceptance Verification & Phase 6 Closure
- [ ] **6.7 — Acceptance verification & Phase 6 closure.**
      Add `phase6_acceptance_cli_flow_end_to_end` in `tests/cli.rs`. Verify physical object count progression (init=2 $\to$ create req=5 $\to$ create dec=8 $\to$ link=11 $\to$ unlink=13), fresh reopen accepted ref $\{S_4, C_4\}$, $S_4$ relationship removal, history $C_4 \to C_3 \to C_2 \to C_1$, unlinking non-existent relationship failure, and unlink on deprecated endpoint success. Update `docs/cli.md` and freeze Phase 6.

---

## Verification Plan

### Automated Tests
- `cargo test` (all unit & integration tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`

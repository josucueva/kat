# Phase 4 Implementation Plan — `SupersedeElement` Operation

## Goal and Context

Implement the `SupersedeElement` mutation operation end-to-end through the Change Engine, object persistence layer, CAS publication, and CLI (`kat supersede`).

`SupersedeElement` is the fourth semantic mutation operation in KAT (following `CreateElement`, `UpdateElement`, and `DeprecateElement`). Unlike single-element mutations, `SupersedeElement` atomically updates **two semantic element identities plus introduces a typed supersedes relationship** in a single atomic `ChangeRevision`:

1. **Superseded Element ($E_1$)**: Current version $V_1$ transitions from `Lifecycle::Active` to `Lifecycle::Superseded` ($V_{1,\text{next}}$).
2. **Replacement Element ($E_2$)**: A newly created element $E_2$ is established with `Lifecycle::Active` ($V_{2,\text{initial}}$).
3. **Superseding Relationship ($R_1$)**: A typed relationship $R_1$ is established ($R_{1,\text{initial}}$) with `source_element_id = E2`, `relationship_type = "kat.core/supersedes"`, `target_element_id = E1`.
4. **Candidate SemanticState ($S_{n+1}$)**: Replaces $E_1 \to V_1$ with $E_1 \to V_{1,\text{next}}$, inserts $E_2 \to V_{2,\text{initial}}$, and inserts $R_1 \to R_{1,\text{initial}}$.
5. **ChangeRevision ($C_{n+1}$)**: Records a single `Operation::Supersede { existing_element: E1, expected_existing_version: V1, replacement_element: E2, replacement_version: V2_initial_id, superseding_relationship: R1_initial_id }`.

---

## User Review & Design Alignments

The following key design decisions govern Phase 4:

1. **Lifecycle Transition**: $E_1$ transitions from `Active` to `Superseded` (using the existing `Lifecycle::Superseded` enum variant), NOT `Deprecated`.
2. **Scope of Replacement (v0.1)**: $E_2$ must be a **newly created element** that does not exist in the base `SemanticState`.
3. **Canonical Operation**: Recorded in `ChangeRevision` as a single `Operation::Supersede`, not a triple composition of Create/Link/Deprecate.
4. **Relationship Direction & Type**: Source is $E_2$ (the replacement), target is $E_1$ (the superseded element), type is `"kat.core/supersedes"` ($E_2 \xrightarrow{\text{supersedes}} E_1$).
5. **Immutability & Authority**: Immutable objects ($V_{1,\text{next}}$, $V_{2,\text{initial}}$, $R_{1,\text{initial}}$, $S_{n+1}$, $C_{n+1}$) materialized prior to CAS publication. CAS remains the sole authority boundary.

---

## Typestate Pipeline Architecture

```text
SupersedeElementInput
    ↓ apply_supersede_element
PreparedElementSuperseded
    ↓ validate_supersede_element_ontology
PreparedElementSuperseded
    ↓ validate_supersede_element_invariants
ValidatedElementSuperseded
    ↓ prepare_supersede_change_revision
PreparedSupersedeChangeRevision
    ↓ persist_prepared_supersede_change
PersistedSupersedeChange
    ↓ publish_persisted_supersede_change
PublishedSupersedeChange
```

---

## Step-by-Step Implementation Roadmap

### Step 4.1 — Engine `SupersedeElementInput` & Apply Logic
- [x] **4.1 — Engine `SupersedeElementInput` & apply logic.**
      Notes: `src/repository/change.rs` — `SupersedeElementInput`, `PreparedElementSuperseded`, and `apply_supersede_element(&Repository, ChangeContext, SupersedeElementInput) -> Result<PreparedElementSuperseded, ChangeError>`. Enforces preconditions: $E_1$ exists, $V_1 == \text{expected}$, $V_1.\text{lifecycle} == \text{Active}$, $E_2 \notin S_n$, $R_1 \notin S_n$. Constructs candidate $V_{1,\text{next}}$ (`Superseded`), $V_{2,\text{initial}}$ (`Active`), $R_{1,\text{initial}}$ ($E_2 \xrightarrow{\text{supersedes}} E_1$), and candidate $S_{n+1}$. Added `PreconditionError::RelationshipAlreadyExists(RelationshipId)`. Purely preparatory: nothing persisted to `ObjectStore` and accepted ref unchanged. Re-exported in `repository/mod.rs`. 2 new unit tests (`tests/change.rs`). `cargo test` 245 pass, fmt/clippy clean.

### Step 4.2 — `SupersedeElement` Ontology Validation
- [x] **4.2 — `SupersedeElement` ontology validation.**
      Notes: `src/repository/validation/ontology.rs` (`validate_relationship`) and `src/repository/change.rs` (`validate_supersede_element_ontology(&PreparedElementSuperseded) -> Result<PreparedElementSuperseded, ChangeError>`). Validates: (1) `replacement_element.type_id` exists in base ontology; (2) `relationship.relationship_type` exists in base ontology; (3) replacement type is allowed as source type; (4) existing element type is allowed as target type. Evaluates strictly against `context.ontology`. Added `OntologyError::UnknownRelationshipType`, `RelationshipSourceTypeNotAllowed`, `RelationshipTargetTypeNotAllowed`. Purely preparatory: leaves `ObjectStore` and accepted ref untouched. Re-exported in `repository/mod.rs`. 10 new unit tests (`tests/change.rs` and `ontology.rs`). `cargo test` 251 pass, fmt/clippy clean.

### Step 4.3 — `SupersedeElement` Invariant Validation
- [x] **4.3 — `SupersedeElement` invariant validation.**
      Notes: `src/repository/validation/invariant.rs` (`validate_supersede_element_invariants`) and `src/repository/change.rs` (`ValidatedElementSuperseded`, `validate_supersede_element_invariants(&PreparedElementSuperseded) -> Result<ValidatedElementSuperseded, ChangeError>`). Enforces 22 candidate-state invariant checks covering $E_1$ identity/type/property preservation, $E_1$ `Active -> Superseded` transition, $E_2$ `Active` lifecycle & identity derivation, $E_2 \neq E_1$ non-aliasing, $R_1$ identity/type (`kat.core/supersedes`)/source ($E_2$)/target ($E_1$) correctness, $S_{n+1}$ candidate state mappings ($E_1 \to V_{1,\text{next}}$, $E_2 \to V_{2,\text{initial}}$, $R_1 \to R_{1,\text{initial}}$), and multi-entry state delta rules ($S_{n+1}.\text{elements} \setminus \{E_1, E_2\} == S_n.\text{elements} \setminus \{E_1\}$, $S_{n+1}.\text{relationships} \setminus \{R_1\} == S_n.\text{relationships}$). Typestate guard `ValidatedElementSuperseded` enforces pipeline order. Added 15 new error variants to `InvariantError`. 2 new comprehensive unit tests in `tests/change.rs`. `cargo test` 253 pass, fmt/clippy clean.

### Step 4.4 — Construct `ChangeRevision Cn+1`
- [x] **4.4 — Construct `ChangeRevision Cn+1`.**
      Notes: `src/repository/change.rs` — `PreparedSupersedeChangeRevision` and `prepare_supersede_change_revision(validated: ValidatedElementSuperseded, change_id: ChangeId, description: Option<String>) -> Result<PreparedSupersedeChangeRevision, ChangeError>`. Constructs `ChangeRevision` with single operation `Operation::Supersede { existing_element: E1, expected_existing_version: V1, replacement_element: E2, replacement_version: V2_initial, superseding_relationship: R1_initial }`. Computes candidate `SemanticState` $S_{n+1}$ ObjectId `state_id` and $C_{n+1}$ ObjectId `change_revision_id`. Dependencies set to accepted change head. Purely preparatory: leaves `ObjectStore` and accepted ref untouched. Re-exported in `repository/mod.rs`. 2 new unit tests in `tests/change.rs`. `cargo test` 255 pass, fmt/clippy clean.

### Step 4.5 — Persist Before Publication
- [ ] **4.5 — Persist before publication.**
      Implement `persist_prepared_supersede_change(repository: &Repository, prepared: PreparedSupersedeChangeRevision) -> Result<PersistedSupersedeChange, ChangeError>`. Materializes $V_{1,\text{next}}$, $V_{2,\text{initial}}$, $R_{1,\text{initial}}$, $S_{n+1}$, $C_{n+1}$ into `ObjectStore`, identity-verified. Leaves `refs/accepted` untouched. Re-export in `repository/mod.rs`. Unit test in `tests/change.rs`.

### Step 4.6 — CAS Publication
- [ ] **4.6 — CAS publication.**
      Implement `publish_persisted_supersede_change(repository: &Repository, persisted: PersistedSupersedeChange) -> Result<PublishedSupersedeChange, ChangeError>`. Pre-CAS defensive check (`change.result_state == state_id`). Performs single CAS on `refs/accepted`. Re-export in `repository/mod.rs`. Integration test in `tests/change.rs`.

### Step 4.7 — CLI `kat supersede` Wiring
- [ ] **4.7 — CLI `kat supersede` wiring.**
      Wire `kat supersede <existing-element-id> <replacement-type> --title "..." [--description "..."]` in `src/main.rs`. Thin CLI parse + dispatch printing stable IDs. End-to-end CLI integration tests in `tests/cli.rs`.

### Step 4.8 — Acceptance Verification & Phase 4 Closure
- [ ] **4.8 — Acceptance verification & Phase 4 closure.**
      Add `phase4_acceptance_cli_flow_end_to_end` in `tests/cli.rs`. Verify `init` -> `create` -> `supersede` -> fresh process reopen -> `accepted` ref $\{S_2, C_2\}$ -> `show E1` resolves $V_{1,\text{next}}$ (`lifecycle: superseded`) -> `show E2` resolves $V_{2,\text{initial}}$ (`lifecycle: active`) -> `history` lists $C_2 \to C_1$ with `Operation::Supersede` -> $V_{1,\text{initial}}$ byte immutability in `ObjectStore`. Update `docs/cli.md` and freeze Phase 4.

---

## Verification Plan

### Automated Tests
- `cargo build`
- `cargo test` (unit, integration, CLI, doc-tests)
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`

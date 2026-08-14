# Phase 3 Implementation Plan — `DeprecateElement`

## Purpose

Phase 3 introduces `DeprecateElement`, KAT's first lifecycle-modifying operation.

An `UpdateElement` operation modifies properties while preserving `Lifecycle::Active`. `DeprecateElement` marks an active knowledge element as no longer active (`Lifecycle::Deprecated`) while preserving its identity, type, properties, relationships, and historical existence in the repository.

Per `operations.md` and `invariants.md`:
- Deprecation is preferred over deletion when an element has participated in the history of the software.
- A deprecated element remains part of `SemanticState` as a historically traceable lifecycle state.
- `kat show` resolves the deprecated element version ($V_{n+1}$) with `lifecycle: deprecated`.
- Attempts to mutate a deprecated element via `UpdateElement` are rejected by precondition checks (`Lifecycle::Active` required).

---

## Normative Operation Typestate Pipeline

$$\text{DeprecateElementInput} \xrightarrow{\text{apply}} \text{PreparedElementDeprecation} \xrightarrow{\text{validate\_ontology}} \text{PreparedElementDeprecation} \xrightarrow{\text{validate\_invariants}} \text{ValidatedElementDeprecation}$$
$$\text{ValidatedElementDeprecation} \xrightarrow{\text{prepare\_revision}} \text{PreparedDeprecateChangeRevision} \xrightarrow{\text{persist}} \text{PersistedDeprecateChange}$$
$$\text{PersistedDeprecateChange} \xrightarrow{\text{publish (CAS)}} \text{PublishedDeprecateChange}$$

---

## Detailed Step Breakdown

### Step 3.1 — Engine `DeprecateElementInput` & Apply Logic
- [x] **3.1 — Engine `DeprecateElementInput` & apply logic.**
      Notes: `src/repository/change.rs` — `DeprecateElementInput { element_id, expected_version }`, `PreparedElementDeprecation`, and `apply_deprecate_element(&Repository, ChangeContext, DeprecateElementInput) -> Result<PreparedElementDeprecation, ChangeError>`. Enforces preconditions (element exists, base version == `expected_version`, current lifecycle == `Active`). Constructs $V_{n+1}$ preserving `element_id`, `type_id`, and `properties` with `lifecycle: Deprecated`. Builds candidate `SemanticState` $S_{n+1}$ replacing $E \to V_n$ with $E \to V_{n+1}$. Purely preparatory: nothing persisted to `ObjectStore` and accepted ref unchanged. Re-exported in `repository/mod.rs`. 3 new unit tests (`tests/change.rs`). `cargo test` 233 pass, fmt/clippy clean.

### Step 3.2 — `DeprecateElement` Ontology Validation
- [x] **3.2 — `DeprecateElement` ontology validation.**
      Notes: `src/repository/change.rs` — `validate_deprecate_element_ontology(prepared: PreparedElementDeprecation) -> Result<PreparedElementDeprecation, ChangeError>` reuses `validate_element_type` to verify $V_{n+1}.type\_id$ against base-state-referenced `OntologyVersion`. Purely preparatory: returns `Ok(prepared)`, leaves `ObjectStore` and accepted ref untouched. Re-exported in `repository/mod.rs`. 1 new unit test (`tests/change.rs`). `cargo test` 234 pass, fmt/clippy clean.

### Step 3.3 — `DeprecateElement` Invariant Validation
- [x] **3.3 — `DeprecateElement` invariant validation.**
      Notes: `src/repository/validation/invariant.rs` (`validate_deprecate_element_invariants`) & `src/repository/change.rs` (`ValidatedElementDeprecation`, `validate_deprecate_element_invariants`) enforce 11 ordered candidate-state invariant checks: canonical structure, identity preserved, base version & expected version match, type preserved, properties preserved (deprecation alters ONLY lifecycle), valid `Active -> Deprecated` transition, $V_{n+1}$ content identity match, $V_{n+1} \neq V_n$, candidate maps $E \to V_{n+1}$, single-state delta rule, and base ontology/relationship preservation. `ValidatedElementDeprecation` typestate guard added so 3.4 accepts only validated deprecations. Re-exported in `repository/mod.rs`. 1 new unit test (`tests/change.rs`). `cargo test` 235 pass, fmt/clippy clean.
- **10 Ordered Invariant Checks**:
  1. Canonical structure of candidate $S_{n+1}$.
  2. Element identity preserved ($V_{n+1}.element\_id == V_n.element\_id$).
  3. Base version & expected version match ($V_n.version == expected\_version$).
  4. Type preserved ($V_{n+1}.type\_id == V_n.type\_id$).
  5. Properties preserved ($V_{n+1}.properties == V_n.properties$).
  6. Lifecycle transition valid ($V_n.lifecycle == Active$, $V_{n+1}.lifecycle == Deprecated$).
  7. $V_{n+1}.version\_id == \text{canonical\_object\_id}(V_{n+1})$.
  8. Candidate state mapping: candidate maps $E \to V_{n+1}$.
  9. Single-state-delta rule: Candidate state minus $E$ equals base state minus $E$.
  10. Ontology & relationships preserved unchanged.
- **Output**: `ValidatedElementDeprecation` typestate guard.

### Step 3.4 — Construct `DeprecateElement` `ChangeRevision Cn+1`
- [x] **3.4 — Construct `DeprecateElement` `ChangeRevision Cn+1`.**
      Notes: `src/repository/change.rs` — `PreparedDeprecateChangeRevision` and `prepare_deprecate_change_revision(ValidatedElementDeprecation, ChangeId, Option<String>) -> Result<PreparedDeprecateChangeRevision, ChangeError>`. Consumes `ValidatedElementDeprecation` (pipeline typestate guard), computes candidate `SemanticState` ObjectId `state_id`, dependencies from accepted head (`accepted.change.into_iter().collect()`), constructs `ChangeRevision` with single operation `DeprecateElement { element_id, expected_version: previous_version_id, new_version: element_version_id }`, derives `change_revision_id` via encode-then-hash. Returns `PreparedDeprecateChangeRevision`. Purely preparatory: nothing persisted to `ObjectStore` and accepted ref unchanged. Re-exported in `repository/mod.rs`. 1 new unit test (`tests/change.rs`). `cargo test` 236 pass, fmt/clippy clean.

### Step 3.5 — Persist Before Publication
- [x] **3.5 — Persist before publication.**
      Notes: `src/repository/change.rs` — `PersistedDeprecateChange` and `persist_prepared_deprecate_change(&Repository, PreparedDeprecateChangeRevision) -> Result<PersistedDeprecateChange, ChangeError>`. Materializes immutable objects into `ObjectStore` in reference order `Vn+1 -> Sn+1 -> Cn+1` and verifies store identity matches prepared identity (`element_version_id`, `state_id`, `change_revision_id`), failing closed on mismatch. `refs/accepted` remains unchanged (no CAS/publication). Re-exported in `repository/mod.rs`. 1 new unit test (`tests/change.rs`). `cargo test` 237 pass, fmt/clippy clean.

### Step 3.6 — CAS Publication
- [x] **3.6 — CAS publication.**
      Notes: `src/repository/change.rs` — `PublishedDeprecateChange` and `publish_persisted_deprecate_change(&Repository, PersistedDeprecateChange) -> Result<PublishedDeprecateChange, ChangeError>`: single CAS `expected = persisted.prepared.deprecation.context.accepted` → `new = { state: Sn+1, change: Some(Cn+1) }`, returning new head in `PublishedDeprecateChange { persisted, accepted }`. Pre-CAS defensive check `prepared.change.result_state == prepared.state_id` (`ChangeError::PublicationStateMismatch`); `RefStoreError::Conflict` surfaced as domain `ChangeError::Conflict`. Pipeline typestate enforced (`PersistedDeprecateChange` required). Re-exported in `repository/mod.rs`. 1 new integration test (`tests/change.rs`). `cargo test` 238 pass, fmt/clippy clean.

### Step 3.7 — CLI `kat deprecate <element-id>` Wiring
- [x] **3.7 — CLI `kat deprecate <element-id>` wiring.**
      Notes: `src/main.rs` (`cmd_deprecate`, `parse_deprecate_args`, `deprecate_pipeline`, `fail_deprecate`) thin CLI adapter over engine deprecation pipeline. Resolves $E \to V_n$ from current accepted state, runs deprecate pipeline, prints stable IDs (`element_id`, `previous_version_id`, `version_id`, `state_id`, `change_id`, `change_revision_id`). 4 CLI end-to-end integration tests (`tests/cli.rs`). `cargo test` 242 pass, fmt/clippy clean. Error Handling: `ElementNotFound`, `ElementNotActive`, `Conflict`, malformed CLI arguments.

### Step 3.8 — Acceptance Verification & Phase 3 Closure
- **Goal**: Add `phase3_acceptance_cli_flow_end_to_end` in `tests/cli.rs`.
- **Verification**: `kat init` -> `kat create requirement --title "A"` -> `kat deprecate <E1>` -> fresh process reopen -> verify accepted head $\{S_2, C_2\}$, $C_2.operations == [DeprecateElement(E1, V1, V2)]$, `kat show E1` resolves $V_2$ (`lifecycle: deprecated`), `kat history` lists $C_2 \to C_1$, $V_1$ byte immutability in `ObjectStore`.

---

## Phase 3 Acceptance Scenario

```text
kat init
kat create requirement --title "A"     -> E1, V1, S1, C1
kat deprecate <E1>                      -> V2 (Deprecated), S2, C2

reopen (fresh process)
    accepted.state == S2, accepted.change == C2
    S2 maps E1 -> V2
    C2.operations == [DeprecateElement{ E1, expected_version: V1, new_version: V2 }]
    C2.base_states == [S1], C2.result_state == S2

kat show E1  -> lifecycle: deprecated (resolves V2)
kat history  -> [C2, C1] (newest first)
V1 still present in objects/ (previous state traceable)
```

## Definition of Done for Phase 3

- [ ] `kat deprecate <element-id>` performs a `DeprecateElement` change end to end.
- [ ] Only `lifecycle` changes (`Active` -> `Deprecated`); `element_id`, `type_id`, and `properties` are preserved.
- [ ] Preconditions enforced: element exists, `Active` (rejects `Deprecated`/`Superseded`), current version == `expected_version` (else `VersionMismatch`).
- [ ] Subsequent `UpdateElement` on deprecated element rejected by precondition check (`Lifecycle::Active` required).
- [ ] Invariants enforced: single-state-delta replacement, exact property & identity preservation.
- [ ] Accepted State and Change head published atomically via CAS; conflict leaves objects unreferenced.
- [ ] Fresh reopen verifies new head; `kat show` resolves $V_{n+1}$ (`lifecycle: deprecated`); `kat history` shows $C_{n+1}$; $V_n$ traceable.
- [ ] Repository persists cleanly across process executions.

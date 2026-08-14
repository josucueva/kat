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
- **Goal**: Implement `validate_deprecate_element_ontology(prepared: PreparedElementDeprecation) -> Result<PreparedElementDeprecation, ChangeError>`.
- **Logic**: Validates $V_{n+1}.type\_id$ against base ontology `context.ontology`. (Reuses `validate_element_type`).
- **Semantics**: Purely preparatory; no state mutation.

### Step 3.3 — `DeprecateElement` Invariant Validation
- **Goal**: Implement `validate_deprecate_element_invariants(&PreparedElementDeprecation) -> Result<(), InvariantError>` and engine wrapper returning `ValidatedElementDeprecation`.
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
- **Goal**: Implement `prepare_deprecate_change_revision(validated: ValidatedElementDeprecation, change_id: ChangeId, description: Option<String>) -> Result<PreparedDeprecateChangeRevision, ChangeError>`.
- **Construction**:
  - `Operation::DeprecateElement { element_id: E, expected_version: Vn, new_version: Vn+1 }`.
  - `base_states = vec![context.base_state_id]`.
  - `result_state = state_id` ($S_{n+1}$).
  - `dependencies = context.accepted.change.into_iter().collect()`.
  - Computes `change_revision_id` = $\text{canonical\_object\_id}(C_{n+1})$.
- **Return**: `PreparedDeprecateChangeRevision`.

### Step 3.5 — Persist Before Publication
- **Goal**: Implement `persist_prepared_deprecate_change(repository: &Repository, prepared: PreparedDeprecateChangeRevision) -> Result<PersistedDeprecateChange, ChangeError>`.
- **Logic**: Encodes and puts $V_{n+1}$, $S_{n+1}$, $C_{n+1}$ into `ObjectStore` in order. Verifies content-derived ObjectIds match precomputed IDs. Leaves `refs/accepted` untouched.
- **Return**: `PersistedDeprecateChange { prepared }`.

### Step 3.6 — CAS Publication
- **Goal**: Implement `publish_persisted_deprecate_change(repository: &Repository, persisted: PersistedDeprecateChange) -> Result<PublishedDeprecateChange, ChangeError>`.
- **Logic**: Pre-CAS defensive check (`prepared.change.result_state == prepared.state_id`). Compare-and-swap `refs/accepted` from `expected` ($S_n, C_n$) to `new` ($S_{n+1}, C_{n+1}$). Surfacing CAS conflict as `ChangeError::Conflict`.
- **Return**: `PublishedDeprecateChange { persisted, accepted }`.

### Step 3.7 — CLI `kat deprecate <element-id>` Wiring
- **Goal**: Wire `kat deprecate <element-id>` in `src/main.rs`.
- **Flow**: Parse `element-id` -> open repo -> prepare change -> resolve $V_n$ from base state -> run deprecate pipeline -> print stable IDs (`element_id`, `previous_version_id`, `version_id`, `state_id`, `change_id`, `change_revision_id`).
- **Error Handling**: `ElementNotFound`, `ElementNotActive`, `Conflict`, malformed CLI arguments.

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

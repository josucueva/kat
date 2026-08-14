# Phase 5 Implementation Plan — `LinkKnowledgeElements`

This document lays out the step-by-step implementation plan for **Phase 5: `LinkKnowledgeElements`**.

Phase 5 introduces explicit relationship mutation into KAT. Up until Phase 4, relationship versions ($R_1 \to R_{1,\text{initial}}$) were materialized only internally during element supersession. Phase 5 exposes standalone relationship creation as a first-class semantic operation through `Operation::Link`.

---

## Key Design Decisions & Normative Rules

### 1. Operation Scope
- `Operation::Link` creates exactly **one new `RelationshipVersion` object** ($R_{1,\text{initial}}$) and inserts `R1 -> R1V` into the candidate `SemanticState.relationships`.
- **No element versions change** ($V_1, V_2, \dots$ remain unchanged in state).
- **Physical object count progression**: Materializes exactly 3 new canonical objects into `ObjectStore`:
  1. $R_{1,\text{initial}}$ (`RelationshipVersion`)
  2. $S_{n+1}$ (`SemanticState`)
  3. $C_{n+1}$ (`ChangeRevision`)

### 2. Stable Identity Authority
- The CLI adapter generates a new `RelationshipId` ($R_1 = \text{RelationshipId::new()}$).
- The Change Engine receives `relationship_id` explicitly in `LinkElementInput`. The engine stays pure and deterministic (never invokes `Uuid::new_v4()`).

### 3. Endpoint Semantics & Lifecycle Constraints
- Both source element ($E_s$) and target element ($E_t$) **must exist in the base state** ($S_n$). If missing $\to$ `PreconditionError::ElementNotFound`.
- **Source Element ($E_s$)**: **Must be `Lifecycle::Active`**. A deprecated or superseded element cannot originate new outgoing links. If non-active $\to$ `PreconditionError::ElementNotActive(source_element_id)`.
- **Target Element ($E_t$)**: **May be `Active`, `Deprecated`, or `Superseded`**. Active knowledge frequently requires tracing back to deprecated or superseded rationale/requirements (*"Traceability must remain possible across superseded and deprecated knowledge when historical explanation requires it"*, `docs/invariants.md`).

### 4. Ontology Authority
- Evaluated by `validate_link_element_ontology(prepared: PreparedElementLinked)`.
- Enforces:
  1. `relationship_type` exists in active base ontology.
  2. Source element $E_s$'s `type_id` is allowed as source.
  3. Target element $E_t$'s `type_id` is allowed as target.
  4. Direction is **exact and asymmetric**: $E_s \xrightarrow{\text{type}} E_t$. Never auto-reversed.

### 5. Duplicate Relationship Rule (Model B: Unique Semantic Triples)
- An accepted `SemanticState` permits **at most one active relationship for any semantic triple `(type, source, target)`**.
- If a link attempt specifies a `(type, source, target)` triple already active in $S_n$, `apply_link_element` fails fail-fast with `PreconditionError::DuplicateRelationshipTriple`.
- In addition, the stable `relationship_id` must not already exist in $S_n$ (`PreconditionError::RelationshipAlreadyExists`).
- *Rationale*: Keeps graph query results crisp and set-theoretic, and ensures `Unlink` in Phase 6 has a clean 1:1 inverse model.

### 6. Candidate-State Invariant Boundary
- Candidate state $S_{\text{candidate}}$ differs from base state $S_n$ **ONLY by the insertion of `R1 -> R1V`**:
  - `elements` array is identical to $S_n$.
  - `relationships` array contains previous $N$ entries untouched plus $R_1 \to R_{1}V$.
  - `ontology_version` is identical.
  - Structural CDDL sorting preserved (`relationships` sorted by `relationship_id`).

### 7. Canonical History Format
- `ChangeRevision` $C_{n+1}$ contains exactly one canonical `Operation::Link`:
  ```text
  Operation::Link { new_relationship_version: R1V }
  ```
- Encodable as CBOR array tag 4: `[4, object-id]` (satisfying `spec/canonical-format.cddl`).

### 8. CLI Surface
- Syntax:
  ```text
  kat link <relationship-type> <source-element-id> <target-element-id> [--description "..."]
  ```
- `<relationship-type>` uses ontology-backed short-name resolution (e.g. `addresses` $\to$ `kat.core/addresses`).

---

## Step-by-Step Implementation Steps

### Step 5.1 — Candidate Application (`apply_link_element`)
- [ ] **5.1 — Candidate application.**
      Define `LinkElementInput` struct and errors `PreconditionError::DuplicateRelationshipTriple` / `PreconditionError::RelationshipAlreadyExists` in `src/repository/change.rs`. Implement `apply_link_element(context: ChangeContext, input: LinkElementInput) -> Result<PreparedElementLinked, ChangeError>`. Validate preconditions ($E_s$ exists & Active, $E_t$ exists, $R_1$ unique, `(type, source, target)` triple unique). Construct candidate state with inserted $R_1 \to R_{1}V$. Return `PreparedElementLinked` typestate guard. Re-export in `repository/mod.rs`. Unit tests in `tests/change.rs`.

### Step 5.2 — Ontology Validation (`validate_link_element_ontology`)
- [ ] **5.2 — Ontology validation.**
      Implement `validate_link_element_ontology(prepared: PreparedElementLinked) -> Result<PreparedElementLinked, ChangeError>` in `src/repository/validation/ontology.rs` and `src/repository/change.rs`. Validate relationship type existence, allowed source element type, allowed target element type, and exact direction. Re-export in `repository/mod.rs`. Unit tests in `tests/change.rs`.

### Step 5.3 — Invariant Validation (`validate_link_element_invariants`)
- [ ] **5.3 — Invariant validation.**
      Implement `validate_link_element_invariants(prepared: PreparedElementLinked) -> Result<ValidatedElementLinked, ChangeError>` in `src/repository/validation/invariant.rs` and `src/repository/change.rs`. Enforce 16 invariant checks (single entry relationship delta, elements array identical, ontology unchanged, canonical CDDL relationship_id ordering, identity derivations). Return `ValidatedElementLinked` typestate guard. Re-export in `repository/mod.rs`. Unit tests in `tests/change.rs`.

### Step 5.4 — Construct `ChangeRevision Cn+1`
- [ ] **5.4 — Construct `ChangeRevision Cn+1`.**
      Define `PreparedLinkChangeRevision` struct in `src/repository/change.rs`. Implement `prepare_link_change_revision(validated: ValidatedElementLinked, change_id: ChangeId, description: Option<String>) -> Result<PreparedLinkChangeRevision, ChangeError>`. Construct `ChangeRevision` with single operation `Operation::Link { new_relationship_version }`. Compute $S_{n+1}$ and $C_{n+1}$ ObjectIds. Purely preparatory. Re-export in `repository/mod.rs`. Unit tests in `tests/change.rs`.

### Step 5.5 — Persist Before Publication (`persist_prepared_link_change`)
- [ ] **5.5 — Persist before publication.**
      Define `PersistedLinkChange` struct in `src/repository/change.rs`. Implement `persist_prepared_link_change(repository: &Repository, prepared: PreparedLinkChangeRevision) -> Result<PersistedLinkChange, ChangeError>`. Materialize 3 objects in order: (1) $R_{1,\text{initial}}$, (2) $S_{n+1}$, (3) $C_{n+1}$. Verify returned `ObjectId`s match precomputed identities. Leave `refs/accepted` untouched (`{ Sn, Cn }`). Re-export in `repository/mod.rs`. Unit tests in `tests/change.rs`.

### Step 5.6 — CAS Publication (`publish_persisted_link_change`)
- [ ] **5.6 — CAS publication.**
      Define `PublishedLinkChange` struct in `src/repository/change.rs`. Implement `publish_persisted_link_change(repository: &Repository, persisted: PersistedLinkChange) -> Result<PublishedLinkChange, ChangeError>`. Pre-CAS check (`change.result_state == state_id`). Single atomic CAS on `refs/accepted`. Re-export in `repository/mod.rs`. Integration tests in `tests/change.rs`.

### Step 5.7 — CLI `kat link` Wiring
- [ ] **5.7 — CLI `kat link` wiring.**
      Wire `kat link <relationship-type> <source-element-id> <target-element-id> [--description "..."]` in `src/main.rs`. Implemented `resolve_relationship_type` for short-name resolution against base ontology. Generate $R_1$ `RelationshipId` in CLI adapter. Integration tests in `tests/cli.rs`.

### Step 5.8 — Acceptance Verification & Phase 5 Closure
- [ ] **5.8 — Acceptance verification & Phase 5 closure.**
      Add `phase5_acceptance_cli_flow_end_to_end` in `tests/cli.rs`. Verify physical object count progression (init=2 $\to$ create E1=5 $\to$ create E2=8 $\to$ link=11), fresh reopen accepted ref $\{S_3, C_3\}$, $S_3$ containing $R_1 \to R_{1}V$, history $C_3 \to C_2 \to C_1$, duplicate link rejection, non-active source rejection, and link to deprecated target success. Update `docs/cli.md` and freeze Phase 5.

---

## Verification Plan

### Automated Tests
- `cargo build`
- `cargo test`
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`

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
- [x] **5.1 — Candidate application.**
      Notes: `src/repository/change.rs` — defined `LinkElementInput`, `PreparedElementLinked`, `apply_link_element(repository: &Repository, context: ChangeContext, input: LinkElementInput) -> Result<PreparedElementLinked, ChangeError>`. Enforces preconditions: $E_s$ exists & `Active`, $E_t$ exists (`Active`, `Deprecated`, or `Superseded`), $R_1$ identity unique (`PreconditionError::RelationshipAlreadyExists`), `(type, source, target)` semantic triple unique in $S_n$ (`PreconditionError::DuplicateRelationshipTriple`). Constructs candidate state with inserted $R_1 \to R_1V$ at canonical sorted position. Re-exported in `repository/mod.rs`. 2 unit tests in `tests/change.rs`. `cargo test` 270 pass, fmt/clippy clean.

### Step 5.2 — Ontology Validation (`validate_link_element_ontology`)
- [x] **5.2 — Ontology validation.**
      Notes: `src/repository/change.rs` — defined `validate_link_element_ontology(prepared: PreparedElementLinked) -> Result<PreparedElementLinked, ChangeError>`. Updated `PreparedElementLinked` to carry loaded endpoint versions `source_element` and `target_element`. Reuses generic `validate_relationship(&prepared.context.ontology, relationship_type, source_type, target_type)`. Re-exported in `repository/mod.rs`. Unit tests in `tests/change.rs` (valid combination, unknown relationship type, forbidden source type, forbidden target type, exact directionality, custom base ontology). `cargo test` 272 pass, fmt/clippy clean.

### Step 5.3 — Invariant Validation (`validate_link_element_invariants`)
- [x] **5.3 — Invariant validation.**
      Notes: `src/repository/validation/invariant.rs` & `src/repository/change.rs` — implemented `validate_link_element_invariants(prepared: PreparedElementLinked) -> Result<ValidatedElementLinked, ChangeError>`. Defined `ValidatedElementLinked` typestate guard. Enforces normative Link invariant: candidate structural canonicality (`validate_canonical_structure`), base ontology version reference preserved (`OntologyVersionChanged`), candidate elements array identical to base (`UnexpectedElementMutation`), relationship ID match (`LinkRelationshipIdentityMismatch`), source/target match (`LinkSourceMismatch`, `LinkTargetMismatch`), relationship version identity match (`LinkRelationshipVersionIdentityMismatch`), candidate state maps $R_1 \to R_1V$ (`LinkCandidateReferenceMismatch`), exact single-relationship delta ($S_{\text{candidate}}.relationships \setminus \{R_1\} == S_{\text{base}}.relationships$, `UnexpectedRelationshipMutation`). Re-exported in `repository/mod.rs`. 2 unit tests in `tests/change.rs` (valid candidate pass & 9 tampering cases). `cargo test` 274 pass, fmt/clippy clean.

### Step 5.4 — Construct `ChangeRevision Cn+1`
- [x] **5.4 — Construct `ChangeRevision Cn+1`.**
      Notes: `src/repository/change.rs` — defined `PreparedLinkChangeRevision` struct and implemented `prepare_link_change_revision(validated: ValidatedElementLinked, change_id: ChangeId, description: Option<String>) -> Result<PreparedLinkChangeRevision, ChangeError>`. Accepts only `ValidatedElementLinked`. Constructs `ChangeRevision` with single operation `Operation::Link { new_relationship_version: link.relationship_version_id }`. Derives $S_{n+1}$ and $C_{n+1}$ ObjectIds via canonical encoding. Re-exported in `repository/mod.rs`. 2 unit tests in `tests/change.rs` (end-to-end preparatory verification & description handling). `cargo test` 276 pass, fmt/clippy clean.

### Step 5.5 — Persist Before Publication (`persist_prepared_link_change`)
- [x] **5.5 — Persist before publication.**
      Notes: `src/repository/change.rs` — defined `PersistedLinkChange` struct and implemented `persist_prepared_link_change(repository: &Repository, prepared: PreparedLinkChangeRevision) -> Result<PersistedLinkChange, ChangeError>`. Materializes 3 objects in reference order: (1) $R_{1,\text{initial}}$ (`RelationshipVersion`), (2) $S_{n+1}$ (`SemanticState`), (3) $C_{n+1}$ (`ChangeRevision`). Zero element-version objects created. Verifies `ObjectId` matches precomputed identity. Leaves `refs/accepted` untouched (`{ Sn, Cn }`). Re-exported in `repository/mod.rs`. Unit tests in `tests/change.rs` (3-object materialization, exact count increase, no element version object created, accepted ref untouched, idempotent re-persistence). `cargo test` 277 pass, fmt/clippy clean.

### Step 5.6 — CAS Publication (`publish_persisted_link_change`)
- [x] **5.6 — CAS publication.**
      Notes: `src/repository/change.rs` — defined `PublishedLinkChange` struct and implemented `publish_persisted_link_change(repository: &Repository, persisted: PersistedLinkChange) -> Result<PublishedLinkChange, ChangeError>`. Enforces pre-CAS state check (`change.result_state == state_id`), atomic CAS update on `refs/accepted`. Zero extra objects created. Re-exported in `repository/mod.rs`. 3 integration tests in `tests/change.rs` (advances accepted head & survives reopen, conflict on stale expected head, publication state mismatch). `cargo test` 280 pass, fmt/clippy clean.

### Step 5.7 — CLI `kat link` Wiring
- [x] **5.7 — CLI `kat link` wiring.**
      Notes: `src/main.rs` — implemented `kat link <relationship-type> <source-element-id> <target-element-id> [--description "..."]`. Implemented `resolve_relationship_type` for short-name lookup (`addresses` $\to$ `kat.core/addresses`) against base ontology. CLI generates `RelationshipId` and `ChangeId`. Added 8 CLI integration tests in `tests/cli.rs` (valid link end-to-end, short and qualified relationship type resolution, unknown relationship type error, forbidden ontology direction error, missing endpoints errors, non-active source error, link to deprecated target success, duplicate relationship triple CLI error, malformed flags, outside repository). `cargo test` 288 pass, fmt/clippy clean.

### Step 5.8 — Acceptance Verification & Phase 5 Closure
- [x] **5.8 — Acceptance verification & Phase 5 closure.**
      Notes: `tests/cli.rs` — added `phase5_acceptance_cli_flow_end_to_end`. Verified 3-object materialization (init=2 $\to$ create requirement=5 $\to$ create decision=8 $\to$ link=11 total objects), fresh process reopen accepted ref $\{S_3, C_3\}$, state maps $R_1 \to R_{1}V$ ($E_{\text{decision}} \xrightarrow{\text{addresses}} E_{\text{requirement}}$), $C_3$ operation `Link { new_relationship_version }`, history $C_3 \to C_2 \to C_1$, duplicate link rejection, non-active source rejection, link to deprecated target success, and byte-for-byte endpoint element immutability. Updated `docs/cli.md` and `docs/invariants.md`. **Phase 5 Frozen.** `cargo test` 289 pass, fmt/clippy clean.

---

## Verification Plan

### Automated Tests
- `cargo build`
- `cargo test`
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`

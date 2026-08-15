> Part of the [master plan](../implementation-plan.md).

## Phase 1: First Semantic Vertical Slice

Per `prototype-design.md`, this milestone proves the semantic-evolution hypothesis. Move here immediately after Phase 0 rather than polishing the object store. The slice is complete only when all of the following work end to end:

```text
CreateElement
    ↓
KnowledgeElementVersion
    ↓
candidate SemanticState S1
    ↓
ontology validation
    ↓
invariant validation
    ↓
ChangeRevision C1
    ↓
persist
    ↓
CAS:
    {S0, none} -> {S1, C1}
    ↓
kat show
    ↓
kat history
```

### Scope for the first slice

Get exactly **one** semantic mutation — `CreateElement` — correct end to end. Do **not** start Phase 1 with `UpdateElement`, `DeprecateElement`, `Link`/`Unlink`, `Supersede`, impact analysis, or general history traversal. The goal changes from "can KAT store canonical objects?" to:

> Can KAT perform a valid semantic change against an accepted state and publish the result atomically?

### Work items (ordered sub-steps, mirroring the Change Application Flow in `prototype-design.md`)

- [x] **1.1 — Change application service.** A library entry point (the Change Engine) that resolves `refs/accepted`, loads the accepted SemanticState and Change head (via `open_repository`), selects the base state, and produces a **candidate** state. It must **not publish anything**; publication is a separate, explicit step.
      Notes: `repository/change.rs` — `prepare_change(&Repository) -> Result<ChangeContext, ChangeError>` returns `ChangeContext { accepted, base_state_id, base_state, ontology }` by resolving the accepted head and loading the base SemanticState + its OntologyVersion. Strictly **prepare-only**: no mutation, no persistence, no publication. `ChangeError` composes `ObjectStoreError`/`DecodingError`/`UnexpectedObjectKind`; further variants (preconditions, ontology, invariants) are added when their steps require them. New `repository/validation/{mod,ontology,invariant}.rs` declare the separate semantic-validation layer (ontology conformance and invariants land in 1.3/1.4; preconditions stay near the engine). 4 new tests (1 unit + 3 integration: context loads + no-mutation invariant, plan-acceptance base, integrity failures rejected at the open boundary before the engine runs). `cargo test` 118 pass, fmt/clippy clean.
- [x] **1.2 — `CreateElement` execution.** Given `ElementId`, `type_id`, `properties`: precondition — the `ElementId` must **not** already be active in the base state; construct a new `KnowledgeElementVersion` (Active lifecycle); validate its canonical form; stage/persist it (or keep staged until persist-before-publish); add `element_id -> new version ObjectId` to the candidate state.
      Notes: `repository/change.rs` — `apply_create_element(ChangeContext, CreateElementInput) -> Result<PreparedElementCreation, ChangeError>`. `PreconditionError::ElementAlreadyExists(ElementId)` rejects a present ID (active/deprecated/superseded — stricter than "not active", since v0.1 state maps one ID to its current version; resurrection is an explicit operation's concern). Constructs `KnowledgeElementVersion { element_id, type_id, lifecycle: Active, properties }`; caller supplies `ElementId` (deterministic engine). Properties are normalized into canonical key order (`cmp_encoded_text`, RFC 8949 §4.2.1) and duplicates rejected (`ChangeError::DuplicatePropertyKey`); nested-structure malformation is rejected fail-closed by `canonical_object_id`. V1's ObjectId is derived by encode-then-hash (not persisted); insertion into the candidate `SemanticState` is ordered by `ElementId` (not append+sort). Candidate keeps base `ontology_version`/`relationships`; base state and accepted ref unchanged. **No ontology conformance (1.3), no invariants (1.4), no persistence, no CAS.** `ChangeError` gains `Encoding`, `DuplicatePropertyKey`, `Precondition`. 6 new tests (5 unit + 1 integration no-persistence). `cargo test` 124 pass, fmt/clippy clean.
- [x] **1.3 — Minimal ontology validation.** For this slice only: **element type exists in the current `OntologyVersion`** (`state.ontology_version`). Do not build every future ontology rule; no relationship type checks yet.
      Notes: `repository/validation/ontology.rs` — `OntologyError::UnknownElementType(String)` and `validate_element_type(&OntologyVersion, type_id)`. Enforces only `element.type_id exists in ontology.element_types`; no relationship/constraint/property-schema/inheritance/architecture rules. The validator uses **only** the ontology referenced by the base state (`context.base_state.ontology_version` → `context.ontology`), never a global/hardcoded core ontology. `repository/change.rs` — `validate_create_element_ontology(PreparedElementCreation) -> Result<PreparedElementCreation, ChangeError>` composes it; `ChangeError` gains `Ontology(#[from] OntologyError)`. Validation does not mutate (candidate, element, ObjectStore, and accepted ref all unchanged). No invariant validation (1.4), no persistence, no ChangeRevision, no CAS. 7 new tests. `cargo test` 131 pass, fmt/clippy clean.
- [x] **1.4 — Minimal invariant validation.** Only what `CreateElement` and candidate-state correctness require:
  - stable identity: no duplicate `ElementId` in the candidate state;
  - valid lifecycle: the new version is Active;
  - referenced objects exist and object kinds match (the new version is a `KnowledgeElementVersion`);
  - candidate state internally coherent (structural canonical validity, per `encoding/validate.rs`).
  - The full `docs/invariants.md` groups (relationship, traceability, authority, validation, history) are **not** enforced in this slice.
    Notes: `repository/validation/invariant.rs` — `InvariantError` + `validate_create_element_invariants(&PreparedElementCreation)`. Validates the **candidate SemanticState**, not persistence (V1/S1 are not yet in the ObjectStore; persistence-existence is 1.6 / repository open/integrity). Checks: (1) structural canonical form via `CanonicalValidate` (wrapped as `InvalidCanonicalStructure`, not reimplemented); (2) `Active` lifecycle; (3) derived V1 identity == `element_version_id`; (4) candidate refs E1 → V1; (5) candidate == `base elements + exactly E1 → V1` (removing the added entry recovers base exactly — no removal/replacement/unrelated insertion/version change); (6) ontology reference preserved; (7) relationships preserved. No repeated ontology validation (1.3 owns it). `change.rs` consuming wrapper `validate_create_element_invariants` → `ChangeError::Invariant`. 12 new tests (11 unit incl. all failure modes + end-to-end no-side-effect). `cargo test` 143 pass, fmt/clippy clean.
- [x] **1.5 — Construct `ChangeRevision C1`.** `change_id` (new), `base_states = [S0]`, `result_state = S1`, `operations = [CreateElement(...)]`, `dependencies = []`, `description = None`.
      Notes: `repository/change.rs` — `prepare_change_revision(ValidatedElementCreation, change_id, description) -> PreparedChangeRevision { creation, state_id, change, change_revision_id }`. Still purely preparatory (no persistence). `state_id` = `canonical_object_id(SemanticState(candidate))`; `change` = `ChangeRevision { change_id, base_states: [base_state_id], result_state: state_id, operations: [CreateElement { new_version: element_version_id }], dependencies, description }`; `change_revision_id` = `canonical_object_id(ChangeRevision)`. Identity + description are caller-supplied (deterministic engine); engine never fabricates prose. `dependencies` is the accepted Change head (`context.accepted.change`) — `none → []`, `Some(Cn) → [Cn]` — so causal ancestry is recorded without hardcoding first-Change semantics. **Typestate guard**: `validate_create_element_invariants` now returns `ValidatedElementCreation` (wraps the prepared creation, exposes `prepared()`); `prepare_change_revision` accepts only `ValidatedElementCreation`, so a `ChangeRevision` cannot be prepared from an unvalidated candidate (compile-time pipeline guarantee). 4 new tests (3 unit: first-change deps, later-change `[Cn]` deps, description preserved; 1 integration e2e with the derived-identity assertions). `cargo test` 147 pass, fmt/clippy clean.
- [x] **1.6 — Persist before publication.** Order matters: immutable objects first (new `KnowledgeElementVersion`, `SemanticState S1`, `ChangeRevision C1`), then `refs/accepted` **last**.
      Notes: `repository/change.rs` — `persist_prepared_change(&Repository, PreparedChangeRevision) -> PersistedChange` materializes immutable objects strictly in dependency order **V1 → S1 → C1** and verifies each `ObjectStore::put` returns the exact pre-derived identity (`element_version_id`, `state_id`, `change_revision_id`). Identity mismatch is fail-closed via `ChangeError::PersistenceIdentityMismatch { kind, expected, actual }` (integrity/programming failure). Step 1.6 performs no CAS/publication and leaves `refs/accepted` byte-for-byte unchanged; partial persistence failures intentionally leave unreachable immutable objects (no rollback/GC). `PersistedChange` is now exported as the typestate boundary for later publication APIs. Added 3 integration tests in `tests/change.rs`: persisted objects exist and decode to expected V1/S1/C1 with IDs matching prepared values; accepted ref unchanged and fresh reopen still opens accepted `S0`; persisting the same prepared revision twice is idempotent with identical IDs and no extra object files. `cargo test -q` passes.
- [x] **1.7 — CAS publication.** Publish only if the accepted ref is still `{ state: S0, change: none }`. On success the new head is `{ state: S1, change: C1 }` and `C1.result_state == S1` holds. On conflict, return a conflict and leave the newly created immutable objects unreferenced (harmless — reachable by ObjectId, not the head).
      Notes: `repository/change.rs` — `publish_persisted_change(&Repository, PersistedChange) -> Result<PublishedChange, ChangeError>` performs the single CAS `expected = persisted.prepared.creation.context.accepted` → `new = { state: S1, change: Some(C1) }` and returns the new head (`PublishedChange { persisted, accepted }`). Before the CAS it verifies the publication-boundary invariant `prepared.change.result_state == prepared.state_id` (fail-closed via `ChangeError::PublicationStateMismatch { expected, actual }`; guaranteed by construction in 1.5/1.6, so a violation is an integrity/programming failure). `RefStoreError::Conflict` is surfaced as the domain-facing `ChangeError::Conflict` (other ref-store failures compose as `ChangeError::RefStore`); `Repository` now owns its `FileRefStore` and exposes `Repository::ref_store()` so publication can reach the CAS. `publish_persisted_change` requires a `PersistedChange` — a raw `PreparedChangeRevision` cannot reach the normal publication API (compile-time progression). No automatic retry, merge, rebase, conflict resolution, rollback of losing objects, or GC. 4 new integration tests (first publication → accepted {S1, C1} + fresh reopen resolving E1→V1; publication changes only refs/accepted — no new immutable objects; stale expected ref → `Conflict` with the concurrent winner kept; two writers from S0 → exactly one publication wins and the losing change's objects remain stored but unreachable; publication-boundary invariant rejection). `cargo test` 154 pass, fmt/clippy clean.
- [x] **1.8 — `kat show <element-id>`.** Resolve the element's current version from the accepted state and display it; proves S1 contains the new element.
      Notes: `repository/query.rs` — read-side query layer (`QueryError`, `ElementView`, `show_element(&Repository, ElementId)`). Resolves the **current** accepted ref at query time (point-in-time read: a handle that just published sees the new head without reopening), loads the SemanticState, binary-searches the element entries (canonically sorted), then loads + decodes + kind-checks the `KnowledgeElementVersion`. `QueryError` distinguishes `ElementNotFound(ElementId)`, `UnexpectedObjectKind`, and composed `ObjectStore`/`Decoding`/`RefStore` failures. Strictly read-only — objects and `refs/accepted` untouched. `domain/property.rs` gains `Display` for `PropertyValue` and `domain/element.rs` for `Lifecycle` (boring deterministic human rendering, documented as non-canonical). CLI `kat show <element-id>` wired as thin parse+dispatch (usage `kat init | kat show <element-id>`; `ElementNotFound` → friendly message, exit 1). 6 integration tests in `tests/query.rs` (view for published element incl. payload == persisted V1, `version_id` == accepted-state entry, unknown → `ElementNotFound`, wrong-kind → `UnexpectedObjectKind` via post-open corruption, fresh reopen, no-mutation invariant) + 3 CLI end-to-end tests in `tests/cli.rs` (spawns the real binary via `CARGO_BIN_EXE_kat`, zero new deps) + 2 unit Display tests. `cargo test` 165 pass, fmt/clippy clean.
- [x] **1.9 — `kat history`.** First case only: accepted head `C1`, `C1.base_states = [S0]`, `C1.result_state = S1`; reconstructing this single linear head proves history works (no general traversal yet).
      Notes: `repository/query.rs` — `history(&Repository) -> Result<Vec<HistoryEntry>, QueryError>` with `HistoryEntry { revision_id, change }`. Reconstructs the accepted Change history from the **dependency graph alone** (never object timestamps or filesystem order), **newest first** — the accepted head is the entry point, traversal is direct, and `cli.md` specifies no order, so newest-first is chosen. Deterministic traversal: head first, then each revision's dependencies in their canonical stored order, depth-first, via a pre-order DFS with a three-state visit set (Unseen/Visiting/Visited): shared ancestry is emitted once; a revision still on the traversal stack is rejected (`QueryError::HistoryCycle`) rather than looping forever. Integrity per revision: the object must exist, decode canonically, and be a ChangeRevision; the accepted head's `change.result_state == accepted.state` is **re-verified against the live ref** (`QueryError::AcceptedChangeStateMismatch`) rather than trusted from the open-time snapshot. Fresh repository (`accepted.change == none`) → `[]`. Multi-revision tests construct ChangeRevisions directly (bypassing the engine) so the traversal is proven not hardcoded to the single-change case. Cycle note: a genuine dependency cycle is **unconstructible** through the content-addressed store (a dependency ObjectId is the SHA-256 of its target's content, so a cycle would require a hash fixed-point); the visiting-state cycle branch is therefore defense-in-depth. CLI `kat history` prints the stable diagnostic block per revision (`revision_id` / `change_id` / `result_state` / `base_states` / `dependencies` / `operations` / `description`), blank-separated; `format_operation` renders operations exactly as stored (`create_element <V1>`, no V1→E1 enrichment). 11 new integration tests in `tests/query.rs` (empty, single change + fields, fresh reopen, missing/wrong-kind head object, head result-state mismatch, missing/wrong-kind dependency, two-revision chain order, diamond shared-dependency-once, no-mutation) + 2 CLI end-to-end tests in `tests/cli.rs`. `cargo test` 178 pass, fmt/clippy clean.

### Phase 1 acceptance test

One end-to-end scenario (integration test + CLI):

```text
kat init
    -> S0

kat create requirement ...
    -> element E1, version V1, state S1, change C1

reopen repository (fresh process)
    -> accepted.state == S1
    -> accepted.change == C1
    -> S1 contains E1 -> V1
    -> C1.result_state == S1

kat show E1
    -> resolves V1

kat history
    -> shows C1
```

This is the first test that validates KAT's actual thesis — a valid semantic change applied and published atomically — not just its repository mechanics.

Implemented as `tests/cli.rs::phase1_acceptance_cli_flow_end_to_end` (black-box CLI flow: init → create → capture IDs → fresh reopen → verify accepted / C1 / S1 / E1→V1 → `kat show` → `kat history`), plus the failure-path CLI tests added at Phase 1 closure.

### Definition of done for Phase 1

- [x] `kat create requirement ...` performs a `CreateElement` change end to end.
- [x] The candidate `SemanticState S1` is constructed and ontology-/invariant-validated.
- [x] Accepted State and Change head are published atomically via CAS.
- [x] A fresh process reopens the repository and verifies the new head (`accepted.state == S1`, `accepted.change == C1`, `C1.result_state == S1`, `S1` maps `E1 -> V1`).
- [x] `kat show E1` resolves `V1`; `kat history` shows `C1`.
- [x] The repository persists across executions.

---

> Part of the master plan: [docs/implementation-plan.md](implementation-plan.md).

## Phase 2: `UpdateElement` vertical slice (design)

The second semantic mutation. It reuses the Phase 1 Change Engine pipeline and
typestates unchanged; only the operation semantics differ. The frozen,
reviewed Phase 1 is the baseline.

### Scope for this slice

Exactly one mutation — `UpdateElement` — against an existing element. No
`DeprecateElement` / `Supersede` / `Link` / `Unlink`, no lifecycle transitions,
no type changes, no property-schema enforcement (v0.1 has none), no impact
analysis or general history work.

### Semantics resolution (grounded in the normative docs)

The six questions resolved **before** touching code:

1. **Full replacement or patch? — PATCH (merge).**
   `operations.md` (Update): "Changes one or more properties"; input is
   "Properties to change"; example `Priority: Medium -> High` changes a single
   property. The input is the **subset of properties to change**; the engine
   merges it onto the element's current properties to construct the full
   immutable `Vn+1` (canonical key order); unspecified properties are
   preserved. A patch is materialized as a complete new immutable version — the
   canonical model has no partial objects.
2. **Can `type_id` change? — No; preserved.** `operations.md` Update input is
   only "Element identity, Properties to change"; a type change is a change of
   kind (retype), which no v0.1 operation defines. The new version carries the
   element's current `type_id` (not an input) and this is enforced as an
   invariant.
3. **Lifecycle restrictions? — Active elements only.** `invariants.md`
   (Lifecycle): "A deprecated element must not be treated as active."
   `operations.md` Update does not define lifecycle transitions (that is
   `DeprecateElement`/`Supersede`). Precondition: the current version is
   `Active`; the new version stays `Active`. Updating deprecated/superseded
   elements is rejected.
4. **`expected_version` precondition semantics? — Explicit input, exact
   ObjectId match.** `canonical-format.md` + `prototype-design.md`
   (UpdateElement): "element_id resolves to expected_version in the base
   state." `expected_version` is an **explicit input** at the engine boundary
   (`UpdateElementInput { element_id, expected_version, properties }`), giving
   real optimistic-concurrency semantics. The engine loads the base state,
   resolves `Vactual = base.elements[E].version`, and requires `Vactual ==
expected_version` before constructing `Vn+1`:
   - element missing → precondition failure;
   - `Vactual != expected_version` → `VersionMismatch`;
   - `Vactual == expected_version` → construct `Vn+1`.
     This is distinct from (and complements) the publication CAS:
     `expected_version` protects the **element-level** assumption, while the CAS
     protects the **repository-level** base state.
5. **Unchanged / no-op updates? — Rejected, two distinct cases.**
   `operations.md` "Changes one or more properties" and `change-model.md`
   "Applying a valid change produces a new semantic state." An update that
   produces no evolution (`result_state == base_state`) is rejected so every
   published Change is meaningful. The two cases are conceptually different:
   - **`EmptyUpdate`** — the patch itself is empty (no properties to change);
   - **`NoEffectiveChange`** — a non-empty patch that produces a
     content-identical `Vn+1` (`Vn+1` ObjectId == `Vn`).
     They may share one error initially, but are distinguished in the design.
6. **Invariants distinguishing `UpdateElement` from `CreateElement`?**
   The core rule, stated normatively as a single-state-delta:
   > For `UpdateElement(E, Vn, Vn+1)`, the candidate Semantic State MUST differ
   > from the base Semantic State **only** in the version mapping for `E`, which
   > changes from `Vn` to `Vn+1`.
   > This is stronger and cleaner than checking unrelated conditions
   > individually; it is Update's analog of Create's "base + exactly E1 → V1".
   > Separately required:
   - `Vn+1.element_id == Vn.element_id == E` (`invariants.md` Identity);
   - `Vn+1.type_id == Vn.type_id` (decision 2);
   - `Vn+1.lifecycle == Vn.lifecycle == Active` (decision 3);
   - `Vn+1 != Vn` by ObjectId (decision 5 — `NoEffectiveChange` rejected).
     Common (shared with Create): candidate structurally canonical; ontology
     reference and relationships preserved; new-version content identity correct
     (encode-then-hash); candidate references the new version. The postcondition
     `E resolves to Vn+1` (`canonical-format.md` UpdateElement) is implied by the
     delta rule plus the candidate reference.

### Work items (ordered sub-steps, mirroring Phase 1)

- [x] **2.1 — `UpdateElement` application.** `apply_update_element(context,
UpdateElementInput { element_id, expected_version, properties })`:
      preconditions — E exists in base (else precondition failure); resolve
      `Vactual = base.elements[E].version` and require `Vactual ==
  expected_version` (else `VersionMismatch`); current version `Vn` is
      `Active`; the patch is non-empty (`EmptyUpdate` rejected) and not a no-op
      (`NoEffectiveChange` rejected). Construct `Vn+1` by merging the patch
      onto `Vn.properties` (canonical order, duplicates rejected); derive
      `Vn+1` ObjectId (encode-then-hash, not persisted); build the candidate
      state with E's entry version = `Vn+1`. No persistence/publication.
      Notes: `repository/change.rs` — `apply_update_element(&Repository, ChangeContext, UpdateElementInput { element_id, expected_version, properties }) -> PreparedElementUpdate { context, previous_element, previous_version_id, element, element_version_id, candidate_state }`. `PreconditionError` gains `ElementNotFound`, `VersionMismatch{element_id, expected, actual}`, `ElementNotActive`, `EmptyUpdate`, `NoEffectiveChange`. Preconditions (in order): element exists → `ElementNotFound`; `Vactual = base.elements[E].version` must equal `expected_version` → `VersionMismatch`; current version loads + decodes + kind-checks as `KnowledgeElementVersion` (wrong kind → `UnexpectedObjectKind` — defense-in-depth, the repository-open layer rejects such states first); lifecycle Active → `ElementNotActive`; patch non-empty → `EmptyUpdate`. The patch is merged onto the current full property set (`merge_property_patch`: preserves unspecified properties, rejects duplicate patch keys, re-canonicalizes encoded-text order); `Vn+1` is built with `element_id`/`type_id`/lifecycle preserved; content identity derived (encode-then-hash, not persisted); `NoEffectiveChange` rejected when `Vn+1` ObjectId == `Vn`. Candidate = base with exactly `E -> Vn+1`. No ontology (2.2), no invariants (2.3), no ChangeRevision, no persistence, no CAS. 9 new integration tests (`tests/change.rs`). `cargo test` 195 pass, fmt/clippy clean.
- [x] **2.2 — Ontology validation.** Reuse `validate_element_type` (`Vn+1`'s
      type is preserved, so this is defense-in-depth and keeps the pipeline
      uniform).
      Notes: `repository/change.rs` — `validate_update_element_ontology(prepared: PreparedElementUpdate) -> Result<PreparedElementUpdate, ChangeError>` **reuses** `validate_element_type(&prepared.context.ontology, &prepared.element.type_id)` — no Update-specific ontology semantics, and `ChangeError::Ontology(#[from] OntologyError)` already composes the error. Validates the newly constructed `Vn+1.type_id` (never an independently supplied type); since 2.1 preserves the type, this proves the updated version stays conformant with the **base-state-referenced** ontology (never `initial_core_ontology()`). Single rule only: `Vn+1.type_id ∈ base ontology.element_types`. No property-schema validation, no invariants (2.3), no ChangeRevision, no persistence, no CAS. 4 new tests (`tests/change.rs`): known type passes; unknown type → `UnknownElementType` (via a manually tampered `Vn+1`, since a reachable repo cannot hold a non-conformant type); custom base ontology proves it uses the base ontology not the global core; validation preserves the prepared update + ObjectStore + accepted ref unchanged. `cargo test` 199 pass, fmt/clippy clean.
- [x] **2.3 — Invariant validation.** Update-specific
      `validate_update_element_invariants` enforcing the normative delta rule
      (candidate differs from base **only** in E's mapping `Vn -> Vn+1`) plus
      identity/type/lifecycle preservation, `Vn+1 != Vn` by ObjectId, `Vn+1`
      identity + reference, and candidate coherence; returns a
      `ValidatedElementCreation`-equivalent typestate so a revision cannot be
      built from an unvalidated candidate.
      Notes: `repository/validation/invariant.rs` — `validate_update_element_invariants(&PreparedElementUpdate) -> Result<(), InvariantError>` enforces the normative rule in order: (1) candidate structurally canonical; (2) `previous.element_id == element.element_id`; (3) `previous_version_id == expected_version == base.elements[E].version`; (4) `element.type_id == previous.type_id`; (5) both lifecycles `Active`; (6) `element_version_id == canonical_object_id(element)`; (7) `element_version_id != previous_version_id` (defensive — the operation-level no-op is rejected at 2.1); (8) candidate maps `E -> element_version_id`; (9) **single-state delta** — removing E's entry from candidate and from base yields identical sets; (10) ontology reference preserved; (11) relationships preserved. New `InvariantError` variants: UpdateIdentityChanged, UpdateTypeChanged, UpdateLifecycleChanged, UpdateBaseVersionMismatch, UpdateVersionIdentityMismatch, UpdateVersionUnchanged, UpdateCandidateReferenceMismatch (UnexpectedElementMutation / OntologyVersionChanged / UnexpectedRelationshipMutation / InvalidCanonicalStructure reused). `PreparedElementUpdate` gains an `expected_version` field (set at apply) so the invariant chain is verifiable. `repository/change.rs` — engine wrapper `validate_update_element_invariants(prepared: PreparedElementUpdate) -> Result<ValidatedElementUpdate, ChangeError>` (delegates to the validator via an aliased import); `ValidatedElementUpdate { prepared }` typestate so 2.4 accepts only validated updates. 16 new tests (`tests/change.rs`): valid passes; identity/type/prev-lifecycle/new-lifecycle/version-identity/expected-version/candidate-wrong-version/candidate-keeps-Vn/another-changed/another-inserted/another-removed/ontology-changed/relationship-added/noncanonical all fail with the specific variant; no-side-effects (ObjectStore + accepted ref unchanged). `cargo test` 215 pass, fmt/clippy clean.
- [x] **2.4 — Construct `ChangeRevision Cn+1`.** `operations =
[UpdateElement { element_id: E, expected_version: Vn, new_version: Vn+1 }]`,
      `base_states = [Sn]`, `result_state = Sn+1`, `dependencies = [accepted
head]` (same rule as 1.5), caller-supplied `change_id`/`description`.
      Notes: `repository/change.rs` — `prepare_update_change_revision(validated: ValidatedElementUpdate, change_id: ChangeId, description: Option<String>) -> Result<PreparedUpdateChangeRevision, ChangeError>` consumes `ValidatedElementUpdate` (pipeline enforced); derives candidate `SemanticState` ObjectId (`state_id` / `Sn+1`), computes `dependencies = accepted.change.into_iter().collect()`, builds `ChangeRevision` with single operation `UpdateElement { element_id: update.element.element_id, expected_version: update.previous_version_id, new_version: update.element_version_id }` and `result_state: state_id`, derives `change_revision_id` (`Cn+1` ObjectId via encode-then-hash). Returns `PreparedUpdateChangeRevision { update: PreparedElementUpdate, state_id, change, change_revision_id }`. Purely preparatory: nothing persisted to `ObjectStore` and accepted ref unchanged. Re-exported in `repository/mod.rs`. 3 new unit/integration tests (`tests/change.rs`): end-to-end preparatory check, `None` description preservation, and `accepted.change == None` (empty `dependencies`). `cargo test` 218 pass, fmt/clippy clean.
- [x] **2.5 — Persist before publication.** `Vn+1 -> Sn+1 -> Cn+1` in dependency
      order, identity-verified (reuses the 1.6 pattern).
      Notes: `repository/change.rs` — `persist_prepared_update_change(&Repository, PreparedUpdateChangeRevision) -> Result<PersistedUpdateChange, ChangeError>` materializes immutable objects into `ObjectStore` in reference order `Vn+1 -> Sn+1 -> Cn+1` and verifies each returned store identity matches the prepared identity (`element_version_id`, `state_id`, `change_revision_id`), failing closed with `ChangeError::PersistenceIdentityMismatch` on mismatch. `refs/accepted` remains unchanged (no CAS/publication), and previous versions (V1) in `ObjectStore` remain byte-for-byte untouched. Exported `PersistedUpdateChange` and `persist_prepared_update_change` in `repository/mod.rs`. 3 new integration tests (`tests/change.rs`): object materialization + exact decoding + V1 immutability + object count delta = +3, reopen/query before publication still resolves V1 (semantic distinction), and idempotent double-persist. `cargo test` 221 pass, fmt/clippy clean.
- [x] **2.6 — CAS publication.** `{Sn, Cn} -> {Sn+1, Cn+1}`; a Conflict leaves
      the new objects unreferenced; `result_state == state_id` guard at the
      boundary (reuses 1.7).
      Notes: `repository/change.rs` — `publish_persisted_update_change(&Repository, PersistedUpdateChange) -> Result<PublishedUpdateChange, ChangeError>`: single CAS `expected = persisted.prepared.update.context.accepted` → `new = { state: Sn+1, change: Some(Cn+1) }`, returning new head in `PublishedUpdateChange { persisted, accepted }`. Pre-CAS defensive check `prepared.change.result_state == prepared.state_id` (`ChangeError::PublicationStateMismatch`); `RefStoreError::Conflict` surfaced as domain `ChangeError::Conflict`. Pipeline typestate enforced (`PersistedUpdateChange` required). Exported `PublishedUpdateChange` and `publish_persisted_update_change` in `repository/mod.rs`. 3 new integration tests (`tests/change.rs`): publication advances head to `{S2, C2}`, zero new objects created during publication, fresh reopen + `show_element` resolves V2, V1 remains in store, CAS conflict on stale head leaves losing objects unaccepted, and tampered `result_state` rejection. `cargo test` 224 pass, fmt/clippy clean.
- [x] **2.7 — CLI `kat update <element-id> ...`.** Thin CLI parse + dispatch (`kat update <element-id> [--title "..."] [--description "..."]`).
      Notes: `src/main.rs` — `cmd_update`, `parse_update_args`, and `update_pipeline` parse CLI arguments, open repository, prepare change context, resolve current version $V_n$ of target element from base state, execute full engine pipeline (`apply_update_element` -> `validate_update_element_ontology` -> `validate_update_element_invariants` -> `prepare_update_change_revision` -> `persist_prepared_update_change` -> `publish_persisted_update_change`), and print stable IDs (`element_id`, `previous_version_id`, `version_id`, `state_id`, `change_id`, `change_revision_id`). Preserves unspecified properties, handles errors cleanly (`ElementNotFound`, `NoEffectiveChange`, `Conflict`, malformed CLI arguments). 5 new integration tests (`tests/cli.rs`): end-to-end update flow + ID output + `kat show` + `kat history` + $V_1$ byte immutability, optional description patch, outside repository failure, unknown element failure, malformed CLI flags / no effective change failure. `cargo test` 229 pass, fmt/clippy clean.
- [ ] **2.8 — Verification.** `kat show E` resolves `Vn+1`; `kat history` shows
      `Cn+1` (newest first) with `UpdateElement`; `Vn` remains in the object
      store (previous state traceable, per `operations.md`).

### Phase 2 acceptance test

```text
kat init
kat create requirement --title "A"     -> E1, V1, S1, C1
kat update <E1> --title "B"  # CLI resolves E1 -> V1, passes expected_version = V1
    -> V2, S2, C2

reopen (fresh process)
    accepted.state == S2, accepted.change == C2
    S2 maps E1 -> V2
    C2.operations == [UpdateElement{ E1, expected_version: V1, new_version: V2 }]
    C2.base_states == [S1], C2.result_state == S2

kat show E1  -> title "B" (resolves V2)
kat history  -> [C2, C1] (newest first)
V1 still present in objects/ (previous state traceable)
```

### Definition of done for Phase 2

- [ ] `kat update <element-id> --title "..."` performs an `UpdateElement` change end to end.
- [ ] Patch semantics: only the named properties change; others are preserved.
- [ ] Preconditions enforced: element exists, Active, current version == `expected_version` (else `VersionMismatch`), `EmptyUpdate` / `NoEffectiveChange` rejected.
- [ ] Invariants enforced: identity/type/lifecycle preserved; exact single-entry replacement.
- [ ] Accepted State and Change head published atomically via CAS; a conflict leaves objects unreferenced.
- [ ] Fresh reopen verifies the new head; `kat show` resolves `Vn+1`; `kat history` shows `Cn+1`; `Vn` traceable.
- [ ] The repository persists across executions.

---

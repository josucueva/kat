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
4. **`expected_version` precondition semantics? — Exact ObjectId match.**
   `canonical-format.md` + `prototype-design.md` (UpdateElement): "element_id
   resolves to expected_version in the base state." The engine resolves the
   current version `Vn = base.elements[E].version`, verifies it is `Active`,
   and uses it as `expected_version` (the caller need not know it; the CAS on
   publication is the concurrency guard). A supplied `expected_version` that
   does not match the base mapping is a rejected precondition.
5. **Unchanged / no-op updates? — Rejected.** `operations.md` "Changes one or
   more properties" and `change-model.md` "Applying a valid change produces a
   new semantic state." An empty patch, or a patch that produces a
   content-identical version (`Vn+1` ObjectId == `Vn`), would yield
   `result_state == base_state` and no evolution. Rejected as a precondition
   failure so every published Change is meaningful.
6. **Invariants distinguishing `UpdateElement` from `CreateElement`?**
   Common (shared): candidate structurally canonical; ontology reference and
   relationships preserved; new-version content identity correct
   (encode-then-hash); candidate references the new version. Update-specific:
   - **identity preserved**: `Vn+1.element_id == E` (`invariants.md` Identity);
   - **type preserved**: `Vn+1.type_id == Vn.type_id` (decision 2);
   - **lifecycle preserved**: `Vn+1.lifecycle == Active` (decision 3);
   - **exact single-entry replacement**: candidate.elements == base.elements
     except E's version == `Vn+1` (no other add/remove/change) — Update's analog
     of Create's "base + exactly E1 → V1";
   - **postcondition** `E resolves to Vn+1` (`canonical-format.md`
     UpdateElement) is equivalent to the replacement invariant plus the
     candidate reference.

### Work items (ordered sub-steps, mirroring Phase 1)

- [ ] **2.1 — `UpdateElement` application.** `apply_update_element(context,
    UpdateElementInput { element_id, properties_to_change })`: preconditions —
      E exists in base, current version `Vn` is Active, patch non-empty and not
      a no-op; resolve `expected_version = Vn`; construct `Vn+1` by merging the
      patch onto `Vn.properties` (canonical order, duplicates rejected); derive
      `Vn+1` ObjectId (encode-then-hash, not persisted); build the candidate
      state with E's entry version = `Vn+1`. No persistence/publication.
- [ ] **2.2 — Ontology validation.** Reuse `validate_element_type` (`Vn+1`'s
      type is preserved, so this is defense-in-depth and keeps the pipeline
      uniform).
- [ ] **2.3 — Invariant validation.** Update-specific
      `validate_update_element_invariants` (identity/type/lifecycle
      preservation, exact single-entry replacement, `Vn+1` identity + reference,
      candidate coherence); returns a `ValidatedElementCreation`-equivalent
      typestate so a revision cannot be built from an unvalidated candidate.
- [ ] **2.4 — Construct `ChangeRevision Cn+1`.** `operations =
    [UpdateElement { element_id: E, expected_version: Vn, new_version: Vn+1 }]`,
      `base_states = [Sn]`, `result_state = Sn+1`, `dependencies = [accepted
    head]` (same rule as 1.5), caller-supplied `change_id`/`description`.
- [ ] **2.5 — Persist before publication.** `Vn+1 -> Sn+1 -> Cn+1` in dependency
      order, identity-verified (reuses the 1.6 pattern).
- [ ] **2.6 — CAS publication.** `{Sn, Cn} -> {Sn+1, Cn+1}`; a Conflict leaves
      the new objects unreferenced; `result_state == state_id` guard at the
      boundary (reuses 1.7).
- [ ] **2.7 — CLI `kat update <element-id> ...`.** Per `cli.md` sketch
      (`kat update <element-id> --property <key>=<value> ...`), thin parse +
      dispatch; prints the new `version_id` / `state_id` / `change_id` /
      `change_revision_id`. (Flag shape is a CLI-layer decision; `--title` /
      `--description` convenience flags for parity with `kat create` are a
      possible addition.)
- [ ] **2.8 — Verification.** `kat show E` resolves `Vn+1`; `kat history` shows
      `Cn+1` (newest first) with `UpdateElement`; `Vn` remains in the object
      store (previous state traceable, per `operations.md`).

### Phase 2 acceptance test

```text
kat init
kat create requirement --title "A"     -> E1, V1, S1, C1
kat update <E1> --title "B"            -> V2, S2, C2

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
- [ ] Preconditions enforced: element exists, Active, `expected_version ==` current version, no-op rejected.
- [ ] Invariants enforced: identity/type/lifecycle preserved; exact single-entry replacement.
- [ ] Accepted State and Change head published atomically via CAS; a conflict leaves objects unreferenced.
- [ ] Fresh reopen verifies the new head; `kat show` resolves `Vn+1`; `kat history` shows `Cn+1`; `Vn` traceable.
- [ ] The repository persists across executions.

---

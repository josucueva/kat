# Multi-Operation Change Design (v0.2)

> **Specification & Design Document for Phase 13**  
> Part of the [v0.2 Master Plan](implementation/v0.2/implementation-plan.md).

---

## 1. Purpose & Motivation

In KAT v0.1, every mutation CLI command (`create`, `update`, `deprecate`, `supersede`, `link`, `unlink`) executed an isolated pipeline:
$$\text{prepare} \longrightarrow \text{apply} \longrightarrow \text{validate} \longrightarrow \text{persist} \longrightarrow \text{publish (CAS)}$$

This created a **one-operation-per-revision** enforcement, causing operational ceremony during real-world software evolution. For example, implementing "Add reduced-motion support" required 4 separate CLI commands, producing 4 separate `ChangeRevision`s in `refs/accepted`:
1. `kat update R3 ...` $\to$ Revision $C_1$
2. `kat update I3 ...` $\to$ Revision $C_2$
3. `kat update V1 ...` $\to$ Revision $C_3$
4. `kat link addresses ...` $\to$ Revision $C_4$

However, KAT's canonical format (`spec/canonical-format.cddl`) and change model (`docs/change-model.md`) have always defined a `ChangeRevision` as an array of operations:
```cddl
change-revision = [
  change_id: uuid,
  result_state: object_id,
  base_states: [* object_id],
  dependencies: [* object_id],
  description: text / null,
  operations: [* operation]
]
```

Phase 13 establishes the design for **staging multiple operations into one atomic `ChangeRevision`** (`kat change begin/status/commit/abort`).

---

## 2. Frozen Architectural Decisions

### 2.1 Storage & Persistence of Open Draft Sessions
- **Location**: Draft session state is persisted locally on disk at `.kat/work/change/session.json` (inside a dedicated `.kat/work/change/` directory outside `.kat/objects`).
- **Private, Non-Canonical & Crash-Safe**: Session files are local, private, non-canonical, not content-addressed, and not part of accepted history. Session file updates use crash-safe atomic replacement (temporary file $\to$ `fsync`/flush $\to$ atomic rename).
- **Session Locking & Single-Draft Invariant**: At most **one** open change session per repository is permitted. Mutation operations on open sessions acquire an exclusive local session lock to prevent concurrent KAT processes from corrupting the session.
- **Session Metadata Anchors**: Persists `base_state` (`ObjectId`), `base_change` (`ObjectId | None`), `created_at` timestamp, `description`, `operations` vector, and `working_state`.

### 2.2 Working Candidate State ($S_{\text{working}}$)
- Upon `kat change begin`, candidate state is initialized to $S_{\text{working}} = S_n$.
- Staged operations ($O_1, O_2, \dots, O_m$) apply sequentially to $S_{\text{working}}$.
- Preconditions and `expected_version` resolution for operation $O_k$ evaluate against $S_{\text{working}}$ (the candidate state produced by operations $O_1 \dots O_{k-1}$).
- **Create-then-Reference Composition**: Staging supports composing newly created elements (e.g. $O_1$ creates $E_2$, $O_2$ links $E_1 \to E_2$, $O_3$ updates $E_2$).

### 2.3 Operation Ordering & Precondition Validity
- **Operation Order**: Operations preserve successful staging order exactly (no sorting or re-ordering).
- **Existing Preconditions**: Validity is determined strictly by $S_{\text{working}}$ + existing operation preconditions. Operations on the same element within one draft (e.g. `update E`, then `deprecate E`) are valid as long as each operation's preconditions hold against $S_{\text{working}}$.

### 2.4 Transient Object Storage (Zero Store Pollution)
- Staged element versions, relationship versions, and candidate state snapshots exist purely in `.kat/work/change/*` during staging.
- **No draft objects are written to `.kat/objects`** until `kat change commit` succeeds. If a draft is aborted or rejected, `.kat/objects` remains unpolluted.

### 2.5 Single Atomic Accepted-State Publication
- At `kat change commit`:
  1. Complete candidate state $S_{\text{working}}$ undergoes whole-candidate validation (ontology + invariants).
  2. Materializes canonical immutable objects ($V_{\text{new}}, S_{\text{new}}, C_{\text{new}}$) and persists them to `.kat/objects`.
  3. Constructs single `ChangeRevision` with `operations = [O_1, O_2, \dots, O_m]`.
  4. Performs single CAS update on `refs/accepted` ($S_n \to S_{\text{new}}$).
  5. Removes `.kat/work/change/session.json` upon successful publication.

### 2.6 Failure & Stale Session Semantics
- **Failed Operation Staging**: If a staged operation fails preconditions, the operation has no effect (candidate remains $C_{k-1}$, operation list unchanged). The draft session remains open and usable.
- **Commit Invariant Failure**: If whole-candidate validation fails during `commit`, commit is aborted; the draft session remains open for user inspection or correction.
- **CAS Conflict & Stale Session State**: If `refs/accepted` moved since `begin` (`accepted != session.base_state`), commit rejects with `Conflict`. The session is marked **stale**:
  - A stale session may be inspected (`kat change status`) or aborted (`kat change abort`).
  - A stale session **may NOT** be committed, staged onto, or silently rebased (no `rebase` command in v0.2).

### 2.7 Dual-Mode Command Behavior (Staged vs Accepted Output)
- **Draft Open**: Mutation commands (`create`, `update`, `deprecate`, `supersede`, `link`, `unlink`) automatically stage onto the open draft, outputting distinct staged status (e.g. `staged update element 7af83d1c / change operations: 2`).
- **No Draft Open**: Mutation commands execute single-operation auto-commit for v0.1 backward compatibility.

### 2.8 Read Query Isolation
- **Accepted State Only**: Standard read commands (`status`, `list`, `show`, `history`, `trace`, `impact`, `validate`, `artifacts`) **always** inspect accepted state ($S_n$).
- **Draft Inspection via `kat change status`**: Draft metadata, staged operations, and candidate summary are inspected explicitly via `kat change status`.

---

## 3. Frozen Summary Matrix

```text
Session
  persisted locally (.kat/work/change/session.json)
  one per repository
  mutable and non-canonical
  stored outside ObjectStore
  based on explicit accepted state + change

Reads
  ordinary read commands see accepted state only
  change status inspects draft/candidate

Operations
  applied sequentially to candidate
  successful staging order preserved
  failed operation has no effect
  existing operation preconditions remain authoritative

Commit
  validate complete candidate
  materialize canonical objects
  create exactly one ChangeRevision
  create exactly one resulting SemanticState
  publish via one accepted-ref CAS

Validation failure
  preserve session

Persistence failure
  preserve session when recoverable
  accepted ref unchanged

Conflict
  no merge
  no automatic rebase
  mark session stale
  allow inspection/abort only

Abort
  remove working session
  no canonical ChangeRevision

Canonical format
  unchanged
```

---

## 4. CLI Command Surface

### `kat change begin [--description "..."]`
Opens a new draft change session on the current accepted state $S_n$.

### `kat change status`
Displays draft session status:
```text
Change session
  state:        open
  description:  Add reduced-motion support
  base_state:   31acb18e09d4
  base_change:  aec57b12ea19
  operations:   3

Operations
  1. update element       7af83d1c  Responsive requirement
  2. update element       b72aa941  Responsive implementation
  3. deprecate element    e9ca1182  Mobile validation

Working state preview
  elements:       12 (+1)
  relationships:  10 (+1)
  consistency:    0 violations, 0 unverified constraints
```

### `kat change commit`
Validates $S_{\text{working}}$, persists canonical objects, publishes single `ChangeRevision` via CAS, and cleans up `.kat/work/change/session.json`.

### `kat change abort`
Discards active `.kat/work/change/session.json` session.

---

## 5. Specification Conformance

| Component | Conformance Status |
| :--- | :--- |
| `spec/canonical-format.cddl` | **Unchanged (100% compatible)** |
| `docs/change-model.md` | Aligned (multi-operation changes explicitly realized) |
| `docs/operations.md` | Aligned (single-op & multi-op staging workflow) |
| `docs/cli.md` | Extended (`kat change begin/status/commit/abort`) |

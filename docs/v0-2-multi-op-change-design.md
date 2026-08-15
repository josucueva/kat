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

## 2. Core Architectural Decisions

### 2.1 Storage & Persistence of Open Drafts
- **Location**: Local draft session state is stored in `.kat/draft.json`.
- **Private & Non-Canonical**: The draft file is local, private to the workspace, and explicitly **non-canonical** (stored as standard JSON, not CBOR). It is never referenced by `refs/accepted` or stored in `.kat/objects`.
- **Single-Draft Invariant**: At most **one** open draft transaction is permitted per KAT repository at any time. Attempting `kat change begin` while a draft is already open returns an error.

### 2.2 Working Candidate State ($S_{\text{working}}$)
- Upon `kat change begin`, the working candidate state is initialized to the current accepted state: $S_{\text{working}} = S_n$.
- Each staged operation ($O_1, O_2, \dots, O_m$) is applied sequentially to $S_{\text{working}}$.
- Preconditions and `expected_version` resolution for operation $O_k$ evaluate against $S_{\text{working}}$ (the candidate state produced by operations $O_1 \dots O_{k-1}$).

### 2.3 Transient Object Construction (Zero Store Pollution)
- Staged element versions, relationship versions, and candidate states exist **only** in memory / `.kat/draft.json` during staging.
- **No draft objects are written to `.kat/objects`** until `kat change commit` succeeds. If a draft is aborted or rejected, `.kat/objects` remains completely unpolluted.

### 2.4 Atomicity & Publication
- At `kat change commit`:
  1. The complete candidate state $S_{\text{working}}$ undergoes whole-candidate validation (ontology + invariants).
  2. A single `ChangeRevision` is constructed with `operations = [O_1, O_2, \dots, O_m]`.
  3. All new canonical objects ($V_{\text{new}}, S_{\text{new}}, C_{\text{new}}$) are persisted to `.kat/objects`.
  4. Single CAS update advances `refs/accepted` from $S_n \to S_{\text{new}}$.
  5. `.kat/draft.json` is removed upon successful CAS.

### 2.5 Failure & Conflict Semantics
- **Failed Operation Staging**: If an operation fails preconditions mid-draft, staging is rejected; `.kat/draft.json` is unchanged.
- **Commit Validation Failure**: If whole-candidate validation fails during `commit`, commit is aborted; the draft remains open for inspection or correction.
- **CAS Conflict**: If `refs/accepted` moved since `begin` (concurrent writer won), commit fails with `Conflict`; the draft remains open on disk (no automatic silent rebase in v0.2.0).
- **Abort (`kat change abort`)**: Deletes `.kat/draft.json`; repository accepted state and object store remain untouched.

### 2.6 Dual-Mode Command Behavior (Auto-Staging vs Auto-Commit)
- **Draft Open**: When `.kat/draft.json` exists, mutation commands (`create`, `update`, `deprecate`, `supersede`, `link`, `unlink`) automatically append their operation to the active draft instead of publishing.
- **No Draft Open**: When no draft exists, mutation commands maintain single-operation auto-commit behavior for v0.1 backward compatibility.

### 2.7 Read Query Isolation
- **Accepted State Only**: Standard read commands (`status`, `list`, `show`, `history`, `trace`, `impact`, `validate`, `artifacts`) **always** inspect accepted state ($S_n$).
- **Draft Inspection via `kat change status`**: Draft contents and candidate validation preview are inspected explicitly via `kat change status`. Standard queries remain completely deterministic and independent of uncommitted drafts.

---

## 3. CLI Command Surface

### `kat change begin [--description "..."]`
Opens a new draft change session on the current accepted state $S_n$.

### `kat change status`
Displays draft session status:
```text
Draft change session
  base_state:   abd76d8bd634
  description:  Implement WebAuthn authentication
  created_at:   2026-08-15T14:50:00Z

Staged operations (3)
  1. create element      design-decision  "Use WebAuthn"
  2. update element      requirement      7af83d1c "User authentication"
  3. link                addresses        7af83d1c -> bc18a910

Working state preview
  elements:       12 (+1)
  relationships:  10 (+1)
  consistency:    0 violations, 0 unverified constraints
```

### `kat change commit`
Validates $S_{\text{working}}$, persists canonical objects, publishes single `ChangeRevision` via CAS, and cleans up `.kat/draft.json`.

### `kat change abort`
Discards active `.kat/draft.json` session.

---

## 4. Draft Session File Schema (`.kat/draft.json`)

```json
{
  "schema_version": 1,
  "base_state_id": "abd76d8bd6344211a7b89234567890abcdef1234567890abcdef1234567890ab",
  "created_at": "2026-08-15T14:50:00Z",
  "description": "Implement WebAuthn authentication",
  "operations": [
    {
      "kind": "CreateElement",
      "new_version": {
        "element_id": "bc18a910-0000-4000-8000-000000000000",
        "version": 1,
        "type_id": "kat.core/design-decision",
        "lifecycle": "active",
        "properties": {
          "title": "Use WebAuthn"
        }
      }
    }
  ]
}
```

---

## 5. Specification Conformance

| Component | Conformance Status |
| :--- | :--- |
| `spec/canonical-format.cddl` | **Unchanged (100% compatible)** |
| `docs/change-model.md` | Aligned (multi-operation changes explicitly realized) |
| `docs/operations.md` | Aligned (single-op & multi-op staging workflow) |
| `docs/cli.md` | Extended (`kat change begin/status/commit/abort`) |

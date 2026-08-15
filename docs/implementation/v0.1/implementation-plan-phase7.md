# Phase 7 Implementation Plan: `Trace Origin` (`kat trace`)

Phase 7 implements `UC-004: Trace Origin`, fulfilling the first concrete slice of the v0.1 Traceability requirements:
> *Tracing an element back to its origin.*

Trace Origin is a **pure read-only query** over the repository's current accepted semantic state (`refs/accepted`). It discovers the authoritative provenance paths that explain where a knowledge element came from, preserving traversal steps, handling cycles safely, traversing deprecated and superseded elements, and deterministically ordering output.

---

## 1. Frozen Design & Query Semantics

1. **Read-Only State Query**:
   - `kat trace <element-id>` inspects the current accepted `SemanticState` ($S_n$).
   - Does **not** mutate the repository, write `ObjectStore` objects, create `ChangeRevisions`, or touch `refs/accepted`.
2. **Accepted State Scoping**:
   - Only relationships currently present in $S_n.relationships$ are traversed. Unlinked relationships belong to `kat history` and are not traversed by `kat trace`.
3. **Lifecycle Transparency**:
   - `kat trace` accepts `Active`, `Deprecated`, and `Superseded` root elements. Traversal continues across deprecated and superseded endpoints.
4. **Normative Origin Traversal Policy**:
   - Provenance direction is relation-specific:

| Relationship Type | Canonical Form | Origin Traversal Direction | Semantic Rationale |
| :--- | :--- | :--- | :--- |
| `kat.core/motivates` | Intent $\xrightarrow{\text{motivates}}$ Req / Decision | Target $\to$ Source (**Backward**) | Req / Decision is motivated by Intent |
| `kat.core/derived-from` | Artifact $\xrightarrow{\text{derived-from}}$ Auth Knowledge | Source $\to$ Target (**Forward**) | Artifact is derived from Requirement / Decision |
| `kat.core/realizes` | Impl $\xrightarrow{\text{realizes}}$ Requirement | Source $\to$ Target (**Forward**) | Implementation realizes Requirement |
| `kat.core/represents` | Artifact $\xrightarrow{\text{represents}}$ Implementation | Source $\to$ Target (**Forward**) | Artifact represents Implementation |
| `kat.core/validates` | Validation $\xrightarrow{\text{validates}}$ Subject | Source $\to$ Target (**Forward**) | Validation validates Subject |
| `kat.core/restricts` | Constraint $\xrightarrow{\text{restricts}}$ Req / Decision / Impl | Target $\to$ Source (**Backward**) | Element is restricted by Constraint |
| `kat.core/addresses` | Decision $\xrightarrow{\text{addresses}}$ Requirement | Source $\to$ Target (**Forward**) | Decision exists to address Requirement |
| `kat.core/supersedes` | Replacement $\xrightarrow{\text{supersedes}}$ Existing Decision | Source $\to$ Target (**Forward**) | Replacement decision supersedes old decision |
| `kat.core/guides` | Decision $\xrightarrow{\text{guides}}$ Implementation | Target $\to$ Source (**Backward**) | Implementation is guided by Decision |
| `kat.core/depends-on` | Impl $\xrightarrow{\text{depends-on}}$ Implementation | *Excluded (Non-Origin)* | Structural dependency (reserved for Impact Analysis) |

5. **Cycle Safety & Determinism**:
   - Traversal tracks visited edge instances per path branch to suppress cyclic infinite loops.
   - Outgoing edges from any node are evaluated in canonical `SemanticState.relationships` order (`RelationshipId` order).
6. **Path Preservation**:
   - Returns structured `TraceResult` containing root element and `Vec<TracePath>` where each path contains ordered `TraceStep` entries detailing `from_element_id`, `relationship_id`, `relationship_type_id`, `direction`, and `to_element_id`.

---

## 2. Work Breakdown & Implementation Steps

### Step 7.1 — Origin Traversal Policy & Data Structures
- Define `TraversalDirection` (`Forward`, `Backward`) in `src/repository/query.rs`.
- Define `TraceStep`, `TracePath`, and `TraceResult` structs.
- Implement `origin_traversal_rule(relationship_type_id: &str) -> Option<TraversalDirection>`.

### Step 7.2 — Core `trace_origin` Query Implementation
- Implement `trace_origin(repository: &Repository, root_element_id: ElementId) -> Result<TraceResult, QueryError>` in `src/repository/query.rs`.
- Verify `root_element_id` presence in $S_n.elements$; return `QueryError::ElementNotFound(root_element_id)` if missing.
- Perform deterministic depth-first path exploration following origin traversal rules.

### Step 7.3 — Query Layer Re-exports & Engine Unit Tests
- Re-export `trace_origin`, `TraceResult`, `TracePath`, `TraceStep`, `TraversalDirection` in `src/repository/mod.rs`.
- Add unit tests in `tests/query.rs`:
  - `trace_origin_single_hop_backward` (`Requirement` $\to$ `Intent`)
  - `trace_origin_multi_hop_forward_and_backward` (`Artifact` $\to$ `Implementation` $\to$ `Requirement` $\to$ `Intent`)
  - `trace_origin_superseded_and_deprecated_endpoints`
  - `trace_origin_cycle_suppression`
  - `trace_origin_unknown_element_returns_not_found`
  - `trace_origin_unlinked_relationship_not_traversed`

### Step 7.4 — CLI `kat trace` Wiring & Formatting
- Implement `cmd_trace(args: &[String]) -> ExitCode` and `parse_trace_args` in `src/main.rs`.
- Syntax: `kat trace <element-id>`.
- Format output as a clear hierarchical tree with element titles, types, lifecycles, and relationship labels.

### Step 7.5 — Acceptance Verification & Phase 7 Closure
- Add `phase7_acceptance_cli_flow_end_to_end` in `tests/cli.rs`.
- Extend AuthX scenario (`Artifact` $\to$ `Implementation` $\to$ `Requirement` $\to$ `Intent`, `Validation` $\to$ `Requirement`).
- Verify CLI output trees and history non-mutation.
- Update `docs/cli.md`, `docs/implementation-plan.md`, and freeze Phase 7.

---

## 3. Verification & Acceptance Criteria

- `cargo test` passes 100% cleanly (all existing + new tests).
- `cargo clippy --all-targets -- -D warnings` clean.
- `cargo fmt --check` clean.

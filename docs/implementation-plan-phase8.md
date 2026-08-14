# Phase 8 Implementation Plan: `Impact Analysis` (`kat impact`)

Phase 8 implements `UC-005: Impact Analysis`, fulfilling the second concrete slice of the v0.1 Traceability requirements:
> *Identifying knowledge elements that may be affected by a proposed or applied change, explicitly distinguishing between directly changed elements, semantically affected elements, and artifacts affected through traceability relationships.*

Impact Analysis is a **pure read-only query** over the repository's current accepted semantic state (`refs/accepted`). It propagates impact conservatively through the semantic graph, preserving propagation paths, handling cycles safely, categorizing targets by element type, and filtering target elements to active current knowledge.

---

## 1. Frozen Design & Query Semantics

1. **Read-Only Query Scoping**:
   - `kat impact <element-id>` inspects the current accepted `SemanticState` ($S_n$).
   - Does **not** mutate the repository, write `ObjectStore` objects, create `ChangeRevisions`, or touch `refs/accepted`.
2. **First-Class Result Categorization**:
   - Explicitly partitions output into three distinct buckets:
     1. `directly_changed`: The root `ElementId` queried.
     2. `semantically_affected`: Non-artifact `Active` elements reached via impact propagation (`Requirement`, `Constraint`, `Design Decision`, `Implementation`, `Validation`, `Intent`).
     3. `affected_artifacts`: Active elements of type `kat.core/artifact` reached via impact propagation.
3. **Lifecycle Filtering**:
   - Query root accepts any lifecycle state (`Active`, `Deprecated`, `Superseded`).
   - Propagated target elements are filtered to **`Active`** elements (only active elements represent current affected knowledge).
4. **Normative Impact Propagation Policy**:

| Relationship Type | Canonical Form | Impact Propagation Direction | Semantic Rationale |
| :--- | :--- | :--- | :--- |
| `kat.core/motivates` | Intent $\xrightarrow{\text{motivates}}$ Req / Decision | Source $\to$ Target (**Forward**) | Changed Intent affects motivated Requirement / Decision |
| `kat.core/addresses` | Decision $\xrightarrow{\text{addresses}}$ Requirement | Target $\to$ Source (**Backward**) | Changed Requirement affects addressing Decision |
| `kat.core/restricts` | Constraint $\xrightarrow{\text{restricts}}$ Req / Decision / Impl | Source $\to$ Target (**Forward**) | Changed Constraint affects restricted elements |
| `kat.core/guides` | Decision $\xrightarrow{\text{guides}}$ Implementation | Source $\to$ Target (**Forward**) | Changed Decision affects guided Implementation |
| `kat.core/realizes` | Impl $\xrightarrow{\text{realizes}}$ Requirement | Target $\to$ Source (**Backward**) | Changed Requirement affects realizing Implementation |
| `kat.core/represents` | Artifact $\xrightarrow{\text{represents}}$ Implementation | Target $\to$ Source (**Backward**) | Changed Implementation affects representing Artifact |
| `kat.core/derived-from` | Artifact $\xrightarrow{\text{derived-from}}$ Auth Knowledge | Target $\to$ Source (**Backward**) | Changed Auth Knowledge affects derived Artifact |
| `kat.core/validates` | Validation $\xrightarrow{\text{validates}}$ Subject | Target $\to$ Source (**Backward**) | Changed Subject affects validating evidence |
| `kat.core/depends-on` | Impl A $\xrightarrow{\text{depends-on}}$ Impl B | Target $\to$ Source (**Backward**) | Changed Dependency B affects dependent Implementation A |
| `kat.core/supersedes` | Replacement $\xrightarrow{\text{supersedes}}$ Existing Decision | *Excluded (Non-Impact)* | Historical evolution relation (omitted from current impact) |

5. **Cycle Safety & Path Preservation**:
   - Traversal tracks visited element IDs to guarantee finite, deterministic exploration.
   - Preserves step sequences (`ImpactStep`, `ImpactPath`, `ImpactedElement`, `ImpactResult`).

---

## 2. Work Breakdown & Implementation Steps

### Step 8.1 — Impact Policy & Query Domain Data Structures
- Define `ImpactStep`, `ImpactPath`, `ImpactedElement`, and `ImpactResult` structs in `src/repository/query.rs`.
- Implement `impact_propagation_direction(relationship_type_id: &str) -> Option<TraversalDirection>`.

### Step 8.2 — Core `analyze_impact` Query Implementation
- Implement `analyze_impact(repository: &Repository, root_element_id: ElementId) -> Result<ImpactResult, QueryError>` in `src/repository/query.rs`.
- Verify `root_element_id` presence in $S_n.elements$; return `QueryError::ElementNotFound(root_element_id)` if missing.
- Perform deterministic depth-first graph expansion following impact propagation rules in canonical `RelationshipId` order.
- Filter reached targets to `Lifecycle::Active` and partition into `semantically_affected` vs `affected_artifacts`.

### Step 8.3 — Query Layer Re-exports & Engine Unit Tests
- Re-export `analyze_impact`, `ImpactResult`, `ImpactedElement`, `ImpactPath`, `ImpactStep` in `src/repository/mod.rs`.
- Add unit tests in `tests/query.rs`:
  - `impact_single_hop_backward` (`Requirement` $\to$ `Decision`)
  - `impact_multi_hop_transitive` (`Requirement` $\to$ `Implementation` $\to$ `Artifact`, `Implementation B` via `depends-on`)
  - `impact_category_partitioning` (separate `semantically_affected` vs `affected_artifacts`)
  - `impact_supersedes_excluded`
  - `impact_non_active_targets_filtered`
  - `impact_unknown_element_returns_not_found`
  - `impact_does_not_mutate_repository`

### Step 8.4 — CLI `kat impact` Wiring & Formatting
- Implement `cmd_impact(args: &[String]) -> ExitCode` in `src/main.rs`.
- Syntax: `kat impact <element-id>`.
- Format output into the three mandatory v0.1 buckets (`Directly changed`, `Semantically affected`, `Affected artifacts`) with element titles, types, and path rationale.

### Step 8.5 — Acceptance Verification & Phase 8 Closure
- Add `phase8_acceptance_cli_flow_end_to_end` in `tests/cli.rs`.
- Extend AuthX scenario to verify impact analysis when `Requirement 1` changes:
  - `Directly changed`: Requirement 1
  - `Semantically affected`: PASETO Decision, Implementation, Validation Suite, Dependent Implementation B
  - `Affected artifacts`: Artifact
- Update `docs/cli.md`, `docs/implementation-plan.md`, and freeze Phase 8.

---

## 3. Verification & Acceptance Criteria

- `cargo test` passes 100% cleanly (all existing + new tests).
- `cargo clippy --all-targets -- -D warnings` clean.
- `cargo fmt --check` clean.

# Phase 10 Implementation Plan: `Artifact Accountability` (`kat artifacts`)

Phase 10 implements `UC-007: Artifact Accountability`, fulfilling the final core functional requirement for KAT v0.1:
> *Artifacts must remain traceable to the authoritative knowledge they represent, implement, validate, or materialize, and the system must be able to identify when an artifact is not consistent with the current semantic model.*

Artifact Accountability is a **pure read-only query** over the repository's current accepted semantic state (`refs/accepted`) and change history (`history`). It evaluates whether active `Artifact` elements remain aligned with the exact versions of authoritative knowledge elements to which they are linked via direct accountability relationships (`kat.core/represents`, `kat.core/derived-from`), without introducing breaking schema changes or automatic code reconciliation.

---

## 1. Frozen Design & Query Semantics

1. **Read-Only Operation & Non-Mutation**:
   - `kat artifacts` inspects the current accepted `SemanticState` ($S_n$), active `OntologyVersion` ($O_n$), and change history chain (`history`).
   - Does **not** mutate repository files, write `ObjectStore` objects, or touch `refs/accepted`.
2. **History-Derived Baseline (Zero Schema Changes)**:
   - For each active `Artifact A` and each direct relationship $R$ (`represents` or `derived-from`) targeting an authoritative element $E$:
   - KAT inspects `history` to locate the `ChangeRevision` $C_{\text{link}}$ that first introduced relationship $R$ into the accepted state $S_{\text{link}}$.
   - In state $S_{\text{link}}$, element $E$ had version $V_{\text{baseline}}$.
   - In current accepted state $S_n$, element $E$ has version $V_{\text{current}}$.
   - **Divergence Rule**: If $V_{\text{baseline}} == V_{\text{current}}$, the relationship is **Current**. If $V_{\text{baseline}} \neq V_{\text{current}}$ (or $E$ is `Deprecated`/`Superseded`), the relationship is **Stale**.
3. **Explicit Re-accountability**:
   - Generic element updates (`kat update <artifact-id>`) do **not** implicitly reset accountability.
   - Establishing a new baseline requires an explicit semantic transition: `kat unlink <old-relationship-id>` followed by `kat link <type> <artifact-id> <target-id>`, which creates a new `ChangeRevision` $C_{\text{link\_new}}$ and establishes $V_{\text{baseline\_new}} = V_{\text{current}}$.
4. **First-Class Status Classification**:
   - `CURRENT`: All direct accountability relationships for the active artifact match current upstream active element versions.
   - `STALE`: At least one direct accountability relationship baseline differs from the current upstream version.
   - `UNACCOUNTED`: The active `Artifact` element has zero `represents` or `derived-from` relationships in the accepted state.
5. **Data Structures**:
   ```rust
   pub enum ArtifactAccountabilityStatus {
       Current,
       Stale,
       Unaccounted,
   }

   pub struct ArtifactBaseline {
       pub relationship_id: RelationshipId,
       pub relationship_type: String,
       pub upstream_element_id: ElementId,
       pub upstream_type_id: String,
       pub baseline_version: ObjectId,
       pub current_version: ObjectId,
       pub is_stale: bool,
   }

   pub struct ArtifactAccountability {
       pub artifact_element_id: ElementId,
       pub artifact_type_id: String,
       pub title: Option<String>,
       pub status: ArtifactAccountabilityStatus,
       pub baselines: Vec<ArtifactBaseline>,
   }

   pub struct ArtifactAccountabilityReport {
       pub artifacts: Vec<ArtifactAccountability>,
   }
   ```
6. **CLI & Exit Code**:
   - `kat artifacts`
   - Exit `0`: All active artifacts have status `CURRENT` (or no active artifacts exist).
   - Exit `1`: One or more active artifacts are `STALE` or `UNACCOUNTED`.

---

## 2. Work Breakdown & Implementation Steps

### Step 10.1 — Data Structures & Baseline Resolution Engine
- Define `ArtifactAccountabilityStatus`, `ArtifactBaseline`, `ArtifactAccountability`, and `ArtifactAccountabilityReport` in `src/repository/query.rs`.
- Implement history inspection helper `resolve_relationship_baseline_version(repository: &Repository, relationship_id: RelationshipId, target_element_id: ElementId) -> Result<ObjectId, QueryError>`.

### Step 10.2 — Core `analyze_artifact_accountability` Implementation
- Implement `analyze_artifact_accountability(repository: &Repository) -> Result<ArtifactAccountabilityReport, QueryError>` in `src/repository/query.rs`.
- Scan accepted state $S_n$ for active elements of type `kat.core/artifact`.
- For each artifact, collect outgoing `kat.core/represents` and `kat.core/derived-from` relationships.
- Compare baseline object IDs against current active target object IDs in $S_n$.
- Classify status (`Current`, `Stale`, `Unaccounted`).

### Step 10.3 — Query Layer Re-exports & Unit Tests
- Re-export `analyze_artifact_accountability`, `ArtifactAccountabilityReport`, `ArtifactAccountability`, `ArtifactBaseline`, and `ArtifactAccountabilityStatus` in `src/repository/mod.rs`.
- Add unit tests in `tests/query.rs`:
  - `accountability_no_artifacts_returns_empty_report`
  - `accountability_unaccounted_artifact_status`
  - `accountability_current_artifact_status`
  - `accountability_stale_artifact_when_upstream_element_updated`
  - `accountability_relink_resets_baseline_to_current`
  - `accountability_does_not_mutate_repository`

### Step 10.4 — CLI `kat artifacts` Wiring & Formatting
- Implement `cmd_artifacts()` in `src/main.rs`.
- Register `Some("artifacts") => cmd_artifacts()` in command dispatch.
- Syntax: `kat artifacts`.
- Format diagnostic output listing active artifacts, status (`current`, `stale`, `unaccounted`), baseline vs current object IDs, and summary line.
- Exit `0` if all artifacts are `CURRENT`, exit `1` if any are `STALE` or `UNACCOUNTED`.

### Step 10.5 — Acceptance Verification & Phase 10 Closure
- Add `phase10_acceptance_cli_flow_end_to_end` in `tests/cli.rs`.
- Extend AuthX scenario:
  - Create Requirement R1, Implementation M1, Artifact A1.
  - Link A1 (represents) -> M1.
  - `kat artifacts` $\to$ verify `CURRENT`.
  - Update M1 (`kat update M1 --title "AuthX Core Module v2"`) $\to$ M1 advances to M2.
  - `kat artifacts` $\to$ verify A1 status `STALE` (baseline M1 != current M2).
  - Unlink old relationship & relink A1 -> M1 $\to$ `kat artifacts` verifies status `CURRENT`.
- Update `docs/cli.md`, `docs/implementation-plan.md`, and freeze Phase 10.

---

## 3. Verification & Acceptance Criteria

- `cargo test` passes 100% cleanly (all existing + new Phase 10 tests).
- `cargo clippy --all-targets -- -D warnings` clean.
- `cargo fmt --check` clean.

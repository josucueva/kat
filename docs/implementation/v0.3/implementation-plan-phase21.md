# Phase 21 Implementation Plan: Accountability Inspection & Real-Project Evaluation — `kat artifacts --stale` and v0.3 Acceptance

> Part of the [v0.3 master plan](implementation-plan.md).

## Purpose

Phase 21 delivers **Accountability Inspection Improvements** and the **Real-Project Evaluation** for v0.3. The real-project evaluation (`docs/implementation/v0.3/experiment.md`) showed that artifact accountability worked reliably, but evaluating stale artifacts required manually parsing detailed target version histories.

This phase completes v0.3:

1. **`kat artifacts --stale`** — filters accountability output to display only stale artifacts.
2. **`kat artifacts <artifact-id>`** — displays detailed baseline version vs current target version differences for a specific artifact.
3. **v0.3 Release Acceptance Evaluation** — verifies that all v0.3 discovery, inspection, validation, draft UX, and accountability features meet acceptance criteria and pass workspace tests.

Phase 21 is **strictly read-side** and evaluation-focused.

---

## 1. Frozen Design & Semantics

### 1.1 `kat artifacts --stale` & `kat artifacts <artifact-id>`

- **`kat artifacts --stale`**:
  Displays only artifacts with accountability status `STALE`:

  ```text
  STALE ARTIFACTS (1)

  Artifact: src/app.js - API route definitions
    Id: 012d3257-...
    Status: STALE

    Relationship: kat.core/represents
      Target: Express REST API routes (56adfdb8-...)
      Recorded Baseline: 2b19c80d (V1)
      Current State Version: 9fa71023 (V2) [CHANGED]
  ```

- **`kat artifacts <artifact-id-or-prefix>`**:
  Displays complete accountability details for one specified artifact, including recorded baselines, current target versions, target lifecycles, and change indicators.

---

## 2. Work Breakdown & Implementation Steps

> Each step is atomic: implement, add tests, then **validate** (`cargo test`, `cargo fmt --check`, `cargo clippy -D warnings`) before starting the next step. Commit after validation.

### Step 21.1 — Query layer: `ArtifactFilter` & detailed artifact accountability view

- `src/repository/query.rs`:
  - `ArtifactFilter { stale_only: bool, artifact_id: Option<ElementId> }`.
  - `inspect_artifact_accountability_filtered(&Repository, ArtifactFilter) -> Result<ArtifactAccountabilityReport, QueryError>`.
- Unit tests in `tests/query.rs`.

### Step 21.2 — CLI wiring: `kat artifacts --stale` and `kat artifacts <id>`

- `src/main.rs`:
  - Add `--stale` flag and optional positional `<artifact-id>` argument to `Artifacts` CLI command.
  - Implement detailed accountability renderer.
- Integration tests in `tests/cli.rs`.

### Step 21.3 — Phase 21 Closure & v0.3 Release Acceptance Suite

- Add `phase21_acceptance_cli_flow_end_to_end` test verifying `kat artifacts --stale` and per-artifact detail output.
- Run complete test suite (`cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`).
- Verify all v0.3 capability milestones defined in `requirements.md` and master implementation plan.

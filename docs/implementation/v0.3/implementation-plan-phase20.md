# Phase 20 Implementation Plan: Change UX & Draft Inspection — `kat change status` & Transaction Feedback

> Part of the [v0.3 master plan](implementation-plan.md).

## Purpose

Phase 20 delivers **Change UX & Draft Inspection**. The real-project evaluation (`docs/implementation/v0.3/experiment.md`) showed two related usability issues:

1. Uncertainty about transaction boundaries and staged vs standalone mutation behavior.
2. Incomplete visibility into candidate draft state prior to commit.

This phase improves Change authoring feedback:

1. **Transaction Feedback on Mutation Output** — explicitly reports when an operation is staged into an open draft session versus published standalone.
2. **Enhanced `kat change status`** — displays staged operations, candidate element/relationship deltas, pre-commit validation status, and expected artifact-accountability consequences.

Phase 20 operates over the existing v0.2 multi-operation draft framework without altering canonical persistence formats.

---

## 1. Frozen Design & Semantics

### 1.1 Mutation Command Feedback

- **When an open draft session is active**:
  Mutation output (`kat create`, `kat update`, `kat deprecate`, `kat supersede`, `kat link`, `kat unlink`, `kat account`) explicitly states:

  ```text
  Staged operation 'AccountArtifact' into open draft session
  Draft: 3 staged operations
  Candidate state: 3f91c28a...
  ```

- **When no draft session is active**:
  Mutation output continues to state:

  ```text
  Published standalone Change Revision 43a28b...
  State: S1
  ```

### 1.2 Enhanced `kat change status` Output

`kat change status` renders a complete candidate status summary:

```text
DRAFT CHANGE SESSION

  Description: Introduce JSON persistence
  Base State:  a12b34cd...
  Session ID:  sess-908123...

STAGED OPERATIONS (4)
  1. CreateElement      [kat.core/design-decision] "Persist data to JSON file"
  2. CreateElement      [kat.core/implementation]  "JSON-file store"
  3. LinkKnowledgeElements [kat.core/guides]       "Persist data..." -> "JSON-file..."
  4. AccountArtifact    [kat.core/artifact]        "src/store.js"

CANDIDATE EFFECT
  Elements:      +2 created, 0 updated, 0 deprecated, 0 superseded
  Relationships: +1 created, 0 unlinked

ARTIFACT ACCOUNTABILITY PREVIEW
  Current:  5
  Stale:    1 artifact expected to become stale upon commit
  Reconciled in draft: 1

CANDIDATE VALIDATION
  Status: Valid (0 violations)
```

---

## 2. Work Breakdown & Implementation Steps

> Each step is atomic: implement, add tests, then **validate** (`cargo test`, `cargo fmt --check`, `cargo clippy -D warnings`) before starting the next step. Commit after validation.

### Step 20.1 — Draft Inspection Query: `inspect_draft_session`

- `src/repository/change.rs` & `src/repository/query.rs`:
  - `DraftSessionView`: struct containing base state ID, description, staged operations, candidate state summary, candidate validation status, and accountability delta preview.
  - `inspect_draft_session(&Repository) -> Result<Option<DraftSessionView>, QueryError>`.
- Unit tests in `tests/change.rs`.

### Step 20.2 — CLI wiring: transaction feedback and `kat change status`

- `src/main.rs`:
  - Update CLI mutation response formatter to display staged vs standalone mode clearly.
  - Update `kat change status` renderer to print the comprehensive candidate summary block.
- Integration tests in `tests/cli.rs`.

### Step 20.3 — Phase 20 Closure & End-to-End Acceptance Test

- Add `phase20_acceptance_cli_flow_end_to_end` test verifying mutation transaction feedback and detailed `kat change status` output.
- Verify `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings`.

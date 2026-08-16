# Phase 19 Implementation Plan: Validation Result Classification & Coverage — `kat validate`

> Part of the [v0.3 master plan](implementation-plan.md).

## Purpose

Phase 19 delivers **Validation Result Classification & Coverage**. The real-project evaluation (`docs/implementation/v0.3/experiment.md`) revealed user confusion between mechanical violations, mechanically unverified constraints, and validation evidence coverage (e.g. 5 unverified constraints reported even when validation evidence elements were attached and verified by tests).

This phase makes three distinct concepts explicit in `kat validate`:

1. **Mechanical Violations**: Structural, lifecycle, or ontology rule breaches evaluated by KAT.
2. **Mechanically Unverified Constraints**: Domain `Constraint` elements for which KAT has no executable mechanical rule.
3. **Validation Evidence Coverage**: `kat.core/validation` elements linked to constraints via `validates` relationships.

Phase 19 is **strictly read-side**: no repository mutation, no canonical format change.

---

## 1. Frozen Design & Semantics

### 1.1 Validation Classification Structure

`kat validate` partitions results into three explicit diagnostic sections:

```text
VALIDATION SUMMARY

Mechanical Violations: 0
Mechanically Unverified Constraints: 5
Validation Evidence Coverage: 5 / 5 constraints have linked evidence

CONSTRAINT VERIFICATION DETAIL

CONSTRAINT                                   MECHANICAL RULE   VALIDATION EVIDENCE
CON-1: Priority enum must be low/medium/high none              1 validation (test_suite_passes)
CON-2: Reopen allowed for tasks             none              1 validation (status_transitions_verified)
CON-3: Delete project with tasks rejected   none              1 validation (deletion_protection_verified)
...
```

### 1.2 CLI Flags

- **`kat validate`**: displays validation summary and constraint breakdown.
- **`kat validate --coverage`**: focuses specifically on evidence coverage reporting across requirements, constraints, and implementations.
- **Exit Code Semantics**:
  - Mechanical violations $> 0$ $\to$ exit code 1.
  - Mechanical violations $= 0$ (even if mechanically unverified constraints exist) $\to$ exit code 0.

---

## 2. Work Breakdown & Implementation Steps

> Each step is atomic: implement, add tests, then **validate** (`cargo test`, `cargo fmt --check`, `cargo clippy -D warnings`) before starting the next step. Commit after validation.

### Step 19.1 — Query layer: `validate_repository_classified`

- `src/repository/validation/mod.rs` & `src/repository/query.rs`:
  - `ConstraintCoverage`: struct tracking constraint element, mechanical rule status (`None`), and linked `kat.core/validation` evidence count/elements.
  - `ClassifiedValidationReport`: struct containing `violations: Vec<ValidationViolation>`, `unverified_constraints: Vec<ConstraintCoverage>`, `evidence_coverage_summary`.
  - `validate_classified(&Repository) -> Result<ClassifiedValidationReport, QueryError>`.
- Unit tests in `tests/query.rs`.

### Step 19.2 — CLI wiring: `kat validate` and `kat validate --coverage`

- `src/main.rs`:
  - Wire `--coverage` flag into `Validate` CLI command.
  - Format diagnostic output displaying mechanical violations, unverified constraints, and evidence coverage.
- Integration tests in `tests/cli.rs`.

### Step 19.3 — Phase 19 Closure & End-to-End Acceptance Test

- Add `phase19_acceptance_cli_flow_end_to_end` test verifying validation classification and coverage output.
- Verify `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings`.

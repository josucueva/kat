# Phase 19 Implementation Plan: Validation Result Classification & Coverage — `kat validate`

> Part of the [v0.3 master plan](implementation-plan.md).
>
> Status: **PLANNED** — Pending review and freezing before implementation.

## Purpose

Phase 19 delivers **Validation Result Classification & Coverage** in `kat validate`. Real-project evaluation (`docs/implementation/v0.3/experiment.md` and `docs/implementation/v0.3/experiment-analysis.md`) showed user ambiguity between engine-evaluated mechanical violations, natural-language unverified constraints, and recorded validation evidence.

Phase 19 establishes explicit conceptual classification in `kat validate`:

1. **Mechanical Violations**: Structural, ontology, or state invariant failures evaluated directly by KAT's engine (exit status 1 if $> 0$).
2. **Mechanically Unverified Constraints**: Active natural-language `Constraint` elements for which KAT has no executable code evaluator (exit status 0).
3. **Validation Evidence**: Recorded `kat.core/validation` elements linked to target subjects via `kat.core/validates` relationships ($V \xrightarrow{\text{validates}} S$).
4. **Evidence Coverage**: The proportion of active knowledge elements (`Constraint`, `Requirement`, `Implementation`) backed by at least one linked `Validation` evidence element.

### Core Invariant

$$ \text{evidence-backed} \neq \text{mechanically verified} $$

Attaching validation evidence to a `Constraint` records operational evidence coverage, but does **not** alter KAT's classification that the constraint remains mechanically unverified by KAT's engine.

Phase 19 is **strictly read-side**: no repository mutation, no canonical format change, zero object creation.

---

## 1. Frozen Design & Semantics

### 1.1 Validation Result Classification & Presentation

`kat validate` partitions results into explicit, non-overlapping diagnostic sections:

```text
VALIDATION SUMMARY

Mechanical Violations:                 0
Mechanically Unverified Constraints:   5
Validation Evidence Coverage:          4 / 5 constraints evidence-backed (80.0%)

MECHANICAL VIOLATIONS (0)

None.

MECHANICALLY UNVERIFIED CONSTRAINTS (5)

CONSTRAINT                                            MECHANICAL RULE   VALIDATION EVIDENCE
CON-1: Priority enum must be low/medium/high          Unverified        1 validation (test_priority_enum)
CON-2: Reopen allowed for tasks                      Unverified        1 validation (test_reopen_task)
CON-3: Delete project with tasks rejected            Unverified        0 validations (Uncovered)
...

> Note: Evidence-backed constraints remain mechanically unverified by KAT (no executable rule engine).
```

### 1.2 CLI Flags & Exit Code Semantics

```bash
kat validate [--coverage] [--compact]
```

- **`kat validate`**: Renders validation summary, mechanical violations, and constraint verification details.
- **`kat validate --coverage`**: Focuses specifically on evidence coverage across active knowledge elements (`Constraints`, `Requirements`, `Implementations`), rendering category summaries and listing uncovered elements.
- **`kat validate --compact`**: Emits concise key-value format for CI scripts and automated pipelines.
- **Exit Code Semantics**:
  - Mechanical violations $> 0 \implies$ exit status 1.
  - Mechanical violations $= 0$ (regardless of unverified constraint count or uncovered elements) $\implies$ exit status 0.

### 1.3 Accepted State Read Isolation

- `kat validate` operates exclusively over the accepted `SemanticState` ($S_n$). An open local draft session does not alter `kat validate` output.

---

## 2. Work Breakdown & Implementation Steps

> Each step is atomic: implement, add tests, then **validate** (`cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`) before starting the next step. Commit after validation.

### Step 19.1 — Query Engine Classification DTOs & Validation Processing

- **`src/repository/validation/repository.rs` & `src/repository/query.rs`**:
  - Define `ValidationEvidenceInfo`: `validation_element_id`, `title`.
  - Define `ConstraintVerificationDetail`: `constraint_id`, `title`, `constrained_element_ids`, `is_mechanically_verified` (always `false`), `validation_evidence: Vec<ValidationEvidenceInfo>`.
  - Define `CategoryCoverageSummary`: `category_type`, `total_count`, `evidence_backed_count`, `uncovered_count`.
  - Define `UncoveredElementDetail`: `element_id`, `type_id`, `title`.
  - Update `ValidationReport` to include `constraint_details`, `category_summaries`, and `uncovered_elements`.
  - Update `validate_repository_state` to process `kat.core/validates` relationships and build category evidence coverage.
- Unit tests in `tests/query.rs`.

### Step 19.2 — CLI Wiring & Formatting (`kat validate` and `kat validate --coverage`)

- **`src/cli.rs`**:
  - Add `coverage: bool` flag to `Command::Validate`.
- **`src/main.rs`**:
  - Implement `print_classified_validation_report` (default mode).
  - Implement `print_coverage_report` (`--coverage` mode).
  - Implement compact formatters for both.
- Integration tests in `tests/cli.rs`.

### Step 19.3 — Specification Updates & Acceptance Test Suite

- Update `docs/specification/operations.md` and `docs/vision/architecture.md`.
- Add `phase19_acceptance_cli_flow_end_to_end` test in `tests/cli.rs`.
- Update master plan and walkthrough.

---

## 3. Verification Plan

### Automated Tests
- Run `cargo test` across all workspace tests (396 tests).
- Unit tests in `tests/query.rs`:
  - `validate_repository` classification with 0 violations and 0 evidence elements.
  - `validate_repository` classification with linked `kat.core/validation` elements.
  - Invariant assertion: `is_mechanically_verified == false` for all constraints regardless of evidence.
  - Category coverage calculation for Constraints, Requirements, Implementations.
- Integration tests in `tests/cli.rs`:
  - `kat validate` default output formatting.
  - `kat validate --coverage` category breakdown and uncovered elements listing.
  - `kat validate --compact` and `kat validate --coverage --compact`.
  - Exit code verification (0 for clean repo with unverified constraints, 1 for mechanical violation).
- Code quality checks: `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`.

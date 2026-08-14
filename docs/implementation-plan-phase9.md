# Phase 9 Implementation Plan: `Validate Consistency` (`kat validate`)

Phase 9 implements `UC-006: Validate Consistency`, fulfilling the Consistency Validation requirement for KAT v0.1:
> *Evaluating current repository state against ontology rules, state invariants, and relationship constraints, reporting all semantic violations and unverified constraint knowledge elements without mutating the repository.*

Consistency validation is a **pure read-only query** over the repository's current accepted semantic state (`refs/accepted`). It runs after a successful `open_repository()`, collects all mechanically decidable semantic violations across the entire repository, and reports active natural-language `Constraint` elements with their target `restricts` subjects as unverified rules.

---

## 1. Frozen Design & Query Semantics

1. **Read-Only Operation & Non-Mutation**:
   - `kat validate` inspects the current accepted `SemanticState` ($S_n$) and active `OntologyVersion` ($O_n$).
   - Does **not** mutate repository files, write `ObjectStore` objects, or touch `refs/accepted`.
2. **Two-Pass Diagnostic Model**:
   - **Repository Open Pass**: `open_repository()` verifies canonical format, object hashing, and referential state integrity. If `open_repository()` fails, `kat validate` exits with code `1` and prints the repository error.
   - **Semantic Validation Scan**: Evaluates the accepted state against ontology rules and persistent invariants, accumulating all violations rather than stopping on first error.
3. **Validation Report Structure**:
   ```rust
   pub enum ValidationViolationKind {
       UnknownRelationshipType,
       RelationshipSourceTypeNotAllowed,
       RelationshipTargetTypeNotAllowed,
       DuplicateRelationshipTriple,
       MissingEndpointElement,
   }

   pub struct ValidationViolation {
       pub kind: ValidationViolationKind,
       pub relationship_id: Option<RelationshipId>,
       pub affected_element_ids: Vec<ElementId>,
       pub message: String,
   }

   pub struct UnverifiedConstraint {
       pub constraint_element_id: ElementId,
       pub constrained_element_ids: Vec<ElementId>,
   }

   pub struct ValidationReport {
       pub violations: Vec<ValidationViolation>,
       pub unverified_constraints: Vec<UnverifiedConstraint>,
   }
   ```
4. **Mechanically Decidable Violations**:
   - **Relationship Type Existence**: `relationship_type` exists in active `OntologyVersion`.
   - **Allowed Source Type**: Current element `type_id` of source element is allowed by relationship definition.
   - **Allowed Target Type**: Current element `type_id` of target element is allowed by relationship definition.
   - **Triple Uniqueness**: `(relationship_type, source_element_id, target_element_id)` is unique in accepted state.
   - **Endpoint Existence**: Both `source_element_id` and `target_element_id` exist in $S_n$.
   - *Note*: `source_element_id` lifecycle is **not** required to be `Active` for existing relationships (historical evolution allows active relationships to persist when source elements become deprecated/superseded).
5. **Unverified Constraint Reporting**:
   - Active elements of type `kat.core/constraint`.
   - Populates `constrained_element_ids` by inspecting outgoing relationships of type `kat.core/restricts`.
   - Informational status only (does not trigger exit code `1`).
6. **Affected Knowledge Boundary**:
   - Violation report includes directly participating element and relationship IDs only. Does not invoke `analyze_impact()` automatically.
7. **CLI & Exit Code**:
   - `kat validate`
   - Exit `0`: `violations.is_empty()` (no mechanical violations).
   - Exit `1`: `!violations.is_empty()` or repository open failure.

---

## 2. Work Breakdown & Implementation Steps

### Step 9.1 — Validation Engine Data Structures & API
- Define `ValidationViolationKind`, `ValidationViolation`, `UnverifiedConstraint`, and `ValidationReport` in `src/repository/validation/mod.rs` (or `src/repository/query.rs`).
- Implement `validate_repository(repository: &Repository) -> Result<ValidationReport, QueryError>` in `src/repository/query.rs`.

### Step 9.2 — Core `validate_repository` Scan Implementation
- Implement mechanical validation checks over accepted state $S_n$:
  - Verify every accepted relationship type against active ontology definitions.
  - Verify source and target element existence and type compatibility.
  - Verify uniqueness of semantic triples `(type, source, target)`.
  - Accumulate all `ValidationViolation` entries.
- Implement `Constraint` scan:
  - Find all active elements of type `kat.core/constraint`.
  - Collect `target_element_id` of outgoing `kat.core/restricts` relationships.
  - Accumulate `UnverifiedConstraint` entries.

### Step 9.3 — Query Layer Re-exports & Unit Tests
- Re-export `validate_repository`, `ValidationReport`, `ValidationViolation`, `ValidationViolationKind`, `UnverifiedConstraint` in `src/repository/mod.rs`.
- Add unit tests in `tests/query.rs`:
  - `validate_clean_repository_returns_no_violations`
  - `validate_reports_invalid_relationship_type`
  - `validate_reports_disallowed_source_and_target_types`
  - `validate_reports_duplicate_relationship_triples`
  - `validate_reports_missing_endpoints`
  - `validate_reports_unverified_constraints_with_restricts_targets`
  - `validate_permits_deprecated_source_on_existing_relationship`
  - `validate_does_not_mutate_repository`

### Step 9.4 — CLI `kat validate` Wiring & Formatting
- Implement `cmd_validate()` in `src/main.rs`.
- Syntax: `kat validate`.
- Format output:
  - If violations exist: render `violations:` section.
  - If unverified constraints exist: render `unverified_constraints:` section with title and target element list.
  - Summary: `semantic consistency: no violations detected` or `semantic consistency: N violations detected`.
  - Exit `0` on clean (or unverified constraints only), exit `1` on violations.

### Step 9.5 — Requirements Documentation & Acceptance Verification
- Add clarification note to `docs/requirements.md` / `docs/prototype-design.md` documenting v0.1 constraint validation semantics.
- Add `phase9_acceptance_cli_flow_end_to_end` in `tests/cli.rs`.
- Update `docs/cli.md`, `docs/implementation-plan.md`, and freeze Phase 9.

---

## 3. Verification & Acceptance Criteria

- `cargo test` passes 100% cleanly (all existing + new Phase 9 tests).
- `cargo clippy --all-targets -- -D warnings` clean.
- `cargo fmt --check` clean.

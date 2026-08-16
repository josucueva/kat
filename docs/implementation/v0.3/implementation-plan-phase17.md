# Phase 17 Implementation Plan: Ontology Discovery — `kat ontology`, `kat ontology show`

> Part of the [v0.3 master plan](implementation-plan.md).

## Purpose

Phase 17 delivers the **Ontology Discovery** capability of v0.3.0. The real-project evaluation (`docs/implementation/v0.3/experiment.md`) demonstrated that requiring users or AI agents to discover relationship type names, allowed source element types, and allowed target element types by trial-and-error or binary inspection is unacceptable.

This phase introduces two CLI discovery commands operating over the active `OntologyVersion` referenced by the accepted repository state $S_n$:

1. **`kat ontology`** — lists registered element types, relationship types, and allowed endpoint combinations in a clean diagnostic overview.
2. **`kat ontology show <type-id>`** — displays detailed properties, endpoint admissibility, and incoming/outgoing relationship capabilities for a specific element or relationship type.

Phase 17 is **strictly read-side**: no repository mutation, no canonical format change, no new persistence objects. It makes the active semantic vocabulary fully discoverable.

---

## 1. Frozen Design & Semantics

### 1.1 `kat ontology` — summary view

- **Read-only**: inspects the active `OntologyVersion` referenced by accepted state $S_n$.
- **Interface** (clap subcommand `Ontology`):

  ```bash
  kat ontology
  ```

- **Output Structure**:
  - **Element Types**: list of registered element type IDs (e.g. `kat.core/requirement`, `kat.core/design-decision`).
  - **Relationship Types**: table listing relationship type IDs, allowed source element types, and allowed target element types.

- **Example Output**:

  ```text
  ELEMENT TYPES
    kat.core/intent
    kat.core/requirement
    kat.core/constraint
    kat.core/design-decision
    kat.core/implementation
    kat.core/artifact
    kat.core/validation

  RELATIONSHIP TYPES
    TYPE                  SOURCE                TARGETS
    kat.core/motivates    intent                requirement, design-decision
    kat.core/addresses    design-decision       requirement
    kat.core/restricts    constraint            requirement, design-decision, implementation
    kat.core/guides       design-decision       implementation
    kat.core/realizes     implementation        requirement
    kat.core/represents   artifact              implementation
    kat.core/derived-from artifact              requirement, constraint, design-decision, implementation
    kat.core/validates    validation            requirement, constraint, implementation
    kat.core/depends-on   implementation        implementation
    kat.core/supersedes   design-decision       design-decision
  ```

### 1.2 `kat ontology show <type-id>` — detailed type view

- **Interface** (clap subcommand `Ontology` with positional argument):

  ```bash
  kat ontology show requirement
  kat ontology show kat.core/realizes
  ```

- **Short Name Resolution**: accepts canonical IDs (e.g. `kat.core/requirement`) or short names (e.g. `requirement`, `realizes`).
- **Detail View for Element Type** (e.g. `kat.core/implementation`):

  ```text
  kat.core/implementation

  Kind:
    element

  Outgoing relationships:
    realizes   -> requirement
    depends-on -> implementation

  Incoming relationships:
    restricts  <- constraint
    guides     <- design-decision
    represents <- artifact
    derived-from <- artifact
    validates  <- validation
  ```

- **Detail View for Relationship Type** (e.g. `kat.core/realizes`):

  ```text
  kat.core/realizes

  Kind:
    relationship

  Source:
    kat.core/implementation

  Targets:
    kat.core/requirement
  ```

- **Unknown Type Handling**: if `<type-id>` is not present in the active ontology, prints a clear diagnostic error message and exits with status 1.

---

## 2. Work Breakdown & Implementation Steps

> Each step is atomic: implement, add tests, then **validate** (`cargo test`, `cargo fmt --check`, `cargo clippy -D warnings`) before starting the next step. Commit after validation.

### Step 17.1 — Query layer: `ontology` inspection functions

- `src/repository/query.rs`:
  - `OntologySummary`: data structure containing active `OntologyVersion` ID, registered element types, and relationship definitions with resolved endpoint names.
  - `OntologyTypeView`: enum covering `ElementTypeView` (with incoming/outgoing allowed relationships) and `RelationshipTypeView` (with source/targets).
  - `inspect_ontology(&Repository) -> Result<OntologySummary, QueryError>`
  - `show_ontology_type(&Repository, type_id: &str) -> Result<OntologyTypeView, QueryError>`
- Unit & integration tests in `tests/query.rs`.

### Step 17.2 — CLI wiring: `kat ontology` and `kat ontology show`

- `src/main.rs`:
  - Add `Ontology` subcommand to `clap` CLI parser with optional `show` subcommand / positional type argument.
  - Format diagnostic rendering for `kat ontology` (default and `--compact` modes).
  - Format detailed rendering for `kat ontology show <type-id>`.
- Integration tests in `tests/cli.rs`.

### Step 17.3 — Phase 17 Closure & End-to-End Acceptance Test

- Add `phase17_acceptance_cli_flow_end_to_end` test verifying `kat ontology` and `kat ontology show` on a populated repository.
- Verify `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings`.

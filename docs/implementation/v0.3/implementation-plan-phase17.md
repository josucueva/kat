# Phase 17 Implementation Plan: Ontology Discovery — `kat ontology`, `kat ontology show`

> Part of the [v0.3 master plan](implementation-plan.md).
>
> Status: **COMPLETED** — All steps (17.1, 17.2, 17.3) implemented, tested, and documented.

## Purpose

Phase 17 delivers the **Ontology Discovery** capability of v0.3.0. The real-project evaluation (`docs/implementation/v0.3/experiment.md`) demonstrated that requiring users or AI agents to discover relationship type names, allowed source element types, and allowed target element types by trial-and-error or binary inspection is unacceptable.

Phase 17 introduces two CLI discovery commands operating over the repository's active `OntologyVersion`:

1. **`kat ontology [--compact]`** — lists registered element types, relationship types, and allowed endpoint combinations in a clean diagnostic overview.
2. **`kat ontology show <type-id> [--compact]`** — displays detailed properties, endpoint admissibility, and incoming/outgoing relationship capabilities for a specific element or relationship type.

Phase 17 is **strictly read-side**: no repository mutation, no canonical format change, no new persistence objects. It makes the active semantic vocabulary fully discoverable.

---

## 1. Frozen Design & Semantics

### 1.1 Active Ontology Access & Identity

- **Repository Context**: Inspects the active `OntologyVersion` associated with the current repository state context (not embedded inside `SemanticState`).
- **Draft Session Independence**: Ontology discovery reads the repository's active ontology as repository context and is independent of candidate draft semantic state ($S_{\text{working}}$). An open draft session neither alters ontology discovery results nor blocks the command.
- **Identity Distinction**: Exposes both stable semantic identity (`OntologyId`, UUID) and immutable version identity (`ObjectId`, SHA-256 hash) in the default summary view.

### 1.2 Command Grammar & Output Modes

- **Grammar**:
  ```bash
  kat ontology [--compact]
  kat ontology show <type-id> [--compact]
  ```

- **CLI Parser Single Ownership Model**:
  The `--compact` flag is owned by the top `Ontology` CLI command structure and applies globally across its subcommands via `global = true`:

  ```rust
  Ontology {
      #[arg(short, long, global = true)]
      compact: bool,
      #[command(subcommand)]
      command: Option<OntologyCommand>,
  }

  OntologyCommand::Show {
      type_id: String,
  }
  ```

- **Output Modes**:
  - **Default**: Displays full canonical type IDs (`kat.core/...`) and human-readable `name` properties across all summary tables and detail views.
  - **Compact (`--compact`)**: Displays shortened type IDs where unambiguous across the active ontology, omitting human-readable names.

### 1.3 `kat ontology` — Summary View

- **Default Summary Output**:

  ```text
  ONTOLOGY
    id:       37c891f2-70b9-4a92-9118-2e86bf12019a
    version:  91fa2c19a8b276d49811029c...

  ELEMENT TYPES (7)
    TYPE                      NAME
    kat.core/intent           Intent
    kat.core/requirement      Requirement
    kat.core/constraint       Constraint
    kat.core/design-decision  Design Decision
    kat.core/implementation   Implementation
    kat.core/artifact          Artifact
    kat.core/validation        Validation

  RELATIONSHIP TYPES (10)
    TYPE                  NAME        SOURCES                  TARGETS
    kat.core/motivates    Motivates   kat.core/intent          kat.core/requirement, kat.core/design-decision
    kat.core/addresses    Addresses   kat.core/design-decision kat.core/requirement
    kat.core/restricts    Restricts   kat.core/constraint      kat.core/requirement, kat.core/design-decision, kat.core/implementation
    kat.core/guides       Guides      kat.core/design-decision kat.core/implementation
    kat.core/realizes     Realizes    kat.core/implementation kat.core/requirement
    kat.core/represents   Represents  kat.core/artifact        kat.core/implementation
    kat.core/derived-from Derived From kat.core/artifact        kat.core/requirement, kat.core/constraint, kat.core/design-decision, kat.core/implementation
    kat.core/validates    Validates   kat.core/validation      kat.core/requirement, kat.core/constraint, kat.core/implementation
    kat.core/depends-on   Depends On  kat.core/implementation kat.core/implementation
    kat.core/supersedes   Supersedes  kat.core/design-decision kat.core/design-decision
  ```

- **Compact Summary Output (`--compact`)**:

  ```text
  ELEMENT TYPES
    intent
    requirement
    constraint
    design-decision
    implementation
    artifact
    validation

  RELATIONSHIP TYPES
    TYPE          SOURCES         TARGETS
    motivates     intent          requirement, design-decision
    addresses     design-decision requirement
    restricts     constraint      requirement, design-decision, implementation
    guides        design-decision implementation
    realizes      implementation  requirement
    represents    artifact        implementation
    derived-from  artifact        requirement, constraint, design-decision, implementation
    validates     validation      requirement, constraint, implementation
    depends-on    implementation  implementation
    supersedes    design-decision design-decision
  ```

### 1.4 `kat ontology show <type-id>` — Detailed Type View

- **Resolution Rules**:
  1. Exact canonical `type_id` match wins (e.g. `kat.core/requirement`).
  2. Otherwise, input is evaluated as a short identifier (the final path segment after `/` in `type_id`, e.g. `requirement`).
  3. If exactly one registered type matches the short identifier $\to$ resolve.
  4. If 0 registered types match $\to$ `QueryError::UnknownOntologyType(query)`.
  5. If $>1$ registered types match $\to$ `QueryError::AmbiguousOntologyType { query, matches }` listing canonical candidates.

- **Note**: Human-readable `name` properties (e.g. `"Design Decision"`) are presentation metadata and do not participate in short-name resolution.

- **Default Element Type Detail** (e.g. `kat ontology show implementation`):

  ```text
  kat.core/implementation

  Kind:
    element

  Name:
    Implementation

  Outgoing relationships:
    kat.core/realizes   -> kat.core/requirement
    kat.core/depends-on -> kat.core/implementation

  Incoming relationships:
    kat.core/restricts    <- kat.core/constraint
    kat.core/guides       <- kat.core/design-decision
    kat.core/represents   <- kat.core/artifact
    kat.core/derived-from <- kat.core/artifact
    kat.core/validates    <- kat.core/validation
  ```

- **Compact Element Type Detail (`--compact`)**:

  ```text
  implementation
  kind: element

  outgoing:
    realizes -> requirement
    depends-on -> implementation

  incoming:
    restricts <- constraint
    guides <- design-decision
    represents <- artifact
    derived-from <- artifact
    validates <- validation
  ```

- **Default Relationship Type Detail** (e.g. `kat ontology show realizes`):

  ```text
  kat.core/realizes

  Kind:
    relationship

  Name:
    Realizes

  Sources:
    kat.core/implementation

  Targets:
    kat.core/requirement
  ```

- **Compact Relationship Type Detail (`--compact`)**:

  ```text
  realizes
  kind: relationship

  sources:
    implementation

  targets:
    requirement
  ```

- **Capabilities Derivation**:
  For element type $T$:
  - **Outgoing**: Every relationship type $R$ where $T \in R.\text{allowed\_source\_types}$, paired with each target type in $R.\text{allowed\_target\_types}$.
  - **Incoming**: Every relationship type $R$ where $T \in R.\text{allowed\_target\_types}$, paired with each source type in $R.\text{allowed\_source\_types}$.

### 1.5 Deterministic Ordering Rules

- Element types: sorted alphabetically by canonical `type_id`.
- Relationship types: sorted alphabetically by canonical `type_id`.
- Sources / targets collections: sorted alphabetically by canonical `type_id`.
- Incoming / outgoing capabilities: sorted by relationship `type_id`, then counterpart `type_id`.

---

## 2. Work Breakdown & Implementation Steps

> Each step is atomic: implement, add tests, then **validate** (`cargo test`, `cargo fmt --check`, `cargo clippy -D warnings`) before starting the next step. Commit after validation.

### Step 17.1 — Query projection DTOs & Resolution Logic

- `src/repository/query.rs`:
  - Data structures:
    - `OntologySummary { ontology_id: OntologyId, ontology_version_id: ObjectId, element_types: Vec<ElementTypeSummary>, relationship_types: Vec<RelationshipTypeSummary> }`
    - `ElementTypeSummary { type_id: TypeId, name: String }`
    - `RelationshipTypeSummary { type_id: TypeId, name: String, allowed_source_types: Vec<TypeId>, allowed_target_types: Vec<TypeId> }`
    - `enum OntologyTypeView { Element(ElementTypeView), Relationship(RelationshipTypeView) }`
    - `ElementTypeView { type_id: TypeId, name: String, outgoing: Vec<RelationshipCapability>, incoming: Vec<RelationshipCapability> }`
    - `RelationshipTypeView { type_id: TypeId, name: String, allowed_source_types: Vec<TypeId>, allowed_target_types: Vec<TypeId> }`
    - `RelationshipCapability { relationship_type_id: TypeId, counterpart_type_id: TypeId }`
  - Error variants added to `QueryError`:
    - `UnknownOntologyType(String)`
    - `AmbiguousOntologyType { query: String, matches: Vec<String> }`
  - Functions:
    - `inspect_ontology(&Repository) -> Result<OntologySummary, QueryError>`
    - `show_ontology_type(&Repository, query: &str) -> Result<OntologyTypeView, QueryError>`
  - Uses `repository.active_ontology()` resolution path.
- Unit tests in `tests/query.rs` testing built-in core ontology, custom extension ontologies (with multiple source/target types and custom namespaces), canonical lookup, short lookup, ambiguity handling, and deterministic ordering.

### Step 17.2 — CLI Wiring & Output Formatting

- `src/main.rs`:
  - Wire `Ontology` clap subcommand with `#[arg(short, long, global = true)] compact: bool` flag and `command: Option<OntologyCommand>` sub-subcommand (`Show { type_id: String }`).
  - Implement summary renderer for default (with `NAME` column) and `--compact` modes.
  - Implement detailed renderer for default (with full canonical IDs) and `--compact` (with short IDs) modes.
- Integration tests in `tests/cli.rs`.

### Step 17.3 — Phase 17 Closure, Documentation Update & Acceptance Test Suite

- Add `phase17_acceptance_cli_flow_end_to_end` test verifying:
  - `kat ontology`
  - `kat ontology --compact`
  - `kat ontology show requirement`
  - `kat ontology show requirement --compact`
  - `kat ontology show kat.core/realizes`
  - `kat ontology show does-not-exist` (returns exit status 1 with clear unknown type error)
  - Extension ontology fixture test with ambiguous short name (returns exit status 1 listing candidate matches).
  - Draft-isolation and read-only invariant: before and after every query, verifies:
    - `accepted.state` is unchanged
    - `accepted.change` is unchanged
    - ObjectStore contents are unchanged
    - Local draft session state is unchanged (if present)
- Update `docs/specification/operations.md` and `docs/vision/architecture.md` to document the new `InspectOntology` query semantics.
- Verify `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings`.

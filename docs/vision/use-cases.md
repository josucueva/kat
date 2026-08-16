# Use Cases

## Purpose

This document defines the core user goals and primary workflows supported by KAT. 

Use cases describe durable user interactions with the semantic repository. Release-specific capability coverage for these use cases is defined in [`docs/vision/requirements.md`](requirements.md).

---

## UC-001: Create Knowledge Element

**Actor:** Developer / Architect

**Goal:** Create a new knowledge element in the software model.

**Preconditions:**
* The KAT repository exists.
* The element type is defined by the active `OntologyVersion`.

**Main flow:**
1. The user defines a new knowledge element (specifying type, title, and optional description/properties).
2. KAT validates the element type and required property structure against the active ontology.
3. KAT assigns a stable 36-character `ElementId` (UUIDv4) to the element.
4. KAT applies the `CreateElement` operation through the Change Engine, either as part of an open draft Change or as a standalone Change.
5. When accepted, the new element becomes part of authoritative `SemanticState` $S_{n+1}$.

**Result:**
The new element is recorded with stable identity and can participate in relationships, validation, evolution, and traceability.

---

## UC-002: Create Design Decision

**Actor:** Developer / Architect

**Goal:** Record a decision about how the software should be designed and preserve the rationale behind it.

**Preconditions:**
* The KAT repository exists.
* Relevant requirements or constraints may already exist in accepted state.

**Main flow:**
1. The user defines a new `kat.core/design-decision` element.
2. The user describes the chosen approach and decision rationale in the element properties.
3. The user establishes ontology-valid relationships involving the decision (e.g. `addresses` a requirement, or is `restricted` by a constraint).
4. KAT applies the element creation and relationship operations through the Change Engine, either as part of an open draft Change or as a standalone Change.
5. Upon acceptance, the decision becomes authoritative within accepted state $S_{n+1}$.

**Result:**
The repository contains a traceable design decision explaining what was decided, why, and how it connects to requirements and constraints.

**Example:**
```text
Constraint: "Payment processing must not block checkout"
    -- restricts -->
Design Decision: "Use an event-driven payment workflow"
    -- addresses -->
Requirement: "Payments must be processed asynchronously"
```

---

## UC-003: Link Knowledge Elements

**Actor:** Developer / Architect

**Goal:** Establish a meaningful typed relationship between two knowledge elements.

**Preconditions:**
* Source and target knowledge elements exist in candidate or accepted state.
* The relationship type is defined by the active `OntologyVersion`.

**Main flow:**
1. The user selects the source and target elements and specifies the relationship type.
2. KAT verifies that the relationship type is valid for the given source and target element types under the active ontology.
3. KAT assigns a stable `RelationshipId` (UUIDv4).
4. KAT applies the `Link` operation through the Change Engine, either as part of an open draft Change or as a standalone Change.
5. When accepted, the relationship becomes available for tracing, impact analysis, and validation.

**Canonical Relationship Types:**
```text
kat.core/motivates
kat.core/addresses
kat.core/restricts
kat.core/guides
kat.core/realizes
kat.core/represents
kat.core/derived-from
kat.core/validates
kat.core/depends-on
kat.core/supersedes
```

**Example:**
```text
Design Decision  -- addresses --> Requirement
Design Decision  -- guides    --> Implementation
```

---

## UC-004: Trace Origin

**Actor:** Developer / Architect

**Goal:** Determine why a software element exists and trace it back to its originating intent or specification.

**Preconditions:**
* The targeted element exists in the current accepted `SemanticState` $S_n$.

**Main flow:**
1. The user requests an origin trace (`kat trace`) for a specific element.
2. KAT traverses applicable relationships according to the Origin Trace policy defined in [`docs/specification/operations.md`](../specification/operations.md).
3. KAT presents the provenance path connecting the element to upstream decisions, requirements, constraints, and intent.

**Example:**
User queries origin for `payment_service.rs` (`Artifact`):

```text
payment_service.rs (Artifact)
    -- represents --> Payment Processing (Implementation)
    <-- guides     -- Event-Driven Payment Architecture (Design Decision)
    -- addresses   --> Asynchronous Payment Processing (Requirement)
    <-- motivates  -- Immediate Checkout Confirmation (Intent)
```

**Result:**
The user understands the origin, design context, and business motivation behind the element.

---

## UC-005: Analyze Impact

**Actor:** Developer / Architect

**Goal:** Determine which parts of the software knowledge model and accountable artifacts may be affected by semantic evolution.

**Preconditions:**
* The root element exists in accepted state $S_n$.

**Main flow:**
1. The user initiates impact analysis (`kat impact`) for a target element.
2. KAT propagates impact through applicable relationships according to the Impact policy defined in [`docs/specification/operations.md`](../specification/operations.md).
3. KAT partitions the result into three semantic impact categories:
   * **Directly Changed Elements**: The root target element itself.
   * **Semantically Affected Elements**: Downstream requirements, decisions, or implementations dependent on the target.
   * **Accountable Artifacts**: `kat.core/artifact` elements that represent or derive from affected knowledge.
4. KAT presents the partitioned impact report for review.

**Result:**
The user receives an explicit impact report outlining which semantic elements and accountable artifacts require review.

---

## UC-006: Validate Consistency

**Actor:** Developer / Architect / CI Pipeline

**Goal:** Evaluate whether a semantic state satisfies all mechanically enforced structural, domain, and ontology rules.

**Preconditions:**
* A KAT repository exists.

**Main flow:**
1. KAT inspects the semantic state: evaluating either accepted state $S_n$ during explicit validation queries (`kat validate`), or candidate state $S_{\text{working}}$ during Change evaluation and commit.
2. KAT evaluates structural integrity, ontology type rules, valid endpoint combinations, and lifecycle consistency.
3. KAT identifies any mechanical violations (e.g. relationship referencing a missing target version, or invalid relationship endpoint types).
4. KAT identifies unverified semantic `Constraint` elements for explicit awareness.
5. KAT reports validation results cleanly without mutating state.

**Example Violation Report:**
```text
Validation Failure: Disallowed Relationship Endpoints

Relationship: rel-01010101
Type: kat.core/guides
Source: req-02020202 (Requirement)
Target: impl-03030303 (Implementation)

Violation: Relationship 'kat.core/guides' allows source type 'kat.core/design-decision', got 'kat.core/requirement'.
```

**Result:**
The user receives deterministic feedback on structural/ontology compliance and unverified domain constraints.

---

## UC-007: Review and Reconcile Artifact Accountability

**Actor:** Developer / Architect

**Goal:** Determine whether an `Artifact` element remains accountable to current versions of the knowledge it represents or derives from, and explicitly reconcile its baseline when appropriate.

**Preconditions:**
* The KAT repository exists.
* The `Artifact` element exists in the current accepted state $S_n$.

**Main flow:**
1. The user reviews artifact accountability (`kat artifacts`).
2. KAT resolves direct accountability edges and their recorded baselines against current target element versions in $S_n$.
3. KAT reports status:
   * `CURRENT`: Baseline matches current target version.
   * `STALE`: Baseline differs from current target version, or target lifecycle is invalid.
   * `UNACCOUNTED`: No direct accountability edge exists.
4. If `STALE`, the user updates physical source files or documentation externally as needed.
5. The user executes `kat account` to stage an `AccountArtifact` reconciliation operation.
6. When accepted, KAT records the updated target version baseline in accepted `ChangeRevision` history.

**Result:**
If a stale artifact is successfully reconciled, its accountability status returns to `CURRENT`. Accountability history remains explicit and traceable without mutating `SemanticState`.

---

## UC-008: Author a Multi-Operation Change

**Actor:** Developer / Architect

**Goal:** Express one meaningful software evolution using multiple ordered semantic operations and publish them atomically.

**Preconditions:**
* The KAT repository exists in accepted state $S_n$.
* No uncommitted draft session is currently active.

**Main flow:**
1. The user opens a local draft session with `kat change begin`.
2. The user stages one or more mutation operations (`CreateElement`, `UpdateElement`, `Link`, `AccountArtifact`, etc.).
3. KAT evaluates each operation sequentially against working candidate state $S_{\text{working}}$.
4. The user inspects candidate draft status with `kat change status`.
5. KAT validates candidate state $S_{\text{working}}$ against ontology and structural rules.
6. The user commits the draft (`kat change commit`). If accepted base state $S_n$ remains unchanged, KAT atomically publishes the new accepted repository state $(S_{n+1}, C_{n+1})$, where $C_{n+1}$ is the single `ChangeRevision` representing the Change.
7. If base state $S_n$ changed concurrently, KAT rejects commit with a stale-base conflict error and preserves $S_n$.

**Result:**
One multi-operation software evolution is committed atomically or rejected cleanly without partial state corruption.

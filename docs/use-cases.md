# Use Cases

## UC-001: Create Knowledge Element

**Actor:** Developer / Architect

**Goal:** Create a new element in the software's knowledge model.

**Preconditions:**

* The project exists in KAT.
* The element type is defined by the project's domain model.

**Main flow:**

1. The user creates a knowledge element.
2. The user provides the required information.
3. KAT assigns a stable identity to the element.
4. KAT stores the element in the semantic model.

**Result:**

The new element becomes part of the project's knowledge and can participate in relationships, validation, evolution, and traceability.

---

## UC-002: Create Design Decision

**Actor:** Developer / Architect

**Goal:** Record a decision about how the software should be designed and preserve the reasoning behind it.

**Preconditions:**

* The project exists in KAT.
* Relevant requirements or constraints may already exist.

**Main flow:**

1. The user creates a design decision.
2. The user describes the chosen approach.
3. The user records the reasoning behind the decision.
4. The user links the decision to relevant requirements, constraints, or other decisions.
5. KAT assigns a stable identity to the decision.
6. The decision becomes part of the project's knowledge.

**Result:**

The project contains a traceable design decision that explains what was decided and why.

**Example:**

```text
Requirement
"Payments must be processed asynchronously"

Constraint
"Payment processing must not block checkout"

Design Decision
"Use an event-driven payment workflow"
```

---

## UC-003: Link Knowledge Elements

**Actor:** Developer / Architect

**Goal:** Establish a meaningful relationship between two or more elements of the software's knowledge.

**Preconditions:**

* The elements exist in the semantic model.
* The relationship type is defined.

**Main flow:**

1. The user selects the knowledge elements.
2. The user specifies the relationship between them.
3. KAT verifies that the relationship is allowed.
4. KAT records the relationship.
5. The relationship becomes available for tracing and validation.

**Example:**

```text
Requirement
    |
    | addressed_by
    v
Design Decision
    |
    | guides
    v
Implementation
```

Possible relationship types include:

```text
motivates
addresses
realizes
guides
depends_on
restricts
validates
supersedes
derived_from
```

---

## UC-004: Trace Origin

**Actor:** Developer / Architect

**Goal:** Determine why a software element exists and trace it back to its originating knowledge.

**Preconditions:**

* The selected element exists.
* Traceability relationships exist between the element and its origin.

**Main flow:**

1. The user selects an element.
2. KAT follows its traceability relationships backwards.
3. KAT presents the relevant chain of knowledge.
4. The user can continue following relationships to understand the origin.

**Example:**

The user asks:

```text
Why does PaymentService exist?
```

KAT could return:

```text
PaymentService
    ↑
Payment Implementation
    ↑
"Use asynchronous payment processing"
    ↑
"Checkout must not block on payment"
    ↑
"Users should receive immediate checkout confirmation"
```

**Result:**

The user can understand the purpose and origin of the selected element.

---

## UC-005: Analyze Impact

**Actor:** Developer / Architect

**Goal:** Determine which parts of the software may be affected by changing a knowledge element.

**Preconditions:**

* The selected element exists.
* Relevant traceability relationships exist.

**Main flow:**

1. The user selects an element to change.
2. KAT follows relevant outgoing traceability relationships.
3. KAT identifies directly and indirectly related elements.
4. KAT groups the affected elements by type or relationship.
5. KAT reports the potential impact.

**Example:**

The developer changes:

```text
Requirement
"All payments must support refunds."
```

KAT might identify:

```text
Affected:

Design Decisions
    Refund workflow

Interfaces
    POST /refunds

Implementations
    RefundService
    PaymentProviderAdapter

Artifacts
    OpenAPI specification

Validation
    RefundIntegrationTest
```

**Result:**

The user receives a traceable set of elements that may require review or modification.

KAT reports potential impact. It does not assume that every affected element will become invalid or require modification.

---

## UC-006: Validate Consistency

**Actor:** Developer / Architect / KAT

**Goal:** Determine whether the current semantic model satisfies its defined consistency rules.

**Preconditions:**

* The project has defined consistency rules.
* The semantic model exists.

**Main flow:**

1. KAT examines the current semantic model.
2. KAT evaluates the defined consistency rules.
3. KAT identifies violations.
4. KAT reports each violated rule and its affected elements.
5. The user can inspect the relevant traceability relationships.

**Example:**

The project defines:

```text
Every accepted Requirement must have
at least one Implementation.
```

The model contains:

```text
Requirement
"Support Apple Pay"

Status
Accepted

Implementation
None
```

KAT reports:

```text
Consistency violation

Requirement: Support Apple Pay

Rule:
Accepted requirements must have an implementation.

Missing:
Implementation
```

**Result:**

The user knows which consistency rules are violated and which knowledge elements are involved in the violations.

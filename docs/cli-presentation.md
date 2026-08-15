# KAT CLI Presentation Standard

This document defines the unified presentation language and rendering conventions for all KAT read-side CLI commands (`status`, `show`, `history`, `trace`, `impact`, `validate`, `artifacts`).

The goal is to ensure KAT's CLI output feels like a deliberate, cohesive software tool rather than an ad-hoc debug printer, preserving semantic precision while optimizing for human scannability.

---

## 1. Boundary: Presentation vs. Semantics

To prevent visual cleanups from accidentally altering command meaning, KAT strictly separates **Presentation Rules** from **Semantic Output Requirements**:

### Presentation (Visual Layer)
- 12-character `ObjectId` display abbreviation
- Title Case section headings
- Standardized indentation (2 spaces for fields, 4 spaces for nested lists/sub-items)
- Human-readable space-separated operation labels (`create element`, `link`)
- Consistent empty-state wording (`none`, `0`)

### Semantics (Domain Layer)
- Which fields are queried and displayed
- Which sections exist
- What counts, status determinations, and relationships are reported
- Full canonical UUIDs and ObjectIds used internally for hash verification and state transitions

> [!IMPORTANT]
> **Display Abbreviation Only**: Abbreviating `ObjectId`s to 12 hex characters applies strictly to **CLI output display**. Command arguments, CLI parsers, object stores, and change engine APIs continue to require and accept full 64-character canonical `ObjectId`s and full 36-character UUIDs.

---

## 2. Shared Formatting Rules

### Identity Formatting
- **Content-Addressed `ObjectId`s**: Displayed as the **first 12 hex characters** in standard read commands (e.g., `state: abd76d8bd634`, `version: b8db0be458a9`).
- **Stable Semantic `UUID`s**: Rendered in **full 36-character hyphenated UUID format** (e.g., `ElementId`, `RelationshipId`, `RepositoryId`, `SoftwareId`) to preserve unambiguous cross-tool identity.

### Operation Naming Vocabulary
- `CreateElement` $\to$ `create element`
- `UpdateElement` $\to$ `update element`
- `DeprecateElement` $\to$ `deprecate element`
- `Supersede` $\to$ `supersede element`
- `Link` $\to$ `link`
- `Unlink` $\to$ `unlink`

### Structural Orientation Types

KAT categorizes read commands into three structural layout models:

1. **Section & Field Oriented** (`status`, `show`, `validate`, `artifacts`): Uses Title Case headers with indented key-value fields.
2. **Revision & Operation Oriented** (`history`): Graph traversal showing revision blocks, metadata, and structured operations.
3. **Path & Tree Oriented** (`trace`, `impact`): Graph paths showing clear step-by-step origin and propagation trees.

---

## 3. Command Output Blueprints

### `kat status` (Section & Field)

```text
KAT repository

Repository
  repository:  9e28e703-3b66-4968-bc9d-11a132041e17
  software:    03edba5b-d4d5-421a-b4f3-a9ebcd0f402f
  state:       abd76d8bd634
  change:      none
  ontology:    28d0db9a988c

Knowledge
  elements:       0
    active:        0
    deprecated:    0
    superseded:    0
  relationships:  0

Consistency
  violations:             0
  unverified constraints: 0

Accountability
  current:      0
  stale:        0
  unaccounted:  0
```

*Note: The `Latest change` section appears between `Repository` and `Knowledge` only when `change` is not `none`.*

```text
Latest change
  revision:    aec57b12ea19
  operation:   create element
  description: none
```

---

### `kat show <element-id>` (Section & Field)

```text
Element 4545db04-173e-48eb-b79c-5e9128940939

Identity
  version:     d6bdf17486fd
  type:        kat.core/requirement
  lifecycle:   active

Details
  title:       User authentication requirement
  description: Requires multi-factor authentication for admin roles

Properties
  none

Relationships
  none
```

---

### `kat history` (Revision & Operation)

```text
Accepted change history (1 revision)

Revision 642e805f447a
  change:        ccdfc67a-955f-4e47-b43f-bdae607e0927
  result_state:  ccc1ae22865e
  base_states:   8925cc406d60
  dependencies:  none
  description:   none
  operations:
    create element
      version:   59ce53d97886
```

---

### `kat trace <element-id>` (Path & Tree)

```text
Trace origin for element 4545db04-173e-48eb-b79c-5e9128940939

Path 1
  Step 1
    from:          4545db04-173e-48eb-b79c-5e9128940939
    relationship:  12345678-1234-1234-1234-123456789012
    type:          kat.core/addresses
    direction:     forward ->
    to:            87654321-4321-4321-4321-210987654321
```

---

### `kat impact <element-id>` (Path & Tree)

```text
Impact analysis for element 4545db04-173e-48eb-b79c-5e9128940939

Impacted elements (1)

  Element 87654321-4321-4321-4321-210987654321
    type:        kat.core/design-decision
    lifecycle:   active
    rationale:
      path 1:
        step 1:  kat.core/addresses (forward ->)

Summary
  total impacted: 1
```

---

### `kat validate` (Section & Field)

```text
Consistency validation

Violations
  none

Unverified constraints
  none

Summary
  violations:             0
  unverified constraints: 0
```

---

### `kat artifacts` (Section & Field)

```text
Artifact accountability

Artifacts (1)

  Artifact 99999999-9999-9999-9999-999999999999 "authx-core-v1.jar"
    status:      current
    baselines:
      represents kat.core/implementation 87654321-4321-4321-4321-210987654321 (version b8db0be458a9)

Summary
  current:      1
  stale:        0
  unaccounted:  0
```

---

## 4. Centralized Formatting Helpers (`src/main.rs`)

To prevent formatting drift, all CLI renderers route through centralized formatting functions:

- `short_object_id(&ObjectId) -> String`: Returns the 12-hex prefix.
- `format_operation_name(&Operation) -> &'static str`: Returns canonical operation vocabulary (`create element`, `link`).
- `format_lifecycle(Lifecycle) -> &'static str`: Returns lowercase lifecycle name (`active`, `deprecated`, `superseded`).
- `format_accountability_status(ArtifactAccountabilityStatus) -> &'static str`: Returns lowercase status (`current`, `stale`, `unaccounted`).

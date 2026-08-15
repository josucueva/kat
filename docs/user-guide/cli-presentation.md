# KAT CLI Presentation Standard

This document defines the unified presentation language and rendering conventions for all KAT read-side CLI commands (`status`, `list`, `show`, `history`, `trace`, `impact`, `validate`, `artifacts`).

The goal is to ensure KAT's CLI output feels like a deliberate, cohesive software tool rather than an ad-hoc debug printer, preserving semantic precision while optimizing for human scannability.

---

## 1. Boundary: Presentation vs. Semantics

To prevent visual cleanups from accidentally altering command meaning, KAT strictly separates **Presentation Rules** from **Semantic Output Requirements**:

### Presentation (Visual Layer)
- 12-character `ObjectId` display abbreviation
- Title Case section headings
- Standardized indentation (2 spaces for fields, 4 spaces for nested lists/sub-items)
- Human-readable space-separated operation labels (`create element`, `account artifact`)
- Consistent empty-state wording (`none`, `0`)
- **Output Mode Distinction (`--compact` vs `--oneline`)**:
  - `--compact`: Reduced presentation detail across commands (omits headers, flattens counts/tables for fast scanning).
  - `--oneline`: Strictly exactly one physical line per `ChangeRevision` (specifically for `kat history`).

### Semantics (Domain Layer)
Presentation controls representation only. It must not alter which semantic records, counts, relationships, paths, or statuses are reported.

> [!IMPORTANT]
> **Display Abbreviation Only**: Abbreviating `ObjectId`s to 12 hex characters applies strictly to **CLI output display**. Canonical `ObjectId`s remain full 32-byte / 64-hex identities internally. UUID-taking CLI commands may accept full UUIDs or supported unique prefixes as defined by `cli.md`; canonical semantic identities remain full UUIDs internally.

---

## 2. Shared Formatting Rules

### Identity Formatting
- **Content-Addressed `ObjectId`s**: Displayed as the **first 12 hex characters** in standard read commands (e.g., `state: abd76d8bd634`, `version: b8db0be458a9`).
- **Stable Semantic `UUID`s**: Rendered in **full 36-character hyphenated UUID format** in detailed section views. Compact tabular views (`list`, relationship neighborhoods) may use standard 8-character hex prefixes for visual scannability.

### Operation Naming Vocabulary
- `CreateElement` $\to$ `create element`
- `UpdateElement` $\to$ `update element`
- `DeprecateElement` $\to$ `deprecate element`
- `SupersedeElement` $\to$ `supersede element`
- `Link` $\to$ `link`
- `Unlink` $\to$ `unlink`
- `AccountArtifact` $\to$ `account artifact`

### Structural Orientation Types

KAT categorizes commands into four structural layout models:

1. **Tabular** (`list`): Header-aligned column rows for multi-entity summaries.
2. **Section & Field Oriented** (`status`, `show`, `validate`, `artifacts`): Uses Title Case headers with indented key-value fields.
3. **Revision & Operation Oriented** (`history`): Graph traversal showing revision blocks, metadata, and structured operations.
4. **Path & Tree Oriented** (`trace`, `impact`): Graph paths showing clear step-by-step origin and propagation trees.

---

## 3. Command Output Blueprints

### `kat list` (Tabular)

```text
ID        TYPE             STATE       TITLE
7af83d1c  requirement      active      User authentication
bc18a910  design-decision  active      Use WebAuthn
91ae7712  requirement      deprecated  Legacy password policy
```

*Note: When no elements match the query criteria, `kat list` outputs `none`.*

---

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

*Note: The `Latest change` section appears between `Repository` and `Knowledge` only when `change` is not `none`. For single-operation changes, it displays the operation name; for multi-operation revisions, it summarizes the operation count:*

```text
Latest change
  revision:    aec57b12ea19
  operation:   create element
  description: none
```

or for multi-operation revisions:

```text
Latest change
  revision:    aec57b12ea19
  operations:  3
  description: Refactor authentication requirements
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
  DIR  REL ID    TYPE             ELEMENT   TITLE
  in   91ab36ef  addresses        7af83d1c  User authentication
  out  47c109df  realizes         bc18a910  Use WebAuthn
```

---

### `kat history` (Revision & Operation)

Single-operation revision:

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

Multi-operation or `AccountArtifact` revision:

```text
Revision aec57b12ea19
  change:        00000000-0000-0000-0000-000000000008
  result_state:  020202020202
  base_states:   010101010101
  dependencies:  none
  description:   Account artifact styles.css
  operations:
    account artifact
      artifact:           00000000-0000-0000-0000-000000000001
      reconciled:         1 relationship
        relationship_id:    00000000-0000-0000-0000-000000000002
        expected:           030303030303
        target_element:     00000000-0000-0000-0000-000000000003
        reconciled_version: 040404040404
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

*Note: For `stale` artifacts, the baseline output displays both the recorded baseline version and the current target element version:*

```text
  Artifact 99999999-9999-9999-9999-999999999999 "authx-core-v1.jar"
    status:      stale
    baselines:
      represents kat.core/implementation 87654321-4321-4321-4321-210987654321
        baseline: b8db0be458a9
        current:  7738e41ac298
```

---

## 4. Shared Rendering Vocabulary

To prevent formatting drift across tools and renderers, KAT establishes standardized formatting helpers:

- **12-Hex Object ID Helper**: Converts 32-byte canonical `ObjectId`s into 12-character hexadecimal prefixes for CLI display.
- **Canonical Operation Name Helper**: Formats operations into canonical space-separated lowercase strings (`create element`, `update element`, `deprecate element`, `supersede element`, `link`, `unlink`, `account artifact`).
- **Lifecycle Name Helper**: Formats lifecycle enums into lowercase text (`active`, `deprecated`, `superseded`).
- **Accountability Status Helper**: Formats artifact accountability statuses into lowercase text (`current`, `stale`, `unaccounted`).

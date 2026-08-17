# KAT v0.4 Machine Interface Specification

## Status

Draft.

This document defines the machine-readable structured output contracts, Data Transfer Objects (DTOs), JSON serialization schemas, and error envelopes for KAT v0.4.

It is derived from:

- the v0.4 foundation documents ([`findings.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/findings.md), [`requirements.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/requirements.md), [`use-cases.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/use-cases.md), [`operations.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/operations.md), [`reference-model.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/reference-model.md));
- the interaction model ([`interaction-model.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/interaction-model.md));
- the context model ([`context-model.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/context-model.md));
- the graph quality model ([`graph-quality-model.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/graph-quality-model.md)).

---

# 1. Fundamental Architectural Separation

KAT v0.4 establishes a fundamental separation between internal storage representation and external machine interfaces:

$$\text{Canonical Format (CBOR / Storage)} \neq \text{Machine Interface (DTOs / JSON Results)}$$

```text
┌────────────────────────────────────────────────────────────────────────┐
│ CANONICAL STORAGE FORMAT (spec/canonical-format.cddl)                  │
│ Internal, deterministic CBOR bytes in .kat/objects/. Derived SHA-256   │
│ ObjectIds. Immutable, persistent, minimal.                             │
├────────────────────────────────────────────────────────────────────────┤
│ MACHINE INTERFACE (docs/implementation/v0.4/machine-interface.md)      │
│ External DTO JSON representations for scripts, tools, IDE extensions,  │
│ and AI agents. Versioned, self-describing, structured.                 │
└────────────────────────────────────────────────────────────────────────┘
```

The machine interface exposes operation outputs without coupling external clients to CBOR byte decoding or human CLI text formatting.

---

# 2. Concrete Serialization Format

The normative serialization format for KAT v0.4 machine-readable output is **JSON** (RFC 8259).

## Key Serialization Rules
1. **UTF-8 Encoding**: JSON text is encoded in UTF-8 without BOM.
2. **Stable Identity Format**: All canonical UUIDs (`ElementId`, `RelationshipId`, `ChangeId`, `RepositoryId`, `OntologyId`) are formatted as 36-character hyphenated lowercase hex strings (e.g. `"56adfdb8-6539-46e7-851b-8e8b86f11e06"`).
3. **ObjectId Naming & Format**: Content-addressed `ObjectId` bytes are formatted as 64-character lowercase hex strings and explicitly named with `_object_id` suffixes (e.g. `change_revision_object_id`, `element_version_object_id`, `relationship_version_object_id`, `new_accepted_state_id`).
4. **Unordered Member Keys**: JSON object member ordering is **not** part of the machine-interface semantic contract. Clients must treat JSON objects as unordered key-value sets.
5. **Deterministic Array Ordering**: Array collections whose semantics are set-like (`provenance`, `requirements`, `findings`, `violations`) are sorted deterministically by stable UUID or key string.

---

# 3. Envelope Invariants & Global Structure

All machine interface outputs share a common top-level envelope.

## 3.1 Envelope Invariant (`INV-MI-01`)

```text
success == true  <==>  data != null && error == null
success == false <==>  data == null && error != null
```

A machine payload shall never contain partial success data alongside a non-null error.

## 3.2 Global Result Envelope (`CommonResultEnvelope<T>`)

```json
{
  "kat_version": "0.4.0",
  "interface_schema_version": 1,
  "success": true,
  "repository_id": "9f1c65c5-1b6d-4ed0-9fe8-4aaec79c2f91",
  "accepted_state_id": "a3b8c9d0e1f2...",
  "data": { },
  "error": null
}
```

- **`kat_version`**: Version string of the KAT binary emitting the payload.
- **`interface_schema_version`**: Version integer of the machine interface DTO schema (`1` for v0.4).
- **`success`**: Boolean indicator (`true` for successful operation execution).
- **`repository_id`**: Canonical UUID string of the repository (`null` if repository context could not be resolved).
- **`accepted_state_id`**: 64-hex string of the current accepted `SemanticState` ObjectId (`null` if repository context could not be resolved).
- **`data`**: Operation-specific DTO payload (non-null when `success == true`).
- **`error`**: Error details envelope (non-null when `success == false`).

## 3.3 Error Envelope (`ErrorEnvelope`)

When `success == false`:

```json
{
  "kat_version": "0.4.0",
  "interface_schema_version": 1,
  "success": false,
  "repository_id": "9f1c65c5-1b6d-4ed0-9fe8-4aaec79c2f91",
  "accepted_state_id": "a3b8c9d0e1f2...",
  "data": null,
  "error": {
    "code": "ONTOLOGY_TARGET_TYPE_DISALLOWED",
    "message": "Relationship type kat.core/realizes does not allow target element type kat.core/constraint",
    "details": {
      "relationship_type": "kat.core/realizes",
      "source_element_id": "56adfdb8-6539-46e7-851b-8e8b86f11e06",
      "target_element_id": "c4039918-ae9d-4d2d-9c2b-28f562b9cbbd",
      "failing_operation_index": 4
    }
  }
}
```

When repository context cannot be resolved (e.g. `NotInRepository` error):

```json
{
  "kat_version": "0.4.0",
  "interface_schema_version": 1,
  "success": false,
  "repository_id": null,
  "accepted_state_id": null,
  "data": null,
  "error": {
    "code": "NOT_IN_REPOSITORY",
    "message": "No KAT repository found at current path or any parent directory",
    "details": {}
  }
}
```

---

# 4. Porcelain Command DTO Schemas

To avoid duplication, machine DTO payloads inside `data` do **not** repeat `repository_id` or `accepted_state_id` (carried globally by `CommonResultEnvelope`).

---

## 4.1 `ContextResultDTO` (`context`)

```json
{
  "max_depth_applied": 2,
  "is_truncated": false,
  "roots": [
    {
      "element_id": "6f63af13-...",
      "type_id": "kat.core/requirement",
      "title": "Task completion and reopening"
    }
  ],
  "provenance": [],
  "requirements": [
    {
      "element_id": "6f63af13-...",
      "type_id": "kat.core/requirement",
      "title": "Task completion and reopening",
      "provenance_paths": []
    }
  ],
  "constraints": [],
  "decisions": [
    {
      "element_id": "915c96d4-...",
      "type_id": "kat.core/design-decision",
      "title": "Model task status as explicit open/completed states",
      "provenance_paths": [
        {
          "root_element_id": "6f63af13-...",
          "hops": [
            {
              "relationship_id": "11223344-...",
              "relationship_type": "kat.core/addresses",
              "target_element_id": "6f63af13-..."
            }
          ]
        }
      ]
    }
  ],
  "implementations": [
    {
      "element_id": "56adfdb8-...",
      "type_id": "kat.core/implementation",
      "title": "Express REST API routes",
      "provenance_paths": []
    }
  ],
  "artifacts": [
    {
      "element_id": "012d3257-...",
      "title": "src/app.js - API route definitions",
      "physical_locator": "src/app.js",
      "accountability_status": "CURRENT",
      "represented_implementation_ids": ["56adfdb8-..."],
      "provenance_paths": []
    }
  ],
  "validations": []
}
```

---

## 4.2 `StatusResultDTO` (`status`)

```json
{
  "accepted_head": {
    "state_object_id": "a3b8c9d0...",
    "change_id": "6a48e7d8...",
    "change_revision_object_id": "b7a6c5d4...",
    "element_count": 40,
    "relationship_count": 84
  },
  "draft_session": {
    "status": "open",
    "base_state_object_id": "a3b8c9d0...",
    "created_at": "2026-08-17T17:00:00Z",
    "description": "Add rest timer presets",
    "staged_operation_count": 3,
    "workflow_references": [
      {
        "handle": "@req-timer",
        "target_element_id": "8899aabb-..."
      }
    ],
    "candidate_delta": {
      "created_elements": 2,
      "created_links": 1,
      "accounted_artifacts": 0
    },
    "candidate_accountability_preview": {
      "total_artifacts": 6,
      "stale_artifacts": 0,
      "reconciled_in_draft": 0
    },
    "candidate_validation_preview": {
      "mechanical_violations_count": 0
    }
  }
}
```

---

## 4.3 `AuthorResultDTO` (`author`)

```json
{
  "staged_operation_count": 5,
  "declared_workflow_references": [
    { "handle": "@req-timer", "resolved_element_id": "8899aabb-..." },
    { "handle": "@impl-timer", "resolved_element_id": "11223344-..." }
  ],
  "candidate_working_state_object_id": "f9e8d7c6...",
  "session_status": "open"
}
```

---

## 4.4 `CheckResultDTO` (`check`)

```json
{
  "mechanical_violations": {
    "status": "PASS",
    "violation_count": 0,
    "violations": []
  },
  "evidence_coverage": {
    "summaries": [
      { "category": "kat.core/requirement", "total": 19, "covered": 18, "uncovered": 1 },
      { "category": "kat.core/constraint", "total": 4, "covered": 1, "uncovered": 3 },
      { "category": "kat.core/implementation", "total": 12, "covered": 10, "uncovered": 2 }
    ]
  },
  "artifact_accountability": {
    "total": 21,
    "current": 21,
    "stale": 0,
    "unaccounted": 0
  },
  "graph_quality": {
    "finding_count": 2,
    "findings": [
      {
        "code": "GQ-02",
        "affected_element_ids": ["8899aabb-..."],
        "summary": "RequirementWithoutRealizationRoute",
        "impact_explanation": "Requirement 'Offline Mode' has no realizing Implementation route."
      }
    ]
  }
}
```

---

## 4.5 `CommitResultDTO` (`commit`)

```json
{
  "change_id": "33445566-...",
  "change_revision_object_id": "b7a6c5d4...",
  "new_accepted_state_id": "e9f8a7b6...",
  "total_operations_committed": 5,
  "committed_at": "2026-08-17T17:30:00Z"
}
```

---

# 5. Plumbing & Mutation Response DTOs

Single mutation plumbing operations (e.g. `kat create`, `kat link`) return structured mutation responses containing canonical IDs and ObjectIds explicitly:

```json
{
  "operation_kind": "CreateElement",
  "element_id": "8899aabb-...",
  "element_version_object_id": "c1d2e3f4...",
  "type_id": "kat.core/requirement",
  "title": "Session Plan Snapshotting"
}
```

This eliminates prose regex parsing for external tools and machine automation.

---

# 6. Schema Versioning & Compatibility Policy

1. **Explicit Version Decoupling**:
   - `kat_version` (binary version, e.g. `0.4.0`) is decoupled from `interface_schema_version` (DTO schema version, `1`).
   - A KAT patch or feature release (e.g. `0.4.1` or `0.5.0`) does not trigger an `interface_schema_version` bump unless breaking DTO schema changes occur.
2. **Major Version (`interface_schema_version`)**: Currently `1`. Incremented only when breaking structural changes (field removals or non-backwards-compatible field type changes) occur.
3. **Additive Rule**: New fields may be added to DTO payloads within `interface_schema_version = 1`. Machine clients must ignore unknown keys.

---

# 7. Deterministic Array Ordering

To support reproducible testing and automated diffing, machine interface outputs enforce deterministic array sorting:

- Array collections (`provenance`, `requirements`, `findings`, `violations`) are sorted deterministically by stable `element_id` or `relationship_id`.

---

# 8. Next Specification Stage

The final detailed specification document in the sequence is:

```text
docs/implementation/v0.4/cli.md
```

It shall define:
- concrete CLI command spellings (`kat context`, `kat check`, `kat author`, `kat status`, `kat commit`);
- process standard stream behavior (stdout / stderr rules for `--json`);
- exit code policies and execution success signal rules;
- interactive vs batch CLI grammar.

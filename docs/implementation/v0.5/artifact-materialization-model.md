# KAT v0.5 Artifact Materialization Model

## Purpose

This document defines how KAT tracks the physical materialization of an
Artifact independently from its semantic accountability.

An Artifact may remain semantically aligned while its represented physical
content changes.

Likewise, its physical content may remain unchanged while an accounted semantic
dependency advances.

KAT therefore evaluates Artifact accountability across two independent
dimensions:

- semantic alignment;
- physical materialization alignment.

This document defines:

- `MaterializationId`;
- physical Artifact baselines;
- materialization resolution;
- semantic and physical accountability states;
- re-accounting;
- the relationship between Artifacts, `RepositoryRevision`, and
  `WorkspaceSnapshot`.

It does not define exact canonical encoding, Git-specific identifiers, locator
syntax, or CLI output.

---

## 1. Artifact accountability dimensions

Artifact accountability has two independent dimensions.

### Semantic alignment

Semantic alignment determines whether the Artifact is still accounted against
the current versions of the semantic knowledge it represents or derives from.

Conceptually:

```text
CURRENT
STALE
UNACCOUNTED
```

### Physical materialization alignment

Physical alignment determines whether the physical content represented by the
Artifact still matches the physical content that was last accounted.

Conceptually:

```text
CURRENT
MODIFIED
MISSING
UNRESOLVED
```

Neither dimension implies the other.

---

## 2. Materialization

An Artifact identifies physical project content through its locator.

For example:

```text
Artifact A
    locator = src/auth/service.rs
```

Resolving that locator against a physical workspace state produces a
materialization.

Conceptually:

```text
resolve(Artifact, WorkspaceSnapshot)
    -> Materialization
```

The materialization is the physical content represented by the Artifact in that
workspace state.

---

## 3. MaterializationId

A `MaterializationId` is an immutable identity for resolved physical Artifact
content.

Conceptually:

```text
MaterializationId
```

must change when the represented physical content changes.

The identity is backend-neutral.

A Git-backed implementation may derive it from Git physical objects, but Git
object identifiers are not part of the Artifact domain model.

---

## 4. Materialization scope

An Artifact may represent more than one physical form.

Examples may include:

```text
single file
directory
set of physical paths
```

The locator determines what physical content belongs to the Artifact.

The materialization identity must deterministically represent the complete
resolved content for that locator.

The exact locator model is defined separately.

---

## 5. Artifact accountability baseline

Accounting an Artifact establishes the semantic and physical state that has
been reviewed as aligned.

Conceptually:

```text
ArtifactAccountabilityBaseline
    artifact
    semantic_baselines[]
    materialization
```

For example:

```text
Artifact A

semantic baseline:
    Implementation I1v3

physical baseline:
    M17
```

This means the contributor asserted that materialization `M17` is aligned with
the recorded semantic baseline.

The exact persisted representation is defined separately.

---

## 6. Semantic staleness

An Artifact becomes semantically stale when an accounted semantic dependency
advances beyond the recorded baseline.

For example:

```text
baseline:
    I1v3
    M17

current:
    I1v4
    M17
```

Result:

```text
semantic:
    STALE

physical:
    CURRENT
```

The physical content has not changed, but its semantic baseline is outdated.

---

## 7. Physical materialization drift

An Artifact has physical materialization drift when its represented physical
content differs from the materialization that was last accounted.

For example:

```text
baseline:
    I1v3
    M17

current:
    I1v3
    M18
```

Result:

```text
semantic:
    CURRENT

physical:
    MODIFIED
```

KAT must not interpret this as semantic staleness.

Physical modification does not prove that semantic meaning changed.

It only proves that the previously accounted physical materialization no longer
matches the current one.

---

## 8. Combined drift

Both dimensions may change.

For example:

```text
baseline:
    I1v3
    M17

current:
    I1v4
    M18
```

Result:

```text
semantic:
    STALE

physical:
    MODIFIED
```

These conditions remain independently reportable.

---

## 9. Missing materialization

An Artifact may have a previously valid physical materialization that no longer
resolves.

For example:

```text
Artifact:
    locator = src/auth/service.rs
```

where the path was present at the accountability baseline but is now absent.

Result:

```text
physical:
    MISSING
```

KAT must not silently interpret deletion as semantic deprecation or removal.

---

## 10. Unresolved materialization

A materialization is `UNRESOLVED` when KAT cannot deterministically resolve the
Artifact locator against the current physical workspace.

Examples may include:

```text
invalid locator
ambiguous locator
unsupported locator form
backend resolution failure
```

`UNRESOLVED` is distinct from `MISSING`.

`MISSING` means the locator is valid but its expected physical content is
absent.

---

## 11. Unaccounted Artifact

An Artifact is semantically `UNACCOUNTED` when no valid semantic accountability
baseline exists.

Physical materialization may still be resolvable.

For example:

```text
semantic:
    UNACCOUNTED

physical:
    CURRENT MATERIALIZATION EXISTS
```

The physical dimension does not create semantic accountability automatically.

---

## 12. RepositoryRevision as coordination point

A `RepositoryRevision` binds:

```text
SemanticState
+
WorkspaceSnapshot
```

and therefore provides the context required to evaluate both accountability
dimensions.

Conceptually:

```text
RepositoryRevision R42
├── semantic_state = S42
└── workspace_snapshot = W42
```

For Artifact `A`, KAT can evaluate:

```text
semantic baseline
    against S42

physical materialization
    by resolving A against W42
```

This preserves consistency between semantic and physical evaluation.

---

## 13. Working-state evaluation

Physical materialization drift must be detectable before a new
`RepositoryRevision` is accepted.

Therefore KAT must also resolve Artifact materialization against the current
physical working state.

For example:

```text
accounted:
    M17

working state:
    M18
```

allows KAT to report:

```text
physical:
    MODIFIED
```

while the contributor is still working.

A new repository revision is not required before drift can be detected.

---

## 14. Re-accounting

Re-accounting asserts that the current Artifact materialization is aligned with
the selected current semantic knowledge.

Conceptually:

```text
before:

semantic baseline:
    I1v3

current semantic:
    I1v4

physical baseline:
    M17

current physical:
    M18
```

After successful re-accounting:

```text
semantic baseline:
    I1v4

physical baseline:
    M18
```

Result:

```text
semantic:
    CURRENT

physical:
    CURRENT
```

Re-accounting must be explicit.

KAT must not automatically establish a new baseline merely because physical
content or semantic knowledge changed.

---

## 15. Physical change without semantic change

A contributor may intentionally modify an Artifact's physical content while its
semantic meaning remains unchanged.

For example:

```text
refactoring
formatting
implementation refinement
performance improvement
```

KAT reports physical drift until the Artifact is reviewed and re-accounted.

The semantic element does not need to receive a new version solely because its
physical representation changed.

---

## 16. Semantic change without physical change

A semantic dependency may change while the Artifact remains physically
unchanged.

KAT reports semantic staleness.

This signals that the existing materialization must be reconsidered against the
new semantic state.

It does not imply that physical modification is necessarily required.

---

## 17. Non-Artifact physical content

Physical content not represented by an Artifact remains outside Artifact
accountability.

For example:

```text
tracked helper.rs
```

may change as part of the `WorkspaceSnapshot`.

If no Artifact resolves to it, KAT records a physical workspace change but does
not create Artifact materialization findings.

This preserves the distinction between complete physical tracking and selective
semantic modeling.

---

## 18. Materialization identity properties

A `MaterializationId` must be:

- deterministic;
- immutable;
- derived from represented physical content;
- independent from mutable physical references;
- comparable for equality.

Given identical represented physical content:

```text
resolve(A, W1) -> M
resolve(A, W2) -> M
```

even if `W1` and `W2` are different workspace snapshots.

If represented content changes:

```text
resolve(A, W3) -> M2
```

then:

```text
M2 != M
```

---

## 19. Backend boundary

The physical backend determines how materialization identity is computed.

For a Git-backed implementation, this may use:

```text
file content -> Git blob
directory content -> Git tree
multiple paths -> deterministic aggregate
```

KAT-facing accountability remains expressed through `MaterializationId`.

The Artifact model must not depend directly on Git hashes.

---

## 20. Baseline persistence

KAT must persist enough information to reproduce the Artifact accountability
baseline.

The exact representation remains open.

Two valid implementation directions are:

```text
store MaterializationId explicitly
```

or:

```text
store an accepted RepositoryRevision reference
and derive materialization from its WorkspaceSnapshot
```

The final choice must preserve deterministic evaluation and historical
accountability.

This decision is deferred to the canonical impact analysis.

---

## Summary

Artifact accountability spans two independent dimensions:

```text
Semantic alignment
    CURRENT
    STALE
    UNACCOUNTED

Physical materialization alignment
    CURRENT
    MODIFIED
    MISSING
    UNRESOLVED
```

An Artifact accountability baseline represents the reviewed relationship
between:

```text
semantic knowledge
+
physical materialization
```

Changing physical content does not implicitly change semantic knowledge.

Changing semantic knowledge does not imply that physical content changed.

`RepositoryRevision` provides the coherent semantic and physical context in
which both dimensions can be evaluated.

Re-accounting explicitly establishes a new semantic and physical baseline.
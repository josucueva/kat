# KAT v0.5 Reconciliation Rules

## Purpose

This document defines the initial deterministic rules used to reconcile
concurrent semantic operations.

It applies to the seven canonical KAT mutation operations:

- `CreateElement`;
- `UpdateElement`;
- `DeprecateElement`;
- `Link`;
- `Unlink`;
- `SupersedeElement`;
- `AccountArtifact`.

These rules determine whether concurrent semantic effects:

- compose automatically;
- require combined validation;
- become a conflict candidate.

The rules are intentionally conservative for v0.5.

---

## 1. General rule

Two concurrent Changes may reconcile automatically when their semantic effects
can both be preserved and the combined result remains valid.

Automatic reconciliation must be deterministic.

A useful strong condition is:

```text
apply(apply(S0, A), B)
    ==
apply(apply(S0, B), A)
```

provided both application orders succeed and produce the same canonical
`SemanticState`.

If this condition holds, the effects are safe candidates for automatic
composition.

Failure of this condition does not always imply conflict, but v0.5 should prefer
explicit resolution over speculative semantic merging.

---

## 2. Identity independence

Operations affecting different stable identities are normally independent.

For example:

```text
Update R1
+
Update I1
```

is eligible for automatic composition when neither operation invalidates a
dependency or repository invariant required by the other.

Identity independence does not bypass final validation.

---

## 3. Combined validation

Every automatically composed result must be validated after composition.

Therefore:

```text
A valid alone
B valid alone
```

does not imply:

```text
A + B valid
```

If the combined result violates structural, lifecycle, ontology, or repository
invariants, reconciliation is blocked.

---

# Operation rules

## 4. CreateElement

### Create different identities

```text
Create A
+
Create B
```

where:

```text
A.id != B.id
```

Expected:

```text
AUTO
```

subject to combined validation.

### Create same identity

```text
Create A
+
Create A
```

Expected:

```text
CONFLICT
```

unless the operations are exactly equivalent and produce the same immutable
result.

Equivalent duplicate creation may be treated as idempotent only if identity and
content are identical.

---

## 5. UpdateElement

### Update different identities

```text
Update A
+
Update B
```

where:

```text
A.id != B.id
```

Expected:

```text
AUTO
```

subject to dependency and combined validation.

### Update same identity

```text
Update A
+
Update A
```

Expected v0.5 behavior:

```text
CONFLICT
```

KAT does not attempt property-level semantic merging in the initial
implementation.

---

## 6. DeprecateElement

### Deprecate different identities

```text
Deprecate A
+
Deprecate B
```

Expected:

```text
AUTO
```

subject to combined validation.

### Deprecate same identity

Equivalent concurrent deprecation may reconcile idempotently if both effects
produce the same lifecycle state.

Otherwise:

```text
CONFLICT
```

### Deprecate versus Update on same identity

```text
Deprecate A
+
Update A
```

Expected:

```text
CONFLICT
```

The two operations express incompatible concurrent lifecycle intent.

---

## 7. Link

### Independent links

```text
Link A -> B
+
Link C -> D
```

Expected:

```text
AUTO
```

when they create distinct relationships and the combined graph remains valid.

### Equivalent same link

If both Changes create the same relationship with equivalent identity and
semantics:

```text
AUTO / IDEMPOTENT
```

provided duplicate relationship creation is not introduced.

### Incompatible relationship effects

If the combined links violate ontology, lifecycle, or repository invariants:

```text
CONFLICT
```

---

## 8. Unlink

### Independent unlinks

```text
Unlink X
+
Unlink Y
```

where:

```text
X != Y
```

Expected:

```text
AUTO
```

### Equivalent same unlink

Concurrent removal of the same existing relationship may be treated as
idempotent.

Expected:

```text
AUTO / IDEMPOTENT
```

### Link versus Unlink same relationship

```text
Link X
+
Unlink X
```

Expected:

```text
CONFLICT
```

unless ancestry shows that the operations do not actually target the same
relationship state.

---

## 9. SupersedeElement

### Supersede unrelated identities

```text
Supersede A -> A2
+
Supersede B -> B2
```

Expected:

```text
AUTO
```

subject to combined validation.

### Competing supersession

```text
Supersede A -> A2
+
Supersede A -> A3
```

Expected:

```text
CONFLICT
```

when both alternatives cannot be preserved under supersession semantics.

### Supersede versus Update existing element

```text
Supersede A -> A2
+
Update A
```

Expected v0.5 behavior:

```text
CONFLICT
```

unless future lifecycle semantics establish a deterministic valid composition.

---

## 10. AccountArtifact

### Account different Artifacts

```text
Account A
+
Account B
```

Expected:

```text
AUTO
```

when:

```text
A != B
```

### Account Artifact and unrelated semantic update

```text
Account A
+
Update B
```

Expected:

```text
AUTO
```

if `B` is not part of A's accountability baseline and the combined state remains
valid.

### Account Artifact while its semantic baseline changes concurrently

Example:

```text
Change A:
    Account Artifact X against I1v3

Change B:
    Update I1 -> I1v4
```

Expected:

```text
COMPOSE + CONSEQUENCE
```

when both operations remain mechanically valid.

The reconciliation may succeed while the Artifact becomes:

```text
semantic = STALE
```

This is not automatically a conflict.

### Concurrent accounting of same Artifact

If two Changes establish different accountability baselines for the same
Artifact:

```text
CONFLICT
```

unless both resulting accountability states are exactly equivalent.

---

# Cross-operation rules

## 11. Operations on unrelated identities

Different operation kinds may reconcile automatically when they affect unrelated
semantic identities.

Examples:

```text
Create R1
+
Update I1
```

```text
Link I1 -> R1
+
Account A2
```

```text
Deprecate R1
+
Create R2
```

Expected:

```text
AUTO
```

subject to dependency and combined validation.

---

## 12. Lifecycle dominance is not inferred

KAT must not assume rules such as:

```text
Deprecate always wins over Update
Supersede always wins over Link
latest operation wins
```

Concurrent lifecycle effects require explicit semantic compatibility.

No last-writer-wins rule is allowed.

---

## 13. Dependency changes

An operation may remain mechanically composable while changing semantic
assumptions used by another Change.

Example:

```text
A:
    Update Requirement R1

B:
    work on Implementation I1 that realizes R1
```

If both effects can coexist:

```text
AUTO
```

or:

```text
AUTO + CONSEQUENCE
```

depending on accountability, impact, and validation results.

Dependency impact alone is not a conflict.

---

## 14. Artifact consequences

Changes that affect semantic knowledge related to an Artifact may produce:

```text
STALE
MODIFIED
validation review
impact findings
```

without blocking reconciliation.

Conflict is reserved for incompatible concurrent effects.

---

# Initial interaction matrix

The following table summarizes the default v0.5 behavior.

| Left | Right | Different identities | Same/overlapping identity |
|---|---|---|---|
| Create | Create | AUTO | CONFLICT or idempotent if identical |
| Update | Update | AUTO | CONFLICT |
| Deprecate | Deprecate | AUTO | Idempotent if equivalent, otherwise CONFLICT |
| Link | Link | AUTO | Idempotent if equivalent, otherwise validate |
| Unlink | Unlink | AUTO | Idempotent if equivalent |
| Supersede | Supersede | AUTO | CONFLICT when competing |
| Account | Account | AUTO | CONFLICT unless equivalent |

Important cross-operation cases:

| Combination | Initial rule |
|---|---|
| Update A + Deprecate A | CONFLICT |
| Update A + Supersede A | CONFLICT |
| Link X + Unlink X | CONFLICT |
| Account Artifact + update accounted dependency | COMPOSE + CONSEQUENCE |
| Independent operations of different kinds | AUTO + VALIDATE |
| Any composition producing invalid state | CONFLICT |

---

## 15. Result classes

Each operation interaction resolves to one of four initial classes.

### AUTO

Effects may be composed automatically.

Final combined validation is still required.

### IDEMPOTENT

Both Changes express the same resulting effect.

The effect is represented once in the reconciled state.

### AUTO + CONSEQUENCE

The semantic composition is valid, but reconciliation may produce health or
accountability findings.

### CONFLICT

KAT cannot preserve both intended effects under the current deterministic rules.

Explicit resolution is required.

---

## 16. Determinism

Given identical:

```text
base SemanticState
left operations
right operations
ontology
repository invariants
```

the rule classification must be identical.

Input ordering must not determine which effect wins.

If automatic composition would require semantic guessing, v0.5 must prefer
`CONFLICT`.

---

## 17. Conservative v0.5 boundary

The initial implementation does not attempt:

- property-level merging of `UpdateElement`;
- semantic interpretation of natural-language fields;
- intent inference;
- probabilistic conflict resolution;
- automatic choice between competing lifecycle effects.

These capabilities may be investigated later.

The first implementation prioritizes deterministic and explainable
reconciliation.

---

## Summary

KAT reconciliation evaluates concurrent semantic operations by their effects
over stable identities.

The initial rule is:

```text
independent + valid
    -> AUTO

equivalent
    -> IDEMPOTENT

compatible but creates downstream findings
    -> AUTO + CONSEQUENCE

incompatible or combined-invalid
    -> CONFLICT
```

Automatic reconciliation remains conservative.

No operation wins because it was newer, local, remote, or processed last.

Every automatically composed semantic result is revalidated before it can
participate in a reconciled `RepositoryRevision`.
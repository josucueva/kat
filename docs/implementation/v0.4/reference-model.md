# KAT v0.4 Reference Model

## Status

Draft.

This document defines how actors refer to semantic objects in KAT v0.4.

It is derived from:

- the v0.4 findings and problems;
- the v0.4 requirements;
- the v0.4 use cases;
- the v0.4 operations model.

The purpose of this document is to reduce identity-related interaction friction while preserving KAT's existing stable identity semantics.

This document defines:

- canonical identity;
- accepted reference forms;
- reference resolution;
- ambiguity handling;
- workflow-local references;
- reference lifetime and scope;
- the distinction between persistent identity and interaction references.

It does not define:

- final CLI syntax;
- batch-authoring syntax;
- structured input/output schemas;
- context-query semantics;
- graph-quality semantics.

Those are specified separately.

---

# 1. Problem

KAT currently uses stable UUID identities for semantic objects.

For example:

```text
ElementId
RelationshipId
ChangeId
RepositoryId
SoftwareId
OntologyId
````

This identity model is correct for persistent semantic identity.

However, normal authoring frequently requires users to interact directly with these identifiers.

A typical workflow is:

```text
create Requirement
    ↓
receive UUID
    ↓
copy UUID

create Implementation
    ↓
receive UUID
    ↓
copy UUID

link <implementation UUID> realizes <requirement UUID>
```

At small scale this is manageable.

At larger scale it creates significant bookkeeping cost.

The Statit experiment demonstrated this directly:

```text
70 create operations
110 link operations
21 accountability operations
```

The construction workflow required external scripting partly to retain and reuse identifiers.

The v0.4 reference model shall reduce this interaction cost without weakening canonical identity.

---

# 2. Design Principle

KAT shall distinguish between:

```text
identity
```

and:

```text
reference
```

They are not the same concept.

## Identity

Identity answers:

> Which semantic object is this?

Identity is stable and canonical.

Example:

```text
ElementId = 9f1c...
```

## Reference

A reference answers:

> How is the actor identifying that object in this interaction?

A reference may be shorter-lived or more convenient.

Conceptually:

```text
reference
    ↓ resolves to
stable identity
```

A reference never replaces the object's canonical identity.

---

# 3. Reference Model Goals

The v0.4 reference model shall satisfy the following goals.

## G-01: Preserve Stable Identity

Existing UUID-based semantic identity remains authoritative.

## G-02: Reduce Manual UUID Bookkeeping

Normal authoring workflows should not require repeated copying and storage of generated UUIDs.

## G-03: Deterministic Resolution

Every supported reference either:

```text
resolves uniquely
```

or:

```text
fails explicitly
```

KAT shall not guess.

## G-04: Scope References Explicitly

Temporary references shall have a clear lifetime and scope.

## G-05: Avoid New Persistent Identity Unless Necessary

v0.4 should prefer interaction-layer references over introducing new persistent semantic identifiers.

## G-06: Preserve Machine Usability

Machine clients shall be able to use canonical IDs directly even when convenience references are available.

---

# 4. Canonical Identity

Canonical identity remains unchanged in v0.4.

For Knowledge Elements:

```text
ElementId = UUID
```

For Relationships:

```text
RelationshipId = UUID
```

For Changes:

```text
ChangeId = UUID
```

For ontology and repository objects, the existing identity rules remain unchanged.

Canonical identity:

* persists across immutable versions;
* is independent of title;
* is independent of description;
* is independent of physical Artifact path;
* is not affected by interaction aliases;
* is stored in canonical semantic objects where already defined.

---

# 5. Reference Classes

KAT v0.4 distinguishes reference classes.

The initial model contains:

```text
CanonicalReference
PrefixReference
WorkflowReference
```

Other possible reference classes, such as title-based or Artifact-path references, remain unresolved and are discussed later.

---

# 6. CanonicalReference

## Definition

A `CanonicalReference` contains the complete stable identity of the target semantic object.

Example:

```text
9f1c65c5-1b6d-4ed0-9fe8-4aaec79c2f91
```

## Properties

Canonical references are:

```text
persistent
globally unambiguous within their identity domain
stable
independent of presentation data
```

## Use

Canonical references remain valid for:

* humans;
* scripts;
* machine clients;
* structured interfaces;
* internal operation resolution.

## Requirement

All reference mechanisms introduced by v0.4 must ultimately resolve to a canonical identity before semantic operation execution.

---

# 7. PrefixReference

## Definition

A `PrefixReference` identifies a canonical UUID through an unambiguous hexadecimal prefix.

This preserves the existing v0.3.1 behavior.

Example:

```text
9f1c65c5
```

may resolve to:

```text
9f1c65c5-1b6d-4ed0-9fe8-4aaec79c2f91
```

## Minimum Length

Existing behavior requires at least:

```text
8 hexadecimal characters
```

v0.4 preserves this minimum unless later CLI design provides strong reason to change it.

## Resolution

A prefix resolves only if exactly one object in the relevant resolution domain matches.

### Unique

```text
1 match
    ↓
success
```

### None

```text
0 matches
    ↓
UnknownReference
```

### Multiple

```text
>1 matches
    ↓
AmbiguousReference
```

## Properties

Prefix references are convenience references.

They are not persistent identities.

Their future uniqueness is not guaranteed as repository size grows.

Therefore they should not be persisted as semantic data.

---

# 8. WorkflowReference

## Status

New in v0.4.

## Definition

A `WorkflowReference` is a temporary actor-defined reference used to identify semantic objects during an active authoring workflow.

Conceptually:

```text
WorkflowReference
    ↓
stable ElementId or RelationshipId
```

Example:

```text
snapshot-requirement
```

could resolve to:

```text
ElementId 788128be-...
```

within the current authoring workflow.

## Primary Purpose

Workflow references solve the empirical authoring problem:

```text
create
capture UUID
store UUID externally
reuse UUID
```

The intended workflow becomes:

```text
create Requirement as snapshot-requirement

create Implementation as persistence-engine

link persistence-engine realizes snapshot-requirement
```

The exact CLI syntax is intentionally not defined here.

---

# 9. WorkflowReference Scope

A workflow reference belongs to an explicit authoring scope.

The preferred v0.4 scope is:

```text
active draft Change
```

Conceptually:

```text
Change Draft
    ├── operation 1
    ├── operation 2
    ├── workflow references
    └── candidate state
```

The workflow-reference table is not part of accepted SemanticState.

---

# 10. WorkflowReference Lifetime

The lifecycle is:

```text
change begin
    ↓
workflow references may be declared
    ↓
references may be reused by later staged operations
    ↓
commit or abort
    ↓
workflow references cease to exist
```

After the Change ends:

```text
canonical identities persist
workflow references do not
```

This avoids introducing persistent alias semantics into the repository.

---

# 11. WorkflowReference Namespace

Workflow references shall be unique within their active workflow scope.

Example:

```text
snapshot
persistence
active-workout
```

If an actor attempts to declare the same reference twice in the same scope:

```text
DuplicateWorkflowReference
```

shall be reported.

Workflow-reference comparison rules shall be deterministic.

The exact lexical syntax is deferred to CLI and authoring-model design.

---

# 12. WorkflowReference Targets

At minimum, workflow references shall support newly created Knowledge Elements.

The design should also support references to newly created Relationships if a later operation needs to identify them.

Conceptually:

```text
WorkflowReferenceTarget =
    ElementId
    | RelationshipId
```

Whether other identity domains should be supported is deferred.

The initial v0.4 use case does not require aliases for:

```text
ChangeId
OntologyId
RepositoryId
ObjectId
```

---

# 13. Workflow References and Existing Objects

Workflow references are primarily intended for newly created objects.

However, an authoring workflow may benefit from assigning a local workflow reference to an already accepted object.

Example:

```text
bind existing Requirement as auth-requirement
```

then:

```text
link new-implementation realizes auth-requirement
```

This could reduce repeated prefix or UUID use in large Changes.

Whether explicit binding of existing objects is included in v0.4 is a design decision for `authoring-model.md`.

The reference model permits it conceptually, provided the binding is deterministic and draft-local.

---

# 14. WorkflowReference Is Not Semantic State

Workflow references shall not appear inside:

```text
KnowledgeElementVersion
RelationshipVersion
SemanticState
OntologyVersion
```

They shall not replace:

```text
ElementId
RelationshipId
```

inside canonical semantic operations or resulting objects.

For example, actor input may contain:

```text
link persistence realizes snapshot
```

but before semantic execution KAT resolves this to:

```text
Link {
    source: ElementId(...),
    target: ElementId(...)
}
```

The canonical mutation contains stable identities only.

---

# 15. Reference Resolution Pipeline

KAT shall resolve actor-supplied references before executing semantic operations.

Conceptually:

```text
Actor Reference
    ↓
classify reference
    ↓
resolve within appropriate domain
    ↓
canonical identity
    ↓
semantic operation
```

The reference layer must not alter the semantics of the operation itself.

---

# 16. Resolution Context

Reference resolution depends on an explicit context.

Conceptually:

```text
ResolutionContext {
    repository,
    accepted_state,
    working_state?,
    workflow_references?,
    identity_domain
}
```

The exact implementation structure is deferred.

Important properties are defined below.

---

# 17. Accepted-State Resolution

For ordinary accepted-state queries:

```text
Show
Trace
Impact
Context
...
```

resolution is based on accepted repository state.

Open draft objects shall not silently enter the accepted-state resolution domain.

This preserves accepted-state isolation.

---

# 18. Draft Resolution

For mutation operations within an active draft:

resolution may use:

```text
accepted base objects
+
objects selected in current working candidate state
+
workflow references defined in the current draft
```

This allows:

```text
operation 1 creates element A

operation 2 references A
```

before the Change has been committed.

---

# 19. Ordered Resolution Semantics

Workflow references respect operation order.

Example:

```text
1. create Requirement as R
2. link I realizes R
```

is valid if `I` is resolvable at step 2.

But:

```text
1. link I realizes R
2. create Requirement as R
```

shall fail if `R` did not exist before operation 1.

The system shall not perform forward-reference inference unless explicitly introduced by a later authoring design.

The preferred v0.4 model is:

```text
references become available after their defining operation succeeds
```

This matches existing sequential working-state semantics.

---

# 20. Resolution Domains

Reference resolution shall occur within an explicit identity domain.

Examples:

```text
Element
Relationship
ElementType
RelationshipType
```

A reference valid in one domain shall not automatically match an object in another domain.

For example:

```text
kat show <reference>
```

expects an Element reference.

A Relationship with a coincidentally similar prefix must not become a candidate.

This prevents cross-domain ambiguity.

---

# 21. Element Type References

Element type resolution preserves current ontology behavior.

Supported forms include:

```text
canonical ontology type ID
```

for example:

```text
kat.core/requirement
```

and an unambiguous short ontology name:

```text
requirement
```

which resolves to:

```text
kat.core/requirement
```

when unique.

Ambiguous short names in extension ontologies shall fail explicitly.

---

# 22. Relationship Type References

Relationship type resolution likewise preserves current ontology behavior.

Supported forms include:

```text
kat.core/realizes
```

and:

```text
realizes
```

when the short name is unambiguous.

Endpoint validity remains a semantic ontology check after reference resolution.

---

# 23. Titles as References

## Status

Not accepted by the base v0.4 reference model yet.

Titles are attractive because they are human-readable.

For example:

```text
"Session Plan Snapshotting"
```

could theoretically identify a Requirement.

However, titles have problematic identity characteristics.

They may be:

```text
non-unique
mutable
long
punctuation-sensitive
presentation-oriented
```

Using them as implicit general references could produce unstable behavior.

Therefore v0.4 should not make arbitrary title-based resolution a default until its semantics are explicitly justified.

---

# 24. Title Lookup vs Title Reference

KAT may still support searching or listing by title without treating the title itself as identity.

This distinction is important.

```text
search title
    ↓
discover ElementId
```

is different from:

```text
title
    directly acts as identity
```

The first is safe as a discovery mechanism.

The second requires stronger reference semantics.

The reference model recommends preserving this distinction.

---

# 25. Persistent Semantic Aliases

## Status

Deferred.

A persistent semantic alias might look like:

```text
req/session-plan-snapshotting
```

and remain valid across Changes.

This could be convenient, but it introduces new semantic questions:

* Is the alias unique repository-wide?
* Is it mutable?
* Is alias history retained?
* Can aliases be reused?
* Does changing an alias create a semantic revision?
* Does it belong to KnowledgeElementVersion or stable identity metadata?
* What happens during future collaboration or merge?
* Does alias identity become a second identity system?

None of the current empirical findings require this complexity.

Therefore persistent aliases are not part of the initial v0.4 reference model.

---

# 26. Artifact Paths as References

## Status

Deferred.

It may appear useful to allow:

```text
lib/features/workout/services/workout_repository.dart
```

to resolve directly to an Artifact.

However, Artifact paths are physical representation data, not stable semantic identity.

Potential problems include:

```text
file rename
multiple Artifacts for same path over history
non-file Artifacts
multiple paths represented by one semantic responsibility
platform-specific path syntax
```

The v0.4 experiments do not require path-based identity resolution.

Artifact discovery may instead remain a query/search concern.

---

# 27. ObjectId References

ObjectId identifies immutable canonical versions.

It is not the default reference for semantic operations that target stable identities.

Example:

```text
UpdateElement
```

targets:

```text
ElementId
```

not a historical `KnowledgeElementVersion ObjectId`.

ObjectId may still be exposed and accepted by version-oriented operations where appropriate, such as history or integrity inspection.

No general ObjectId reference expansion is required by v0.4.

---

# 28. Reference Resolution Priority

When syntax allows multiple possible reference classes, resolution priority must be explicit.

The recommended conceptual priority is:

```text
1. canonical stable identity
2. valid UUID prefix
3. workflow reference
```

However, the exact ordering depends on the lexical form chosen for workflow references.

A better CLI syntax may make reference classes syntactically distinguishable, avoiding priority ambiguity entirely.

For example:

```text
@snapshot
```

could unambiguously mean workflow reference.

The reference model does not freeze such syntax.

## Preferred Principle

Where possible:

> Reference classes should be syntactically distinguishable rather than relying on heuristic interpretation.

---

# 29. No Heuristic Reference Guessing

KAT shall not perform heuristic resolution such as:

```text
maybe this is a title
maybe this is a UUID
maybe this is a file
```

until one happens to match.

Resolution shall follow defined reference classes and deterministic rules.

This protects scripts and agents from changes in repository content altering interpretation unexpectedly.

---

# 30. Resolution Results

Reference resolution conceptually returns one of:

```text
ResolvedReference
UnknownReference
AmbiguousReference
InvalidReference
```

---

# 31. ResolvedReference

Conceptually:

```text
ResolvedReference {
    domain,
    stable_id,
    source_reference_kind
}
```

Additional diagnostic information may be exposed at the interface layer.

The semantic operation needs only the canonical stable identity.

---

# 32. UnknownReference

Returned when a syntactically valid reference matches no object within its resolution context.

Example:

```text
UUID prefix does not match any active ElementId
```

or:

```text
workflow reference has not been declared
```

The failure must not cause a mutation.

---

# 33. AmbiguousReference

Returned when a reference matches multiple objects where uniqueness is required.

Example:

```text
UUID prefix
    ↓
2 matching ElementIds
```

KAT shall expose sufficient candidate information to allow the actor to choose a more precise reference.

At minimum this should include stable identities.

Human presentation may additionally include:

```text
type
title
```

Structured-error representation is defined later.

---

# 34. InvalidReference

Returned when the supplied value is not valid syntax for the expected reference domain or class.

Examples may include:

```text
UUID prefix shorter than the supported minimum
malformed UUID
invalid workflow-reference syntax
```

Invalid input shall not be reinterpreted unpredictably as another reference type.

---

# 35. Reference Consistency Across Operations

Where an operation accepts an Element reference, the same basic reference model should apply consistently.

For example:

```text
Show
UpdateElement
DeprecateElement
Trace
Impact
Context
Link source
Link target
AccountArtifact artifact target
```

should not each invent unrelated element-reference semantics.

Operation-specific semantic restrictions may still differ.

For example:

```text
AccountArtifact
```

may require the resolved element to be:

```text
kat.core/artifact
```

Resolution succeeds first.

Semantic validation occurs second.

---

# 36. Resolution and Semantic Validation

Reference resolution and semantic validation are separate stages.

Example:

```text
link realizes A B
```

may have:

```text
A resolves successfully
B resolves successfully
```

but still fail because:

```text
A's element type is not an allowed realizes source
```

Therefore:

```text
reference valid
!=
operation semantically valid
```

This separation should remain explicit in implementation and errors.

---

# 37. Resolution and Lifecycle

Resolving an ElementId identifies stable semantic identity.

Whether the currently selected element version is:

```text
Active
Deprecated
Superseded
```

is a separate semantic concern.

Operations may place lifecycle restrictions after resolution.

The reference layer itself should not silently redirect references from a deprecated element to another element.

For example:

```text
reference deprecated Design Decision
```

must not automatically resolve to its superseding decision.

Traceability requires stable identity to remain explicit.

---

# 38. Resolution and Candidate State

During a draft, an element may have a candidate version differing from accepted state.

A reference to its ElementId still resolves to the same stable identity.

Semantic operations against the draft observe the version selected by `S_working`.

Conceptually:

```text
reference
    ↓
ElementId
    ↓
candidate state
    ↓
selected candidate ObjectId
```

This preserves stable identity across versions.

---

# 39. WorkflowReference Storage

Workflow references are draft interaction state.

Conceptually:

```text
Draft {
    base_state,
    staged_operations,
    workflow_references
}
```

This is not a canonical schema commitment.

The exact persistence mechanism is deferred to authoring design.

Important semantic requirements are:

* references survive separate CLI invocations during the same open draft;
* references disappear when the draft is committed or aborted;
* reference storage does not affect ObjectId;
* reference storage does not alter SemanticState;
* accepted repository semantics remain independent of reference names.

Because KAT's authoring workflow spans multiple CLI invocations, purely process-memory aliases would be insufficient.

The authoring model must therefore define draft-local persistence for workflow references.

---

# 40. WorkflowReference Rename

The need for renaming a workflow reference is not established.

Because workflow references are temporary, v0.4 should prefer a minimal model.

Potential behavior:

```text
reference name fixed after declaration
```

If an incorrect name is used, the user may abort/recreate or a later authoring design may introduce explicit rebinding.

No semantic requirement currently justifies reference rename operations.

---

# 41. WorkflowReference Deletion

Likewise, explicit deletion may not be necessary initially.

References naturally expire when the draft ends.

If a staged operation that created an object is removed in a future editable-draft model, associated reference behavior would need specification.

The current draft model does not yet require this capability.

---

# 42. Relationship Workflow References

Relationship references may become useful in workflows such as:

```text
create relationship as implementation-link
...
unlink implementation-link
```

or for artifact-accountability operations that need RelationshipId.

Therefore the reference model should not assume workflow references can only target Elements.

The initial implementation may stage support if needed, but the model should remain domain-capable.

---

# 43. Reference Visibility in Inspection

Draft inspection should be able to expose workflow references where useful.

Conceptually:

```text
STAGED OPERATIONS

1. Create Requirement
   ref: snapshot

2. Create Implementation
   ref: persistence

3. Link persistence realizes snapshot
```

This improves authoring explainability.

However, accepted-state queries should not expose expired workflow references as repository semantics.

---

# 44. Machine Clients

Machine clients may use either:

```text
canonical stable identities
```

or supported workflow references during draft authoring.

Structured mutation results shall still expose the canonical identities generated by operations.

Therefore automation does not become dependent on temporary aliases.

Conceptually:

```text
input:
workflow ref = snapshot

result:
ElementId = ...
ObjectId = ...
workflow ref = snapshot
```

The exact result schema belongs to `machine-interface.md`.

---

# 45. Human Users

Human users should normally be able to author a coherent Change without tracking generated UUIDs manually.

Canonical IDs remain visible when needed for:

* explicit disambiguation;
* debugging;
* history;
* interoperability;
* scripts;
* precise inspection.

Convenience should reduce UUID exposure, not hide identity entirely.

---

# 46. Agent Usage

No agent-specific reference semantics exist in KAT core.

An Agent uses the same:

```text
CanonicalReference
PrefixReference
WorkflowReference
```

model as any other actor.

A future Agent Extension may recommend how agents should choose and reuse workflow reference names.

That policy remains outside the core reference model.

---

# 47. Reference Model Invariants

## INV-REF-01: Canonical Identity Authority

Every semantic object targeted by a reference ultimately resolves to its canonical stable identity.

---

## INV-REF-02: No Alias Identity Replacement

A convenience reference never replaces canonical identity.

---

## INV-REF-03: Deterministic Resolution

Given the same:

```text
reference
resolution domain
repository state
draft state
workflow-reference table
KAT version
```

resolution produces the same result.

---

## INV-REF-04: Explicit Ambiguity

Multiple matches never result in silent selection.

---

## INV-REF-05: Draft Isolation

Workflow references from an open draft do not affect accepted-state query resolution.

---

## INV-REF-06: Workflow Lifetime

A workflow reference is invalid after the draft scope that owns it ends.

---

## INV-REF-07: No Canonical Encoding Impact

Workflow references do not affect canonical object bytes or ObjectIds.

---

## INV-REF-08: Ordered Availability

A workflow reference becomes usable only after the operation defining its target has successfully executed in the working authoring sequence.

---

# 48. Reference Model Non-Goals

The v0.4 reference model does not attempt to provide:

```text
persistent semantic slugs
global user-defined aliases
filesystem-path identity
fuzzy title matching
AI-based reference inference
cross-repository naming
distributed namespace management
```

These may be considered in future versions if independent evidence justifies them.

---

# 49. Compatibility

Existing v0.3.1 references remain valid.

Therefore:

```text
full UUID
UUID prefix >= 8 hex characters
```

continue to work.

v0.4 reference improvements are additive at the interaction layer.

Existing scripts that use canonical IDs should not be forced to adopt workflow references.

---

# 50. Canonical Model Impact

The reference model requires no new canonical semantic object type.

No change is required to:

```text
KnowledgeElementVersion
RelationshipVersion
ChangeRevision
SemanticState
OntologyVersion
```

Workflow references exist outside canonical semantic storage.

This is a deliberate design choice.

---

# 51. Repository Model Impact

Accepted Repository State remains unchanged.

Potential draft interaction state may gain:

```text
workflow reference bindings
```

but these bindings are not part of accepted SemanticState.

The exact draft storage representation is deferred to `authoring-model.md`.

---

# 52. Empirical Success Criteria

The reference model should be considered successful if a Statit-scale authoring workflow can be performed without requiring the actor to manually:

```text
capture newly generated ElementIds
store them externally
copy them into later Link commands
```

Canonical identities must still be produced and available.

The expected interaction should move from:

```text
create
parse UUID
store UUID
create
parse UUID
store UUID
link UUID UUID
```

toward:

```text
create as R
create as I
link I R
```

without changing the underlying semantic operations.

---

# 53. Open Questions for Authoring Design

The following questions now belong to `authoring-model.md`.

1. How is a workflow reference declared?
2. What lexical forms are allowed?
3. Should references require an explicit marker such as `@`?
4. Are names case-sensitive?
5. Which characters are permitted?
6. Can an existing accepted object be bound to a workflow reference?
7. Can Relationships receive workflow references?
8. How are workflow references persisted between CLI invocations?
9. How are they shown by draft status?
10. Can a reference be rebound?
11. How does batch input declare and use them?
12. How are duplicate declarations handled inside a batch?
13. Should standalone auto-commit mutations support temporary references, or only open Changes?
14. How does reference behavior interact with abort and commit?

---

# 54. Deferred Reference Questions

These are intentionally not required for the initial v0.4 design.

## Persistent Aliases

Deferred until there is evidence that draft-local references are insufficient.

## Title-Based Operation References

Deferred because titles are mutable and potentially ambiguous.

## Artifact Path References

Deferred because physical paths are not stable semantic identity.

## Cross-Repository References

Deferred until collaboration or synchronization semantics exist.

---

# 55. Reference Model Summary

The v0.4 reference model establishes a strict separation:

```text
Canonical Identity
    persistent
    stable
    UUID-based
    semantic

Interaction Reference
    convenient
    deterministic
    scoped
    resolves to canonical identity
```

The reference classes are:

```text
CanonicalReference
PrefixReference
WorkflowReference
```

The principal v0.4 addition is:

```text
WorkflowReference
```

scoped to an active authoring workflow.

This solves the measured UUID-bookkeeping problem without introducing a second persistent identity system.

The resulting authoring principle is:

> Users should be able to refer to semantic knowledge by convenient deterministic references during authoring, while KAT continues to store and evolve that knowledge using stable canonical identities.

---

# 56. Next Specification Stage

The next document is:

```text
authoring-model.md
```

It shall define how the reference model participates in:

```text
Change begin
operation staging
workflow-reference declaration
multi-operation submission
batch authoring
candidate-state construction
failure handling
draft inspection
commit
abort
```

The authoring model shall preserve the existing Change semantics while reducing the interaction cost demonstrated by the Statit graph-construction experiment.

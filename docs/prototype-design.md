# Prototype Design

## Purpose

This document defines the physical and implementation-level design of KAT v0.1.

The prototype is intended to prove the core KAT model:

* Software knowledge is represented independently from source code artifacts.
* Authoritative semantic state evolves through explicit semantic changes.
* Persisted semantic objects are immutable.
* Stable semantic identity is distinct from immutable object identity.
* Semantic States reference immutable object versions.
* Accepted semantic history remains explicitly reachable.
* Traceability, impact analysis, validation, and history operate on semantic knowledge.
* Artifacts remain downstream from authoritative knowledge.

The prototype should remain small enough to validate these ideas without prematurely implementing distributed collaboration, advanced materialization, persistent query databases, or repository optimization.

## v0.1 Goals

KAT v0.1 should demonstrate:

* Local repository initialization
* Persistent semantic knowledge
* Immutable canonical objects
* Stable semantic identities
* Content-addressed immutable versions
* Semantic State construction
* Persistent accepted Change history
* Atomic accepted repository publication
* Semantic mutation through Changes
* Typed relationships
* Ontology validation
* Invariant validation
* Traceability
* Impact analysis
* Semantic history
* Repository persistence across executions

## v0.1 Non-Goals

KAT v0.1 does not require:

* Remote repositories
* Network synchronization
* Distributed collaboration
* Automatic semantic merge
* Persistent conflicted states
* CRDTs
* Artifact generation
* Automatic divergence detection
* Automatic reconciliation
* Persistent query databases
* Graph databases
* Object packing
* Garbage collection
* Repository compression
* AI integration
* Plugin loading
* Architecture-specific modeling

These concerns may be introduced after the semantic repository model has been validated.

# Technology

## Language

KAT v0.1 is implemented in Rust.

Rust is used for:

* Strong representation of semantic types
* Explicit error handling
* Immutable data modeling
* Efficient binary encoding
* Filesystem operations
* CLI development
* Future portability as a standalone executable

The persistent repository format must remain independent from Rust-specific implementation details.

## Canonical Encoding

Canonical KAT objects use CBOR.

The canonical representation follows the deterministic encoding rules defined by the KAT canonical format so that the same logical canonical object produces the same byte representation.

The Object ID is calculated from the complete canonical encoded representation.

Conceptually:

```text
Logical Canonical Object
      |
      v
Deterministic CBOR
      |
      v
Canonical Bytes
      |
      v
SHA-256
      |
      v
Object ID
```

The Rust serialization implementation must conform to the KAT canonical format.

Rust struct layout, enum discriminants, map iteration order, or serializer-specific behavior must not define the repository specification.

## Canonical Schema

The canonical binary structures are specified using CDDL.

Conceptually:

```text
CDDL Schema
     |
     v
KAT Canonical Format
     |
     v
Rust Implementation
```

The normative structural schema is `spec/canonical-format.cddl`.

The normative protocol semantics and encoding rules, including deterministic encoding, ordering, hashing, validation, and compatibility behavior, are defined by `canonical-format.md`.

This document consumes both. Values such as the UUID CBOR tag, envelope field identifiers, object-kind identifiers, operation encodings, and lifecycle values are repeated here only as implementation context. They are defined by the canonical format and must not be independently redefined.

## Repository Metadata

Repository-level configuration uses TOML.

This information is intentionally human-readable because it describes the repository itself rather than immutable semantic objects.

# Identity Model

KAT distinguishes stable semantic identity from immutable object identity.

## Stable Semantic Identity

Stable semantic identities identify concepts that persist through evolution.

UUIDv4 is used for v0.1.

Stable identities include:

* Repository ID
* Software ID
* Knowledge Element ID
* Relationship ID
* Change ID
* Ontology ID

Example:

```text
Requirement

Element ID:
7c8e0c81-...
```

The Element ID remains unchanged while the Requirement evolves.

## Immutable Object Identity

Immutable canonical objects are identified using SHA-256 over their deterministic canonical CBOR encoding.

```text
Object ID =
SHA-256(Canonical CBOR Object)
```

Object IDs identify exact immutable representations.

They are used for:

* Knowledge Element Versions
* Relationship Versions
* Change Revisions
* Semantic States
* Ontology Versions

Example:

```text
Requirement Element ID:
7c8e0c81-...

Version 1:
89d21a...

Version 2:
b74ac3...
```

## Identity Summary

```text
Repository
    stable identity: UUID

Software
    stable identity: UUID

Knowledge Element
    stable identity: UUID
    version identity: SHA-256 Object ID

Relationship
    stable identity: UUID
    version identity: SHA-256 Object ID

Change
    stable identity: UUID
    revision identity: SHA-256 Object ID

Semantic State
    identity: SHA-256 Object ID

Ontology
    stable identity: UUID
    version identity: SHA-256 Object ID
```

Semantic identity must never depend on content.

# Repository Layout

The initial physical repository layout is:

```text
project/
├── .kat/
│   ├── repository.toml
│   │
│   ├── objects/
│   │   └── ...
│   │
│   ├── refs/
│   │   └── accepted
│   │
│   ├── locks/
│   │
│   └── tmp/
│
└── software artifacts...
```

The conceptual repository boundary is independent from this filesystem layout.

The `.kat` directory is the v0.1 physical implementation of that repository.

## repository.toml

`repository.toml` contains repository metadata.

Initial fields:

```toml
format_version = 1

repository_id = "<uuid>"
software_id = "<uuid>"

object_encoding = "cbor-deterministic-v1"
hash_algorithm = "sha256"
```

Dynamic semantic state and accepted history must not be stored in `repository.toml`.

Those belong in repository refs.

## objects

`objects/` contains immutable canonical objects.

Conceptually:

```text
objects/
    <sha256>
    <sha256>
    <sha256>
```

An object is addressed by the SHA-256 hash of its canonical bytes.

The initial implementation may store all objects directly in this directory.

Directory fan-out may be introduced later if object counts require it.

Example future representation:

```text
objects/
    8b/
        7c24ae...
```

This optimization is not required in v0.1.

## refs

`refs/` contains mutable repository references to immutable canonical objects.

For v0.1:

```text
refs/
    accepted
```

The accepted ref identifies both:

* The currently authoritative Semantic State
* The current accepted Change Revision head

Conceptually:

```text
accepted

    state  -> SemanticState ObjectId
    change -> ChangeRevision ObjectId | none
```

For a newly initialized repository:

```text
state  -> S0
change -> none
```

After accepting a Change Revision:

```text
state  -> S1
change -> C1
```

The accepted Semantic State defines authoritative software knowledge.

The accepted Change head identifies the root of accepted semantic history.

Future repository refs may include:

```text
refs/
    accepted
    working/<workspace>
```

Only `accepted` is required for v0.1.

## locks

`locks/` contains temporary repository lock information required for safe mutation and ref publication.

Locks are operational state and are not canonical repository knowledge.

## tmp

`tmp/` contains temporary files used while constructing or publishing immutable objects and refs.

Temporary files do not form part of canonical repository state.

# Canonical Object Envelope

All immutable canonical objects use a common envelope.

Conceptually:

```text
CanonicalObject

envelope_version
object_kind
schema_version
payload
```

The canonical format defines the envelope field identifiers:

```text
0  envelope_version
1  object_kind
2  schema_version
3  payload
```

The Object ID itself is not encoded inside the object.

It is derived from the complete encoded envelope:

```text
Object ID =
SHA-256(
    deterministic_cbor(CanonicalObject)
)
```

This avoids redundant self-identification.

Including `object_kind` in the hashed representation provides domain separation between object types.

## Object Kinds

The canonical object kinds for v0.1 are:

```text
1  KnowledgeElementVersion
2  RelationshipVersion
3  ChangeRevision
4  SemanticState
5  OntologyVersion
```

These identifiers are defined by the canonical format and must not be inferred from Rust enum values.

# Canonical Primitive Representation

UUID values use the canonical representation defined by `spec/canonical-format.cddl`:

```text
CBOR tag 37
containing exactly 16 bytes
```

Object IDs are represented as exactly 32 bytes in canonical CBOR.

Textual Object IDs use the full 64-character lowercase hexadecimal SHA-256 representation.

## Property Values

The canonical property model supports:

```text
null
boolean
integer
text
byte string
UUID
list
map
```

Floating-point values are not supported in v0.1.

Property maps use text keys.

List order is semantic and is preserved.

Property map encoding must follow the deterministic CBOR rules defined by the canonical format.

# Knowledge Element Version

A `KnowledgeElementVersion` represents one immutable version of a knowledge element.

Logical structure:

```text
KnowledgeElementVersion

element_id
type_id
lifecycle
properties
```

## element_id

Stable UUID identifying the semantic knowledge element.

## type_id

Identifies the element type defined by the active ontology.

Examples:

```text
kat.core/intent
kat.core/requirement
kat.core/constraint
kat.core/design-decision
kat.core/implementation
kat.core/artifact
kat.core/validation
```

## lifecycle

The canonical format defines the lifecycle values:

```text
0  active
1  deprecated
2  superseded
```

## properties

Properties store ontology-defined information associated with the element.

Property names use textual keys.

Property values follow the canonical property-value grammar.

## Excluded Information

A Knowledge Element Version does not contain:

* Previous versions
* History
* Incoming relationships
* Outgoing relationships
* Derived query information
* Artifact file contents

Those are available through Semantic States, Relationships, Changes, artifacts, or derived indexes.

# Relationship Version

A `RelationshipVersion` represents one immutable version of a semantic relationship.

Logical structure:

```text
RelationshipVersion

relationship_id
source_element_id
relationship_type
target_element_id
properties
```

## relationship_id

Stable UUID representing the semantic relationship.

## source_element_id

Stable Element ID of the source element.

## relationship_type

Ontology-defined relationship type.

Examples:

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

## target_element_id

Stable Element ID of the target element.

Relationships reference semantic identities rather than specific Knowledge Element Version IDs.

The active Semantic State determines which versions of those identities are current.

## properties

Optional relationship-specific semantic properties.

The initial ontology may leave this empty for most relationships.

# Ontology Version

An `OntologyVersion` represents one immutable version of the repository ontology.

Logical structure:

```text
OntologyVersion

ontology_id
element_types
relationship_types
```

## ontology_id

Stable UUID identifying the ontology across its evolution.

## element_types

Defines available knowledge element types.

Each type contains:

```text
type_id
name
```

Example:

```text
type_id:
kat.core/requirement

name:
Requirement
```

Element type definitions are canonically sorted by `type_id`.

Duplicate `type_id` values are invalid.

## relationship_types

Defines relationship semantics.

Each relationship type contains:

```text
type_id
name
allowed_source_types
allowed_target_types
```

Example:

```text
type:
kat.core/addresses

source:
kat.core/design-decision

target:
kat.core/requirement
```

Relationship type definitions are canonically sorted by `type_id`.

Allowed source and target type lists are sorted lexicographically and contain no duplicates.

## Ontology Extensions

The ontology model is intended to support future extensions.

KAT v0.1 does not define a separate canonical extension structure.

Repository-specific types may be represented through normal ontology element and relationship type definitions where appropriate.

A more explicit extension mechanism is deferred until its requirements are established.

# Semantic State

A `SemanticState` represents one immutable composition of software knowledge.

Logical structure:

```text
SemanticState

ontology_version

elements:
    ElementId -> KnowledgeElementVersionId

relationships:
    RelationshipId -> RelationshipVersionId
```

The Semantic State answers:

> What software knowledge is active in this state?

It does not describe how that state was reached.

## ontology_version

Object ID of the Ontology Version used to interpret and validate the state.

## elements

Logical mapping from stable Element IDs to immutable Knowledge Element Version Object IDs.

The canonical physical representation is a sorted array of entries:

```text
[
    ElementId,
    KnowledgeElementVersion ObjectId
]
```

Entries are sorted lexicographically by the raw 16-byte Element ID.

Duplicate Element IDs are invalid.

## relationships

Logical mapping from stable Relationship IDs to immutable Relationship Version Object IDs.

The canonical physical representation is a sorted array of entries:

```text
[
    RelationshipId,
    RelationshipVersion ObjectId
]
```

Entries are sorted lexicographically by the raw 16-byte Relationship ID.

Duplicate Relationship IDs are invalid.

## Immutability

Once a Semantic State exists, it is never modified.

Evolution produces another Semantic State.

```text
S1 + Change -> S2
```

## Structural Sharing

Unchanged object versions are reused between states.

Example:

```text
State S1

req-1 -> R1
dec-1 -> D1
imp-1 -> I1


State S2

req-1 -> R2
dec-1 -> D1
imp-1 -> I1
```

Only `R2` and the new Semantic State require new canonical objects.

## Physical Representation

v0.1 encodes complete identity-to-version mappings in each Semantic State.

This remains compatible with structural sharing because referenced immutable objects are reused.

Persistent trees, Merkle trees, or chunked manifests are deferred until repository scale justifies them.

# Change Revision

A `ChangeRevision` represents one immutable revision of a meaningful software Change.

Logical structure:

```text
ChangeRevision

change_id
base_states
result_state
operations
dependencies
description
```

## change_id

Stable UUID identifying the logical Change.

Multiple local revisions may share the same Change ID.

## base_states

Semantic States from which the Change Revision was constructed.

For normal v0.1 changes:

```text
base_states:
    [S0]
```

The canonical format permits one or more base states for future collaborative reconciliation.

KAT v0.1 executes only single-base Changes.

If multiple base states are present, they are canonically sorted by Object ID until a future schema defines semantic ordering.

## result_state

Semantic State produced by successfully applying the Change Revision.

The resulting Semantic State does not reference the Change Revision.

This avoids cyclic content-addressed object dependencies.

## operations

Ordered semantic operations comprising the Change Revision.

Operation order is semantically meaningful and must be preserved.

The canonical format defines the initial operations:

```text
1  CreateElement
2  UpdateElement
3  DeprecateElement
4  Link
5  Unlink
6  Supersede
```

## dependencies

References to immutable Change Revision Object IDs on which the Change semantically depends.

Dependencies represent causality rather than arbitrary chronological order.

They are canonically sorted by Object ID and contain no duplicates.

## description

Optional human-readable description of the semantic Change.

The description is part of the immutable Change Revision if present and therefore participates in its Object ID.

Incidental runtime metadata should not be stored in canonical Changes unless it is intentionally part of historical meaning.

# Operation Representation

Operations are values contained inside Change Revisions.

They are not separate canonical objects in v0.1.

Operations are represented using canonical tagged arrays.

## CreateElement

Canonical shape:

```text
[
    1,
    new_version
]
```

`new_version` references a Knowledge Element Version object.

The Element ID contained by the referenced version must not already be active in the base Semantic State.

## UpdateElement

Canonical shape:

```text
[
    2,
    element_id,
    expected_version,
    new_version
]
```

`expected_version` acts as a deterministic precondition.

The operation succeeds normally only when the base state currently maps the Element ID to that Object ID.

## DeprecateElement

Canonical shape:

```text
[
    3,
    element_id,
    expected_version,
    new_version
]
```

The new Knowledge Element Version must have lifecycle `deprecated`.

## Link

Canonical shape:

```text
[
    4,
    new_relationship_version
]
```

The referenced Relationship Version must satisfy ontology rules.

Its Relationship ID must not already be active in the base Semantic State.

## Unlink

Canonical shape:

```text
[
    5,
    relationship_id,
    expected_version
]
```

The relationship is removed from the resulting Semantic State while its historical object remains in the canonical object store.

## Supersede

Supersede remains an explicit operation because it carries semantic lifecycle meaning.

Canonical shape:

```text
[
    6,
    existing_element,
    expected_existing_version,
    replacement_element,
    replacement_version,
    superseding_relationship
]
```

Semantic validation must verify that:

* `existing_element` resolves to `expected_existing_version`
* `replacement_version` belongs to `replacement_element`
* `replacement_element` is not already active
* The superseding relationship links the expected elements
* The relationship type is valid according to the active ontology
* The resulting lifecycle state satisfies KAT invariants

# Preconditions and Postconditions

Common preconditions and postconditions are implied by operation semantics rather than redundantly stored.

Example:

```text
UpdateElement

element_id: E
expected_version: V1
new_version: V2
```

implies:

```text
Precondition:
E resolves to V1 in the base state.

Postcondition:
E resolves to V2 in the resulting state.
```

Additional explicit condition objects are not introduced in v0.1 unless a requirement cannot be expressed through operation semantics and repository invariants.

# Change Application Flow

Mutation follows one controlled path.

```text
User Command
    |
    v
Application Layer
    |
    v
Change Engine
```

The Change Engine performs:

1. Resolve `refs/accepted`.
2. Load the accepted Semantic State and accepted Change head.
3. Select the base Semantic State.
4. Build the proposed semantic operations.
5. Evaluate operation preconditions.
6. Apply operations to a candidate state.
7. Create any new Knowledge Element Versions.
8. Create any new Relationship Versions.
9. Construct the candidate Semantic State.
10. Validate ontology conformance.
11. Evaluate operation postconditions.
12. Evaluate required invariants.
13. Persist all new canonical objects required by the candidate state.
14. Create and persist the Change Revision referencing the resulting Semantic State.
15. Atomically publish both the candidate Semantic State and the new Change Revision head through `refs/accepted`.

Conceptually:

```text
accepted:

    state  -> S0
    change -> C0

        |
        v

      Change
        |
        v

Candidate S1
    |
    +--> ontology valid?
    +--> postconditions valid?
    +--> invariants valid?
    |
    v

Persist immutable objects
    |
    v
Persist Change Revision C1
    |
    v
Publish accepted ref
    |
    v

accepted:

    state  -> S1
    change -> C1
```

For the initial repository, `C0` is `none`.

No canonical historical object is modified during this process.

# Atomic Publication

The accepted repository state is published using compare-and-swap semantics.

Conceptually:

```text
compare_and_swap(
    ref = accepted,

    expected = {
        state: S0,
        change: C0
    },

    new = {
        state: S1,
        change: C1
    }
)
```

The operation succeeds only if both accepted values still match the expected repository state.

This prevents lost updates when multiple KAT processes attempt to publish Changes concurrently.

The accepted repository must maintain the invariant:

```text
accepted.change.result_state == accepted.state
```

whenever `accepted.change` is present.

## Physical Flow

The v0.1 filesystem implementation should conceptually perform:

```text
1. Acquire accepted-ref lock.

2. Read refs/accepted.

3. Verify:
       current.state == expected.state
       current.change == expected.change

4. Write the new accepted-ref contents to a temporary file.

5. Flush the temporary ref contents.

6. Atomically replace refs/accepted.

7. Release the lock.
```

The platform-specific implementation remains behind a dedicated reference-storage abstraction.

# Immutable Object Writes

Canonical objects must never be modified after publication.

Writing an object follows:

```text
Encode canonical object
    |
    v
Calculate SHA-256
    |
    v
Write temporary object
    |
    v
Flush
    |
    v
Publish at objects/<hash>
```

If the object already exists with the same Object ID, no new object needs to be written.

An existing canonical object must never be overwritten with different bytes.

When loading an object, KAT verifies:

```text
SHA-256(object bytes)
    ==
requested Object ID
```

A mismatch is a repository integrity error.

# Failed Changes

If a Change fails before accepted repository publication:

```text
accepted

    state  -> old state
    change -> old Change head
```

remains unchanged.

Some newly written immutable objects may remain unreferenced.

These may include:

* Knowledge Element Versions
* Relationship Versions
* Semantic States
* Change Revisions

This does not make the repository semantically invalid.

Unreferenced objects may later be collected by a garbage collector.

Garbage collection is outside v0.1 scope.

# Repository Initialization

`kat init` creates a new repository.

Initialization should:

1. Verify that no existing KAT repository is being overwritten.
2. Generate a Repository ID.
3. Generate a Software ID.
4. Create repository directories.
5. Write `repository.toml`.
6. Create the initial core Ontology Version.
7. Create an empty Semantic State referencing that ontology.
8. Persist both canonical objects.
9. Set `refs/accepted` to the initial Semantic State with no accepted Change head.

Conceptually:

```text
kat init
    |
    v
Core Ontology O1
    |
    v
Empty State S0
    |
    v

accepted

    state  -> S0
    change -> none
```

# Repository Open

Opening a repository performs lightweight integrity checks.

Initial checks include:

* `.kat/repository.toml` exists
* Repository format is supported
* `refs/accepted` exists
* Accepted State Object ID is syntactically valid
* Accepted Change Revision Object ID is syntactically valid when present
* Accepted Semantic State object exists
* Accepted State object hash is valid
* Accepted Change Revision exists when present
* Accepted Change Revision object hash is valid when present
* Accepted Change Revision `result_state` equals the accepted Semantic State
* Referenced Ontology Version exists
* Referenced Knowledge Element Versions exist
* Referenced Relationship Versions exist
* Referenced object kinds match expected kinds

Full semantic validation is not required every time the repository is opened.

# In-Memory Projection

v0.1 does not require a persistent query database.

When a repository is opened, KAT may build in-memory projections from the accepted repository state and canonical objects.

Possible indexes include:

```text
ElementId -> KnowledgeElementVersion

ElementType -> ElementIds

ElementId -> outgoing RelationshipIds

ElementId -> incoming RelationshipIds

RelationshipType -> RelationshipIds

Artifact path -> Artifact ElementId

StateId -> ChangeRevisionId

ChangeId -> ChangeRevisionIds

ChangeRevisionId -> causal dependencies

ChangeRevisionId -> base StateIds
```

These projections are derived and disposable.

They must be reconstructable from canonical repository objects and accepted repository roots.

# Query Flow

Read-only operations do not modify repository state.

Example:

```text
CLI
 |
 v
Application Layer
 |
 v
Query Engine
 |
 +--> In-Memory Projection
 |
 +--> Semantic Repository
 |
 v
Result
```

Initial query operations include:

* Trace
* Impact
* Explain
* History
* Semantic diff

# Trace

Trace traverses semantic relationships forward or backward.

It operates on relationship semantics rather than physical object references alone.

Example:

```text
Implementation
    ^
    |
Design Decision
    ^
    |
Requirement
    ^
    |
Intent
```

# Impact

Impact begins from a selected semantic element and traverses relevant relationships to identify potentially affected knowledge.

It may distinguish:

* Direct impact
* Indirect impact
* Artifact impact

Impact does not automatically declare affected elements invalid.

# Explain

Explain combines available semantic relationships and accepted history to show why knowledge exists and how it relates to the software system.

It does not generate new authoritative knowledge.

# History

History is reconstructed from:

* Accepted Change head
* Change Revisions
* Semantic States
* Stable semantic identities
* Change dependencies
* Base-state relationships

A separate canonical `History` object is not required.

The accepted Change head provides the root of accepted semantic history.

Conceptually:

```text
accepted.change -> C2
accepted.state  -> S2

C2
 |
 +--> base state: S1
 +--> result state: S2
```

A simple history may appear as:

```text
S0 --C1--> S1 --C2--> S2
```

Change dependencies may also form a causal graph rather than a strictly linear history.

Change Revisions that exist in the object store but are not reachable from accepted or working history roots are not automatically considered accepted history.

History queries may use derived in-memory indexes for traversal.

# Semantic Diff

Semantic diff is derived from two Semantic States.

The initial algorithm compares:

```text
ElementId -> ObjectId mappings
RelationshipId -> ObjectId mappings
```

Possible results include:

```text
Element added
Element removed from active state
Element version changed

Relationship added
Relationship removed
Relationship version changed
```

Semantic diff is a query result.

It is not the canonical representation of Change history.

# Ontology Validation

Before a candidate Semantic State becomes accepted, KAT validates:

* Knowledge Element types exist
* Relationship types exist
* Relationship source types are allowed
* Relationship target types are allowed
* Required ontology structure is satisfied

Ontology validation operates on semantic meaning rather than storage layout.

# Invariant Validation

After operations are applied, the candidate state must satisfy all required invariants.

Initial invariant groups include:

* Identity
* Relationship
* Lifecycle
* Change
* Traceability
* Authority
* Validation
* History

Repository-specific invariants may be introduced as the prototype evolves.

If a required invariant fails:

```text
candidate state rejected

accepted:
    state  -> unchanged
    change -> unchanged
```

# CLI Surface

The initial CLI should remain small and map directly to established operations and use cases.

## Repository

```text
kat init
kat status
```

## Knowledge Mutation

```text
kat create
kat update
kat deprecate
kat supersede
kat link
kat unlink
```

The exact argument syntax should be designed separately from semantic behavior.

## Query

```text
kat trace
kat impact
kat explain
kat history
```

## Validation

```text
kat validate
```

## Debug / Repository Inspection

Low-level inspection commands are useful for prototype development.

Possible commands:

```text
kat show <semantic-id>
kat object show <object-id>
kat state show <state-id>
```

These are tooling concerns and do not define semantic operations.

# Rust Module Structure

A possible v0.1 module organization is:

```text
src/
├── main.rs
│
├── cli/
│   └── mod.rs
│
├── application/
│   ├── mod.rs
│   ├── commands.rs
│   └── queries.rs
│
├── domain/
│   ├── mod.rs
│   ├── identity.rs
│   ├── element.rs
│   ├── relationship.rs
│   ├── ontology.rs
│   ├── operation.rs
│   ├── change.rs
│   └── state.rs
│
├── repository/
│   ├── mod.rs
│   ├── repository.rs
│   ├── object_store.rs
│   ├── ref_store.rs
│   └── metadata.rs
│
├── encoding/
│   ├── mod.rs
│   ├── cbor.rs
│   └── hash.rs
│
├── engine/
│   ├── mod.rs
│   ├── change.rs
│   ├── query.rs
│   ├── ontology.rs
│   └── validation.rs
│
└── index/
    ├── mod.rs
    └── memory.rs
```

This structure is illustrative.

Module boundaries should follow responsibilities rather than becoming part of the persistent repository specification.

# Core Rust Types

Semantic identifiers should use distinct Rust wrapper types.

Conceptually:

```rust
struct RepositoryId(Uuid);
struct SoftwareId(Uuid);

struct ElementId(Uuid);
struct RelationshipId(Uuid);
struct ChangeId(Uuid);
struct OntologyId(Uuid);

struct ObjectId([u8; 32]);
```

Using separate wrapper types prevents accidental interchange of semantic identifiers.

Content-addressed version identities use `ObjectId`.

For example:

```rust
type ElementVersionId = ObjectId;
type RelationshipVersionId = ObjectId;
type ChangeRevisionId = ObjectId;
type StateId = ObjectId;
type OntologyVersionId = ObjectId;
```

These aliases may later become stronger wrapper types if additional type safety is useful.

A repository ref should be represented explicitly.

Conceptually:

```rust
struct AcceptedRef {
    state: StateId,
    change: Option<ChangeRevisionId>,
}
```

# Error Categories

The prototype should distinguish between major error classes.

Initial categories include:

```text
RepositoryError
EncodingError
IntegrityError
OntologyError
InvariantError
PreconditionError
ConflictError
RefUpdateError
QueryError
```

This distinction is important because:

```text
invalid CBOR
```

is not the same problem as:

```text
invalid relationship semantics
```

or:

```text
accepted ref changed concurrently
```

Error categories are introduced in phases rather than designed up front:

* Phase 0 (substrate): `EncodingError`, `IntegrityError`, `RepositoryError`, `RefUpdateError`.
* Phase 1 (first semantic slice): `OntologyError`, `InvariantError`, `PreconditionError`.
* Later phases: `ConflictError`, `QueryError`.

Core domain, engine, and storage APIs expose typed errors. Convenience error types such as `anyhow::Error` may be used at the CLI or application boundary but must not appear in the domain, engine, or storage APIs.

# Validation Layers

The implementation should distinguish three levels of correctness.

## Encoding Validity

Question:

> Is this object valid according to the canonical binary schema?

Includes:

* Correct CBOR structure
* Supported envelope version
* Supported object kind
* Supported schema version
* Required fields present
* Field types valid
* Canonical collection ordering valid
* Duplicate canonical entries absent

## Repository Integrity

Question:

> Is the repository structurally intact?

Includes:

* Object hash matches contents
* Referenced objects exist
* Expected object kinds match
* Accepted ref identifies a valid Semantic State
* Accepted Change head identifies a valid Change Revision when present
* Accepted Change Revision `result_state` matches the accepted Semantic State

## Semantic Validity

Question:

> Is this state valid according to KAT semantics?

Includes:

* Ontology conformance
* Invariants
* Lifecycle rules
* Change semantics
* Repository-specific validation rules

These validation layers must remain separate.

# Canonical Object Decoding

Canonical object decoding should fail closed.

The implementation should reject:

* Unsupported envelope versions
* Unsupported schema versions
* Unknown required object kinds
* Object-kind and payload mismatches
* Malformed UUIDs
* Malformed Object IDs
* Missing required fields
* Duplicate required fields
* Invalid canonical ordering
* Duplicate semantic identifiers in set-like structures
* Invalid references
* Hash mismatches

Canonical repository data should not be silently repaired or interpreted through best-effort parsing.

# Determinism

Given identical:

```text
Canonical logical object
Canonical schema
Canonical encoding rules
```

the implementation must produce identical bytes and therefore the same Object ID.

Given identical:

```text
Base Semantic State
Semantic operations
Ontology
Invariants
```

the Change Engine should produce the same resulting semantic state.

Non-deterministic behavior must not participate in core semantic validity.

# Future Content Transfer

Content-addressed objects provide a future basis for repository exchange.

A repository may eventually identify missing objects by their Object IDs and transfer only objects not already present.

This capability is not implemented in v0.1 but should not require changes to canonical object identity.

# Future Persistent Index

A future implementation may introduce a rebuildable local database such as SQLite for query acceleration.

Conceptually:

```text
Canonical Object Store
        |
        v
Rebuild
        |
        v
index.db
```

The database must remain derived.

Deleting it must not remove semantic knowledge or accepted history.

# Future State Structure

The v0.1 Semantic State contains flat sorted mappings of semantic identities to Object IDs.

If repository size makes this inefficient, the physical representation may later evolve toward:

* Persistent trees
* Merkle trees
* Chunked manifests
* Other structurally shared representations

The logical Semantic State model must remain unchanged.

# Deferred Object Types

The following concepts are not canonical object types in v0.1:

* Operation
* History
* Validation cache
* Semantic diff
* Conflict
* Reconciliation
* Materialization record
* Participant
* Permission
* Query index

They should become canonical only if future requirements demonstrate that their meaning cannot be reconstructed from existing canonical information.

# Implementation Strategy

Implementation proceeds through vertical milestones.

A preliminary repository substrate may be implemented first where required to support the initial vertical milestone. This substrate is limited to the canonical encoding, object persistence, repository references, initialization, and integrity mechanisms required by that milestone.

The substrate is not considered a successful prototype milestone by itself.

The first semantic milestone is complete only when KAT can create a knowledge element through a Change, construct and validate a new Semantic State, publish the accepted State and Change head atomically, and inspect the resulting knowledge and history.

## Phase 0: Canonical Repository Substrate

The substrate proves the repository-format mechanics. It is not a successful prototype milestone by itself.

1. Rust project / module skeleton
2. Identity types (UUID semantic IDs, `ObjectId([u8; 32])`)
3. Canonical domain structures matching `spec/canonical-format.cddl`
4. Deterministic CBOR implementation (RFC 8949 core deterministic profile)
5. Golden and negative format vectors
6. SHA-256 object identity
7. Immutable ObjectStore
8. RepositoryMetadata
9. AcceptedRef and compare-and-swap abstraction
10. `kat init` (OntologyVersion O1, empty SemanticState S0, accepted = {S0, none})
11. Repository open and integrity checks

Phase 0 must not add object fan-out, packfiles, garbage collection, compression, persistent indexes, verify-every-object-on-every-read, or remote exchange. Those are deferred and must not change semantic behavior when introduced later.

## Phase 1: First Semantic Vertical Slice

The first semantic milestone proves the semantic-evolution hypothesis and is complete only when all of the following work end to end.

12. `CreateElement`
13. Minimal ontology validation
14. Candidate SemanticState S1
15. Minimal required invariant validation
16. ChangeRevision C1
17. Persist objects
18. CAS publish {S0, none} -> {S1, C1}
19. `kat show`
20. `kat history`

# Initial Implementation Milestone

The first useful vertical slice should prove:

```text
kat init

        |
        v

Create Requirement

        |
        v

Immutable Requirement Version

        |
        v

Semantic State S1

        |
        v

Validate

        |
        v

Change Revision C1

        |
        v

Atomic publication

        |
        v

accepted
    |
    +--> state  -> S1
    |
    +--> change -> C1

        |
        v

kat show / kat history
```

The next slice should add:

```text
Create Design Decision
        |
        v
Link Decision -> Requirement
        |
        v
Trace Origin
        |
        v
Impact Analysis
```

The prototype should grow vertically through complete semantic behavior rather than implementing every subsystem independently before any useful workflow exists.

# Core Prototype Decisions

KAT v0.1 uses:

* Rust as the implementation language.
* CDDL as the normative canonical schema language.
* Deterministic CBOR as the canonical binary encoding.
* Canonical UUID representation as defined by the canonical format (CBOR tag 37, 16-byte payload).
* UUIDv4 for stable semantic identities.
* SHA-256 for immutable canonical Object IDs.
* Immutable files as canonical object persistence.
* Content-addressed object storage.
* A mutable `accepted` ref containing the authoritative Semantic State and accepted Change head.
* Compare-and-swap semantics for atomic accepted Semantic State and Change-head publication.
* Flat sorted Semantic State manifests for v0.1.
* Structural sharing of immutable referenced objects.
* In-memory derived indexes.
* No persistent database initially.
* No graph database.
* Explicit ontology and invariant validation before publication.
* A single controlled Change Engine for authoritative mutation.
* A canonical object set consisting of Knowledge Element Version, Relationship Version, Change Revision, Semantic State, and Ontology Version.

# Open Questions

The following implementation questions remain intentionally unresolved:

* Which Rust CBOR library best supports the required deterministic profile?
* Should timestamps exist in Change Revisions?
* What metadata belongs in canonical Changes versus non-canonical UI information?
* Should Relationship lifecycle be explicit or represented only through Semantic State membership?
* How should repository-specific invariants be encoded?
* What explicit ontology extension mechanism, if any, is needed beyond normal ontology definitions?
* How should lock recovery work after a crashed process?
* Which filesystem synchronization guarantees are required on each supported platform?
* When should object-directory fan-out be introduced?
* When should full object verification occur?
* How should abbreviated Object IDs be resolved safely?
* Which CLI syntax should be used for element properties and relationship creation?

# Constraint Validation Semantics (v0.1 Clarification)

For KAT v0.1, consistency rules encoded by KAT's ontology and semantic invariants are mechanically evaluated. Constraint knowledge elements that do not have executable semantics are reported as unverified rather than assumed satisfied or violated.


# Canonical Format

## Purpose

This document defines the canonical repository format used by KAT to persist immutable semantic objects.

The canonical format must preserve the meaning, integrity, and reproducibility of KAT repository objects independently from the implementation language.

The format defines:

* Canonical object framing
* Binary encoding rules
* Object kinds
* Identity and hashing rules
* Primitive value representation
* Ordering requirements
* Object validation
* Schema versioning
* Compatibility behavior
* Diagnostic representation

The complete structural schema is defined separately in `spec/canonical-format.cddl`.

## Normative Authority

The canonical repository format is specified by two normative documents.

```text
canonical-format.md
    normative protocol semantics and encoding rules

spec/canonical-format.cddl
    normative structural schema
```

`canonical-format.md` defines the protocol semantics, encoding rules, identity and hashing rules, ordering requirements, validation layers, and compatibility behavior of canonical KAT objects.

`spec/canonical-format.cddl` defines the structural schema of canonical objects using CDDL. It is the normative source for field structure, field identifiers, object kinds, operation encodings, and the property-value grammar.

Both documents are normative. Details specified by the CDDL schema are not restated as open questions by this document.

Other KAT specifications, such as `prototype-design.md`, may summarize canonical format values for implementation context but must not independently redefine them.

## Format Goals

The canonical format should be:

* Deterministic
* Compact
* Implementation-independent
* Verifiable
* Evolvable
* Suitable for immutable content-addressed storage
* Precise enough for independent implementations to produce identical object identities

The canonical format must not depend on Rust struct layout, Serde defaults, filesystem paths, or internal implementation details.

# Normative Representation

Canonical KAT objects use CBOR as their binary representation.

Their structure is defined using CDDL in `spec/canonical-format.cddl`.

Conceptually:

```text
Logical KAT Object
        |
        v
CDDL-defined structure
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

The CDDL schema defines structural validity.

This document defines additional semantic and encoding rules that are not completely expressible through CDDL.

# Canonical Object Identity

Every immutable canonical object is identified by the SHA-256 digest of its complete canonical encoded representation.

```text
ObjectId =
    SHA-256(CanonicalObjectBytes)
```

An Object ID represents one exact immutable representation.

Changing any canonical part of the object produces a different Object ID.

The Object ID is not stored inside the encoded object.

## Binary Representation

An Object ID is a 32-byte SHA-256 digest.

Conceptually:

```text
object-id = bytes .size 32
```

## Text Representation

When displayed or stored in text files, Object IDs use lowercase hexadecimal encoding.

The full representation contains 64 hexadecimal characters.

Example:

```text
8b7c24ae...
```

Abbreviated Object IDs may be supported by user interfaces but are not canonical identifiers.

# Semantic Identity

Content-addressed Object IDs do not replace stable semantic identities.

KAT distinguishes:

```text
Semantic Identity
    identifies the conceptual entity

Object Identity
    identifies one immutable representation
```

For example:

```text
Requirement Element ID:
    UUID

Requirement Version:
    SHA-256 Object ID
```

The canonical format therefore contains stable semantic IDs inside objects where required.

Stable semantic identities use UUIDv4 in KAT v0.1.

# UUID Representation

UUIDs are encoded in canonical CBOR as exactly 16 bytes.

The canonical representation must distinguish UUID values from arbitrary byte strings.

The exact tagged representation is defined by `spec/canonical-format.cddl`: CBOR tag 37 containing exactly 16 bytes.

When shown to users or stored in human-readable repository metadata, UUIDs use the standard textual UUID representation.

Example:

```text
7c8e0c81-b9fc-4c31-9974-b9db8fa72e51
```

# Canonical Object Envelope

Every canonical object uses a common envelope.

Conceptually:

```text
CanonicalObject

envelope_version
object_kind
schema_version
payload
```

The envelope is part of the hashed canonical representation.

The Object ID is therefore sensitive to:

* Envelope version
* Object kind
* Schema version
* Payload

## Envelope Version

`envelope_version` defines the common framing used by canonical KAT objects.

KAT v0.1 uses:

```text
envelope_version = 1
```

Future envelope versions may introduce incompatible framing changes.

An implementation must reject unsupported envelope versions.

## Object Kind

`object_kind` identifies the type of canonical object contained in the payload.

Initial object kinds are:

```text
1  KnowledgeElementVersion
2  RelationshipVersion
3  ChangeRevision
4  SemanticState
5  OntologyVersion
```

Object-kind identifiers are permanent protocol values.

An identifier must not later be reused for a different object kind.

## Schema Version

`schema_version` identifies the payload schema version for the selected object kind.

Schema versions evolve independently for each object kind.

For example:

```text
KnowledgeElementVersion schema 2

SemanticState schema 1
```

may coexist in the same repository.

Changing one object's payload format does not require changing all other object schemas.

## Payload

`payload` contains the object-kind-specific canonical structure.

The payload must match the declared `object_kind`.

For example:

```text
object_kind = 1
```

must contain a valid `KnowledgeElementVersion` payload.

An object whose kind and payload do not correspond is invalid.

# Deterministic CBOR Requirements

Canonical KAT objects must use deterministic CBOR encoding following the RFC 8949 Core Deterministic Encoding Requirements (RFC 8949, Section 4.2.1).

Two conforming implementations encoding the same logical canonical object must produce identical bytes.

The following rules apply.

## Definite Lengths

Indefinite-length encoding must not be used.

This applies to:

* Arrays
* Maps
* Byte strings
* Text strings

## Integer Encoding

Integers must use the shortest valid CBOR representation.

## Floating-Point Values

Floating-point property values are not supported by the KAT v0.1 canonical property model.

This avoids ambiguity around representation, equality, and canonicalization.

A later schema version may introduce floating-point values if a concrete semantic requirement justifies them.

## Text

Text values must contain valid UTF-8.

No implicit text normalization is performed by the canonical encoder unless explicitly defined by a future schema.

Therefore semantically equivalent but byte-distinct Unicode strings may produce different Object IDs.

## Map Ordering

CBOR maps used by canonical objects follow the RFC 8949 Core Deterministic Encoding Requirements (RFC 8949, Section 4.2.1).

Map keys are ordered by the bytewise lexicographic comparison of their deterministic encoded forms.

For example, the following keys appear in this deterministic order:

```text
10
100
-1
"z"
"aa"
```

because ordering is based on the encoded bytes rather than on encoded length. In particular, this is not the length-first alternative defined for RFC 7049 Canonical CBOR compatibility (RFC 8949, Section 4.2.3).

However, semantic collections whose logical meaning is a set should normally use explicitly sorted arrays rather than relying on map ordering alone.

## Unknown Fields

Canonical encoders must not emit fields that are not defined by the active object schema.

Canonical decoders must reject unsupported required structure rather than silently guessing its meaning.

# Canonical Collections

Some KAT structures are logically sets or maps where ordering is not semantically meaningful.

Their physical canonical representation must nevertheless have a unique order.

For such collections, this specification defines explicit sorting rules.

## UUID Ordering

Collections sorted by semantic identity use lexicographic ordering over the raw 16 UUID bytes.

## Object ID Ordering

Collections sorted by Object ID use lexicographic ordering over the raw 32 digest bytes.

## Text Identifier Ordering

Collections sorted by ontology identifiers use lexicographic ordering over their UTF-8 encoded bytes.

## Duplicate Values

Canonical set-like collections must not contain duplicate identities or identifiers.

Duplicates make the object invalid.

# Property Values

Knowledge Element and Relationship properties use a restricted canonical value model.

KAT v0.1 supports:

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

Lists preserve order.

Maps use text keys.

Nested values may use the same supported value types.

Floating-point numbers are excluded from v0.1.

## Property Map Keys

Property maps use textual keys.

Property keys are ontology-defined or application-defined semantic names.

Examples:

```text
title
description
priority
path
status
```

Property key ordering must follow deterministic CBOR map ordering.

# Knowledge Element Version

A `KnowledgeElementVersion` represents one immutable state of a knowledge element.

Its logical fields are:

```text
element_id
type_id
lifecycle
properties
```

## element_id

Stable UUID identifying the knowledge element across its evolution.

## type_id

Textual ontology identifier defining the element's semantic type.

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

KAT v0.1 defines the following lifecycle states:

```text
0  active
1  deprecated
2  superseded
```

These numeric values are canonical protocol identifiers and must not be reassigned.

## properties

A map of semantic properties associated with this element version.

The meaning and validity of properties may be further constrained by the active ontology.

## Excluded Fields

A Knowledge Element Version does not contain:

* Previous version pointers
* History
* Incoming relationships
* Outgoing relationships
* Query indexes
* Artifact file contents

Those concerns are represented elsewhere or derived.

# Relationship Version

A `RelationshipVersion` represents one immutable state of a typed semantic relationship.

Its logical fields are:

```text
relationship_id
source_element_id
relationship_type
target_element_id
properties
```

## relationship_id

Stable UUID identifying the relationship across its evolution.

## source_element_id

Stable UUID of the relationship source.

## relationship_type

Ontology-defined textual relationship identifier.

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

Stable UUID of the relationship target.

Relationships reference stable semantic identities rather than immutable Knowledge Element Version IDs.

The active Semantic State determines which element versions are current.

## properties

Optional semantic properties of the relationship.

The active ontology determines whether a relationship type permits or requires particular properties.

# Semantic State

A `SemanticState` represents one immutable composition of software knowledge.

Its logical fields are:

```text
ontology_version
elements
relationships
```

The state answers:

> What semantic knowledge is active in this state?

It does not describe how that state was reached.

## ontology_version

Object ID of the Ontology Version used to interpret and validate the state.

## elements

Logical mapping:

```text
ElementId -> KnowledgeElementVersion ObjectId
```

Physically, KAT v0.1 encodes this mapping as a sorted array of entries.

Each entry contains:

```text
[
    ElementId,
    KnowledgeElementVersion ObjectId
]
```

Entries must be sorted by raw Element ID bytes.

Duplicate Element IDs are invalid.

## relationships

Logical mapping:

```text
RelationshipId -> RelationshipVersion ObjectId
```

Physically, this is also encoded as a sorted array.

Entries must be sorted by raw Relationship ID bytes.

Duplicate Relationship IDs are invalid.

## State Identity

The Semantic State Object ID is the SHA-256 digest of the complete canonical Semantic State object.

Any change to:

* Active element versions
* Active relationship versions
* Ontology version

produces a different Semantic State Object ID.

## Historical Information

Semantic States do not contain:

* Parent states
* Originating changes
* Change descriptions
* Author information
* History indexes

History belongs to Change Revisions and derived history structures.

# Ontology Version

An `OntologyVersion` represents one immutable version of an ontology.

Its logical fields are:

```text
ontology_id
element_types
relationship_types
```

## ontology_id

Stable UUID identifying the ontology across its evolution.

## Element Type Definitions

Each element type definition contains at least:

```text
type_id
name
```

Element type definitions must be sorted lexicographically by `type_id`.

Duplicate `type_id` values are invalid.

## Relationship Type Definitions

Each relationship type definition contains:

```text
type_id
name
allowed_source_types
allowed_target_types
```

Relationship definitions must be sorted lexicographically by `type_id`.

`allowed_source_types` and `allowed_target_types` represent semantic sets.

They must:

* Be sorted lexicographically by type identifier
* Contain no duplicates

## Ontology Extensions

KAT v0.1 may reserve structural space for ontology extensions.

The extension mechanism is not considered stable until explicitly specified.

Core ontology objects must not embed architecture-specific assumptions.

# Change Revision

A `ChangeRevision` represents one immutable revision of a meaningful semantic change.

Its logical fields are:

```text
change_id
base_states
result_state
operations
dependencies
description
```

## change_id

Stable UUID identifying the logical Change.

Different immutable revisions may share the same Change ID while work remains mutable before final acceptance.

## base_states

One or more Semantic State Object IDs on which the Change Revision is based.

KAT v0.1 executes only single-base changes.

The canonical structure permits multiple base states to preserve compatibility with future collaborative reconciliation.

If multiple base states are present, their order is semantically significant only if explicitly defined by a future schema.

Until then, multiple base states must use canonical Object ID ordering.

## result_state

Semantic State Object ID produced by the Change Revision.

A Semantic State must not contain a reverse canonical reference to its originating Change Revision.

This prevents cyclic content-addressed object dependencies.

## operations

Ordered list of semantic operations comprising the Change Revision.

Operation order is semantically meaningful and must be preserved exactly.

Operations must not be sorted.

## dependencies

Immutable Change Revision Object IDs on which the change causally depends.

Dependencies represent a semantic set.

They must be sorted by Object ID and must not contain duplicates.

## description

Optional human-readable description of the semantic change.

If present, the description is part of the immutable Change Revision and therefore participates in its Object ID.

Incidental runtime metadata should not be included here unless it is intentionally part of historical meaning.

# Operations

Operations are encoded inside Change Revisions.

They are not independent canonical objects in KAT v0.1.

Operations use a closed numeric operation vocabulary.

Initial operation kinds are:

```text
1  CreateElement
2  UpdateElement
3  DeprecateElement
4  Link
5  Unlink
6  Supersede
```

Operation identifiers are permanent protocol values.

## CreateElement

Creates a new semantic element.

Canonical semantic content:

```text
new_version
```

where `new_version` is the Object ID of a Knowledge Element Version.

The referenced Element ID must not already be active in the base state.

## UpdateElement

Updates an existing semantic element.

Canonical semantic content:

```text
element_id
expected_version
new_version
```

The operation implies:

```text
Precondition:
    element_id resolves to expected_version
    in the base state.

Postcondition:
    element_id resolves to new_version
    in the resulting state.
```

## DeprecateElement

Changes an element to a deprecated lifecycle state.

Canonical semantic content:

```text
element_id
expected_version
new_version
```

The new Knowledge Element Version must have lifecycle `deprecated`.

## Link

Introduces a semantic relationship.

Canonical semantic content:

```text
new_relationship_version
```

The referenced Relationship Version must satisfy the active ontology.

## Unlink

Removes a relationship from the active semantic state.

Canonical semantic content:

```text
relationship_id
expected_version
```

The historical Relationship Version object remains in the canonical object store.

## Supersede

Represents semantic replacement while preserving traceability between previous and replacement knowledge.

Its canonical structure includes enough information to identify:

* Existing element
* Expected existing version
* Replacement element
* Replacement version
* Superseding relationship

The complete field structure is defined by `spec/canonical-format.cddl` (operation kind 6).

Supersede remains an explicit operation because it carries semantic lifecycle meaning that should not be reduced to unrelated primitive mutations.

# Preconditions and Postconditions

Standard operation preconditions and postconditions are defined by operation semantics.

They are not redundantly encoded as separate objects.

For example:

```text
UpdateElement(
    E,
    expected = V1,
    new = V2
)
```

already defines the expected transition:

```text
E: V1 -> V2
```

Additional explicit condition structures are outside the KAT v0.1 canonical format.

Repository invariants provide additional validation where operation-local semantics are insufficient.

# Hashing Procedure

To calculate an Object ID:

1. Construct the logical canonical object.
2. Validate that its structure conforms to the active CDDL schema.
3. Normalize all set-like collections according to canonical sorting rules.
4. Encode the complete object envelope using deterministic CBOR.
5. Calculate SHA-256 over the exact encoded bytes.
6. Use the resulting 32-byte digest as the Object ID.

Conceptually:

```text
Object
  |
  v
Normalize
  |
  v
Deterministic CBOR
  |
  v
SHA-256
  |
  v
Object ID
```

Object hashing must never depend on:

* File path
* Modification time
* Hostname
* Process ID
* Memory representation
* Rust field order
* Filesystem metadata

# Object Storage Verification

When an object is loaded by Object ID, the implementation should verify its integrity.

Conceptually:

```text
Read object bytes
      |
      v
SHA-256
      |
      v
Compare with requested Object ID
```

A mismatch is a repository integrity error.

Only after integrity verification should the object be interpreted semantically.

# Validation Layers

Canonical object processing distinguishes three validation layers.

## Encoding Validity

Determines whether the bytes represent a valid canonical KAT object.

Includes:

* Valid CBOR
* Supported envelope version
* Known object kind
* Supported schema version
* Correct field structure
* Valid primitive types
* Correct canonical ordering

## Repository Integrity

Determines whether canonical repository references are structurally valid.

Includes:

* Object hash matches its Object ID
* Referenced objects exist
* Referenced object kinds are correct
* Repository refs point to expected object kinds

## Semantic Validity

Determines whether objects and Semantic States conform to KAT semantics.

Includes:

* Ontology conformance
* Relationship validity
* Lifecycle rules
* Preconditions
* Postconditions
* Invariants
* Repository-specific semantic rules

An object may be structurally valid while still being semantically invalid.

# Decoder Behavior

Canonical object decoders must fail closed.

An implementation must reject:

* Unsupported envelope versions
* Unsupported required schema versions
* Unknown required object kinds
* Kind/payload mismatches
* Invalid UUID encoding
* Invalid Object ID encoding
* Duplicate required fields
* Missing required fields
* Non-canonical set ordering
* Duplicate semantic identifiers in set-like structures
* Hash mismatches
* Invalid object references

Canonical repository data must not be silently repaired during normal decoding.

Repair tooling, if introduced later, must operate explicitly.

# Schema Evolution

KAT repository compatibility is versioned at multiple levels.

## Repository Format Version

Stored in repository metadata.

Defines overall physical repository compatibility.

## Envelope Version

Defines the common canonical-object framing.

## Object Schema Version

Defines the payload format for one object kind.

Each object kind evolves independently.

## Ontology Version

Defines semantic vocabulary and relationship rules.

Ontology evolution is independent from canonical storage-format evolution.

Conceptually:

```text
Repository Format
        |
        +--> Canonical Envelope
        |
        +--> Object Schemas
        |
        +--> Ontology Versions
```

These versions must not be treated as interchangeable.

# Compatibility Rules

KAT v0.1 follows these initial compatibility rules.

* Unsupported repository format versions must not be opened for mutation.
* Unsupported envelope versions must be rejected.
* Unsupported object schema versions must be rejected unless an explicit compatible decoder exists.
* Unknown ontology types may be structurally decoded but may prevent semantic validation if the active ontology cannot interpret them.
* Existing numeric object-kind and operation identifiers must never be reassigned.
* Existing canonical field identifiers must not be reused with incompatible meaning.

A future implementation may support read-only access to partially understood repositories.

That behavior is not required by v0.1.

# Diagnostic Representation

Canonical KAT objects are binary.

KAT should provide a human-readable diagnostic representation for inspection and debugging.

Example:

```text
kat object show <object-id>
```

may display:

```json
{
  "kind": "knowledge-element-version",
  "element_id": "7c8e0c81-b9fc-4c31-9974-b9db8fa72e51",
  "type": "kat.core/requirement",
  "lifecycle": "active",
  "properties": {
    "title": "Support refunds"
  }
}
```

The diagnostic representation is not canonical.

It must not be used directly to calculate Object IDs.

Conceptually:

```text
Canonical CBOR
    = repository representation

JSON / text
    = diagnostic representation
```

# Canonical Format Invariants

The canonical format follows these rules:

* Immutable object identity is derived from canonical contents.
* Stable semantic identity remains independent from content.
* The same logical canonical object must encode to the same bytes.
* The same canonical bytes must produce the same Object ID.
* Object kinds participate in content identity.
* Semantic set ordering is deterministic.
* Canonical objects do not contain their own Object IDs.
* Semantic States contain composition, not history.
* Change Revisions contain evolution, not current state duplication.
* Operations are ordered and remain part of their Change Revision.
* History is not represented as a separate canonical object.
* Incidental runtime metadata must not influence canonical identity.
* Canonical decoding fails closed.
* Storage integrity and semantic validity remain separate concerns.

# v0.1 Canonical Object Set

The canonical object set for KAT v0.1 is:

```text
KnowledgeElementVersion
RelationshipVersion
OntologyVersion
SemanticState
ChangeRevision
```

No additional canonical object types should be introduced unless the existing conceptual model cannot preserve their required meaning.

The following remain non-canonical or derived in v0.1:

```text
Operation
History
Semantic Diff
Query Index
Validation Cache
Conflict
Reconciliation
Materialization Record
Participant
Permission
```

# Open Questions

The following details remain to be finalized in the implementation specification:

* Maximum nesting or object-size limits
* Whether empty optional property maps are omitted or encoded explicitly
* Whether Change descriptions allow empty strings
* Whether Change metadata beyond description belongs in v0.1
* Whether ontology extensions require a canonical field in schema version 1
* Whether canonical decoders must re-encode objects to verify deterministic encoding
* How future hash-algorithm migration is represented


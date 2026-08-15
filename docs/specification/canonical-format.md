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

Other KAT specifications may summarize canonical format values for implementation context but must not independently redefine them.

## Format Goals

The canonical format is designed to be:

* Deterministic
* Compact
* Implementation-independent
* Verifiable
* Evolvable
* Suitable for immutable content-addressed storage
* Precise enough for independent implementations to produce identical object identities

The canonical format does not depend on Rust struct layout, Serde defaults, filesystem paths, or internal implementation details.

---

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

---

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

---

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

The canonical format contains stable semantic IDs inside objects where required.

Stable semantic identities use UUIDv4 in schema version 1.

---

# UUID Representation

UUIDs are encoded in canonical CBOR as exactly 16 bytes.

The canonical representation must distinguish UUID values from arbitrary byte strings.

The exact tagged representation is defined by `spec/canonical-format.cddl`: CBOR tag 37 containing exactly 16 bytes.

When shown to users or stored in human-readable repository metadata, UUIDs use the standard textual UUID representation.

Example:

```text
7c8e0c81-b9fc-4c31-9974-b9db8fa72e51
```

---

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

The Object ID is sensitive to:

* Envelope version
* Object kind
* Schema version
* Payload

## Envelope Version

`envelope_version` defines the common framing used by canonical KAT objects.

The current canonical format uses:

```text
envelope_version = 1
```

Future envelope versions may introduce incompatible framing changes.

An implementation must reject unsupported envelope versions.

## Object Kind

`object_kind` identifies the type of canonical object contained in the payload.

Envelope version 1 object kinds are:

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

---

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

Floating-point property values are not supported by the schema version 1 canonical property model.

This avoids ambiguity around representation, equality, and canonicalization.

## Text

Text values must contain valid UTF-8.

No implicit text normalization is performed by the canonical encoder unless explicitly defined by a future schema.

Therefore semantically equivalent but byte-distinct Unicode strings produce different Object IDs.

## Map Ordering

CBOR maps used by canonical objects follow the RFC 8949 Core Deterministic Encoding Requirements (RFC 8949, Section 4.2.1).

Map keys are ordered by the bytewise lexicographic comparison of their deterministic encoded forms.

For example, the following keys appear in this deterministic order based on their encoded bytes:

```text
Key      Encoded Bytes (Hex)
10    -> 0a
100   -> 1864
-1    -> 20
"z"   -> 617a
"aa"  -> 626161
```

ordering is based on the encoded bytes rather than on encoded length. In particular, this is not the length-first alternative defined for RFC 7049 Canonical CBOR compatibility (RFC 8949, Section 4.2.3).

Semantic collections whose logical meaning is a set use explicitly sorted arrays rather than relying on map ordering alone.

## Unknown Fields

Canonical encoders must not emit fields that are not defined by the active object schema.

Current schema versions are closed schemas. Decoders must reject fields not defined by the declared schema version.

Forward-compatible field skipping is not part of schema version 1.

---

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

---

# Property Values

Knowledge Element and Relationship properties use a restricted canonical value model.

Schema version 1 supports:

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

Property maps must not contain duplicate text keys.

Nested values may use the same supported value types.

Floating-point numbers are excluded from schema version 1.

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

---

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

Schema version 1 defines the following lifecycle states:

```text
0  active
1  deprecated
2  superseded
```

These numeric values are canonical protocol identifiers and must not be reassigned.

## properties

A map of semantic properties associated with this element version.

## Excluded Fields

A Knowledge Element Version does not contain:
* Previous version pointers
* History
* Incoming relationships
* Outgoing relationships
* Query indexes
* Artifact file contents

---

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

Stable UUID identifying the logical relationship independently of its immutable canonical representation.

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

## properties

Optional semantic properties of the relationship.

---

# Semantic State

A `SemanticState` represents one immutable composition of software knowledge.

Its logical fields are:

```text
ontology_version
elements
relationships
```

The state answers:

> What semantic knowledge is selected by this state?

It does not describe how that state was reached.

## ontology_version

Object ID of the Ontology Version used to interpret and validate the state.

## elements

Logical mapping:

```text
ElementId -> KnowledgeElementVersion ObjectId
```

Schema version 1 encodes this mapping as a sorted array of entries:

```text
[
    ElementId,
    KnowledgeElementVersion ObjectId
]
```

Entries must be sorted by raw Element ID bytes. Duplicate Element IDs are invalid.

## relationships

Logical mapping:

```text
RelationshipId -> RelationshipVersion ObjectId
```

Encoded as a sorted array of entries:

```text
[
    RelationshipId,
    RelationshipVersion ObjectId
]
```

Entries must be sorted by raw Relationship ID bytes. Duplicate Relationship IDs are invalid.

## State Identity

The Semantic State Object ID is the SHA-256 digest of the complete canonical Semantic State object.

Any change to:
* selected element versions;
* selected relationship versions;
* ontology version;

produces a different Semantic State Object ID.

## Historical Information

Semantic States do not contain parent states, originating changes, change descriptions, author information, or history indexes.

---

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

Each element type definition contains at least `type_id` and `name`.

Element type definitions must be sorted lexicographically by `type_id`. Duplicate `type_id` values are invalid.

## Relationship Type Definitions

Each relationship type definition contains `type_id`, `name`, `allowed_source_types`, and `allowed_target_types`.

Relationship definitions must be sorted lexicographically by `type_id`. `allowed_source_types` and `allowed_target_types` must be sorted lexicographically and contain no duplicates.

---

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

`change_id` is the stable semantic identity of the logical Change represented by this revision.

## base_states

`base_states` is a canonical array of `SemanticState` Object IDs.

KAT v0.2 accepted publication uses exactly one base state. The schema permits multiple base-state references for future composition or merge semantics.

Until semantics for multiple base states are defined, such values are structurally representable but are not valid for normal v0.2 publication. If multiple base states are present, they must use canonical Object ID ordering.

## result_state

Semantic State Object ID produced by the Change Revision.

A Semantic State does not contain a reverse canonical reference to its originating Change Revision.

## operations

Ordered list of semantic operations comprising the Change Revision.

Operation order is semantically meaningful and must be preserved exactly. Operations must not be sorted.

## dependencies

Immutable Change Revision Object IDs on which the change causally depends.

Dependencies represent a semantic set. They must be sorted by Object ID and must not contain duplicates.

## description

Optional human-readable description of the semantic change.

---

# Operations

Operations are encoded inside Change Revisions. They are not independent canonical objects in schema version 1.

Operations use a closed numeric operation vocabulary.

Schema version 1 operation kinds are:

```text
1  CreateElement
2  UpdateElement
3  DeprecateElement
4  Link
5  Unlink
6  SupersedeElement
7  AccountArtifact
```

Operation identifiers are permanent protocol values.

## CreateElement

Canonical semantic structure (operation kind 1):

```text
[
    1,
    new_version
]
```

where `new_version` is the Object ID of a Knowledge Element Version.

## UpdateElement

Canonical semantic structure (operation kind 2):

```text
[
    2,
    element_id,
    expected_version,
    new_version
]
```

## DeprecateElement

Canonical semantic structure (operation kind 3):

```text
[
    3,
    element_id,
    expected_version,
    new_version
]
```

## Link

Canonical semantic structure (operation kind 4):

```text
[
    4,
    new_relationship_version
]
```

where `new_relationship_version` is the Object ID of a Relationship Version.

## Unlink

Canonical semantic structure (operation kind 5):

```text
[
    5,
    relationship_id,
    expected_version
]
```

Unlink removes the relationship mapping from the resulting `SemanticState`; it does not encode deletion or mutation of the referenced `RelationshipVersion`.

## SupersedeElement

Canonical semantic structure (operation kind 6):

```text
[
    6,
    existing_element_id,
    expected_existing_version,
    replacement_element_id,
    replacement_version,
    superseding_relationship_version
]
```

## AccountArtifact

Canonical semantic structure (operation kind 7):

```text
[
    7,
    artifact_id,
    reconciliations
]
```

where `reconciliations` is a non-empty array of baseline reconciliation entries:

```text
[
    relationship_id,
    expected_relationship_version,
    target_element_id,
    reconciled_target_version
]
```

### Canonical Reconciliation Ordering Rule

The entries in `reconciliations` MUST be strictly sorted lexicographically by the canonical 16-byte UUID representation of `relationship_id` (RFC 4122 UUID bytes).

Duplicate `relationship_id` entries within `reconciliations` are invalid.

---

## Operation Contract Representation

Operation preconditions and postconditions are defined by [`docs/operations.md`](operations.md) and are not encoded as separate canonical objects unless explicitly included in an operation's structural schema.

Fields such as `expected_version` participate in the canonical operation payload because they are part of that operation's structural identity.

The canonical format defines structural representation and binary encoding, not execution semantics.

---

# Hashing Procedure

To calculate an Object ID:

1. Construct the logical canonical object.
2. Validate that its structure conforms to the active CDDL schema.
3. Construct each set-like collection in its required canonical order. Existing encoded input is never normalized before identity verification. Canonical decoding must validate the received encoding as-is; decoding and re-encoding must not be used to silently canonicalize malformed input.
4. Encode the complete object envelope using deterministic CBOR.
5. Calculate SHA-256 over the exact encoded bytes.
6. Use the resulting 32-byte digest as the Object ID.

Object hashing must never depend on file paths, modification times, hostnames, process IDs, memory layout, or environment metadata.

---

# Object Storage Verification

Canonical object integrity is verified by recomputing SHA-256 over stored canonical bytes and comparing it to the claimed `ObjectId`.

Detailed storage layout, ref updates, and filesystem operations are governed by repository implementation specifications.

---

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
* Deterministic CBOR encoding
* Canonical collection ordering

## Repository Integrity

Determines whether canonical repository references are structurally valid.

Includes:
* Object hash matches its Object ID
* Referenced objects exist
* Referenced object kinds are correct
* Repository refs point to expected object kinds

## Semantic Validity

Determines whether objects and Semantic States conform to KAT semantic rules, including ontology conformance, operation contracts, and model invariants.

---

# Decoder Behavior

Canonical object decoders must fail closed.

An implementation must reject:
* Unsupported envelope versions
* Unsupported required schema versions
* Unknown required object kinds
* Kind/payload mismatches
* Invalid UUID encoding
* Invalid Object ID encoding
* Duplicate map keys where the schema requires uniqueness (including duplicate property-map keys)
* Missing required fields
* Non-canonical set ordering
* Non-deterministic CBOR

Canonical repository data must not be silently repaired during normal decoding.

---

# Schema Evolution & Compatibility Rules

KAT repository compatibility is versioned at multiple levels:
* **Repository Format Version**: Stored in repository metadata.
* **Envelope Version**: Defines common canonical-object framing.
* **Object Schema Version**: Defines payload format per object kind.
* **Ontology Version**: Defines semantic vocabulary and relationship rules.

Rules:
* Unsupported repository format versions must not be opened for mutation.
* Unsupported envelope versions must be rejected.
* Unsupported object schema versions must be rejected unless an explicit compatible decoder exists.
* Existing numeric object-kind and operation identifiers must never be reassigned.
* Existing canonical field identifiers must not be reused with incompatible meaning.

---

# Non-Normative Diagnostic Representation

Canonical KAT objects are binary CBOR.

Tooling may provide a human-readable diagnostic representation (e.g. JSON/text) for inspection and debugging. The diagnostic representation is non-canonical and must not be used directly to calculate Object IDs.

---

# Canonical Format Properties

The canonical format guarantees these properties:

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

---

# Canonical Object Set

The canonical object set for envelope version 1 and current object schema versions is:

```text
KnowledgeElementVersion
RelationshipVersion
OntologyVersion
SemanticState
ChangeRevision
```

The following remain non-canonical or derived:

```text
Operation (embedded in ChangeRevision, not a top-level canonical object)
Standalone Reconciliation object (reconciliations are embedded in AccountArtifact)
History
Semantic Diff
Query Index
Validation Cache
Conflict
Materialization Record
Participant
Permission
```

---

# Open Questions

The following details remain open for future format extensions:

* Maximum nesting or object-size limits in future envelope versions
* Whether implementations must verify deterministic encoding by byte-level validation, re-encoding comparison, or another equivalent method
* How future hash-algorithm migration is represented in envelope version 2

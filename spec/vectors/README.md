# Format Vectors

Golden and negative test vectors for the KAT canonical format.

Canonical Object IDs are:

```text
ObjectId = SHA-256(deterministic CBOR bytes)
```

The repository format is interoperable only if independent implementations agree byte-for-byte on canonical encodings. These vectors are the compatibility contract: a conforming implementation must produce the exact bytes and ObjectId of every valid vector, and must reject every invalid vector.

The normative sources for these vectors are `spec/canonical-format.cddl` (structural schema) and `docs/canonical-format.md` (encoding and semantics rules).

## Layout

```text
vectors/
├── valid/
│   └── <name>.json
└── invalid/
    └── <name>.json
```

## Valid fixture format

Each valid fixture is a single JSON file containing:

```json
{
  "name": "short-name",
  "comment": "what this fixture covers",
  "object": {
    "kind": "knowledge-element-version",
    "element_id": "7c8e0c81-b9fc-4c31-9974-b9db8fa72e51",
    "type": "kat.core/requirement",
    "lifecycle": "active",
    "properties": {}
  },
  "cbor_hex": "a400010101020103a400d82550...",
  "object_id": "8f83be9fd3d31de06ad47004929ac094f830ff57058cb08fceb9c8b2b5210185"
}
```

* `object` — logical diagnostic representation (non-canonical; for readability, per `docs/canonical-format.md` Diagnostic Representation)
* `cbor_hex` — lowercase hex of the exact canonical CBOR bytes
* `object_id` — 64-character lowercase hex SHA-256 of `cbor_hex`

## Invalid fixture format

Each invalid fixture is a single JSON file containing:

```json
{
  "name": "short-name",
  "comment": "what this fixture covers",
  "reason": "which canonical rule it violates",
  "cbor_hex": "..."
}
```

Invalid fixtures have no `object` or `object_id`: they are not canonical objects.

## Coverage requirements

Valid vectors must cover at minimum:

* every canonical object kind
* UUID tag 37 representation
* integer boundaries (0, 23, 24, 255, 256, and negative values)
* empty and non-empty property maps
* nested property values
* Semantic State element and relationship ordering
* Ontology element and relationship type ordering
* all six operation encodings
* ChangeRevision
* the empty initial Semantic State

Invalid vectors must cover at minimum:

* indefinite-length values
* non-shortest integer encodings
* wrong map ordering
* duplicate map keys
* invalid UUID representation
* object kind / payload mismatch
* malformed Object IDs
* unsorted Semantic State entries
* duplicate semantic IDs
* unsupported schema or envelope versions

## Generation policy

Vectors are generated from an implementation of the canonical encoder and reviewed byte-by-byte against `spec/canonical-format.cddl` and `docs/canonical-format.md`. Hand-written fixtures must be verified against an independent encoder or decoder before becoming authoritative, because a single wrong byte silently corrupts the compatibility contract.

Objects that reference other objects by ObjectId (Semantic State, Ontology Version, Change Revision, operation payloads) must be generated after their referenced objects so the embedded hashes are correct.

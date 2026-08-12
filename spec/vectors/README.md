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
│   └── <name>.json          logical object + exact canonical bytes + ObjectId
└── invalid/
    ├── structural/          non-canonical logical objects (rejected by canonical_bytes)
    │   └── <name>.json
    └── encoded/             raw malformed CBOR bytes (rejected by a decoder)
        └── <name>.cbor
```

The two invalid directories separate the two distinct failure layers:

```text
canonical construction failure   (invalid/structural/)
        vs.
malformed repository bytes       (invalid/encoded/)
```

This mirrors the validation layers defined in `docs/prototype-design.md`
(Encoding Validity vs. Repository Integrity vs. Semantic Validity).

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

- `object` — logical diagnostic representation (non-canonical; for readability). The JSON is only test metadata, never canonical KAT data.
- `cbor_hex` — lowercase hex of the exact canonical CBOR bytes.
- `object_id` — 64-character lowercase hex SHA-256 of `cbor_hex` (the canonical ObjectId).

### Diagnostic representation

`object.kind` selects one of five logical shapes:

- `knowledge-element-version` — `element_id`, `type`, `lifecycle` (`active` | `deprecated` | `superseded`), `properties`
- `relationship-version` — `relationship_id`, `source_element_id`, `relationship_type`, `target_element_id`, `properties`
- `ontology-version` — `ontology_id`, `element_types` (`type_id`, `name`), `relationship_types` (`type_id`, `name`, `allowed_source_types`, `allowed_target_types`)
- `semantic-state` — `ontology_version` (ObjectId hex), `elements` (`element_id`, `version`), `relationships` (`relationship_id`, `version`)
- `change-revision` — `change_id`, `base_states`, `result_state`, `operations`, `dependencies`, optional `description`

ObjectIds are 64 lowercase hex characters. Operation kinds: `create-element`, `update-element`, `deprecate-element`, `link`, `unlink`, `supersede` (field names follow the CDDL).

Property values are JSON with these mappings:

| JSON                         | PropertyValue                           |
| ---------------------------- | --------------------------------------- |
| `null`                       | `Null`                                  |
| boolean                      | `Bool`                                  |
| number                       | `Integer`                               |
| string                       | `Text`                                  |
| array                        | `List`                                  |
| object `{"$uuid": "<uuid>"}` | `Uuid`                                  |
| object `{"$bytes": "<hex>"}` | `Bytes`                                 |
| other object                 | `Map` (key order is the declared order) |

Property map keys must be declared in canonical order. The keys `$uuid` and `$bytes` are reserved: a single-key object with one of them is a tagged scalar, not a map. Duplicate keys cannot be represented in a JSON object; duplicate-property-key cases are exercised as constructed Rust values in `tests/structural_invalid.rs`.

### Valid coverage

Valid vectors cover at minimum:

- every canonical object kind
- UUID tag 37 representation
- integer boundaries (0, 23, 24, 255, 256, and negative values)
- empty and non-empty property maps
- nested property values
- all `PropertyValue` variants
- Semantic State with multiple entries proving UUID ordering
- Ontology element and relationship type ordering
- all six operation encodings (via ChangeRevision fixtures)
- multiple base states and dependencies proving ObjectId ordering
- optional `description` present and absent
- the empty initial Semantic State

## Invalid structural fixtures

Each `invalid/structural/` fixture is a JSON file documenting a logical object that is **constructible as a Rust value but not canonical**, so `canonical_bytes` must reject it:

```json
{
  "name": "semantic-state-unsorted",
  "comment": "element entries are not sorted by element ID",
  "reason": "SemanticState element entries must be sorted by element ID",
  "object": { "...": "logical diagnostic representation" }
}
```

Consumption: `tests/structural_invalid.rs` constructs the corresponding Rust value and asserts `canonical_bytes` returns the expected `CanonicalStructureError`. Cases that JSON cannot express (e.g. duplicate property keys) are documented here and tested as constructed Rust values.

## Invalid encoded fixtures

Each `invalid/encoded/` fixture is a raw `.cbor` file of malformed canonical bytes. These are normative vectors whose rejection tests remain pending until a canonical decoder exists. Examples: indefinite-length values, non-shortest integer encodings, wrong UUID tag, wrong-length UUID byte strings, duplicate CBOR map keys, object-kind/payload mismatch, malformed ObjectIds, and unsupported envelope or schema versions.

## Generation policy

**The implementation under test must never be the sole oracle for a golden byte sequence.** The dependency is one-directional:

```text
Specification (CDDL + canonical-format.md)
        |
        v
Golden vector
        |
        v
Encoder must match it
```

An encoder must not authoritatively define a golden vector. Vectors are derived from the specification — hand-derived for the initial set, and byte-by-byte reviewed against `spec/canonical-format.cddl` and `docs/canonical-format.md`; an independent CBOR implementation or external verification script may be used as a second implementation as the suite grows. The conformance harness (`tests/vector_conformance.rs`) then verifies that the encoder reproduces the exact bytes and ObjectId.

Objects that reference other objects by ObjectId (Semantic State, Ontology Version, Change Revision, operation payloads) embed the hashes of their referenced objects, so their canonical bytes depend on those referenced objects' bytes.

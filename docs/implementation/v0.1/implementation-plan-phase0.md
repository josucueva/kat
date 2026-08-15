> Part of the master plan: [docs/implementation-plan.md](implementation-plan.md).
## Phase 0: Canonical Repository Substrate

The substrate proves the repository-format mechanics. Per `prototype-design.md`, it is **not** a successful prototype milestone by itself. It must not add object fan-out, packfiles, garbage collection, compression, persistent indexes, verify-every-object-on-every-read, or remote exchange.

### 0.1 — Rust project skeleton

- [x] Create `Cargo.toml` (binary crate, Rust edition current stable = 2024).
- [x] Create only the modules needed immediately:

```text
src/
├── lib.rs          (library: owns the modules)
├── main.rs         (thin CLI binary over the library)
├── domain/
│   ├── mod.rs
│   └── identity.rs
├── encoding/
│   ├── mod.rs
│   ├── cbor.rs
│   └── hash.rs
└── repository/
    ├── mod.rs
    ├── object_store.rs
    ├── ref_store.rs
    └── metadata.rs
```

- [x] Do **not** create collaboration, materialization, plugins, indexes, or query-database modules yet.

Structure note (added in step 0.2): the crate uses the standard **library + binary split** (`src/lib.rs` owns the modules; `src/main.rs` is a thin CLI). This keeps newly added public types free of transient `dead_code` warnings before they are wired into the CLI, and it enables integration tests (used for the step 0.5 vectors).

### 0.2 — Identity primitives

- [x] Typed UUID semantic IDs (distinct wrapper types so they cannot be interchanged):

```text
RepositoryId
SoftwareId
ElementId
RelationshipId
ChangeId
OntologyId
```

- [x] `ObjectId([u8; 32])` — SHA-256 content identity, distinct from semantic IDs.
- [x] Parsing/display for textual UUIDs and for 64-character lowercase hexadecimal `ObjectId`s.
- [x] UUID uses canonical representation per `spec/canonical-format.cddl`: CBOR tag 37, exactly 16 bytes.

Reference: `docs/prototype-design.md` → Core Rust Types.

Notes: `ObjectId` parsing accepts **only** the canonical form (exactly 64 chars, `0-9`/`a-f`, lowercase rejected). No CBOR serialization on identity types yet — that belongs to the step 0.4 encoder. Dependencies added: `uuid` (v4), `hex`, `thiserror`. `sha2`/CBOR deliberately deferred.

### 0.3 — Canonical Rust data model

Implement the model directly from `spec/canonical-format.cddl` (the CDDL is authoritative — do not design these types independently):

- [x] `PropertyValue` (null / bool / int / text / byte string / UUID / list / map; **no floating point**).
- [x] `KnowledgeElementVersion` (element_id, type_id, lifecycle, properties).
- [x] `RelationshipVersion` (relationship_id, source, relationship_type, target, properties).
- [x] `OntologyVersion` (ontology_id, element_type_definitions, relationship_type_definitions).
- [x] `SemanticState` (ontology_version, sorted element entries, sorted relationship entries).
- [x] `ChangeRevision` (change_id, base_states, result_state, operations, dependencies, optional description).
- [x] `Operation` — the six operation encodings: CreateElement(1), UpdateElement(2), DeprecateElement(3), Link(4), Unlink(5), Supersede(6).
- [x] `CanonicalObject` envelope: `envelope_version = 1`, `object_kind`, `schema_version = 1`, `payload`; Object IDs are **not** encoded inside the object.

Notes: maps, states, and ontology definitions use **ordered vectors** (not `BTreeMap`/`HashMap`) so malformed duplicates/order remain observable to validation. `Lifecycle`, `Operation`, and `ObjectKind` are semantic enums — numeric protocol IDs are assigned by the step 0.4 encoder, not hard-coded as discriminants. **Structural validation** lives in `encoding/validate.rs` (`CanonicalValidate` trait + `CanonicalStructureError`): sorted/unique SemanticState entries, sorted/unique ontology definitions and allowed-type lists, sorted (non-unique) multi-base states, sorted/unique dependencies, non-empty base_states/operations, and canonical property-key order. No CBOR, SHA-256, filesystem, or Change Engine logic added.

### 0.4 — Deterministic CBOR encoding

The highest-risk technical component. Implement RFC 8949 Core Deterministic Encoding (Section 4.2.1):

- [x] Definite-length collections only (no indefinite-length arrays, maps, or strings).
- [x] Shortest integer encoding.
- [x] UUID as CBOR tag 37 containing exactly 16 bytes.
- [x] Map keys ordered by bytewise lexicographic comparison of their deterministic encoded forms (RFC 8949 §4.2.1 — not the RFC 7049 length-first alternative, §4.2.3).
- [x] Exact numeric envelope / object-kind / operation identifiers from the CDDL.
- [x] Sorted Semantic State entries (lexicographic over raw 16-byte IDs).
- [x] Sorted ontology element/relationship type sets (lexicographic over UTF-8 type_id).
- [x] No floating-point property values.
- [x] Property key ordering follows deterministic CBOR map ordering.

Reference: `docs/canonical-format.md` → Deterministic CBOR Requirements & Canonical Collections.

Notes: implemented as a **small explicit encoder** (`encoding/cbor.rs`), not a Serde-derived serializer — every protocol number is emitted literally. Public API: `encoding::canonical_bytes(&CanonicalObject) -> Result<Vec<u8>, EncodingError>`, which structurally validates first and refuses non-canonical objects (fail-closed; never sorts/repairs). Property-map key order uses the **encoded-key-byte comparator** (`cmp_encoded_text`), shared by the validator and encoder. `EncodingError` currently has only `InvalidCanonicalStructure` (the only reachable variant with the infallible in-memory writer). Golden byte tests interleaved with step 0.5: the authoritative `knowledge-element-version-empty-properties` fixture plus hand-verified fixtures for every object kind and all PropertyValue variants.

### 0.5 — Golden test vectors (created alongside encoding, not after)

- [x] Fixtures live in `spec/vectors/valid/` and `spec/vectors/invalid/`.
- [x] First useful test: `known logical object → exact expected CBOR bytes → exact expected SHA-256`.
- [x] Cover per `spec/vectors/README.md`: every object kind, UUID tag 37, integer boundaries, empty/non-empty property maps, nested values, Semantic State ordering, ontology ordering, all six operations, ChangeRevision, empty initial Semantic State.
- [x] Invalid vectors: indefinite-length, non-shortest integers, wrong map ordering, duplicate map keys, invalid UUID, kind/payload mismatch, malformed ObjectIds, unsorted state entries, duplicate semantic IDs, unsupported versions.

Notes: 10 valid fixtures (all object kinds, all PropertyValue variants, integer boundaries, all six operations, UUID ordering, ObjectId ordering, description present/absent, empty initial state). `invalid/structural/` (6 JSON fixtures; tested via `tests/structural_invalid.rs` through `canonical_bytes` rejection) vs `invalid/encoded/` (10 raw `.cbor` fixtures; tests pending until a decoder exists) separate construction failure from malformed bytes. `tests/vector_conformance.rs` walks the valid directory, builds logical objects from the diagnostic JSON, and asserts exact bytes. `object_id` (SHA-256) values in fixtures were computed with an **independent** SHA-256 (verified the anchor fixture's stored value). `serde_json` added as a **dev-dependency** (preserve_order) only; the library stays serde-free. Vectors README documents the diagnostic representation and the "encoder must not be the sole oracle" derivation policy.

### 0.6 — SHA-256 Object IDs

- [x] API: `canonical_bytes(object) -> Vec<u8>` and `object_id(bytes) -> ObjectId`.
- [x] Test: same canonical object → same bytes → same ObjectId.
- [x] Test: changed canonical content → different ObjectId.

Notes: `encoding/hash.rs` implements `object_id(&[u8]) -> ObjectId` (SHA-256 only; no re-encoding/normalization) and a thin `canonical_object_id(&CanonicalObject)` composed as `canonical_bytes` then `object_id`. No `ObjectId::new()` — ObjectId is always _derived_ (`from_bytes`/`FromStr` reconstruct existing identities). The conformance harness now asserts both exact bytes and the independently-derived `object_id` for all 10 valid fixtures. `sha2 = "0.10"` added. The full identity pipeline is complete: object → structural validation → deterministic CBOR → canonical bytes → SHA-256 → ObjectId.

### 0.7 — Immutable ObjectStore

- [x] Layout: `.kat/objects/<sha256>` (no fan-out in v0.1).
- [x] API: `put(canonical_bytes) -> ObjectId`, `get(ObjectId) -> bytes`, `exists(ObjectId)`.
- [x] `put` must never overwrite different bytes; an existing object with the same ObjectId needs no write.
- [x] `get` supports integrity verification (SHA-256 of bytes must equal requested ObjectId).

Notes: `repository/object_store.rs` — stores **bytes**, not `CanonicalObject`s (no CBOR/metadata knowledge). `put` computes the ObjectId first, writes to a unique temp file under `tmp/`, flushes, then atomically renames into `objects/<64 hex>`; concurrent same-bytes writers are harmless (one canonical object, both succeed). `get` verifies the requested ObjectId (single-object integrity; no recursive graph re-verification). `exists` is purely physical (no read/hash). `ObjectStoreError` distinguishes `NotFound` vs `Integrity { expected, actual }` vs `Io`. `tempfile` added as a dev-dependency for tests. 10 required test scenarios all pass.

### 0.8 — `repository.toml` and `AcceptedRef`

- [x] `repository.toml`: `format_version = 1`, `repository_id`, `software_id`, `object_encoding = "cbor-deterministic-v1"`, `hash_algorithm = "sha256"`. Dynamic state/history must **not** live here.
- [x] `AcceptedRef`: `{ state: StateId, change: Option<ChangeRevisionId> }`.
- [x] Compare-and-swap publication abstraction (strongest test comes with the first Change).
- [x] Locked/temp-file atomic replace behind a dedicated ref-storage abstraction.

Notes: `repository/metadata.rs` — `RepositoryMetadata` with typed closed enums `ObjectEncoding`/`HashAlgorithm`; parser rejects unsupported format_version/encoding/algorithm and malformed UUIDs. `repository/ref_store.rs` — `AcceptedRef` (`state`, `change: Option<ObjectId>`; physical format `state <64 hex>` / `change <64 hex>|none`), `RefStore` trait (`read_accepted`, `init_accepted` create-only, `compare_and_swap_accepted`), `FileRefStore` with a `refs/accepted.lock` exclusive-create lock and atomic temp+rename publication (cleaned up after publication). `RefStore` has **no** semantic interpretation — the `accepted.change.result_state == accepted.state` invariant belongs to open/integrity and Change publication. `toml` dependency added. 14 new tests (round-trips, rejections, CAS success/stale, single concurrent winner, cleanup).

### 0.9 — `kat init`

First user-visible command (`docs/cli.md` contract). Must:

- [x] Verify no existing KAT repository is overwritten.
- [x] Generate RepositoryId and SoftwareId.
- [x] Create `.kat/` directory structure.
- [x] Write `repository.toml`.
- [x] Create core `OntologyVersion O1`, persist it.
- [x] Create empty `SemanticState S0`, persist it.
- [x] Publish `refs/accepted` → `{ state: S0, change: none }`.
- [x] Result: a repository that can be closed and reopened.

Notes: `repository/init.rs` — `init_repository(&Path) -> Result<InitResult, RepositoryError>` is the thin-CLI entry (main.rs only parses `kat init` and prints the result). `initial_core_ontology(OntologyId)` carries the spec-derived core ontology (7 element types; 10 relationship types with allowed source/target sets, from `docs/ontology.md` + canonical IDs in `docs/canonical-format.md`). `repository/error.rs` adds `RepositoryError` (AlreadyExists + composed layer errors). Metadata is written via temp + atomic rename; the `accepted` ref is the publication point (immutable O1/S0 left behind by a failed init are harmless). Verified end-to-end from the CLI; `kat init` twice is rejected. 4 new tests (core ontology spec conformance + 3 integration tests: layout/metadata/O1/S0/ref, re-init rejected & unchanged, unrelated files untouched).

### 0.10 — Repository open + integrity checks

Reopening must prove:

- [x] `repository.toml` metadata valid and format supported.
- [x] `refs/accepted` exists and is syntactically valid.
- [x] `S0` exists and `hash(S0 bytes) == S0 ObjectId`.
- [x] `O1` exists.
- [x] Referenced object kinds match expected kinds.

Notes: split into (1) **canonical decoding** and (2) **repository open + integrity**.

1. `encoding/decode.rs` — `decode_canonical(&[u8]) -> Result<CanonicalObject, DecodingError>` as strict as the encoder. Pipeline: strict CBOR reader (definite lengths, shortest integers, canonical map-key order, no duplicate keys, valid UTF-8) → strict protocol tree → typed `CanonicalObject` → `CanonicalValidate` (sorted/unique collections). `DecodingError` is separate from `EncodingError` (UnexpectedEof, TrailingData, InvalidCbor, NonCanonicalEncoding, DuplicateMapKey, UnsupportedEnvelopeVersion, UnsupportedSchemaVersion, UnknownObjectKind, InvalidObjectShape, InvalidUuid, InvalidObjectId, InvalidOperation, InvalidCanonicalStructure). `ObjectStore::get` stays byte+hash only — decoding lives solely in `encoding`.
2. `repository/open.rs` — `open_repository(&Path) -> Result<Repository, RepositoryError>`: metadata → accepted ref → load `accepted.state` (hash + decode + require SemanticState) → load `state.ontology_version` (require OntologyVersion) → load every element/relationship version (require KnowledgeElementVersion / RelationshipVersion, correct even though S0 is empty) → if `accepted.change` present, load + require ChangeRevision and require `change.result_state == accepted.state`. `RepositoryError` gains NotFound, Decoding, UnexpectedObjectKind, AcceptedChangeStateMismatch.

The 10 `invalid/encoded/` fixtures are now executable: `tests/encoded_invalid.rs` walks them and asserts rejection (variant asserted where deterministic). `tests/vector_conformance.rs` adds the decode→encode round-trip invariant for all 10 valid fixtures. `tests/open.rs` has 14 integration tests (init→open, corrupt metadata, malformed ref, missing/tampered/wrong-kind state, missing/wrong-kind ontology, missing/wrong-kind change, change result-state mismatch, non-empty state kind verification, wrong-kind element version). 22 new tests total (`cargo test` 114 pass).

---

**Phase 0 complete** — a `kat init` repository can be closed and reopened with all integrity checks passing.

---

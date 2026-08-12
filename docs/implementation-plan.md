# KAT v0.1 Implementation Plan

This document is the working plan and progress tracker for implementing the KAT v0.1 prototype in Rust. It follows the implementation workflow and the phasing defined in `prototype-design.md` (Phase 0: Canonical Repository Substrate; Phase 1: First Semantic Vertical Slice).

Status: **Phase 0 complete** — steps 0.1-0.10 done: skeleton, identity primitives, canonical data model + structural validation, deterministic CBOR encoding, golden/negative vectors with a conformance harness, SHA-256 Object IDs, the immutable ObjectStore, repository metadata + `AcceptedRef`, `kat init`, and repository open + integrity checks (including strict canonical decoding). `cargo test` (114 passing), `cargo fmt --check`, and `cargo clippy -D warnings` all pass. `main` pushed to `origin/main` at step 0.9. A repository written by `kat init` can now be closed and reopened by a completely new process with all integrity checks passing.

Toolchain: Rust **stable**, pinned via `rust-toolchain.toml` (`channel = "stable"`); it resolves to each host's default target on both Linux and Windows. Machine-local toolchain flavour is set with `rustup override` and is not committed.

## Authoritative Sources

The implementation must not independently redefine semantics. Ground every decision in:

| Concern                                                                | Normative source                                                   |
| ---------------------------------------------------------------------- | ------------------------------------------------------------------ |
| Structural schema (object kinds, envelopes, operations, ordering)      | `spec/canonical-format.cddl`                                       |
| Encoding and semantics rules (deterministic CBOR, hashing, validation) | `docs/canonical-format.md`                                         |
| Physical design, repository layout, phases, error categories           | `docs/prototype-design.md`                                         |
| Semantic operations, change model, invariants                          | `docs/operations.md`, `docs/change-model.md`, `docs/invariants.md` |

## First Technical Target

Before building the full encoder, produce one tiny canonical fixture and make it pass permanently:

```text
KnowledgeElementVersion
        ↓
exact deterministic CBOR bytes
        ↓
known SHA-256 ObjectId
        ↓
test passes forever
```

The minimal fixture is a `KnowledgeElementVersion` with a fixed UUID, `kat.core/requirement` type, `active` lifecycle, and empty properties. Its authoritative bytes and ObjectId live in `spec/vectors/valid/knowledge-element-version-empty-properties.json` (already present). Once this passes, extend the encoder to the remaining canonical objects and generate the rest of the vector suite.

---

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

## Phase 1: First Semantic Vertical Slice

Per `prototype-design.md`, this milestone proves the semantic-evolution hypothesis. Move here immediately after Phase 0 rather than polishing the object store. The slice is complete only when all of the following work end to end:

```text
CreateElement
    ↓
KnowledgeElementVersion
    ↓
candidate SemanticState S1
    ↓
ontology validation
    ↓
invariant validation
    ↓
ChangeRevision C1
    ↓
persist
    ↓
CAS:
    {S0, none} -> {S1, C1}
    ↓
kat show
    ↓
kat history
```

### Scope for the first slice

Get exactly **one** semantic mutation — `CreateElement` — correct end to end. Do **not** start Phase 1 with `UpdateElement`, `DeprecateElement`, `Link`/`Unlink`, `Supersede`, impact analysis, or general history traversal. The goal changes from "can KAT store canonical objects?" to:

> Can KAT perform a valid semantic change against an accepted state and publish the result atomically?

### Work items (ordered sub-steps, mirroring the Change Application Flow in `prototype-design.md`)

- [x] **1.1 — Change application service.** A library entry point (the Change Engine) that resolves `refs/accepted`, loads the accepted SemanticState and Change head (via `open_repository`), selects the base state, and produces a **candidate** state. It must **not publish anything**; publication is a separate, explicit step.
  Notes: `repository/change.rs` — `prepare_change(&Repository) -> Result<ChangeContext, ChangeError>` returns `ChangeContext { accepted, base_state_id, base_state, ontology }` by resolving the accepted head and loading the base SemanticState + its OntologyVersion. Strictly **prepare-only**: no mutation, no persistence, no publication. `ChangeError` composes `ObjectStoreError`/`DecodingError`/`UnexpectedObjectKind`; further variants (preconditions, ontology, invariants) are added when their steps require them. New `repository/validation/{mod,ontology,invariant}.rs` declare the separate semantic-validation layer (ontology conformance and invariants land in 1.3/1.4; preconditions stay near the engine). 4 new tests (1 unit + 3 integration: context loads + no-mutation invariant, plan-acceptance base, integrity failures rejected at the open boundary before the engine runs). `cargo test` 118 pass, fmt/clippy clean.
- [x] **1.2 — `CreateElement` execution.** Given `ElementId`, `type_id`, `properties`: precondition — the `ElementId` must **not** already be active in the base state; construct a new `KnowledgeElementVersion` (Active lifecycle); validate its canonical form; stage/persist it (or keep staged until persist-before-publish); add `element_id -> new version ObjectId` to the candidate state.
  Notes: `repository/change.rs` — `apply_create_element(ChangeContext, CreateElementInput) -> Result<PreparedElementCreation, ChangeError>`. `PreconditionError::ElementAlreadyExists(ElementId)` rejects a present ID (active/deprecated/superseded — stricter than "not active", since v0.1 state maps one ID to its current version; resurrection is an explicit operation's concern). Constructs `KnowledgeElementVersion { element_id, type_id, lifecycle: Active, properties }`; caller supplies `ElementId` (deterministic engine). Properties are normalized into canonical key order (`cmp_encoded_text`, RFC 8949 §4.2.1) and duplicates rejected (`ChangeError::DuplicatePropertyKey`); nested-structure malformation is rejected fail-closed by `canonical_object_id`. V1's ObjectId is derived by encode-then-hash (not persisted); insertion into the candidate `SemanticState` is ordered by `ElementId` (not append+sort). Candidate keeps base `ontology_version`/`relationships`; base state and accepted ref unchanged. **No ontology conformance (1.3), no invariants (1.4), no persistence, no CAS.** `ChangeError` gains `Encoding`, `DuplicatePropertyKey`, `Precondition`. 6 new tests (5 unit + 1 integration no-persistence). `cargo test` 124 pass, fmt/clippy clean.
- [x] **1.3 — Minimal ontology validation.** For this slice only: **element type exists in the current `OntologyVersion`** (`state.ontology_version`). Do not build every future ontology rule; no relationship type checks yet.
  Notes: `repository/validation/ontology.rs` — `OntologyError::UnknownElementType(String)` and `validate_element_type(&OntologyVersion, type_id)`. Enforces only `element.type_id exists in ontology.element_types`; no relationship/constraint/property-schema/inheritance/architecture rules. The validator uses **only** the ontology referenced by the base state (`context.base_state.ontology_version` → `context.ontology`), never a global/hardcoded core ontology. `repository/change.rs` — `validate_create_element_ontology(PreparedElementCreation) -> Result<PreparedElementCreation, ChangeError>` composes it; `ChangeError` gains `Ontology(#[from] OntologyError)`. Validation does not mutate (candidate, element, ObjectStore, and accepted ref all unchanged). No invariant validation (1.4), no persistence, no ChangeRevision, no CAS. 7 new tests. `cargo test` 131 pass, fmt/clippy clean.
- [x] **1.4 — Minimal invariant validation.** Only what `CreateElement` and candidate-state correctness require:
  - stable identity: no duplicate `ElementId` in the candidate state;
  - valid lifecycle: the new version is Active;
  - referenced objects exist and object kinds match (the new version is a `KnowledgeElementVersion`);
  - candidate state internally coherent (structural canonical validity, per `encoding/validate.rs`).
  - The full `docs/invariants.md` groups (relationship, traceability, authority, validation, history) are **not** enforced in this slice.
  Notes: `repository/validation/invariant.rs` — `InvariantError` + `validate_create_element_invariants(&PreparedElementCreation)`. Validates the **candidate SemanticState**, not persistence (V1/S1 are not yet in the ObjectStore; persistence-existence is 1.6 / repository open/integrity). Checks: (1) structural canonical form via `CanonicalValidate` (wrapped as `InvalidCanonicalStructure`, not reimplemented); (2) `Active` lifecycle; (3) derived V1 identity == `element_version_id`; (4) candidate refs E1 → V1; (5) candidate == `base elements + exactly E1 → V1` (removing the added entry recovers base exactly — no removal/replacement/unrelated insertion/version change); (6) ontology reference preserved; (7) relationships preserved. No repeated ontology validation (1.3 owns it). `change.rs` consuming wrapper `validate_create_element_invariants` → `ChangeError::Invariant`. 12 new tests (11 unit incl. all failure modes + end-to-end no-side-effect). `cargo test` 143 pass, fmt/clippy clean.
- [x] **1.5 — Construct `ChangeRevision C1`.** `change_id` (new), `base_states = [S0]`, `result_state = S1`, `operations = [CreateElement(...)]`, `dependencies = []`, `description = None`.
  Notes: `repository/change.rs` — `prepare_change_revision(ValidatedElementCreation, change_id, description) -> PreparedChangeRevision { creation, state_id, change, change_revision_id }`. Still purely preparatory (no persistence). `state_id` = `canonical_object_id(SemanticState(candidate))`; `change` = `ChangeRevision { change_id, base_states: [base_state_id], result_state: state_id, operations: [CreateElement { new_version: element_version_id }], dependencies, description }`; `change_revision_id` = `canonical_object_id(ChangeRevision)`. Identity + description are caller-supplied (deterministic engine); engine never fabricates prose. `dependencies` is the accepted Change head (`context.accepted.change`) — `none → []`, `Some(Cn) → [Cn]` — so causal ancestry is recorded without hardcoding first-Change semantics. **Typestate guard**: `validate_create_element_invariants` now returns `ValidatedElementCreation` (wraps the prepared creation, exposes `prepared()`); `prepare_change_revision` accepts only `ValidatedElementCreation`, so a `ChangeRevision` cannot be prepared from an unvalidated candidate (compile-time pipeline guarantee). 4 new tests (3 unit: first-change deps, later-change `[Cn]` deps, description preserved; 1 integration e2e with the derived-identity assertions). `cargo test` 147 pass, fmt/clippy clean.
- [x] **1.6 — Persist before publication.** Order matters: immutable objects first (new `KnowledgeElementVersion`, `SemanticState S1`, `ChangeRevision C1`), then `refs/accepted` **last**.
  Notes: `repository/change.rs` — `persist_prepared_change(&Repository, PreparedChangeRevision) -> PersistedChange` materializes immutable objects strictly in dependency order **V1 → S1 → C1** and verifies each `ObjectStore::put` returns the exact pre-derived identity (`element_version_id`, `state_id`, `change_revision_id`). Identity mismatch is fail-closed via `ChangeError::PersistenceIdentityMismatch { kind, expected, actual }` (integrity/programming failure). Step 1.6 performs no CAS/publication and leaves `refs/accepted` byte-for-byte unchanged; partial persistence failures intentionally leave unreachable immutable objects (no rollback/GC). `PersistedChange` is now exported as the typestate boundary for later publication APIs. Added 3 integration tests in `tests/change.rs`: persisted objects exist and decode to expected V1/S1/C1 with IDs matching prepared values; accepted ref unchanged and fresh reopen still opens accepted `S0`; persisting the same prepared revision twice is idempotent with identical IDs and no extra object files. `cargo test -q` passes.
- [x] **1.7 — CAS publication.** Publish only if the accepted ref is still `{ state: S0, change: none }`. On success the new head is `{ state: S1, change: C1 }` and `C1.result_state == S1` holds. On conflict, return a conflict and leave the newly created immutable objects unreferenced (harmless — reachable by ObjectId, not the head).
  Notes: `repository/change.rs` — `publish_persisted_change(&Repository, PersistedChange) -> Result<PublishedChange, ChangeError>` performs the single CAS `expected = persisted.prepared.creation.context.accepted` → `new = { state: S1, change: Some(C1) }` and returns the new head (`PublishedChange { persisted, accepted }`). Before the CAS it verifies the publication-boundary invariant `prepared.change.result_state == prepared.state_id` (fail-closed via `ChangeError::PublicationStateMismatch { expected, actual }`; guaranteed by construction in 1.5/1.6, so a violation is an integrity/programming failure). `RefStoreError::Conflict` is surfaced as the domain-facing `ChangeError::Conflict` (other ref-store failures compose as `ChangeError::RefStore`); `Repository` now owns its `FileRefStore` and exposes `Repository::ref_store()` so publication can reach the CAS. `publish_persisted_change` requires a `PersistedChange` — a raw `PreparedChangeRevision` cannot reach the normal publication API (compile-time progression). No automatic retry, merge, rebase, conflict resolution, rollback of losing objects, or GC. 4 new integration tests (first publication → accepted {S1, C1} + fresh reopen resolving E1→V1; publication changes only refs/accepted — no new immutable objects; stale expected ref → `Conflict` with the concurrent winner kept; two writers from S0 → exactly one publication wins and the losing change's objects remain stored but unreachable; publication-boundary invariant rejection). `cargo test` 154 pass, fmt/clippy clean.
- [x] **1.8 — `kat show <element-id>`.** Resolve the element's current version from the accepted state and display it; proves S1 contains the new element.
  Notes: `repository/query.rs` — read-side query layer (`QueryError`, `ElementView`, `show_element(&Repository, ElementId)`). Resolves the **current** accepted ref at query time (point-in-time read: a handle that just published sees the new head without reopening), loads the SemanticState, binary-searches the element entries (canonically sorted), then loads + decodes + kind-checks the `KnowledgeElementVersion`. `QueryError` distinguishes `ElementNotFound(ElementId)`, `UnexpectedObjectKind`, and composed `ObjectStore`/`Decoding`/`RefStore` failures. Strictly read-only — objects and `refs/accepted` untouched. `domain/property.rs` gains `Display` for `PropertyValue` and `domain/element.rs` for `Lifecycle` (boring deterministic human rendering, documented as non-canonical). CLI `kat show <element-id>` wired as thin parse+dispatch (usage `kat init | kat show <element-id>`; `ElementNotFound` → friendly message, exit 1). 6 integration tests in `tests/query.rs` (view for published element incl. payload == persisted V1, `version_id` == accepted-state entry, unknown → `ElementNotFound`, wrong-kind → `UnexpectedObjectKind` via post-open corruption, fresh reopen, no-mutation invariant) + 3 CLI end-to-end tests in `tests/cli.rs` (spawns the real binary via `CARGO_BIN_EXE_kat`, zero new deps) + 2 unit Display tests. `cargo test` 165 pass, fmt/clippy clean.
- [ ] **1.9 — `kat history`.** First case only: accepted head `C1`, `C1.base_states = [S0]`, `C1.result_state = S1`; reconstructing this single linear head proves history works (no general traversal yet).

### Phase 1 acceptance test

One end-to-end scenario (integration test + CLI):

```text
kat init
    -> S0

kat create requirement ...
    -> element E1, version V1, state S1, change C1

reopen repository (fresh process)
    -> accepted.state == S1
    -> accepted.change == C1
    -> S1 contains E1 -> V1
    -> C1.result_state == S1

kat show E1
    -> resolves V1

kat history
    -> shows C1
```

This is the first test that validates KAT's actual thesis — a valid semantic change applied and published atomically — not just its repository mechanics.

### Definition of done for Phase 1

- [ ] `kat create requirement ...` performs a `CreateElement` change end to end.
- [ ] The candidate `SemanticState S1` is constructed and ontology-/invariant-validated.
- [ ] Accepted State and Change head are published atomically via CAS.
- [ ] A fresh process reopens the repository and verifies the new head (`accepted.state == S1`, `accepted.change == C1`, `C1.result_state == S1`, `S1` maps `E1 -> V1`).
- [ ] `kat show E1` resolves `V1`; `kat history` shows `C1`.
- [ ] The repository persists across executions.

---

## Progress Log

| Date       | Milestone / step completed                              | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| ---------- | ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 2026-08-11 | Plan created                                            | Phase 0 not yet started; no Rust implementation exists                                                                                                                                                                                                                                                                                                                                                                                                             |
| 2026-08-11 | Step 0.3 — Canonical data model + structural validation | Domain types (`property`, `element`, `relationship`, `ontology`, `operation`, `change`, `state`) mirroring the CDDL + typed `CanonicalObject` envelope (`encoding/object.rs`). `CanonicalValidate` structural checks (`encoding/validate.rs`): sorted/unique collections, non-empty base_states/operations, canonical property-key order. `cargo test` 32 pass, fmt/clippy clean.                                                                                  |
| 2026-08-11 | Step 0.1 — Rust project skeleton                        | `Cargo.toml` (edition 2024), `.gitignore`, `rust-toolchain.toml` (GNU), `src/{main,domain,encoding,repository}` wired. `cargo build`/`test`/`fmt`/`clippy -D warnings` clean.                                                                                                                                                                                                                                                                                      |
| 2026-08-11 | Step 0.2 — Identity primitives                          | `src/domain/identity.rs`: six typed UUID IDs + `ObjectId([u8;32])` with strict lowercase-hex parse. Deps: `uuid`/`hex`/`thiserror`. Added library+binary split (`src/lib.rs`). `cargo test` 12 pass, fmt/clippy clean.                                                                                                                                                                                                                                             |
| 2026-08-11 | Step 0.4 — Deterministic CBOR encoding                  | Explicit encoder (`encoding/cbor.rs`): primitive writer (shortest ints, definite lengths), tag-37 UUIDs, all five payloads, all six operations, property maps. `canonical_bytes()` validates then encodes (fail-closed). Corrected map-key comparator to encoded-byte ordering, shared validator+encoder. `cargo test` 48 pass, fmt/clippy clean.                                                                                                                  |
| 2026-08-11 | Step 0.5 — Golden + negative vectors                    | 10 valid fixtures (all kinds, all PropertyValue variants, integer boundaries, all six ops, ordering proofs, description present/absent), 6 `invalid/structural/` + 10 `invalid/encoded/` fixtures. `tests/vector_conformance.rs` harness (walks valid dir, asserts exact bytes); `tests/structural_invalid.rs` (canonical_bytes rejection). object_id values computed with independent SHA-256. `serde_json` dev-dep only. `cargo test` 58 pass, fmt/clippy clean. |
| 2026-08-11 | Step 0.6 — SHA-256 Object IDs                           | `encoding/hash.rs`: `object_id(&[u8]) -> ObjectId` (SHA-256) + thin `canonical_object_id`; no `ObjectId::new()`. Conformance harness asserts both bytes and externally-derived object_id for all 10 fixtures. `sha2` dep. `cargo test` 63 pass, fmt/clippy clean.                                                                                                                                                                                                  |
| 2026-08-11 | Step 0.7 — Immutable ObjectStore                        | `repository/object_store.rs`: `put`/`get`/`exists` over flat `objects/<64 hex>`; no-overwrite + concurrent same-bytes races harmless; `get` verifies ObjectId; `exists` physical only. `ObjectStoreError` {NotFound, Integrity, Io}. `tempfile` dev-dep. All 10 required test scenarios pass. `cargo test` 73 pass, fmt/clippy clean.                                                                                                                              |
| 2026-08-11 | Step 0.8 — `repository.toml` + `AcceptedRef`            | `repository/metadata.rs` (typed `RepositoryMetadata`, rejects unsupported/ malformed values) + `repository/ref_store.rs` (`AcceptedRef`, `RefStore` trait, `FileRefStore` with lock + atomic temp/rename CAS; no semantic interpretation). `toml` dep. 14 new tests. `cargo test` 88 pass, fmt/clippy clean.                                                                                                                                                       |
| 2026-08-11 | Step 0.9 — `kat init`                                   | `repository/init.rs` (`init_repository`, `initial_core_ontology` from spec), `repository/error.rs` (`RepositoryError`), thin CLI in `main.rs` (manual parsing). Atomic metadata write; accepted ref = publication point. Verified end-to-end via CLI; re-init rejected. `cargo test` 92 pass, fmt/clippy clean. Pushed to `origin/main` (3669acc..673b2d3).                                                                                                        |
| 2026-08-11 | Step 0.10 — Repository open + integrity checks          | `encoding/decode.rs` strict canonical decoder (`decode_canonical`, separate `DecodingError`) as strict as the encoder; `repository/open.rs` (`open_repository`, `Repository`; hash + kind + accepted-head invariant checks). `invalid/encoded/` fixtures now executable (`tests/encoded_invalid.rs`); valid-vector decode/encode round-trip; 14 open integration tests. `cargo test` 114 pass, fmt/clippy clean. **Phase 0 complete.**                             |
| 2026-08-12 | Step 1.1 — Change Engine: prepare-only                    | `repository/change.rs` (`prepare_change` → `ChangeContext { accepted, base_state_id, base_state, ontology }`, `ChangeError`); `repository/validation/{mod,ontology,invariant}.rs` declare the semantic-validation layer. Prepare resolves accepted, loads base state + ontology, and mutates nothing. `cargo test` 118 pass (3 integration tests prove object store + accepted ref unchanged), fmt/clippy clean. |
| 2026-08-12 | Step 1.2 — `CreateElement` execution                        | `repository/change.rs` (`apply_create_element`, `CreateElementInput`, `PreparedElementCreation`, `PreconditionError::ElementAlreadyExists`; `ChangeError` gains `Encoding`, `DuplicatePropertyKey`, `Precondition`). Operator-applies CreateElement into a candidate: Active V1, canonicalized+de-duplicated properties, derived V1 ObjectId (encode-then-hash, not persisted), ordered candidate insertion. Blocks ontology/invariant via scoping (unknown type still succeeds at 1.2). `cargo test` 124 pass, fmt/clippy clean. Not pushed (credential blocker). |
| 2026-08-12 | Step 1.3 — Minimal ontology validation                     | `repository/validation/ontology.rs` (`OntologyError::UnknownElementType`, `validate_element_type`); `change.rs` composes `validate_create_element_ontology`; `ChangeError::Ontology`. Enforces only `type_id ∈ ontology.element_types` against the **base-state-referenced** ontology (never a global core; custom-ontology test proves it). No mutatation; no invariants/persistence/Change/ CAS. `cargo test` 131 pass, fmt/clippy clean. Not pushed (credential blocker). |
| 2026-08-12 | Step 1.4 — Minimal invariant validation                    | `repository/validation/invariant.rs` (`InvariantError`, `validate_create_element_invariants(&PreparedElementCreation)`); `change.rs` consuming wrapper + `ChangeError::Invariant`. Validates the **candidate state** (not persistence): canonical structure (wrapped `CanonicalStructureError`), Active lifecycle, correct V1 identity + reference, candidate == base + exactly E1→V1, ontology + relationships preserved. 11 failure-mode unit tests + end-to-end no-side-effect. `cargo test` 143 pass, fmt/clippy clean. Not pushed (credential blocker). |
| 2026-08-12 | Step 1.5 — Construct `ChangeRevision C1`                   | `change.rs` `prepare_change_revision(ValidatedElementCreation, change_id, description) -> PreparedChangeRevision`. Derives S1 + C1 ObjectIds (encode-then-hash); `dependencies` from accepted head (`none→[]`, `Some→[Cn]`). Introduced `ValidatedElementCreation` (invariants stage now returns it) so a revision can only be built from a validated candidate (compile-time pipeline guard). 3 unit + 1 integration tests. `cargo test` 147 pass, fmt/clippy clean. Not pushed (credential blocker). |
| 2026-08-12 | Step 1.6 — Persist before publication                       | `change.rs` `persist_prepared_change(&Repository, PreparedChangeRevision) -> PersistedChange` persists immutable objects in strict order `V1 -> S1 -> C1` and verifies each returned store identity matches the prepared identity (`element_version_id`, `state_id`, `change_revision_id`), failing closed with `ChangeError::PersistenceIdentityMismatch` on mismatch. `refs/accepted` remains unchanged (no CAS/publication), and no rollback/GC is introduced for partial writes. Exported `PersistedChange` and `persist_prepared_change` from `repository/mod.rs`. Added 3 integration tests for object materialization+decode+ID equality, accepted-head immutability + reopen-at-S0, and idempotent double-persist with no extra objects. `cargo test` 150 pass, fmt/clippy clean. Not pushed (credential blocker). |
| 2026-08-12 | Step 1.7 — CAS publication                        | `change.rs` `publish_persisted_change(&Repository, PersistedChange) -> Result<PublishedChange, ChangeError>`: single CAS `expected = prepared.creation.context.accepted` → `new = {state: S1, change: Some(C1)}`, returning the new head in `PublishedChange { persisted, accepted }`. Pre-CAS defensive check `prepared.change.result_state == prepared.state_id` (fail-closed `ChangeError::PublicationStateMismatch`); `RefStoreError::Conflict` surfaced as domain `ChangeError::Conflict` (+ `ChangeError::RefStore` composition). `Repository` now owns its `FileRefStore` (`Repository::ref_store`). Publication requires `PersistedChange` — a raw `PreparedChangeRevision` cannot reach the normal publication API (compile-time progression). No retry/merge/rollback/GC. 4 new integration tests: first publication + fresh reopen (accepted {S1, C1}, E1→V1), publication changes only refs (no new objects), stale expected → Conflict with concurrent winner kept, two writers from S0 → exactly one winner with loser objects stored-but-unreachable, publication-boundary invariant rejection. `cargo test` 154 pass, fmt/clippy clean. Not pushed (credential blocker). |
| 2026-08-12 | Step 1.8 — `kat show` read-side query                        | `repository/query.rs` (`QueryError`, `ElementView`, `show_element`): resolves the **current** accepted ref at query time, loads the SemanticState, binary-searches `elements`, then loads + decodes + kind-checks the `KnowledgeElementVersion`. `QueryError` = `ElementNotFound` / `UnexpectedObjectKind` / composed `ObjectStore`+`Decoding`+`RefStore`; strictly read-only. `Display` added for `PropertyValue` (domain/property.rs) and `Lifecycle` (domain/element.rs) — boring deterministic rendering, documented non-canonical. CLI `kat show <element-id>` wired (thin parse+dispatch; element-not-found → exit 1; usage now `kat init | kat show <element-id>`). 6 query integration tests (`tests/query.rs`, incl. wrong-kind rejection via post-open corruption and no-mutation invariant) + 3 CLI end-to-end tests (`tests/cli.rs`, spawns the real binary via `CARGO_BIN_EXE_kat`, zero new deps) + 2 display unit tests. `cargo test` 165 pass, fmt/clippy clean. Not pushed (credential blocker). |

## Non-goals during this work (do not build yet)

Remote repositories, network synchronization, distributed collaboration, branching, automatic merge/conflict resolution, CRDTs, artifact generation, materialization, persistent query databases, graph databases, object packing, garbage collection, compression, AI integration, plugins, and architecture-specific modeling. See `docs/prototype-design.md` → v0.1 Non-Goals.

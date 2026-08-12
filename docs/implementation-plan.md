# KAT v0.1 Implementation Plan

This document is the working plan and progress tracker for implementing the KAT v0.1 prototype in Rust. It follows the implementation workflow and the phasing defined in `prototype-design.md` (Phase 0: Canonical Repository Substrate; Phase 1: First Semantic Vertical Slice).

Status: **Phase 0 in progress** — steps 0.1 (skeleton) and 0.2 (identity primitives) complete. `cargo test` (12 passing), `cargo fmt --check`, and `cargo clippy -D warnings` all pass.

Toolchain: Rust **stable GNU `x86_64-pc-windows-gnu` 1.97.1**, pinned via `rust-toolchain.toml` (MSVC Build Tools are not installed on this machine; MinGW-w64 provides the linker).

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

- [ ] `PropertyValue` (null / bool / int / text / byte string / UUID / list / map; **no floating point**).
- [ ] `KnowledgeElementVersion` (element_id, type_id, lifecycle, properties).
- [ ] `RelationshipVersion` (relationship_id, source, relationship_type, target, properties).
- [ ] `OntologyVersion` (ontology_id, element_type_definitions, relationship_type_definitions).
- [ ] `SemanticState` (ontology_version, sorted element entries, sorted relationship entries).
- [ ] `ChangeRevision` (change_id, base_states, result_state, operations, dependencies, optional description).
- [ ] `Operation` — the six operation encodings: CreateElement(1), UpdateElement(2), DeprecateElement(3), Link(4), Unlink(5), Supersede(6).
- [ ] `CanonicalObject` envelope: `envelope_version = 1`, `object_kind`, `schema_version = 1`, `payload`; Object IDs are **not** encoded inside the object.

### 0.4 — Deterministic CBOR encoding

The highest-risk technical component. Implement RFC 8949 Core Deterministic Encoding (Section 4.2.1):

- [ ] Definite-length collections only (no indefinite-length arrays, maps, or strings).
- [ ] Shortest integer encoding.
- [ ] UUID as CBOR tag 37 containing exactly 16 bytes.
- [ ] Map keys ordered by bytewise lexicographic comparison of their deterministic encoded forms (RFC 8949 §4.2.1 — not the RFC 7049 length-first alternative, §4.2.3).
- [ ] Exact numeric envelope / object-kind / operation identifiers from the CDDL.
- [ ] Sorted Semantic State entries (lexicographic over raw 16-byte IDs).
- [ ] Sorted ontology element/relationship type sets (lexicographic over UTF-8 type_id).
- [ ] No floating-point property values.
- [ ] Property key ordering follows deterministic CBOR map ordering.

Reference: `docs/canonical-format.md` → Deterministic CBOR Requirements & Canonical Collections.

### 0.5 — Golden test vectors (created alongside encoding, not after)

- [ ] Fixtures live in `spec/vectors/valid/` and `spec/vectors/invalid/`.
- [ ] First useful test: `known logical object → exact expected CBOR bytes → exact expected SHA-256`.
- [ ] Cover per `spec/vectors/README.md`: every object kind, UUID tag 37, integer boundaries, empty/non-empty property maps, nested values, Semantic State ordering, ontology ordering, all six operations, ChangeRevision, empty initial Semantic State.
- [ ] Invalid vectors: indefinite-length, non-shortest integers, wrong map ordering, duplicate map keys, invalid UUID, kind/payload mismatch, malformed ObjectIds, unsorted state entries, duplicate semantic IDs, unsupported versions.

### 0.6 — SHA-256 Object IDs

- [ ] API: `canonical_bytes(object) -> Vec<u8>` and `object_id(bytes) -> ObjectId`.
- [ ] Test: same canonical object → same bytes → same ObjectId.
- [ ] Test: changed canonical content → different ObjectId.

### 0.7 — Immutable ObjectStore

- [ ] Layout: `.kat/objects/<sha256>` (no fan-out in v0.1).
- [ ] API: `put(canonical_bytes) -> ObjectId`, `get(ObjectId) -> bytes`, `exists(ObjectId)`.
- [ ] `put` must never overwrite different bytes; an existing object with the same ObjectId needs no write.
- [ ] `get` supports integrity verification (SHA-256 of bytes must equal requested ObjectId).

### 0.8 — `repository.toml` and `AcceptedRef`

- [ ] `repository.toml`: `format_version = 1`, `repository_id`, `software_id`, `object_encoding = "cbor-deterministic-v1"`, `hash_algorithm = "sha256"`. Dynamic state/history must **not** live here.
- [ ] `AcceptedRef`: `{ state: StateId, change: Option<ChangeRevisionId> }`.
- [ ] Compare-and-swap publication abstraction (strongest test comes with the first Change).
- [ ] Locked/temp-file atomic replace behind a dedicated ref-storage abstraction.

### 0.9 — `kat init`

First user-visible command (`docs/cli.md` contract). Must:

- [ ] Verify no existing KAT repository is overwritten.
- [ ] Generate RepositoryId and SoftwareId.
- [ ] Create `.kat/` directory structure.
- [ ] Write `repository.toml`.
- [ ] Create core `OntologyVersion O1`, persist it.
- [ ] Create empty `SemanticState S0`, persist it.
- [ ] Publish `refs/accepted` → `{ state: S0, change: none }`.
- [ ] Result: a repository that can be closed and reopened.

### 0.10 — Repository open + integrity checks

Reopening must prove:

- [ ] `repository.toml` metadata valid and format supported.
- [ ] `refs/accepted` exists and is syntactically valid.
- [ ] `S0` exists and `hash(S0 bytes) == S0 ObjectId`.
- [ ] `O1` exists.
- [ ] Referenced object kinds match expected kinds.

---

**Phase 0 complete** when a `kat init` repository can be closed and reopened with all integrity checks passing.

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

### Work items

- [ ] `CreateElement` operation (element must not already be active in base state).
- [ ] Minimal ontology validation (types exist; relationship source/target types allowed).
- [ ] Candidate `SemanticState S1` construction.
- [ ] Minimal required invariant validation (identity, relationship, lifecycle, change, traceability, authority, validation, history groups per `docs/invariants.md`).
- [ ] `ChangeRevision C1` referencing `result_state = S1`.
- [ ] Persist new immutable objects (KnowledgeElementVersion, SemanticState, ChangeRevision).
- [ ] CAS publish `{S0, none} -> {S1, C1}`; invariant `accepted.change.result_state == accepted.state`.
- [ ] `kat show` — inspect resulting knowledge.
- [ ] `kat history` — reconstruct history from accepted Change head, Change Revisions, and Semantic States.

### Definition of done for Phase 1

- [ ] KAT can create a knowledge element through a Change.
- [ ] A new Semantic State is constructed and validated.
- [ ] Accepted State and Change head are published atomically.
- [ ] Resulting knowledge and history can be inspected via `kat show` and `kat history`.
- [ ] Repository persists across executions.

---

## Progress Log

| Date       | Milestone / step completed       | Notes                                                                                                                                                                                                                  |
| ---------- | -------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-08-11 | Plan created                     | Phase 0 not yet started; no Rust implementation exists                                                                                                                                                                 |
| 2026-08-11 | Step 0.1 — Rust project skeleton | `Cargo.toml` (edition 2024), `.gitignore`, `rust-toolchain.toml` (GNU), `src/{main,domain,encoding,repository}` wired. `cargo build`/`test`/`fmt`/`clippy -D warnings` clean.                                          |
| 2026-08-11 | Step 0.2 — Identity primitives   | `src/domain/identity.rs`: six typed UUID IDs + `ObjectId([u8;32])` with strict lowercase-hex parse. Deps: `uuid`/`hex`/`thiserror`. Added library+binary split (`src/lib.rs`). `cargo test` 12 pass, fmt/clippy clean. |

## Non-goals during this work (do not build yet)

Remote repositories, network synchronization, distributed collaboration, branching, automatic merge/conflict resolution, CRDTs, artifact generation, materialization, persistent query databases, graph databases, object packing, garbage collection, compression, AI integration, plugins, and architecture-specific modeling. See `docs/prototype-design.md` → v0.1 Non-Goals.

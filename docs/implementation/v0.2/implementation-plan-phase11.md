# Phase 11 Implementation Plan: Discovery — `kat list`, ID-prefix resolution, relationship-aware `kat show`

> Part of the [v0.2 master plan](../implementation-plan.md).

## Purpose

Phase 11 delivers the **Discovery** pillar of v0.2.0. The v0.1 real-world experiment proved that requiring users to maintain a scratch UUID map is unacceptable beyond toy repositories. This phase removes that friction and enables local semantic navigation:

1. **`kat list`** — enumerate knowledge elements, filtered by type and lifecycle, with a compact default table.
2. **Unique-prefix ID resolution** — accept a unique prefix of any stable semantic ID as _input_ on every identity-taking command.
3. **Relationship-aware `kat show`** — `show` becomes the local semantic inspection command (incoming/outgoing relationships), explicitly distinct from `trace` (transitive provenance) and `impact` (transitive consequences).

Phase 11 is **strictly read-side**: no repository mutation, no canonical-format change, no new operations. It preserves v0.1 semantics exactly.

---

## 1. Frozen Design & Semantics

### 1.1 `kat list` — element enumeration

- **Read-only**: enumerates elements present in the **current accepted `SemanticState`** ($S_n$), in deterministic canonical `ElementId` order.
- **Interface** (clap subcommand `List`):

  ```
  kat list
  kat list --type requirement
  kat list --lifecycle active
  kat list requirement          # positional shorthand == --type requirement
  ```

  Filters compose (`--type` AND `--lifecycle`). Default = all elements, all lifecycles.

- **Type resolution**: `--type` accepts the same short-name → canonical-ID resolution as `kat create` (e.g. `requirement` → `kat.core/requirement`); fully-qualified type IDs pass through; unknown type → clear error, exit 1.
- **Lifecycle values**: `active`, `deprecated`, `superseded` (case-insensitive, matching `Lifecycle` display).
- **Default output** — compact table (stable header, one row per element):

  ```
  ID                                    TYPE             STATE       TITLE
  7af83d1c-...                           requirement      active      User must authenticate
  41bc98e2-...                           constraint       active      TLS 1.3 required
  ```

  Empty repository → header only (or a short "no elements" note), exit 0.

- **Semantics**: `kat list` never mutates; it must be safe against concurrent writers (reads the live accepted ref once per invocation).

### 1.2 Unique-prefix ID resolution

- **Display abbreviation ≠ input resolution.** v0.2 only introduces abbreviated _input_; existing display behaviour (full UUIDs) is unchanged in Phase 11.
- **Rule** for any identity domain:

  ```
  0 matches   -> ElementNotFound (or domain-appropriate not-found)
  1 match     -> resolve
  >1 matches  -> ambiguous prefix, reject
  ```

- **Resolution domain is command-specific** (a prefix resolves only within the identity domain the command consumes):
  - `kat show <prefix>` → `ElementId` only
  - `kat unlink <prefix>` → `RelationshipId` only
  - `kat update/deprecate/supersede/trace/impact <prefix>` → `ElementId`
  - `kat history --element <prefix>` (Phase 12) → `ElementId`
- **Candidate set**: prefixes resolve against the IDs **currently present in the accepted state** for that domain (elements in $S_n$, relationships in $S_n$, changes in history reachable from the head). A prefix of 0 length is never accepted.
- **Canonical identities are unchanged** — resolution is a pure input convenience; the engine continues to receive full IDs.
- **Implementation shape**: a resolution layer (e.g. `resolve_id_prefix` helpers in the query layer) used by the CLI adapter; the engine (`src/repository/change.rs`) stays prefix-agnostic.
- Phase 11 implements resolution for **`ElementId` and `RelationshipId`** (the IDs every current command needs). `ChangeId`/`ObjectId` resolution is deferred (not required by any current command).

### 1.3 Relationship-aware `kat show`

- `kat show <id>` keeps its current element rendering and **adds** an `Incoming` / `Outgoing` relationship section.
- **Scope**: only relationships currently present in $S_n.relationships$ (consistent with `trace`/`impact` scoping; unlinked relationships are history-only).
- **Grouping**: by relationship type (canonical short type name), each row = relationship ID + the other endpoint's title (or "no title").
- **Distinction (normative)**: `show` = local neighborhood (one hop); `trace` = transitive provenance; `impact` = transitive consequences. `show` never walks transitively.
- Example rendering (default detail mode):

  ```
  Requirement
    id:     7af83d1c
    state:  active
    title:  User must authenticate using MFA

  Relationships
    Incoming
      motivates
        c091eb3a  Secure authentication portal
      validates
        a18f7721  Authentication acceptance test
    Outgoing
      none
  ```

---

## 2. Work Breakdown & Implementation Steps

> Each step is atomic: implement, add tests, then **validate** (`cargo test`, `cargo fmt --check`, `cargo clippy -D warnings`) before starting the next step. Commit after validation.

### Step 11.1 — Query layer: `list_elements`

- `src/repository/query.rs`: `ListFilter { type_id: Option<TypeId>, lifecycle: Option<Lifecycle> }` and `list_elements(&Repository, ListFilter) -> Result<Vec<ElementView>, QueryError>`.
- Read-only; resolves the live accepted ref, loads $S_n$, filters `S_n.elements`, loads + decodes + kind-checks each `KnowledgeElementVersion`, returns views in canonical `ElementId` order.
- Re-export `ListFilter` + `list_elements` from `src/repository/mod.rs`.
- **Tests** (`tests/query.rs`): empty repo → `[]`; all elements default; `--type` filter; `--lifecycle` filter; combined filters; unknown element type error (via existing type resolution); wrong-kind version → `UnexpectedObjectKind`; no-mutation (ObjectStore + accepted ref unchanged).
- **Validation**: `cargo test` pass, fmt/clippy clean.

### Step 11.2 — CLI: `kat list`

- `src/cli.rs`: clap `List` subcommand with `--type <type>`, `--lifecycle <active|deprecated|superseded>`, and positional shorthand (`kat list requirement`). Short-name type resolution reused from `kat create` (`resolve_element_type`); unknown type/lifecycle → exit 1 with clear message.
- `src/main.rs`: thin dispatch to `list_elements`; compact table renderer (stable aligned columns).
- **Tests** (`tests/cli.rs`): full list flow after init+create (init=0 elements → create req → create constraint → list all/type/lifecycle/shorthand); list outside repo → error; unknown type/lifecycle → exit 1; no-mutation.
- Update `docs/cli.md`.
- **Validation**: `cargo test` pass, fmt/clippy clean.

### Step 11.3 — Resolution layer: unique-prefix ID resolution

- New `src/repository/resolve.rs` (or in `query.rs`): `resolve_element_id_prefix(&Repository, &str) -> Result<ElementId, ResolveError>` and `resolve_relationship_id_prefix(&Repository, &str) -> Result<RelationshipId, ResolveError>`.
- `ResolveError`: `NotFound`, `Ambiguous { candidates: Vec<Id> }`, `InvalidPrefix`. Exact full-ID input must also work (full ID = valid prefix).
- Resolution scans the accepted state's ID domain; ambiguity is **rejected explicitly** (never silently pick).
- Re-export from `src/repository/mod.rs`.
- **Tests** (`tests/query.rs` or new `tests/resolve.rs`): full ID; unique prefix (first 8 hex chars); ambiguous prefix → error listing candidates; no-match → NotFound; exact-vs-prefix boundary; no-mutation.
- **Validation**: `cargo test` pass, fmt/clippy clean.

### Step 11.4 — Wire prefix resolution into identity-taking commands

- CLI adapters for `show`, `update`, `deprecate`, `supersede`, `link`, `unlink`, `trace`, `impact` resolve their ID argument through the domain-appropriate resolver before calling the engine/query.
- Engine and query APIs remain unchanged (still receive full IDs); resolution happens in the CLI layer only.
- **Tests** (`tests/cli.rs`): prefix `show`, prefix `update`, prefix `unlink` (RelationshipId domain), ambiguous prefix → exit 1 with ambiguity message; no-match → exit 1 not-found.
- Update `docs/cli.md` (document prefix input).
- **Validation**: `cargo test` pass, fmt/clippy clean.

### Step 11.5 — Relationship-aware `kat show`

- Extend `ElementView` (or add `ElementNeighborhood`) with `incoming: Vec<RelationshipView>` / `outgoing: Vec<RelationshipView>` (each: relationship id, type, other endpoint id + title).
- `show_element` gains the neighborhood from $S_n.relationships$ (one hop, grouped by type, deterministic order). Strictly read-only.
- `src/main.rs`: render the `Relationships` block (Incoming/Outgoing groups). Existing element fields unchanged.
- **Tests** (`tests/query.rs` + `tests/cli.rs`): incoming only; outgoing only; both; none; deprecated/superseded endpoints still shown; unlinked relationships NOT shown; no-mutation; end-to-end CLI rendering.
- Update `docs/cli.md`.
- **Validation**: `cargo test` pass, fmt/clippy clean.

### Step 11.6 — Acceptance verification & Phase 11 closure

- End-to-end acceptance flow in `tests/cli.rs` (`phase11_acceptance_cli_flow_end_to_end`):
  - init → create requirement → create constraint → create artifact → link (decision `addresses` requirement) → verify:
    - `kat list` shows all elements in canonical order;
    - `kat list requirement` / `kat list --lifecycle active` / combined filters correct;
    - `kat show <8-char prefix>` resolves uniquely and renders element + Incoming/Outgoing relationships;
    - ambiguous prefix rejected; no-match prefix rejected;
    - `kat list`/`kat show` leave ObjectStore and `refs/accepted` byte-for-byte unchanged;
    - fresh reopen reproduces identical output.
- All Definition-of-Done items checked. `cargo test`, `cargo fmt --check`, `cargo clippy -D warnings` clean. **Phase 11 Frozen.**

---

## 3. Acceptance Scenario

```text
kat init
kat create requirement --title "User must authenticate"
kat create constraint  --title "TLS 1.3 required"
kat create decision    --title "Use WebAuthn"
kat link addresses <decision> <requirement>

kat list                                -> all three rows, canonical order
kat list requirement                    -> requirement row only
kat list --lifecycle active             -> all three
kat show <8-char prefix of requirement> -> element + Incoming (addresses: decision) block
kat show <ambiguous-prefix>             -> exit 1: ambiguous
kat unlink <prefix-of-relationship-id>  -> resolves in RelationshipId domain
```

---

## 4. Definition of Done for Phase 11

- [x] `kat list` enumerates accepted elements with `--type`, `--lifecycle`, and positional type shorthand; unknown type/lifecycle → exit 1.
- [x] `kat list` output is a stable compact table; empty repo handled; strictly read-only.
- [x] Unique-prefix input resolution works for `ElementId` and `RelationshipId` with the 0/1/>1 rule; ambiguity rejects explicitly.
- [x] Prefix resolution wired into `show`, `update`, `deprecate`, `supersede`, `link`, `unlink`, `trace`, `impact`.
- [x] `kat show` renders incoming/outgoing relationships (one hop, grouped by type/direction) distinct from `trace`/`impact`.
- [x] Canonical identities and the canonical format are unchanged; no repository mutation anywhere in Phase 11.
- [x] `docs/cli.md` documents `kat list`, prefix input, and the `show` neighborhood.
- [x] All steps validated (`cargo test`, `fmt --check`, `clippy -D warnings`) and committed atomically.

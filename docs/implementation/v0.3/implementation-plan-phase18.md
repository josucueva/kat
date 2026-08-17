# Phase 18 Implementation Plan: Scalable Query Inspection — Bounded & Collapsed `kat trace` / `kat impact`

> Part of the [v0.3 master plan](implementation-plan.md).
>
> Status: **COMPLETED** — Implemented, verified (396 tests passing), documented, committed, and pushed to `main`.

## Purpose

Phase 18 delivers **Scalable Query Inspection** for `kat trace` and `kat impact`. The real-project evaluation (`docs/implementation/v0.3/experiment.md` and `docs/implementation/v0.3/experiment-analysis.md`) showed that exhaustive path enumeration produces large multi-path outputs (19 KB for 40 elements), leading to path redundancy and cognitive overload.

Phase 18 refactors trace and impact query processing and presentation:

1. **Query Evaluation Bounding (`--max-depth <N>`)** — bounds traversal at the Query Engine evaluation level to stop expansion after $N$ relationship hops ($N \ge 1$), making query evaluation deterministic, efficient, and semantically honest.
2. **Collapsed Path Tree Default** — renders Trace results as a hierarchical path projection that merges identical leading path prefixes into shared visual branches, while preserving nodes reached through distinct parent paths under each relevant branch.
3. **Exhaustive Path Rendering (`--paths`)** — preserves discrete path list rendering when explicitly requested for `kat trace`.
4. **Normative Traversal Policies Invariant** — underlying origin trace and impact propagation policies defined in [`docs/specification/operations.md`](../specification/operations.md) remain completely frozen.

Phase 18 is **strictly read-side**: no repository mutation, no canonical format change, zero object creation.

---

## 1. Frozen Design & Semantics

### 1.1 Query Engine Scope vs Presentation Rendering

- **Accepted State Read Isolation**:
  - `Trace` and `Impact` operate exclusively over the accepted `SemanticState` ($S_n$). An open local draft session does not alter query results.

- **Query Engine Scope (`max_depth: Option<usize>`)**:
  - Operating at the Query Engine layer in `src/repository/query.rs`.
  - **Depth 0**: Queried root element.
  - **Depth 1**: Elements reached after 1 relationship traversal.
  - **Depth $N$**: Elements reached after $N$ relationship traversals.
  - When `max_depth = Some(N)` ($N \ge 1$), nodes at depth $N$ are included in the evaluated result graph, but their outgoing traversal expansion is not performed.
  - Traversal depth bounds in KAT are positive hop counts. Specifying `--max-depth 0` returns `QueryError::InvalidMaxDepth(0)`.

- **Actual Truncation Condition**:
  - Supplying `max_depth = Some(N)` does not imply truncation unless traversal expansion was actively suppressed. Truncation indicators apply only when unvisited outgoing relationships exist at depth $N$ that were omitted due to the `max_depth` boundary. If the graph naturally terminates at depth $M \le N$, the query result is complete and untruncated.

- **Path-Local Cycle Prevention**:
  - Graph traversal checks `visited_rels` per path branch to prevent cyclic looping (e.g. $A \to B \to A \to B$), while permitting elements reachable via multiple distinct parent paths (e.g. $A \to B \to D$ and $A \to C \to D$) to be explored under each branch.

- **Result Graph vs Rendering Separation**:
  - `trace_origin` and `analyze_impact` return structured query result objects (`TraceResult`, `ImpactResult`).
  - Renderers process the result objects into:
    - Default collapsed path tree (`to_tree()`) with shared common path prefixes.
    - `--paths` explicit discrete path enumeration (all paths present in the $N$-hop evaluated query result graph).
    - Compact table/single-line view (`--compact`).
  - **Trace vs Impact Asymmetry**: `kat trace` supports `--paths` for discrete provenance path enumeration. `kat impact` focuses on category partitions (`directly_changed`, `semantically_affected`, `affected_artifacts`) with supporting propagation context; `--paths` is intentionally omitted from `kat impact`.

### 1.2 CLI Grammar & Flag Ownership

```bash
kat trace <element> [--paths] [--max-depth <N>] [--compact]
kat impact <element> [--max-depth <N>] [--compact]
```

- **`kat trace <element>`** (Default): Renders a collapsed ASCII tree hierarchy starting at the queried element.
- **`kat trace <element> --paths`**: Renders all discrete linear paths found in the query result graph.
- **`kat trace <element> --max-depth <N>`**: Bounds origin trace expansion to $N$ hops.
- **`kat impact <element>`** (Default): Renders partitioned impact targets with supporting propagation context for each impacted element.
- **`kat impact <element> --max-depth <N>`**: Bounds impact propagation to $N$ hops.

### 1.3 Formatting Specifications

#### Default Collapsed Tree View (`kat trace payment_service.rs`)

```text
payment_service.rs (kat.core/artifact)
└── [represents] Payment Processing (kat.core/implementation)
    ├── [<- guides] Event-Driven Payment Architecture (kat.core/design-decision)
    │   └── [addresses ->] Asynchronous Payment Processing (kat.core/requirement)
    │       └── [<- motivates] Immediate Checkout Confirmation (kat.core/intent)
    └── [<- restricts] Non-Blocking Checkout Policy (kat.core/constraint)
```

If outgoing traversal expansion was suppressed at depth $N$ because additional eligible relationships existed:
```text
... (depth limit reached: 2 hops)
```

#### `--paths` View (`kat trace payment_service.rs --paths`)

```text
TRACE ORIGIN
  root_element_id: 37c891f2-70b9-4a92-9118-2e86bf12019a
  paths_found:     2

Path 1 (3 steps):
  [0] payment_service.rs (kat.core/artifact)
      --[represents]-> Payment Processing (kat.core/implementation)
  [1] Payment Processing (kat.core/implementation)
      <-[guides]-- Event-Driven Payment Architecture (kat.core/design-decision)
  [2] Event-Driven Payment Architecture (kat.core/design-decision)
      --[addresses]-> Asynchronous Payment Processing (kat.core/requirement)

Path 2 (2 steps):
  [0] payment_service.rs (kat.core/artifact)
      --[represents]-> Payment Processing (kat.core/implementation)
  [1] Payment Processing (kat.core/implementation)
      <-[restricts]-- Non-Blocking Checkout Policy (kat.core/constraint)
```

#### Compact Tree View (`kat trace payment_service.rs --compact`)

```text
payment_service.rs
└── represents -> Payment Processing
    ├── guides <- Event-Driven Payment Architecture
    │   └── addresses -> Asynchronous Payment Processing
    └── restricts <- Non-Blocking Checkout Policy
```

---

## 2. Work Breakdown & Implementation Steps

> Each step is atomic: implement, add tests, then **validate** (`cargo test`, `cargo fmt --check`, `cargo clippy -D warnings`) before starting the next step. Commit after validation.

### Step 18.1 — Query Engine Evaluation Bounding & Result DTO Extensions

- `src/repository/query.rs`:
  - Add `QueryError::InvalidMaxDepth(usize)` error variant.
  - Update `trace_origin` signature: `trace_origin(&Repository, element_query: &str, max_depth: Option<usize>) -> Result<TraceResult, QueryError>`.
  - Update `analyze_impact` signature: `analyze_impact(&Repository, element_query: &str, max_depth: Option<usize>) -> Result<ImpactResult, QueryError>`.
  - Update recursive traversal functions (`explore_origin_paths`, `explore_impact_paths`) to accept depth counter and stop expansion when `current_path.len() == max_depth`.
  - Validate `max_depth` parameter: if `Some(0)`, return `Err(QueryError::InvalidMaxDepth(0))`.
  - Add `TraceTree` structural helper for converting `TraceResult` graph into hierarchical tree nodes.
- Unit tests in `tests/query.rs`:
  - Verify `trace_origin` with `max_depth = Some(1)`, `Some(2)`, `None`.
  - Verify `analyze_impact` with `max_depth = Some(1)`, `Some(2)`, `None`.
  - Verify `max_depth = Some(0)` returns `QueryError::InvalidMaxDepth(0)`.
  - Verify read-only non-mutation invariant across query execution.

### Step 18.2 — CLI Wiring & Collapsed Tree / Path Output Formatters

- `src/cli.rs`:
  - Update `Command::Trace`: add `paths: bool` and `max_depth: Option<usize>`.
  - Update `Command::Impact`: add `max_depth: Option<usize>`.
- `src/main.rs`:
  - Update `run_trace` and `run_impact` command handlers.
  - Implement `print_trace_tree` (default tree renderer with ASCII tree connectors).
  - Implement `print_trace_paths` (`--paths` exhaustive path renderer).
  - Implement `print_trace_tree_compact` (`--compact` tree renderer).
  - Update `print_impact` formatters to include tree propagation details and honor `--max-depth`.
- Integration tests in `tests/cli.rs`:
  - Test `kat trace <element>` default tree output.
  - Test `kat trace <element> --paths` exhaustive path output.
  - Test `kat trace <element> --max-depth 1`.
  - Test `kat trace <element> --max-depth 0` (fails with error message).
  - Test `kat impact <element> --max-depth 1`.
  - Test `kat trace <element> --compact` and `kat impact <element> --compact`.

### Step 18.3 — Specification Updates & Phase 18 Closure

- Update [`docs/specification/operations.md`](../specification/operations.md) to document `max_depth` bounding semantics for `Trace` and `Impact`.
- Update [`docs/vision/architecture.md`](../vision/architecture.md) and [`docs/implementation/v0.3/implementation-plan.md`](implementation-plan.md).
- Add `phase18_acceptance_cli_flow_end_to_end` acceptance test suite in `tests/cli.rs`.
- Verify full workspace validation (`cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`).

# Phase 18 Implementation Plan: Scalable Query Inspection — Bounded & Collapsed `kat trace` / `kat impact`

> Part of the [v0.3 master plan](implementation-plan.md).

## Purpose

Phase 18 delivers **Scalable Query Inspection** for `kat trace` and `kat impact`. The real-project evaluation (`docs/implementation/v0.3/experiment.md`) showed that exhaustive path enumeration produces large multi-path outputs (19 KB for 40 elements), leading to path redundancy and cognitive overload.

This phase refactors trace and impact rendering:

1. **Collapsed Tree/Graph Default** — renders shared subgraphs as a collapsed tree hierarchy instead of enumerating every independent path.
2. **`--paths` Flag** — preserves exhaustive path list rendering when explicitly requested.
3. **`--max-depth <N>` Flag** — bounds graph traversal depth for large models.
4. **Traversal Semantics Invariant** — rendering options alter presentation structure only; underlying origin trace and impact propagation policies defined in `operations.md` remain unchanged.

Phase 18 is **strictly read-side**: no repository mutation, no canonical format change.

---

## 1. Frozen Design & Semantics

### 1.1 Collapsed Tree Rendering (Default)

- **Default presentation** for `kat trace <element>`: renders a clean, deduplicated hierarchy tree rooted at the query element:

  ```text
  payment_service.rs (Artifact)
  └── [represents] Payment Processing (Implementation)
      ├── [<- guides] Event-Driven Payment Architecture (Design Decision)
      │   └── [addresses ->] Asynchronous Payment Processing (Requirement)
      │       └── [<- motivates] Immediate Checkout Confirmation (Intent)
      └── [<- restricts] Non-Blocking Checkout Policy (Constraint)
  ```

- Shared intermediate nodes are displayed once with indented children rather than repeating full paths from the root.

### 1.2 `--paths` & `--max-depth <N>` Flags

- **`kat trace <element> --paths`**: outputs the complete, un-collapsed list of discrete provenance paths (matching v0.1/v0.2 behavior).
- **`kat trace <element> --max-depth <N>`**: limits traversal depth to $N$ hops ($N \ge 1$). Nodes beyond depth $N$ are truncated with a visual indicator `... (depth limit reached)`.
- **Composition**: `--max-depth` can be combined with default tree mode or `--paths` mode.

### 1.3 `kat impact` Rendering Alignment

- **Default Tree Presentation**: directly changed element root $\to$ semantically affected elements $\to$ accountable artifacts rendered in a deduplicated tree format.
- **`kat impact <element> --max-depth <N>`**: bounds impact propagation depth to $N$ hops.

---

## 2. Work Breakdown & Implementation Steps

> Each step is atomic: implement, add tests, then **validate** (`cargo test`, `cargo fmt --check`, `cargo clippy -D warnings`) before starting the next step. Commit after validation.

### Step 18.1 — Query layer: `TraceResult` tree structural projection & depth limiting

- `src/repository/query.rs`:
  - Extend `TraceOptions { max_depth: Option<usize>, exhaustive_paths: bool }`.
  - Add `TraceTree` node representation building a deduplicated directed tree from traversal results.
  - Implement depth limiting in `trace_origin` and `analyze_impact`.
- Unit tests in `tests/query.rs`.

### Step 18.2 — CLI wiring: `kat trace` and `kat impact` tree rendering

- `src/main.rs`:
  - Add `--paths` and `--max-depth <N>` flags to `Trace` and `Impact` CLI commands.
  - Implement ASCII tree renderer for `TraceTree`.
  - Update compact and default rendering modes.
- Integration tests in `tests/cli.rs`.

### Step 18.3 — Phase 18 Closure & End-to-End Acceptance Test

- Add `phase18_acceptance_cli_flow_end_to_end` test verifying default tree rendering, `--paths`, and `--max-depth` options.
- Verify `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings`.

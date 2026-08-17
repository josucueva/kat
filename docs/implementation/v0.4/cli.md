# KAT v0.4 CLI Specification

## Status

Draft.

This document defines the concrete command grammar, flags, standard stream behavior, exit codes, and CLI presentation conventions for KAT v0.4.

It is derived from:

- the v0.4 foundation documents ([`findings.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/findings.md), [`requirements.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/requirements.md), [`use-cases.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/use-cases.md), [`operations.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/operations.md), [`reference-model.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/reference-model.md));
- the interaction model ([`interaction-model.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/interaction-model.md));
- the authoring infrastructure model ([`authoring-model.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/authoring-model.md));
- the context model ([`context-model.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/context-model.md));
- the graph quality model ([`graph-quality-model.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/graph-quality-model.md));
- the machine interface specification ([`machine-interface.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/machine-interface.md)).

---

# 1. Command Hierarchy & Classification

KAT v0.4 organizes commands into three explicit tiers:

```text
┌────────────────────────────────────────────────────────────────────────┐
│ 1. PORCELAIN COMMANDS (Everyday Task Workflows)                        │
│    kat status                                                          │
│    kat context <root>...                                               │
│    kat author [file]                                                   │
│    kat check                                                           │
│    kat commit                                                          │
│    kat abort                                                           │
├────────────────────────────────────────────────────────────────────────┤
│ 2. ADVANCED INSPECTION (Targeted Analysis & Schema Discovery)          │
│    kat show <element>                                                  │
│    kat trace <element>                                                 │
│    kat impact <element>                                                │
│    kat artifacts [artifact]                                            │
│    kat ontology [show <type>]                                          │
│    kat history <target>                                                │
├────────────────────────────────────────────────────────────────────────┤
│ 3. PLUMBING PRIMITIVES (Direct Canonical Graph Mutations)              │
│    kat create <type> --title <title>                                   │
│    kat update <element>                                                │
│    kat deprecate <element>                                             │
│    kat supersede <existing> <replacement>                              │
│    kat link <type> <source> <target>                                   │
│    kat unlink <relationship>                                           │
│    kat account <artifact>                                              │
└────────────────────────────────────────────────────────────────────────┘
```

---

# 2. Global Options & Flags

The following global options apply across query and mutation commands:

```text
Global Flags:
      --json               Emit machine-readable JSON envelope to stdout
      --compact            Emit single-line compact human text output
      --max-depth <N>      Limit traversal depth to N relationship hops (queries only)
  -h, --help               Print help
  -V, --version            Print KAT version
```

---

# 3. Porcelain Command Grammar & Syntax

## 3.1 `kat status`
Inspect active draft transaction state and accepted repository head.

```text
Usage: kat status [OPTIONS]

Options:
      --json      Emit StatusResultDTO JSON envelope to stdout
      --compact   Display compact single-line dashboard summary
```

---

## 3.2 `kat context`
Retrieve bounded, semantically grouped development context around one or more root elements.

```text
Usage: kat context [OPTIONS] <ROOT>...

Arguments:
  <ROOT>...  One or more accepted element IDs (UUID or prefix)

Options:
      --max-depth <N>   Limit traversal depth (default: 2 hops)
      --direction <DIR> Both [default], Upstream, Downstream
      --json            Emit ContextResultDTO JSON envelope to stdout
      --compact         Display compact single-line path summaries
```

---

## 3.3 `kat author`
Express declarative semantic changes to be staged into the open Change transaction.

```text
Usage: kat author [OPTIONS] [FILE]

Arguments:
  [FILE]  Optional path to declarative authoring file (reads from stdin if omitted)

Options:
      --description <TEXT>  Optional Change description (sets or updates draft description)
      --json                Emit AuthorResultDTO JSON envelope to stdout
```

---

## 3.4 `kat check`
Run a comprehensive repository health evaluation.

```text
Usage: kat check [OPTIONS]

Options:
      --json      Emit CheckResultDTO JSON envelope to stdout
      --compact   Display compact single-line health summary
```

---

## 3.5 `kat commit`
Publish candidate working state $S_{\text{working}}$ to accepted repository head.

```text
Usage: kat commit [OPTIONS]

Options:
      --description <TEXT>  Optional final Change description override
      --json                Emit CommitResultDTO JSON envelope to stdout
```

---

## 3.6 `kat abort`
Discard open draft transaction, working state, and declared workflow references.

```text
Usage: kat abort [OPTIONS]

Options:
      --json      Emit success envelope to stdout
```

---

# 4. Standard Streams (stdout / stderr) Policy

To guarantee clean script and agent pipeline integration, KAT enforces strict standard stream rules:

## 4.1 When `--json` is Active
1. **stdout**: Contains **exactly one** valid UTF-8 JSON result envelope (`CommonResultEnvelope<T>`). No prose, banners, ANSI color codes, or human text shall ever be printed to stdout when `--json` is specified.
2. **stderr**: Used exclusively for process diagnostics, logging, or panic messages. Never mixed with stdout.

## 4.2 When Human Output is Active (Default or `--compact`)
1. **stdout**: Formatted terminal presentation text (tables, indented trees, block key-value pairs).
2. **stderr**: Diagnostic notes, progress indicators, or warning notes (e.g. accountability target-version alignment note).

---

# 5. Exit Code Policy & Execution Status

KAT enforces a clear distinction between **Operation Execution Status** and **Domain Health Outcome**:

```text
Operation Execution Status (Envelope success boolean)
    !=
Domain Health Outcome (kat check mechanical violations)
    !=
Process Exit Code Policy
```

## Exit Code Rules

| Exit Code | Meaning | Triggers |
| :--- | :--- | :--- |
| `0` | **Success** | Operation executed successfully AND (for `kat check`) mechanical violations count equals `0`. |
| `1` | **Failure / Violation** | Mechanical validation violation detected (`kat check` or `kat validate` failed), execution error, or candidate state precondition failure. |
| `2` | **Usage Error** | Invalid CLI flag, missing mandatory argument, or malformed CLI syntax. |

## `kat check --json` Exit Code Behavior

When `kat check --json` is executed:
- If `kat check` completes the evaluation cleanly, `success == true` in the JSON result envelope on `stdout`.
- If mechanical violations exist in Section 1 of the report, the JSON envelope is emitted to `stdout` (`success == true`), but the process exit code is **`1`**.
- This allows external scripts to parse the complete structured violation payload from `stdout` while still detecting health failure via standard shell `$?` exit code checks.

> **Advisory Rule**: Advisory Graph Quality findings (`GQ-01` through `GQ-04`) shall **NEVER** cause a non-zero exit code.

---

# 6. Summary Matrix of KAT v0.4 Commands

| Command | Tier | Default Mode | `--json` DTO Payload | Exit 1 Trigger |
| :--- | :--- | :--- | :--- | :--- |
| `kat status` | Porcelain | Formatted text | `StatusResultDTO` | Draft session stale / I/O error |
| `kat context` | Porcelain | Categorized tree | `ContextResultDTO` | Unknown root / invalid ID |
| `kat author` | Porcelain | Staged summary | `AuthorResultDTO` | Declarative syntax / precondition error |
| `kat check` | Porcelain | 4-section report | `CheckResultDTO` | Mechanical violations > 0 |
| `kat commit` | Porcelain | Commit summary | `CommitResultDTO` | Candidate validation error / Stale draft |
| `kat abort` | Porcelain | Abort confirmation | Success envelope | No open draft session |
| `kat show` | Inspection | Element block | `ShowResultDTO` | Element not found |
| `kat trace` | Inspection | Collapsed tree | `TraceResultDTO` | Element not found |
| `kat impact` | Inspection | Impact table | `ImpactResultDTO` | Element not found |
| `kat artifacts` | Inspection | Status table | `ArtifactsResultDTO` | Invalid filter / artifact ID |
| `kat ontology` | Inspection | Type list | `OntologyResultDTO` | Unknown type ID |
| `kat history` | Inspection | Revision log | `HistoryResultDTO` | Target not found |
| `kat create` | Plumbing | Created block | `MutationResponseDTO` | Unknown element type |
| `kat link` | Plumbing | Link block | `MutationResponseDTO` | Disallowed endpoint types |

---

# 7. Next Specification Stage

With the detailed design suite complete (`authoring-model.md`, `interaction-model.md`, `context-model.md`, `graph-quality-model.md`, `machine-interface.md`, `cli.md`), the next phase is preparing the execution specifications:

```text
docs/implementation/v0.4/implementation-plan.md
```

It shall define:
- step-by-step Rust module architecture (`src/cli/`, `src/porcelain/`, `src/query/context.rs`, `src/query/quality.rs`);
- acceptance integration test suite plan;
- phased delivery roadmap for KAT v0.4.

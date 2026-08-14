# KAT v0.1.1 (Knowledge Abstraction Tracker)

**KAT** (**Knowledge Abstraction Tracker**) is a **semantic software repository**: a specification-first system for representing, evolving, tracing, and validating software as _knowledge_ rather than as source-code files.

KAT treats the specification — intent, requirements, constraints, design decisions, and their relationships — as the authoritative knowledge of a software system. Source code, tests, configuration, and documentation are artifacts that represent, implement, validate, or materialize that knowledge. They remain traceable to it, but they never independently redefine the intended state of the software.

**Status**: KAT v0.1.1 Release — Complete specification-first substrate with immutable semantic change, history reconstruction, provenance tracing, impact analysis, consistency validation, version-relative artifact accountability, plus `clap`-based CLI UX polish, per-command help, generated UNIX man pages, and shell completions.

---

## Key Capabilities (v0.1.1)

* **Knowledge Representation**: First-class Intent, Requirement, Constraint, Design Decision, Implementation, Artifact, and Validation elements with stable UUID identities and immutable CBOR-encoded version Object IDs.
* **Immutable Semantic Evolution**: Authoritative change engine supporting 6 primary operations (`create`, `update`, `deprecate`, `supersede`, `link`, `unlink`) published atomically via compare-and-swap state refs (`refs/accepted`).
* **History Reconstruction**: Complete dependency-graph reconstruction of accepted change history (`kat history`).
* **Origin Traceability**: Traversal back to authoritative origin (`kat trace <element-id>`) along relation-specific provenance policies.
* **Impact Analysis**: Propagation of potential change consequences (`kat impact <element-id>`) partitioned into directly changed elements, semantically affected elements, and affected artifacts.
* **Consistency Validation**: Mechanical evaluation of ontology rules and invariant conditions (`kat validate`), with explicit reporting of natural-language constraints as `unverified`.
* **Artifact Accountability**: History-derived baseline version resolution (`kat artifacts`), detecting when upstream authoritative knowledge has evolved since artifact accountability was established.

---

## Installation & Quick Start

### Build & Install

```bash
cargo build --release
cargo install --path .
```

### CLI Help and Generated Assets

KAT v0.1.1 includes structured command help through `clap`:

```bash
kat --help
kat create --help
kat trace --help
```

UNIX man pages and shell completion scripts are generated from the same CLI definition:

```bash
cargo run --bin generate_assets
```

Generated files are written to:

```text
generated/man/
generated/completions/
```

See [`docs/install.md`](docs/install.md) for installation instructions.

### Quick Walkthrough Example

```bash
# 1. Initialize a new KAT repository in the current workspace
kat init

# 2. Create authoritative knowledge elements
kat create constraint --title "TLS 1.3 Encryption Required"
# -> element_id: <C1_UUID>

kat create design-decision --title "Use PASETO Tokens for AuthX"
# -> element_id: <D1_UUID>

kat create implementation --title "AuthX Token Verifier Module"
# -> element_id: <M1_UUID>

kat create artifact --title "authx-core-v1.jar"
# -> element_id: <A1_UUID>

# 3. Establish traceability relationships
kat link restricts <C1_UUID> <D1_UUID>
kat link realizes <M1_UUID> <D1_UUID>
kat link represents <A1_UUID> <M1_UUID>

# 4. Perform read-side queries
kat trace <M1_UUID>       # Traces origin back to design decision D1 and constraint C1
kat impact <D1_UUID>      # Evaluates potential consequences if D1 changes
kat validate             # Evaluates semantic model consistency & reports unverified constraints
kat artifacts            # Reports artifact accountability status (CURRENT / STALE / UNACCOUNTED)

# 5. Evolve knowledge and observe accountability divergence
kat update <M1_UUID> --title "AuthX Token Verifier Module v2"
kat artifacts            # Artifact authx-core-v1.jar transitions to STALE status

# 6. Re-account artifact via explicit re-linking
kat unlink <RELATIONSHIP_ID>
kat link represents <A1_UUID> <M1_UUID>
kat artifacts            # Artifact authx-core-v1.jar returns to CURRENT status

# 7. Reconstruct immutable change history
kat history
```

---

## Supported Commands

| Command | Description |
|---|---|
| `kat init` | Initialize a `.kat/` repository in the current directory. |
| `kat create <type> --title "..."` | Create a new knowledge element (`requirement`, `constraint`, `design-decision`, `implementation`, `artifact`, etc.). |
| `kat update <element-id> --title "..."` | Update properties on an existing active element (advances version ObjectId). |
| `kat deprecate <element-id>` | Mark an active element as Deprecated. |
| `kat supersede <existing-id> <replacement-type> --title "..."` | Supersede an existing element with a new replacement element. |
| `kat link <type> <source-id> <target-id>` | Establish a semantic relationship (`motivates`, `addresses`, `realizes`, `represents`, `restricts`, `validates`, `guides`, `depends-on`). |
| `kat unlink <relationship-id>` | Remove a relationship from current accepted state. |
| `kat show <element-id>` | Inspect resolved active element details. |
| `kat history` | Output accepted change revision dependency graph. |
| `kat trace <element-id>` | Perform origin traversal for an element. |
| `kat impact <element-id>` | Evaluate potential change impact. |
| `kat validate` | Run mechanical consistency validation across accepted state. |
| `kat artifacts` | Evaluate artifact accountability baselines against current accepted state. |

---

## Explicit v0.1 Limitations

* **Consistency Validation**: Consistency rules defined by KAT's ontology and semantic invariants are mechanically evaluated. Constraint knowledge elements written in natural language (without executable code rules) are reported as `unverified` rather than assumed satisfied or violated.
* **Artifact Accountability**: `CURRENT` status in `kat artifacts` indicates that no accountability-baseline divergence has been detected relative to the accepted change history ($S_{\text{link}}$). It does **not** imply that KAT has automatically inspected, parsed, or verified physical file contents (such as source code files, binaries, or PDF documents).
* **Scope Limitations**: Distributed synchronization, branching, remote repositories, AI knowledge extraction, automatic code generation, and automatic reconciliation are explicitly excluded from v0.1.

---

## Documentation

* [docs/philosophy.md](docs/philosophy.md) — What KAT is and why.
* [docs/ontology.md](docs/ontology.md) — Knowledge element and relationship types.
* [docs/requirements.md](docs/requirements.md) — Functional requirements and scope limitations.
* [docs/operations.md](docs/operations.md) — Semantic operations specification.
* [docs/cli.md](docs/cli.md) — CLI invocation contract.
* [docs/v0-1-release-acceptance-review.md](docs/v0-1-release-acceptance-review.md) — Full v0.1 release acceptance review.


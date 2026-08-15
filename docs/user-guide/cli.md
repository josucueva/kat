# CLI Reference

## Purpose

The CLI defines how users invoke KAT behavior.

It is an invocation contract only:
* [`docs/specification/operations.md`](../specification/operations.md) defines semantic operations and read queries.
* `cli.md` defines how users invoke that behavior.
* `cli.md` must not redefine an operation's semantics.

CLI syntax does not contaminate the operation model: `operations.md` remains free of command-line syntax, and CLI ergonomics may evolve without changing underlying semantic behavior.

---

## Command Classes

CLI commands fall into two classes, matching the classification in `operations.md`.

### Semantic Commands

Commands that invoke semantic operations or read queries defined by `operations.md`.

**Mutation Commands**:
* `kat create`
* `kat update`
* `kat deprecate`
* `kat supersede`
* `kat link`
* `kat unlink`
* `kat account`

**Query Operations**:
* `kat list`
* `kat show`
* `kat status`
* `kat trace`
* `kat impact`
* `kat history`
* `kat artifacts`

**Validation Operations**:
* `kat validate`

### Repository / Tooling Commands

Commands that manage repository initialization or transaction session state.

* `kat init`
* `kat change begin`
* `kat change status`
* `kat change commit`
* `kat change abort`

---

## Authority

All mutation commands must route through the Change Engine. The CLI must not bypass ontology validation, invariant validation, or atomic publication, and it must not directly modify accepted semantic state.

---

## Command Syntax (v0.2)

```text
kat init
kat status [--compact]
kat list [<type>] [--type <type>] [--lifecycle <active|deprecated|superseded>]
kat create <type> --title "..." [--description "..."]
kat update <element-id-or-prefix> [--title "..."] [--description "..."]
kat deprecate <element-id-or-prefix>
kat supersede <existing-id-or-prefix> <replacement-type> --title "..." [--description "..."]
kat link <relationship-type> <source-id-or-prefix> <target-id-or-prefix> [--description "..."]
kat unlink <relationship-id-or-prefix> [--description "..."]
kat account <artifact-id-or-prefix> [--description "..."]
kat show <element-id-or-prefix> [--compact]
kat history [--oneline] [--compact] [--limit N] [--element <element-id-or-prefix>]
kat trace <element-id-or-prefix> [--compact]
kat impact <element-id-or-prefix> [--compact]
kat validate [--compact]
kat artifacts [--compact]

kat change begin [--description "..."]
kat change status [--compact]
kat change commit
kat change abort
```

---

## Multi-Operation Change Transactions

KAT supports multi-operation change transactions:

* `kat change begin [--description "..."]`: Opens a local transaction draft session initialized at the currently accepted repository state $S_n$.
* When a draft session is open, mutation commands (`create`, `update`, `deprecate`, `supersede`, `link`, `unlink`, `account`) automatically stage operations onto the working candidate state $S_{\text{working}}$.
* `kat change status`: Displays open draft details, staged operations, and candidate consistency preview.
* `kat change commit`: Validates candidate state $S_{\text{working}}$ and atomically accepts the staged Change as one `ChangeRevision`.
* `kat change abort`: Discards the local draft session. Standard read and validation commands (`status`, `list`, `show`, `history`, `trace`, `impact`, `validate`, `artifacts`) inspect accepted state $S_n$ only unless explicitly inspecting draft session state via `kat change status`.

See [`multi-op-change-design.md`](../implementation/v0.2/multi-op-change-design.md) for complete transactional semantics.

---

## Unique-Prefix ID Resolution

All identity-taking CLI commands (`show`, `update`, `deprecate`, `supersede`, `link`, `unlink`, `account`, `trace`, `impact`, `history --element`) accept either a full 36-character hyphenated UUID or a unique prefix of at least **8 hexadecimal digits**.

* **Resolution Scope**:
  * **Query, read, and validation commands**: Resolve prefixes strictly against current accepted state entries ($S_n$).
  * **Mutation commands during an open draft**: Resolve prefixes against the working candidate state ($S_{\text{working}}$) so that newly staged elements can be consumed by subsequent staged operations.
* **Type-Scoped Resolution**:
  * Element commands resolve `ElementId` prefixes against elements.
  * `link` accepts two element ID arguments (`source-id-or-prefix` and `target-id-or-prefix`) and resolves both against elements.
  * `unlink` resolves `RelationshipId` prefixes against relationships in state.
  * `account` resolves `artifact-id-or-prefix` against element IDs (and semantically requires the resolved element to be of type `kat.core/artifact`).
* **Historical Traversal Boundary**: For `kat history --element <id-or-prefix>`, prefix resolution occurs against the current accepted `SemanticState` ($S_n$). Once resolved to an `ElementId`, history filtering traverses the complete historical change revision graph.
* **Ambiguity Rejection**: If a prefix matches multiple candidates in the target resolution scope, execution is rejected with an explicit error listing matching candidates.

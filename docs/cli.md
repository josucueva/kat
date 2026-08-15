# CLI

## Purpose

The CLI defines how users invoke KAT behavior.

It is an invocation contract only:

* `operations.md` defines semantic behavior.
* `cli.md` defines how users invoke that behavior.
* `cli.md` must not redefine an operation's semantics.

CLI syntax therefore does not contaminate the operation model: `operations.md` remains free of command-line syntax, and CLI ergonomics may evolve without changing semantic behavior.

## Command Classes

CLI commands fall into two classes.

### Semantic Commands

Commands that invoke semantic operations defined by `operations.md`.

```text
kat create
kat update
kat deprecate
kat supersede
kat link
kat unlink

kat trace
kat impact
kat explain
kat history

kat validate
```

### Repository / Tooling Commands

Commands that inspect or manage repository state. They do not represent semantic operations.

```text
kat init
kat status
kat show
kat object show
kat state show
```

For example, `kat status` inspects repository and tool state; it does not mutate or define semantic knowledge and therefore does not appear in `operations.md`.

## Syntax

The exact argument syntax is a CLI concern and may evolve without changing semantic behavior.

Initial syntax sketch:

```text
kat init

kat create requirement --title "..." [--description "..."]
kat update <element-id> [--title "..."] [--description "..."]
kat deprecate <element-id>
kat supersede <existing-id> <replacement-type> --title "..." [--description "..."]
kat link <relationship-type> <source-element-id> <target-element-id> [--description "..."]
kat unlink <relationship-id> [--description "..."]

kat trace <element-id> [--direction backward] [--type <relationship-type>]
kat impact <element-id>
kat explain <element-id>
kat history <element-id>

kat validate

kat status
kat show <element-id>
kat object show <object-id>
kat state show <state-id>
```

## Authority

All mutation commands must route through the Change Engine. The CLI must not bypass ontology validation, invariant validation, or atomic publication, and it must not directly modify accepted semantic state.

## Implemented Command Syntax (v0.1)

```text
kat init
kat status
kat list [<type>] [--type <type>] [--lifecycle <active|deprecated|superseded>]
kat create <type> --title "..." [--description "..."]
kat update <element-id> [--title "..."] [--description "..."]
kat deprecate <element-id>
kat supersede <existing-id> <replacement-type> --title "..." [--description "..."]
kat link <relationship-type> <source-id> <target-id> [--description "..."]
kat unlink <relationship-id> [--description "..."]
kat show <element-id> [--compact]
kat history [--oneline] [--compact] [--limit N] [--element <element-id-or-prefix>]
kat trace <element-id> [--compact]
kat impact <element-id> [--compact]
kat validate [--compact]
kat artifacts [--compact]

kat change begin [--description "..."]
kat change status
kat change commit
kat change abort
```

## Multi-Operation Change Transactions (v0.2 Design)

KAT supports multi-operation change transactions:
- `kat change begin [--description "..."]`: Opens a local transaction session (`.kat/draft.json`) initialized at accepted $S_n$.
- When a draft transaction is open, mutation commands (`create`, `update`, `deprecate`, `supersede`, `link`, `unlink`) automatically stage operations onto the draft candidate $S_{\text{working}}$.
- `kat change status`: Displays open draft details, staged operations, and candidate consistency preview.
- `kat change commit`: Validates candidate $S_{\text{working}}$, persists canonical objects, publishes single `ChangeRevision` via CAS ($S_n \to S_{\text{new}}$), and cleans up `.kat/draft.json`.
- `kat change abort`: Discards draft session `.kat/draft.json`. Standard read commands (`status`, `list`, `show`, `history`, `trace`, `impact`, `validate`, `artifacts`) inspect accepted state $S_n$ only.

See [`docs/v0-2-multi-op-change-design.md`](v0-2-multi-op-change-design.md) for complete transactional semantics.

## Unique-Prefix ID Resolution (v0.2)

All identity-taking CLI commands (`show`, `update`, `deprecate`, `supersede`, `link`, `unlink`, `trace`, `impact`, `history --element`) accept either a full 36-character hyphenated UUID or a unique prefix of at least **8 hexadecimal digits**.

- **Type-Scoped**: Element commands resolve `ElementId` prefixes against current accepted elements; `unlink` resolves `RelationshipId` prefixes against current accepted relationships.
- **Accepted State Only Resolution**: Prefixes resolve strictly against current accepted state entries ($S_n$), ignoring unlinked or historical objects.
- **Historical Traversal Boundary**: For `kat history --element <id|prefix>`, prefix resolution resolves the current `ElementId`. Once resolved, history filtering inspects the complete historical change revision graph, including historical operations involving relationships that are no longer part of $S_n$.
- **Ambiguity**: If a prefix matches multiple candidates, execution is rejected with an explicit error listing matching candidates.


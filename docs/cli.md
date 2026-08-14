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
kat unlink <source-id> --type <relationship-type> <target-id>

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

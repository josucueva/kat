# KAT

KAT is a **semantic software repository**: a specification-first system for representing, evolving, tracing, and validating software as *knowledge* rather than as source-code files.

KAT treats the specification — intent, requirements, constraints, design decisions, and their relationships — as the authoritative knowledge of a software system. Source code, tests, configuration, and documentation are artifacts that represent, implement, validate, or materialize that knowledge. They remain traceable to it, but they never independently redefine the intended state of the software.

Status: KAT v0.1 is in specification. The conceptual model, architecture, and canonical storage format are defined; the Rust prototype is not yet implemented.

## Repository layout

```text
kat/
├── README.md
├── version.txt
├── spec/
│   ├── canonical-format.cddl   normative structural schema (CDDL)
│   └── vectors/                golden and negative format test vectors
└── docs/
    ├── philosophy.md           what KAT is and why
    ├── concepts.md             core concept definitions
    ├── first-principles.md     fundamental assumptions
    ├── non-goals.md            what KAT fundamentally does not want to become
    ├── domain-model.md         core entities and relationships
    ├── ontology.md             knowledge element and relationship types
    ├── requirements.md         v0.1 functional requirements and scope
    ├── use-cases.md            user-facing scenarios
    ├── operations.md           semantic operations
    ├── change-model.md         evolution of authoritative knowledge
    ├── invariants.md           conditions every accepted state must satisfy
    ├── materialization-model.md  knowledge-to-artifact relationship
    ├── collaboration-model.md  multi-participant evolution
    ├── repository-model.md     conceptual repository semantics
    ├── architecture.md         logical architecture
    ├── prototype-design.md     physical implementation design (Rust)
    ├── canonical-format.md     normative protocol semantics and encoding rules
    └── cli.md                  CLI invocation contract
```

## Documentation layering

Higher-level specifications define semantics; lower-level specifications refine them but must not independently redefine them.

```text
Philosophy / Concepts
        ↓
Domain / Ontology / Requirements
        ↓
Operations / Change / Invariants
        ↓
Repository / Architecture
        ↓
Canonical Format / Prototype Design
        ↓
CLI / Implementation
```

For example, `spec/canonical-format.cddl` defines UUID as CBOR tag 37, and `prototype-design.md` may repeat that fact for implementation context, but it cannot choose tag 38 later. Similarly, `cli.md` may expose `kat supersede`, but it cannot redefine what Supersede means semantically.

## Non-goals vs. scope

`non-goals.md` answers *"What does KAT fundamentally not want to become?"* `requirements.md` (Scope Limitations) answers *"What capabilities are excluded from v0.1?"* The two are deliberately kept separate: a capability excluded from v0.1, such as branching, may appear in a later release without making KAT a Git replacement.

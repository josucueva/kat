## Scenario D: Machine Interface Consumption

### Result

PASS

A small external client successfully consumed KAT's versioned JSON
machine interface without parsing human-readable CLI output.

The client used structured output from `status`, `check`, and `context`
to derive:

- accepted repository state and inventory;
- mechanical repository cleanliness;
- constraint evidence coverage and uncovered constraints;
- Artifact accountability and affected identities;
- GraphQuality diagnostics grouped by rule;
- semantic context around a known Requirement;
- Artifact anchors and structured graph relationships.

The client inspected `interface_schema_version = 1` and distinguished
command execution success from repository health semantics.

Controlled domain/state failures returned structured machine envelopes
with `success: false` and stable error codes (`ResolveError` and
`NotInRepository`).

No human-output parsing, regular-expression parsing, or table scraping
was required.

### Finding

F-MACHINE-01: (RESOLVED in v0.4.4) Machine-output coverage has been completed across all 8 Inspection subcommands (`list`, `show`, `history`, `trace`, `impact`, `artifacts`, `ontology`, `validate`), fully satisfying schema version 1 machine envelope coverage (`interface_schema_version = 1`).
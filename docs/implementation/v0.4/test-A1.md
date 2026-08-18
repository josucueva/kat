## Scenario A1: Statit Semantic Authoring Replay

### Result

PARTIAL FAILURE

KAT successfully represented and validated the Statit project, but the
v0.4 porcelain authoring workflow did not become the actor's natural
construction path.

The actor independently attempted to discover `kat author`, probed several
declarative input representations, and then reverted to direct `kat create`
and `kat link` orchestration through an external Python helper.

### Observed metrics

- Accepted Changes: 4
- Knowledge Elements: 84
- Relationships: 90
- Mechanical violations: 0
- Manual/external UUID mappings: 84
- External orchestration script: yes
- Direct primitive graph authoring: yes
- Primitive Exposure: approximately 1.0
- Target Primitive Exposure: <= 0.05

### Primary finding

The repository engine and semantic primitives support successful large-scale
authoring, but the v0.4 `author` porcelain does not yet provide a sufficiently
discoverable and usable declarative authoring contract to replace primitive
graph orchestration.

### Supporting observations

1. `kat author` input grammar was not discoverable from normal CLI exploration.
2. Unsupported declarative-looking input could result in a successful
   zero-operation authoring invocation.
3. UUID capture and reuse remained necessary after falling back to plumbing.
4. Large-scale authoring again caused an external orchestration script to emerge.
5. Ontology errors correctly reject invalid triples but provide limited recovery
   guidance.
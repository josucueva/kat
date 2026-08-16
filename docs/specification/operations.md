# Operations

## Purpose

Operations define the actions that can be performed on the KAT semantic model.

An operation represents a meaningful action involving software knowledge. Operations are defined independently from the interface used to invoke them.

Operations are divided into three categories:

* Mutation operations
* Query operations
* Validation operations

---

## Operation Model

KAT uses the term operation in two related senses:

* **Semantic mutation operations** are canonical operations that may appear in a `ChangeRevision`.
* **Query and validation operations** are read-side semantic actions. They inspect accepted knowledge without mutating state and do not appear in `ChangeRevision.operations`.

Mutation operations are specified using:
* **Preconditions**: Conditions that must be satisfied for the mutation to be valid.
* **Candidate Effects**: Semantic effects applied to the candidate state $S_{\text{working}}$.
* **Postconditions**: Semantic properties guaranteed after successful application.

Query and validation operations are specified by their inputs and results and do not produce candidate-state effects.

Sequential composition of operations into atomic Changes and transaction mechanics are defined in [`docs/change-model.md`](change-model.md).

---

## Mutation Operations

Mutation operations produce accepted semantic changes. They may modify the candidate `SemanticState`, record semantic history, or both.

A mutation operation is evaluated against a candidate semantic state $S_{\text{working}}$. Multiple mutation operations may be composed sequentially within a Change.

---

### CreateElement

Creates a new knowledge element.

**Preconditions:**
* The requested `element_id` must not exist in $S_{\text{working}}$.
* The requested `type_id` must exist in the active `OntologyVersion`.
* Property keys must be unique.

**Candidate Effects:**
* Produces a new `KnowledgeElementVersion` object ($V_{\text{initial}}$) with `lifecycle = Active`.
* Adds entry `(element_id, V_{\text{initial}})` to candidate element state map.

**Postconditions:**
* The new `ElementId` identifies one logical knowledge element and remains stable across subsequent versions of that element.
* The candidate state maps the new `ElementId` to $V_{\text{initial}}$.

---

### UpdateElement

Changes properties of an existing knowledge element.

**Preconditions:**
* The target `element_id` must exist in $S_{\text{working}}$.
* The current version of `element_id` in $S_{\text{working}}$ must equal `expected_version`.
* The current version's `lifecycle` must be `Active`.
* The property patch must contain at least one property and produce a content-different version ($V_{\text{next}} \neq V_{\text{current}}$).

**Candidate Effects:**
* Produces a new `KnowledgeElementVersion` object ($V_{\text{next}}$) with `lifecycle = Active`.
* Updates candidate element state entry for `element_id` to point to $V_{\text{next}}$.

**Postconditions:**
* The `ElementId` and `type_id` remain unchanged.
* The resulting version remains `Active`.

---

### DeprecateElement

Marks a knowledge element as deprecated.

**Preconditions:**
* The target `element_id` must exist in $S_{\text{working}}$.
* The current version of `element_id` in $S_{\text{working}}$ must equal `expected_version`.
* The current version's `lifecycle` must be `Active`.

**Candidate Effects:**
* Produces a new `KnowledgeElementVersion` object ($V_{\text{deprecated}}$) with `lifecycle = Deprecated`.
* Updates candidate element state entry for `element_id` to point to $V_{\text{deprecated}}$.

**Postconditions:**
* Existing relationships present in the candidate state that reference `element_id` remain intact.

---

### Link

Creates a typed relationship between two knowledge elements.

**Preconditions:**
* The requested `relationship_id` must not exist in $S_{\text{working}}$.
* The source element must exist in $S_{\text{working}}$ and its current version lifecycle must be `Active`.
* The target element must exist in $S_{\text{working}}$ (lifecycle may be `Active`, `Deprecated`, or `Superseded`).
* The relationship type must exist in the active `OntologyVersion` and allow the source and target element types.
* The triple `(relationship_type, source_id, target_id)` must not already exist in $S_{\text{working}}$.

**Candidate Effects:**
* Produces a new `RelationshipVersion` object ($R_{\text{initial}}$).
* Adds entry `(relationship_id, R_{\text{initial}})` to candidate relationship state map.

**Postconditions:**
* The relationship is established in $S_{\text{working}}$.

---

### Unlink

Removes a relationship from the current candidate state.

**Preconditions:**
* The target `relationship_id` must exist in $S_{\text{working}}$.
* The current relationship version in $S_{\text{working}}$ must equal `expected_version`.
* Unlink eligibility is independent of endpoint element lifecycle and current ontology conformance.

**Candidate Effects:**
* Removes the entry for `relationship_id` from candidate relationship state map.
* No element or relationship version objects are produced.

**Postconditions:**
* The referenced `RelationshipVersion` object remains immutable in history.

---

### SupersedeElement

Replaces an existing knowledge element with a replacement element.

**Preconditions:**
* The existing element must exist in $S_{\text{working}}$ and its current version lifecycle must be `Active`.
* The current version of existing element must equal `expected_existing_version`.
* The replacement `element_id` and relationship `relationship_id` must not exist in $S_{\text{working}}$.
* The existing and replacement element types must be permitted by `kat.core/supersedes` in the active ontology.

**Candidate Effects:**
* Produces a new replacement `KnowledgeElementVersion` ($V_{\text{replacement}}$) with `lifecycle = Active`.
* Produces a new superseded `KnowledgeElementVersion` ($V_{\text{superseded}}$) with `lifecycle = Superseded`.
* Produces a new `kat.core/supersedes` `RelationshipVersion` ($R_{\text{supersedes}}$) from the replacement element to the existing element.
* Updates candidate state element and relationship maps.

**Postconditions:**
* The existing element is superseded and traceable to replacement.

---

### AccountArtifact

Re-baselines direct accountability relationships originating from an artifact element.

**Preconditions:**
* The target `artifact_id` must exist in $S_{\text{working}}$.
* The artifact element current version lifecycle must be `Active`.
* The artifact element type must be `kat.core/artifact`.
* The artifact must have at least one direct accountability relationship (`kat.core/represents`, `kat.core/derived-from`) present in $S_{\text{working}}$.
* Every target element referenced by an accountability relationship must exist in $S_{\text{working}}$ and its current version lifecycle must be `Active`.
* The operation must produce an effective change: at least one reconciled target version must differ from the relationship's previous baseline version in history. Re-accounting an already current artifact is rejected as a semantic no-op.

**Candidate Effects:**
* No `KnowledgeElementVersion` or `RelationshipVersion` objects are produced.
* The candidate `SemanticState` after `AccountArtifact` is identical to the candidate `SemanticState` immediately before `AccountArtifact`.

**Postconditions:**
* The recorded reconciliation set covers every direct accountability relationship of the artifact present in the candidate state.
* For every recorded reconciliation:
  - `relationship_id` identifies a relationship present in the candidate state.
  - `expected_relationship_version` is the relationship version selected by the candidate state.
  - That relationship originates from `artifact_id`.
  - That relationship targets `target_element_id`.
  - `reconciled_target_version` is the version of `target_element_id` selected by the candidate state.
  - The relationship type is `kat.core/represents` or `kat.core/derived-from`.
* When the enclosing Change is accepted, the operation records explicit reconciliation baselines for its direct accountability relationships in accepted history.
* If an accepted reconciliation exists for an accountability relationship, the latest accepted reconciliation defines its current accountability baseline.
* Otherwise, the relationship's initial accepted accountability baseline applies.

---

## Query Operations

Query operations inspect the semantic model without changing its state.

### List

Provides a filtered listing of elements present in a semantic state.

**Input:**
* Optional element type criterion
* Optional lifecycle criterion

**Result:**
* Element identities, titles, types, and lifecycles satisfying the filter criteria.

### Show

Displays detailed information for a single knowledge element.

**Input:**
* Element identity

**Result:**
* Element properties, current version identity, lifecycle, and immediate incoming and outgoing relationships.

### Status

Provides a high-level summary of the current accepted repository state.

**Input:**
* Repository scope

**Result:**
* Repository and software identity
* Current accepted semantic state and ontology identity
* Latest accepted change when present
* Element and relationship counts
* Consistency summary
* Artifact accountability summary

### Trace

Traverses provenance paths associated with a knowledge element following the normative origin traversal policy.

**Input:**
* Element identity

**Result:**
* Provenance paths and the relationships and elements traversed along each path.

#### Origin Traversal Policy

When performing Origin Tracing (`kat trace`), relationships participate in provenance navigation according to a normative direction relative to their canonical definitions:

| Relationship Type | Canonical Form | Origin Traversal Direction | Semantic Rationale |
| :--- | :--- | :--- | :--- |
| `kat.core/motivates` | Intent $\xrightarrow{\text{motivates}}$ Req / Decision | Target $\to$ Source (**Backward**) | Requirement / Decision is motivated by Intent |
| `kat.core/derived-from` | Artifact $\xrightarrow{\text{derived-from}}$ Auth Knowledge | Source $\to$ Target (**Forward**) | Artifact is derived from Requirement / Decision / Constraint / Impl |
| `kat.core/realizes` | Impl $\xrightarrow{\text{realizes}}$ Requirement | Source $\to$ Target (**Forward**) | Implementation realizes Requirement |
| `kat.core/represents` | Artifact $\xrightarrow{\text{represents}}$ Implementation | Source $\to$ Target (**Forward**) | Artifact represents Implementation |
| `kat.core/validates` | Validation $\xrightarrow{\text{validates}}$ Subject | Source $\to$ Target (**Forward**) | Validation validates Requirement / Constraint / Implementation |
| `kat.core/restricts` | Constraint $\xrightarrow{\text{restricts}}$ Req / Decision / Impl | Target $\to$ Source (**Backward**) | Element is restricted by Constraint |
| `kat.core/addresses` | Decision $\xrightarrow{\text{addresses}}$ Requirement | Source $\to$ Target (**Forward**) | Decision exists to address Requirement |
| `kat.core/supersedes` | Replacement $\xrightarrow{\text{supersedes}}$ Existing Decision | Source $\to$ Target (**Forward**) | Replacement decision supersedes old decision |
| `kat.core/guides` | Decision $\xrightarrow{\text{guides}}$ Implementation | Target $\to$ Source (**Backward**) | Implementation is guided by Decision |
| `kat.core/depends-on` | Impl $\xrightarrow{\text{depends-on}}$ Implementation | *Excluded (Non-Origin)* | Structural dependency (reserved for Impact Analysis) |

Relationships classified as *Excluded* (such as `kat.core/depends-on`) represent horizontal or operational dependencies rather than authoritative origin or rationale, and are omitted from Origin Tracing.

### Impact

Evaluates which knowledge and artifacts may be affected if a selected knowledge element changes.

**Input:**
* Element identity

**Result:**
* The selected root element (`directly_changed` in the query model), `semantically_affected` elements, and `affected_artifacts`.

#### Impact Propagation Policy

Impact Analysis (`kat impact`) answers: *If a knowledge element changes, what other current knowledge may be affected?*

Impact propagation travels through relationships according to a normative direction relative to their canonical definitions:

| Relationship Type | Canonical Form | Impact Propagation Direction | Semantic Rationale |
| :--- | :--- | :--- | :--- |
| `kat.core/motivates` | Intent $\xrightarrow{\text{motivates}}$ Req / Decision | Source $\to$ Target (**Forward**) | Changed Intent affects motivated Requirement / Decision |
| `kat.core/addresses` | Decision $\xrightarrow{\text{addresses}}$ Requirement | Target $\to$ Source (**Backward**) | Changed Requirement affects addressing Decision |
| `kat.core/restricts` | Constraint $\xrightarrow{\text{restricts}}$ Req / Decision / Impl | Source $\to$ Target (**Forward**) | Changed Constraint affects restricted elements |
| `kat.core/guides` | Decision $\xrightarrow{\text{guides}}$ Implementation | Source $\to$ Target (**Forward**) | Changed Decision affects guided Implementation |
| `kat.core/realizes` | Impl $\xrightarrow{\text{realizes}}$ Requirement | Target $\to$ Source (**Backward**) | Changed Requirement affects realizing Implementation |
| `kat.core/represents` | Artifact $\xrightarrow{\text{represents}}$ Implementation | Target $\to$ Source (**Backward**) | Changed Implementation affects representing Artifact |
| `kat.core/derived-from` | Artifact $\xrightarrow{\text{derived-from}}$ Auth Knowledge | Target $\to$ Source (**Backward**) | Changed Auth Knowledge affects derived Artifact |
| `kat.core/validates` | Validation $\xrightarrow{\text{validates}}$ Subject | Target $\to$ Source (**Backward**) | Changed Subject affects validating evidence |
| `kat.core/depends-on` | Impl A $\xrightarrow{\text{depends-on}}$ Impl B | Target $\to$ Source (**Backward**) | Changed Dependency B affects dependent Implementation A |
| `kat.core/supersedes` | Replacement $\xrightarrow{\text{supersedes}}$ Existing Decision | *Excluded (Non-Impact)* | Historical evolution relation (omitted from current impact) |

Impact Analysis categorizes impacted elements into three distinct buckets:
1. **Directly Changed Elements**: The root element(s) being analyzed.
2. **Semantically Affected Elements**: Non-artifact Active elements reached via impact propagation (`Requirement`, `Constraint`, `Design Decision`, `Implementation`, `Validation`, `Intent`).
3. **Affected Artifacts**: Active elements of type `kat.core/artifact` reached via impact propagation.

> **Lifecycle Policy Distinction**: Filtering reached target elements to `Lifecycle::Active` is an **Impact-specific query policy**. Because Impact Analysis identifies potential consequences for the *current accepted operational state*, historical (`Deprecated` or `Superseded`) targets are excluded from impact results. By contrast, **Trace Origin** retains all historical lifecycle states in trace paths to preserve full provenance history.

### History

Retrieves the ordered accepted evolution of a knowledge element or repository.

**Input:**
* Element identity or repository scope

**Result:**
* Ordered accepted change revisions affecting the target scope, including their operations, descriptions, base states, and result states when present.

### ArtifactAccountability

Evaluates artifact accountability status from the current accepted semantic state and its accepted change history.

**Input:**
* Repository scope

**Result:**
* Artifact accountability report categorizing artifacts into `CURRENT`, `STALE`, or `UNACCOUNTED`.

### InspectOntology

Queries and inspects active ontology element types, relationship types, and endpoint capabilities.

**Input:**
* Scope: `Summary` or `TypeDetail(query)`
* Context: Active `OntologyVersion` referenced by accepted state ($S_n$)

**Result:**
* `Summary`: Complete list of element and relationship type definitions, human-readable names, and allowed source/target endpoints, canonically ordered.
* `TypeDetail`: Resolved element or relationship type view including human-readable name, endpoint admissibility, and derived incoming/outgoing relationship capabilities. Resolves exact canonical IDs or unique short identifiers.

---

## Validation Operations

Validation operations evaluate the semantic model against defined rules without modifying its state.

### Validate

Evaluates mechanically defined ontology and semantic consistency rules.

**Input:**
* Current accepted semantic state

**Result:**
* Mechanical consistency violations (e.g. invalid types, duplicate triples).
* Affected knowledge elements.
* Active constraints whose semantics cannot be mechanically verified (reported as unverified).

Validation reports the state of the model. It does not silently modify knowledge.

---

## Deferred Operations

The following operations are intentionally outside the scope of KAT v0.2:

* Explain
* Merge
* Synchronize
* Branch
* Materialize
* Import
* Export

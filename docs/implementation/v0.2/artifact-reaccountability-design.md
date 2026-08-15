# Artifact Re-accountability Design Specification (Phase 15)

> **Specification & Design Document for Phase 15**  
> Part of the [v0.2 Master Plan](implementation-plan.md).

---

## 1. Purpose & Motivation

In KAT v0.1, active artifacts (`kat.core/artifact`) declare explicit accountability dependencies on upstream knowledge elements via accountability relationships (`kat.core/represents`, `kat.core/derived-from`). 

When an upstream knowledge element is updated or evolved:
$$\text{Upstream Element } E \text{ updated } (V_1 \longrightarrow V_2)$$
$$\Downarrow$$
$$\text{Artifact } A \text{ accountability status becomes } \mathbf{STALE}$$

In KAT v0.1, the developer's ceremony to record that the physical artifact $A$ has been reviewed and reconciled against the new upstream version $V_2$ required destroying and recreating the relationship:
```bash
kat unlink <rel-id>
kat link represents <artifact-id> <upstream-id>
```

This workaround is problematic:
1. **Destroys Relationship Identity**: Destroys the stable `RelationshipId` and replaces it with a new identity.
2. **Obscures Developer Intent**: History records an `Unlink` followed by a `Link`, rather than recording an explicit semantic acknowledgment of artifact reconciliation.
3. **High Ceremony**: Requires multi-step CLI bookkeeping for every stale relationship.

Phase 15 introduces a first-class, dedicated semantic operation:
```bash
kat account <artifact-id-or-prefix> [--description "..."]
```

---

## 2. Frozen Core Guarantees & Contracts

### 2.1 `account != verify`
- KAT does **not** inspect, hash, or verify physical file contents on disk.
- `kat account` records the explicit semantic acknowledgment by the developer that the target artifact has been reviewed and reconciled against the current versions of its direct accountability dependencies (`represents`, `derived-from`).
- `UNACCOUNTED`, `STALE`, and `CURRENT` accountability statuses retain their exact v0.1 semantic definitions.

### 2.2 Relationship Identity Preservation
- `kat account` does **not** destroy, recreate, or alter stable `RelationshipId`s.
- Existing accountability relationships (`represents`, `derived-from`) originating from the artifact remain intact in $S_{\text{working}}$.

### 2.3 Integration with Phase 14 Draft Sessions
- `kat account` is fully dual-mode:
  - **No open draft**: Executes immediate single-operation `ChangeRevision` published via CAS to `refs/accepted`.
  - **Open draft**: Auto-stages an `AccountArtifact` operation onto `.kat/work/change/session.json` against candidate working state $S_{\text{working}}$.

### 2.4 SemanticState vs ChangeRevision Identity
- `AccountArtifact` is a canonical semantic operation that produces a candidate `SemanticState` $S_{n+1}$ content-identical to base state $S_n$ ($S_{n+1} == S_n$), while publishing a distinct `ChangeRevision` $C_{n+1} \neq C_n$.
- Baselines are resolved from accepted change history, keeping `SemanticState` and `RelationshipVersion` schemas clean and immutable.

---

## 3. Frozen Canonical CDDL Extension (Schema v0.2)

To make artifact re-accountability a first-class canonical operation, `spec/canonical-format.cddl` defines operation tag `7`:

```cddl
operation = [1, create-element]
          / [2, update-element]
          / [3, deprecate-element]
          / [4, link-element]
          / [5, unlink-element]
          / [6, supersede-element]
          / [7, account-artifact]  ; Phase 15 extension

account-artifact = [
  7,
  artifact_id: uuid,
  reconciliations: [* relationship-reconciliation]
]

relationship-reconciliation = [
  relationship_id: uuid,
  expected_relationship_version: object_id,
  target_element_id: uuid,
  reconciled_target_version: object_id
]
```

### Key Properties of `account-artifact`:
- **`artifact_id`**: The stable `ElementId` of the `kat.core/artifact` element.
- **`reconciliations`**: Non-empty vector of relationship baseline reconciliations for all currently accepted direct accountability relationships (`represents`, `derived-from`) originating from `artifact_id`. MUST be strictly sorted by raw `RelationshipId` bytes.
- **`reconciled_target_version`**: The exact SHA-256 `ObjectId` of the target `KnowledgeElementVersion` in $S_{\text{working}}$ at the moment of reconciliation.

---

## 4. Normative Preconditions & Invariants

1. **Artifact Element Preconditions**:
   - `artifact_id` exists in $S_{\text{working}}$.
   - Artifact element version lifecycle is `Active`.
   - Artifact element type is `kat.core/artifact`.
2. **Accountability Relationships Precondition**:
   - The artifact must have at least one currently accepted direct accountability relationship of type `represents` or `derived-from`.
   - If 0 accountability relationships exist $\to$ `PreconditionError::NoAccountabilityRelationships`.
3. **Target Element Lifecycle Rule**:
   - Each target element referenced by an accountability relationship must exist in $S_{\text{working}}$ and its current version lifecycle must be `Active`.
   - If an upstream target element is `Deprecated` or `Superseded`, `kat account` rejects reconciliation (`PreconditionError::ElementNotActive(target_id)`). The developer must first evolve the semantic relationships to point to authoritative active knowledge.
4. **Reconciliation Consistency Invariants**:
   - For every reconciliation entry `(R, RV, E, EV)`:
     - $S_{\text{working}}.\text{relationships}[R] == RV$
     - $\text{decode}(RV).\text{source} == \text{artifact\_id}$
     - $\text{decode}(RV).\text{target} == E$
     - $S_{\text{working}}.\text{elements}[E] == EV$
     - $\text{decode}(RV).\text{relationship\_type} \in \{\text{kat.core/represents}, \text{kat.core/derived-from}\}$
5. **No-Op Rejection**:
   - `kat account` compares each prospective reconciliation `reconciled_target_version` against the relationship's previous baseline version in history (resolved via `resolve_relationship_baseline_version`).
   - If for every reconciliation, `reconciled_target_version == previous_baseline_version`, the artifact is already current and the operation is rejected as a no-op (`PreconditionError::NoEffectiveChange`).

---

## 5. Query Semantics (`analyze_artifact_accountability`)

When `analyze_artifact_accountability` evaluates an artifact $A$:
1. Finds all currently accepted accountability relationships ($R_1, R_2, \dots$) originating from $A$.
2. For each relationship $R_i$, resolves its baseline target version:
   - If an `account-artifact` operation exists for $R_i$ in history, the baseline target version is the most recent `reconciled_target_version`.
   - Otherwise, fallback to the target version when $R_i$ was originally created.
3. Compares `baseline_target_version` against the current target version in $S_n$:
   - `current_version == baseline_target_version` $\implies$ `CURRENT`
   - `current_version != baseline_target_version` $\implies$ `STALE`

---

## 6. Verification & Acceptance Criteria

1. **Schema & CDDL Conformance**: `spec/canonical-format.cddl` updated and validated with golden test vector `spec/vectors/valid/change-revision-account-artifact.json` using tag `7`.
2. **Engine Typestate Pipeline**: Typestate safety (`AccountArtifactInput` $\to$ `PreparedElementAccounted` $\to$ `ValidatedElementAccounted` $\to$ `PublishedAccountChange`).
3. **Dual-Mode CLI**: Auto-staging under open `kat change` sessions and immediate CAS publication without open draft.
4. **End-to-End Acceptance Tests**:
   - `phase15_acceptance_cli_flow_end_to_end` in `tests/cli.rs`.
   - Multi-operation staged draft test (`update I` + `account A` in `kat change begin` $\to$ `kat change commit`).
   - Multiple accountability edges test (`A` derived from $R_5$ and representing $I_3$).

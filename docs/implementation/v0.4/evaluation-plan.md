# KAT v0.4 Empirical Evaluation Plan

## Status

Draft.

This document defines the empirical evaluation protocol for verifying the success of KAT v0.4 against its core problem statements and research hypotheses.

It is derived from:

- the v0.4 problem statements ([`problems-findings.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/problems-findings.md));
- the interaction model ([`interaction-model.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/interaction-model.md));
- the v0.4 requirements ([`requirements.md`](file:///home/joshua/Projects/kat/docs/implementation/v0.4/requirements.md)).

The central research thesis being evaluated is:

> **KAT v0.4 eliminates authoring friction, fragmented retrieval, and output parsing complexity by introducing a task-oriented porcelain layer over KAT's existing semantic primitives.**

---

# 1. Primary Evaluation Metrics

We evaluate v0.4 using 5 primary metrics compared directly against the v0.1–v0.3.1 baselines established in the Statit, Task Management API, and Feature Flag experiments:

| Metric | Metric Formula / Definition | Baseline (v0.3.1) | Target (v0.4) |
| :--- | :--- | :--- | :--- |
| **1. Primitive Exposure ($PE$)** | $\frac{\text{Primitive Operations Explicitly Invoked by Actor/Script}}{\text{Total Primitive Operations Executed}}$ | $1.0$ ($100\%$ exposed primitives) | **$\le 0.05$ ($\le 5\%$ exposed)** |
| **2. Interaction Amplification ($IA$)** | $\frac{\text{Count of Primitive KAT Operations Executed}}{\text{Count of Distinct User Intentions}}$ | $35.5$ ops/intent | Directional design target |
| **3. Porcelain Interaction Count ($PIC$)** | Count of user-visible porcelain CLI invocations | N/A (plumbing only) | **$\le 3.0$ porcelain calls** per task workflow |
| **4. Manual UUID Bookkeeping** | Direct manual UUID capture, tracking, or copy-pasting required by actor | Required (external script/log mapping) | **0 manual UUID bookkeeping** (handled via `@handles` & porcelain) |
| **5. Feature Context Retrieval Calls** | Count of CLI queries required to retrieve bounded feature context | $5 - 10$ queries (`show`, `trace`, `impact`, etc.) | **1 query** (`kat context`) |
| **6. Machine Parsing Regex/Prose Count** | Count of regex matches or line splits required by scripts to parse CLI outputs | High (custom Python parser) | **0** (100% structured JSON DTO envelopes) |

---

# 2. Experimental Scenarios

The evaluation will re-run the three benchmark authoring and inspection scenarios under controlled v0.4 conditions:

## Scenario A: Statit Semantic Graph Construction
- **Task**: Construct the complete Statit semantic repository graph (70 Knowledge Elements, 110 Relationships, 21 Accountability Baselines).
- **Comparison**:
  - *v0.3.1 Baseline*: 213 primitive CLI mutation commands orchestrated via external Python helper script (`create` $\times 70$, `link` $\times 110$, `account` $\times 21$, transaction control $\times 12$). $PE = 1.0$.
  - *v0.4 Porcelain Condition*: Declarative porcelain authoring submission using workflow reference handles (`@req-timer`, `@impl-timer`), followed by `kat check` and `kat commit`.
- **Expected Outcome**: Reduction from 213 manual primitive CLI commands down to 3 porcelain commands (`author`, `check`, `commit`). $PE = 0.0$.

## Scenario B: Task Management API & Statit Feature Context Discovery
- **Task**: Retrieve bounded development context (rationale, requirements, constraints, decisions, implementations, artifact anchors, validation status) for the "Task Reopening" and "Rest Timer Preset" features.
- **Comparison**:
  - *v0.3.1 Baseline*: Sequence of `show`, `trace`, `impact`, `artifacts`, `validate` (5+ separate primitive queries).
  - *v0.4 Porcelain Condition*: Single call to `kat context --json <root>`.
- **Expected Outcome**: 1-command context retrieval returning the specified bounded semantic projection, including all applicable result categories, in one invocation.

## Scenario C: Repository Health & Advisory Quality Verification
- **Task**: Verify repository integrity, evidence coverage, artifact staleness, and graph quality for a modified semantic repository.
- **Comparison**:
  - *v0.3.1 Baseline*: Separate executions of `kat validate`, `kat validate --coverage`, `kat status`, `kat artifacts`.
  - *v0.4 Porcelain Condition*: Single execution of `kat check`.
- **Expected Outcome**: 4-section consolidated health report in a single interaction.

---

# 3. Evaluation Verification Protocol

1. **Automated Benchmark Runner**: Execute automated evaluation scripts in `tests/` that invoke v0.4 porcelain commands and measure command count, stdout payload structure, and exit codes.
2. **Backward Compatibility Regression Check**: Verify that all baseline v0.3.1 plumbing scripts and primitive commands run with 0 regressions.
3. **JSON Envelope Verification**: Validate all machine-mode outputs against `machine-interface.md` schemas using automated JSON schema validators.
4. **Man Page & Asset Audit**: Verify that `generated/man/kat.1` and shell completion scripts accurately reflect all porcelain commands, global flags, and plumbing options.

---

# 4. Success Criteria

The v0.4 release shall be considered empirically successful if and only if:

1. **Zero Regression**: 100% of existing unit and integration tests pass cleanly; canonical golden vectors and ObjectIds remain byte-identical.
2. **Zero UUID Bookkeeping**: Standard authoring workflows require 0 manual UUID copy-pasting or mapping by the user or agent.
3. **Bounded Context Retrieval**: `kat context` returns the specified bounded semantic projection, including all applicable result categories, in one invocation.
4. **Single-Interaction Health Check**: `kat check` returns mechanical violations, evidence coverage, artifact staleness, and advisory quality in a single call.
5. **Clean Machine Integration**: `--json` outputs exactly 1 machine envelope on `stdout`, cleanly parseable without prose regex hacks.

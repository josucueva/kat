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

We evaluate v0.4 using 5 quantitative metrics compared directly against the v0.1–v0.3.1 baselines established in the Statit, Task Management API, and Feature Flag experiments:

| Metric | Metric Formula / Definition | Baseline (v0.3.1) | Target (v0.4) |
| :--- | :--- | :--- | :--- |
| **1. Primitive Exposure ($PE$)** | $\frac{\text{Manually Invoked Primitive Operations}}{\text{Total Primitive Operations Executed}}$ | $1.0$ ($100\%$) | **$\le 0.05$ ($\le 5\%$)** |
| **2. Interaction Amplification ($IA$)** | $\frac{\text{Count of KAT CLI Commands Executed}}{\text{Count of Distinct User Intentions}}$ | $35.5$ ops/intent | **$\le 2.0$ ops/intent** |
| **3. Manual UUID Orchestration Count** | Count of raw 36-char UUIDs captured, stored, or typed by actor during authoring | $180+$ UUIDs | **0 UUIDs** (handled via `@handles` & porcelain compiler) |
| **4. Feature Context Retrieval Calls** | Count of CLI queries required to retrieve complete feature context | $5 - 10$ queries | **1 query** (`kat context`) |
| **5. Machine Parsing Regex/Prose Count** | Count of regex matches or line splits required by scripts to parse CLI outputs | High (custom Python helper) | **0** (100% structured JSON DTO envelopes) |

---

# 2. Experimental Scenarios

The evaluation will re-run the three benchmark authoring and inspection scenarios under controlled v0.4 conditions:

## Scenario A: Statit Rest Timer Preset Authoring
- **Task**: Introduce the per-exercise rest timer preset feature into the Statit semantic repository (70 Knowledge Elements, 110 Relationships, 21 Artifacts).
- **Comparison**:
  - *v0.3.1 Baseline*: 213 CLI mutation commands executed via external Python script. 180+ UUIDs manually captured and mapped.
  - *v0.4 Porcelain Condition*: Single `kat author` declarative submission using workflow reference handles (`@req-timer`, `@impl-timer`), followed by `kat check` and `kat commit`.
- **Expected Outcome**: Reduction from 213 manual primitive CLI commands down to 3 porcelain commands (`author`, `check`, `commit`). $PE = 0.0$.

## Scenario B: Task Management API Feature Context Discovery
- **Task**: Retrieve complete development context (rationale, requirements, constraints, decisions, implementations, artifact anchors, validation status) for the "Task Reopening" requirement.
- **Comparison**:
  - *v0.3.1 Baseline*: Sequence of `show`, `trace`, `impact`, `artifacts`, `validate` (5+ separate queries).
  - *v0.4 Porcelain Condition*: Single call to `kat context --json <root>`.
- **Expected Outcome**: 1-command context retrieval yielding a complete structured JSON response grouping all 8 semantic roles.

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

1. **Zero Regression**: 100% of existing unit and integration tests pass cleanly.
2. **Zero UUID Orchestration**: Standard authoring workflows require 0 raw UUID copy-pasting or manual mapping by the user or agent.
3. **1-Command Context**: `kat context` returns complete 8-role context in a single call.
4. **1-Command Health**: `kat check` returns mechanical violations, evidence coverage, artifact staleness, and advisory quality in a single call.
5. **Clean Machine Integration**: `--json` outputs exactly 1 machine envelope on `stdout`, cleanly parseable without prose regex hacks.

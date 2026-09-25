# RealityTrace

**RealityTrace binds what was expected to what actually happened.**

AI systems are good at producing explanations after the fact.

That is a problem.

Once an outcome is visible, an agent can construct a plausible account of why it happened. What that account cannot recover is what the system actually expected *before* reality answered.

RealityTrace preserves that boundary.

```text
expectation
    │
    │  frozen before observation
    ▼
action / event
    │
    ▼
observation
    │
    ▼
comparison
    │
    ├── match
    ├── discrepancy
    └── unresolved
    │
    ▼
evidence + provenance
```

Its job is narrow: **preserve expectation, observation, difference, evidence, and provenance well enough that another system can make an honest judgment from them.**

---

## Why RealityTrace Exists

Consider an agent asked to change a software system. Before acting, the expectation is:

```text
- reject expired credentials
- preserve valid sessions
- leave unrelated authorization behavior unchanged
- pass the existing regression suite
```

The agent acts. Tests pass. Is the expectation confirmed?

Not necessarily. Valid sessions might now expire early. An unrelated authorization path might have changed. The tests might never have exercised the relevant condition.

Without a preserved expectation and an independently captured observation, the system is left with:

```text
"Implemented successfully."
```

RealityTrace instead preserves the comparison:

```text
EXPECTED                               OBSERVED
──────────────────────────────────────────────────────────────────
expired credentials rejected           expired credentials rejected
valid sessions preserved               valid sessions expire after refresh
unrelated authorization unchanged      authorization unchanged in observed scope
regression suite passes                regression suite passes

DIFFERENCE
└── valid-session behavior diverged

EVIDENCE
└── test results, execution trace, observed state, artifact references
```

The failure bought information.

---

## Core Principle

> **You can only measure error against something that existed before the outcome was known.**

If the expected result is reconstructed after observing the result, the comparison is contaminated.

```text
t0                     t1                     t2

EXPECTATION ──────────► ACTION ─────────────► OBSERVATION
    │                                             │
    └──────────── immutable for this act ─────────┘
                         │
                         ▼
                     COMPARISON
```

Expectations can be revised. Revision creates a new expectation. It does not rewrite evidence produced under the old one.

```text
expectation v1
    │
    ├── governs act 1
    └── evidence remains attached to v1

expectation v2
    │
    └── governs a later act
```

---

## The Trace Object

A trace is a relationship, not a log entry:

```text
expected representation
          ↕
observed representation
          │
          ▼
structured difference
          +
       evidence
          +
      provenance
```

A useful expectation captures:

- what should change
- what should remain unchanged
- what should appear or not appear
- what evidence should exist
- what would contradict the expectation

The observation records what the available observation machinery actually registered. Comparison makes discrepancy explicit.

### Evidence and Provenance

| Concept | What it is |
|---------|-----------|
| Claim | Agent assertion ("all tests pass") |
| Evidence | Test runner output |
| Provenance | What produced that output, against which artifact, when |

A trace should answer: who or what produced this observation, against which expectation and artifact, was the expectation already frozen, has the evidence changed since capture.

### Conceptual Trace Schema

> RealityTrace models traces canonically as append-only records wrapped in [CEP-0008](https://cep.dev) semantic envelopes (including `questions`, `thresholds`, `observations`, `claims`, `evidence`, `residuals`, and `settlements`). Conceptually, every trace binds:

```yaml
trace:
  id: trace-001

  expectation:
    version: 1
    frozen_at: ...
    subject: ...
    expected:
      ...
    invariants:
      ...
    contradiction_conditions:
      ...

  observation:
    captured_at: ...
    observer: ...
    subject: ...
    observed:
      ...

  comparison:
    status: mismatch
    differences:
      - ...
    unresolved:
      - ...

  evidence:
    - kind: ...
      source: ...
      digest: ...
      captured_at: ...

  provenance:
    expectation_source: ...
    observation_source: ...
    artifact_identity: ...
    environment: ...
```

---

## Comparison States

| State | Meaning | RealityTrace Residual Kind |
|-------|---------|-----------------------------|
| `match` | Observation satisfies the expectation | `observed_consistent` |
| `mismatch` | Observation contradicts the expectation | `declared_contradicted`, `contradictory` |
| `undefined` | Available evidence cannot establish the comparison | `declared_unobserved` |

`undefined` is not success. Missing evidence remains missing until an observer provides verified telemetry.

In addition, observations with no prior declared expectation are surfaced as `observed_undeclared`, and settlements with unresolved obligations are tracked as `settled_with_debt`.

```text
expected:    no unauthorized bypass exists
observed:    scanner found no bypass
comparison:  undefined unless the search scope is sufficient
```

```text
expected:    latency remains below threshold
observed:    telemetry unavailable
comparison:  undefined
```

Turning unavailable evidence into success destroys the value of the trace.

---

## Structured Differences

Collapsing to `FAIL` destroys information. These three are not equivalent:

| Attempt | Expected | Observed | Actual situation |
|---------|----------|----------|-----------------|
| A | output = 42 | output = 41 | Wrong result |
| B | output = 42 | process terminated | Execution failure |
| C | output = 42 | observer unavailable | Missing evidence |

RealityTrace preserves the structure. Diagnosis can happen downstream.

---

## Negative Expectations

Expectations should also capture what is expected **not** to change:

```text
expected change:      retry logic becomes idempotent
expected invariants:  public API remains compatible
                      persistence format unchanged
expected irrelevance: request ordering does not affect correctness
```

If request ordering later changes the result, the system has learned something stronger than "the test failed." A distinction previously believed irrelevant became consequential. That can justify a representational change.

---

## Workflow

1. **Establish the expectation.** Before the consequential observation is available, define what the bounded act is expected to produce. An expectation that cannot be wrong cannot generate useful error.

2. **Freeze the evaluation boundary.** The executor cannot rewrite the criterion after seeing the result.

3. **Act.** RealityTrace does not own execution. The action may be performed by a human, AI agent, Gauntlet, CI pipeline, or any governed system.

4. **Observe.** Capture consequence through the strongest affordable mechanism. A partial observer produces partial evidence, not a full claim.

5. **Compare.** Bind observed to expected. Produce explicit differences, not only a scalar result.

6. **Preserve the trace.** Evidence should remain inspectable: expectation → attempt → observation → comparison → evidence → provenance.

7. **Hand evidence downstream.** RealityTrace stops before making claims the trace cannot support. Consumers may reach different conclusions from the same trace. That is acceptable.

---

## Position in the GodSpeed Stack

```text
                    .sea
                     │
                     ▼
                DomainForge       domain meaning
                     │
                     ▼
               Context Kernel     context
                     │
                     ▼
                SEA-Forge         authority
                     │
                     ▼
                 Gauntlet         execution
                     │
                     ▼
              ┌──────────────┐
              │ RealityTrace │    evidence + discrepancy
              └──────┬───────┘
                     │
                     ▼
             settlement / diagnosis
                     │
                     ▼
              GodSpeed-Agent      developmental update
```

Authority (SEA-Forge) and observation (RealityTrace) are separate. Execution (Gauntlet) and evidence binding (RealityTrace) are separate.

---

## What RealityTrace Is Not

- **Not an agent framework.** It does not orchestrate work it observes.
- **Not an authorization system.** Permission belongs at the governance boundary.
- **Not a reward function.** Translating evidence into reward is a separate representational decision.
- **Not a capability registry.** A successful trace is evidence. Durable capability requires repeated settlement under relevant variation.
- **Not a logging system.** Logs record events. RealityTrace binds a pre-observation expectation to a post-observation record and preserves the discrepancy as evidence.

---

## Design Principles

- **Expectation precedes observation.** Otherwise error becomes retrospective storytelling.
- **Observation is represented, not equated with reality.** Every observer has limits.
- **Undefined is not success.** Missing evidence remains missing.
- **Preserve differences before interpreting them.** Diagnosis happens downstream.
- **Evidence keeps its provenance.** An assertion without lineage is not evidence.
- **Revision does not rewrite history.** New expectations govern new acts.
- **One success does not establish capability.**

---

## Installation

```bash
cargo install realitytrace
```

Verify installation and environment status:

```bash
realitytrace --version
realitytrace doctor
```

---

## Quick Start

The minimal RealityTrace loop:

```text
1. establish expectation (question) and threshold
2. freeze evaluation boundary
3. execute verification plan or ingest telemetry
4. capture observation and derive structured evidence
5. compare and classify residual discrepancies (delta)
6. verify cryptographic ledger integrity and export projections
```

```bash
# 1. Enroll a repository (mints ULID repo and checkout identity)
realitytrace repo enroll .

# 2. Inspect enrolled repositories
realitytrace repo list

# 3. Declare what is expected (Question) and acceptance criteria (Threshold)
realitytrace question declare --repo <REPO_ID> --file question.json
realitytrace threshold declare --repo <REPO_ID> --question <QUESTION_ID> --file threshold.json

# 4. Begin a verification run and create an execution plan
realitytrace run begin --repo <REPO_ID> --snapshot <SNAPSHOT_HASH> --question <QUESTION_ID> --threshold <THRESHOLD_ID>
realitytrace plan create --run <RUN_ID> --verifier cargo-test --target concept:auth

# 5. Execute verification or ingest observation stream
realitytrace verify run --run <RUN_ID>
# or pipe test runner telemetry directly:
# cargo test --message-format=json | realitytrace ingest --run <RUN_ID> --verifier cargo-test --stdin-format cargo-test-json

# 6. Derive structured evidence from observation
realitytrace evidence create --run <RUN_ID> --observation <OBSERVATION_ID> --derive

# 7. Compare declared vs observed to inspect discrepancy residuals
realitytrace delta --repo <REPO_ID> --question <QUESTION_ID> --snapshot <SNAPSHOT_HASH>

# 8. Cryptographically verify hash-chain integrity of the append-only ledger
realitytrace ledger verify --repo <REPO_ID>

# 9. Export canonical projections and anti-collapse evidence corpus
realitytrace projections export --repo <REPO_ID> --canonical-json
realitytrace projections export-anti-collapse-corpus --repo <REPO_ID> --question <QUESTION_ID> --snapshot <SNAPSHOT_HASH>
```

A complete trace contains: Expectation, Observation, Comparison, Evidence, Provenance. If any cannot be reconstructed, the trace is incomplete.

---

## Status

RealityTrace is part of the GodSpeed AI architecture. Stable contract:

```text
EXPECTED ↕ OBSERVED → DIFFERENCE + EVIDENCE + PROVENANCE
```

---

## License

Licensed under the [Apache License, Version 2.0](LICENSE).

---

*Developed as part of [GodSpeed AI](https://godspeed.ai).*
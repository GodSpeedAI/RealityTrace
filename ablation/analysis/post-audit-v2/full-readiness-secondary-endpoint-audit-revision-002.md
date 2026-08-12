# Final pre-FULL scientific-contract coverage audit — revision-002

## Disposition

**FULL_BLOCKED_SECONDARY_SPEC**

Offline only. Pilot outcomes were **not consulted when choosing semantics**. No provider/model call or experimental unit was made. FULL remains unauthorized and unexecuted.

The canonical primary remains ready and the completed pilot remains valid. FULL is blocked because `matched_evidence_use` is a frozen secondary name without a frozen operational rule, and the frozen contract does not affirmatively permit that endpoint to be unavailable. Selecting a proxy now would create post-pilot semantics.

## Pre-pilot boundary

- Typed decision contract: commit `7d8da3d`, 2026-08-11 09:24:55 -0400.
- Metric readiness: `bfbc753ffaff85d2c32bec0ca6f1a2125135bde7`, 09:45:07 -0400.
- Scientific freeze: `5182a61942c7d90f8a081bb3835bfdfe6530cebe`, 13:49:52 -0400.
- Pilot authorization: `86f77ad6a7a293de215f9fb7059af67fe3e22366`, 17:18:10 -0400.
- Implementation-bound release: `bb1102948b6f2a162e8f55ae8693ab99a8fb3976`, 18:00:08 -0400.
- Completed pilot began: `2026-08-11T22:17:24Z`.
- First pilot-outcome analysis came later: `a7811dd3d045a281fb0724b73a245c690b287be9`.

## revision_rate

**REVISION_RATE_DETERMINISTIC_WITHOUT_NEW_SEMANTICS**

### Exact denominator

Include a unit when:

```text
round1.proposed_settlement is not null
AND round1.proposed_settlement != evaluator true_locus
```

Exclude `null`: it is no settlement/no claim. Exclude `unresolved_distinction`: it is state metadata, not a claim. Include explicit `"unknown"` whenever it differs from evaluator truth: it is a typed settlement claim, not null.

The numerator is:

```text
final gate-blind last_model_claim != round1.proposed_settlement
```

### Exact pre-pilot basis

1. `ablation/eval/v2/evidence_revision.py::SECONDARY_ENDPOINTS["revision_rate"]` freezes `P(final claim != round-1 claim | round-1 claim wrong)`.
2. `ablation/eval/v2/decision.py::ControllerDecision.proposed_settlement` is `str | None`; `is_settlement` is exactly `proposed_settlement is not None`; `_validate` admits non-null settlements only from the frozen vocabulary.
3. `ablation/eval/v2/replication.py::VOCAB` contains literal `"unknown"`.
4. `ablation/eval/v2/controller.py::INSTRUCTION`, `FORCED_FINAL_INSTRUCTION`, and `DiagnosticController.run` treat `unknown` as an explicit settlement, null as no settlement, and only non-null proposals as claims.
5. `ablation/eval/v2/dstate.py::DiagnosticState.with_decision` stores `unresolved_distinction` separately from `current_claim`.
6. `ablation/eval/v2/evidence_revision.py::PRIMARY_POPULATION` is determinate and `PRIMARY_ENDPOINT` defines correctness by equality to concrete truth.

No endpoint-specific denominator function existed, but composing these frozen types and equality semantics leaves no judgment. The corrected offline proof is R0 1/1, R1 1/1, R2 1/1, R3 0/1, R4 0/1. Those values were computed only after the semantic disposition was fixed from pre-pilot sources.

## matched_evidence_use

**MATCHED_EVIDENCE_USE_UNDERSPECIFIED**

### Historical 73.7% / 73.7% trace

The historical rule is exact and recoverable:

- At commit `8fd347d`, `ablation/eval/governed_scorer.py::score_unit` defined `A_use` as correct settlement conditional on evaluator-discriminating evidence. Possession meant acquired action IDs intersected sealed `discriminating_probes`; use meant `exact_correct` on determinate units.
- At the same commit, `ablation/eval/governed_analyze.py::analyze` restricted O/G to feasible determinate same-`(case_id, rep)` records where each surviving arm held discriminating evidence, then scored `exact_correct`.
- Pre-pilot corrective commit `4c82ab48a4c4d01321df2da210400dc2295b90cc`, `ablation/eval/rescore/historical.py::rescore_governed`, independently rebuilt the same rule.
- `ablation/analysis/post-audit-v2/governed.json::matched_evidence_use_O_vs_G` records 19 matched pairs and G=14/19, O=14/19: 73.7% / 73.7%.
- `ablation/eval/v2/semantics.py::METRIC_CONSTRUCT["A_use_evaluator_evidence"]` binds historical A_use to `EVALUATOR_DISCRIMINATING_ACTION`, explicitly distinct from gate-qualifying and hypothesis-bearing evidence.

### Why the rule did not carry forward operationally

`ablation/eval/v2/replication.py::SUPPORTED_METRICS` says the matched-pair form replaces unmatched historical A_use, but supplies only `utilization on units where both arms held evidence`. The scientific freeze copies only that sentence into `ablation/eval/v2/evidence_revision.py::SECONDARY_ENDPOINTS["matched_evidence_use"]` and the serialized contract.

It does not bind:

- which R-arm pair or pairs;
- evaluator-discriminating, gate-qualifying, hypothesis-bearing, or any evidence;
- gate-blind correctness, governed exact correctness, citation, transition, or another utilization observation;
- population/exclusions;
- case versus case×rep reporting or repetition aggregation;
- missing-pair handling.

The pilot persists enough ingredients to compute several candidate historical-style proxies: pair identity, acquired probe IDs, last gate-blind claim, governed settlement, and joins to evaluator truth/discriminating sets. It does **not** persist a direct citation/utilization/attribution observation. More importantly, no pre-pilot frozen source selects one measurable proxy. Therefore there is no frozen pilot endpoint value to recompute without creating semantics. No candidate arm performance was used to make this decision.

## FULL readiness

The frozen contract explicitly labels `response_mode` POST-HOC/not a test. It does not similarly permit `matched_evidence_use` to be unavailable. The pilot stop gate lists integrity requirements but does not repair secondary semantics. The runtime also requires an unissued, separately bound FULL matrix and `FULL_AUTHORIZED` owner state.

Under the owner's stated readiness criterion, this is **FULL_BLOCKED_SECONDARY_SPEC**.

## Minimum MECE owner options

| Option | Meaning | Instrumentation | Scientific revision/new ID | New design/executable/runner identities | Pilot rerun | Owner reauthorization |
|---|---|---:|---:|---:|---:|---:|
| O1 | Prospectively define a complete derived proxy from already-persisted inputs. This is **new post-pilot semantics**, not clarification. | No | Yes | Yes / Yes / Yes | No, only if existing pilot is explicitly accepted as instrument validation and outcomes are not used to choose the rule | Yes |
| O2 | Add a direct, prospectively defined utilization/citation/attribution observation and scorer. | Yes | Yes | Yes / Yes / Yes | Yes | Yes |
| O3 | Prospectively remove the endpoint or explicitly freeze it as unavailable/descriptive, making no utilization claim. | No | Yes | Yes / Yes / Yes | No, if predecessor pilot is explicitly accepted as unchanged-instrument validation | Yes |
| O4 | Keep the current contract and defer/stop FULL. | No | No | No / No / No | No | No |

There is no analysis-only clarification of already-existing matched-use semantics: the missing choices were never frozen. O1 creates new semantics even if it chooses the historical proxy.

## Current pilot and identity consequences

- Completed pilot remains valid: **YES**. Integrity, primary reconstruction, and execution measurement do not depend on matched-use semantics.
- Completed pilot must be rerun now: **NO**. Do not rerun it merely to resolve prose. O2 would require a new pilot; O1/O3 can avoid provider reruns only by explicit prospective owner acceptance of predecessor pilot evidence.
- Design fingerprint must change to unblock: **YES**.
- Executable fingerprint must change to unblock: **YES**.
- Runner implementation identity must change to unblock: **YES** under the current bound architecture, because `replication.py` and `validity_revision.py` are implementation-bound and their scientific/executable bindings must move.

## Exact next owner action

Withhold FULL. Choose O1, O2, O3, or O4 explicitly. If preserving the valid pilot and avoiding provider calls is the priority, authorize a new revision under O1 or O3, freeze every missing semantic/status prospectively without consulting pilot outcomes, update and independently review all identities/bindings, and only then separately adjudicate FULL authorization.

```text
provider_calls_performed = 0
model_calls_performed = 0
FULL authorized = false
FULL executed = false
```

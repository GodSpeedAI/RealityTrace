# Revision-002 pilot offline adjudication

**ANALYSIS_READY**

This is a descriptive six-case protocol-pilot measurement validation, not confirmatory inference and not FULL authorization.

## Integrity
PASS: 60/60 unique results, 198/198 unique committed/recoverable calls, 60/60 committed results; all raw/result/checkpoint hashes and identities match. Input run manifest: `ae2827c1ea32c2860e53f19b763955908860865266791932f1454252ffeb2a13`.

No score artifacts exist: execution stopped at `RESULT_COMMITTED`, so no score can precede raw commitment.

## Revision to correct

Eligibility is the persisted round-1 proposed settlement not already equal to evaluator truth; null is explicit UNKNOWN/UNRESOLVED. Numerator uses the final gate-blind model claim, never governed settlement.

| Arm | numerator/denominator | rate | wrong→correct | UNKNOWN→correct | blind accuracy | governed accuracy |
|---|---:|---:|---:|---:|---:|---:|
| R0 | 1/7 | 14.3% | 0/1 | 1/6 | 1/12 (8.3%) | 0/12 (0.0%) |
| R1 | 3/8 | 37.5% | 0/1 | 3/7 | 5/12 (41.7%) | 5/12 (41.7%) |
| R2 | 3/8 | 37.5% | 0/1 | 3/7 | 3/12 (25.0%) | 3/12 (25.0%) |
| R3 | 4/8 | 50.0% | 0/1 | 4/7 | 8/12 (66.7%) | 8/12 (66.7%) |
| R4 | 7/9 | 77.8% | 0/1 | 7/8 | 8/12 (66.7%) | 8/12 (66.7%) |

## Secondary outcomes

Counts are unit-level out of 12 unless a transition denominator is shown. Useful means eligible→true; harmful means initially true→not true. With categorical loci, no unprespecified distance ordering is invented.

| Arm | any revision | useful | harmful | wrong→correct | UNKNOWN→correct | persist wrong | gate accepted/concrete | blind UNKNOWN | governed UNKNOWN |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| R0 | 8/12 | 1/12 | 5/12 | 0/1 | 1/6 | 0/1 | 0/12 | 10/12 | 12/12 |
| R1 | 6/12 | 3/12 | 2/12 | 0/1 | 3/7 | 0/1 | 5/12 | 7/12 | 7/12 |
| R2 | 10/12 | 3/12 | 4/12 | 0/1 | 3/7 | 0/1 | 4/12 | 7/12 | 8/12 |
| R3 | 6/12 | 4/12 | 0/12 | 0/1 | 4/7 | 1/1 | 10/12 | 1/12 | 2/12 |
| R4 | 10/12 | 7/12 | 2/12 | 0/1 | 7/8 | 1/1 | 10/12 | 0/12 | 2/12 |

R0 spontaneous revision to correct: 1/7 (14.3%).
R0 control transitions: any revision 8/12; wrong→different-wrong 0; UNKNOWN→concrete 2; persistent wrong 0; persistent UNKNOWN 4.

## Descriptive contrasts

- C1 R1-R0: 3/8 minus 1/7 = +23.2 pp.
- C2 R2-R1: 3/8 minus 3/8 = +0.0 pp.
- C3 R3-R1: 4/8 minus 3/8 = +12.5 pp.
- C4 R4-R3: 7/9 minus 4/8 = +27.8 pp.

No p-values or authorization inference is made.

## Representation pairs

- R1/R2, final correctness: {'pairs': 12, 'same_outcome': 10, 'raw_better': 2, 'legible_better': 0, 'both_correct': 3, 'neither_correct': 7, 'discordant_pairs': 2}; revision-to-correct: {'pairs': 12, 'same_outcome': 10, 'raw_better': 1, 'legible_better': 1, 'both_correct': 2, 'neither_correct': 8, 'discordant_pairs': 2}.
- R3/R4, final correctness: {'pairs': 12, 'same_outcome': 10, 'raw_better': 1, 'legible_better': 1, 'both_correct': 7, 'neither_correct': 3, 'discordant_pairs': 2}; revision-to-correct: {'pairs': 12, 'same_outcome': 7, 'raw_better': 1, 'legible_better': 4, 'both_correct': 3, 'neither_correct': 4, 'discordant_pairs': 5}.

All R1/R2 and R3/R4 persisted evidence memberships, evidence-set hashes, and semantic hashes match for all case/rep pairs.

## Six governance divergences

- `8233417f81ed8ca7` ag-04 R2 rep 2: truth `verification_plan`, blind `projection`, governed `unknown`, `INCORRECT_AND_REJECTED`, evidence `inspect_target_binding,inspect_permissions,inspect_fixture_integrity`, reason `BUDGET_EXHAUSTED_GATE_REJECTED:EVIDENCE_INSUFFICIENT`, payment 3, terminal `BUDGET_EXHAUSTED_GATE_REJECTED`.
- `c10209b7d83f8294` ag-04 R4 rep 1: truth `verification_plan`, blind `verifier`, governed `unknown`, `INCORRECT_AND_REJECTED`, evidence `inspect_target_binding,inspect_source_symbols,inspect_environment,inspect_permissions,inspect_fixture_integrity,validate_semantic_model,rerun_verifier_isolated,inspect_verifier_config`, reason `BUDGET_EXHAUSTED_GATE_REJECTED:EVIDENCE_INSUFFICIENT`, payment 14, terminal `BUDGET_EXHAUSTED_GATE_REJECTED`.
- `85cd771319143a8e` sr-10 R0 rep 2: truth `evidence_collection`, blind `evidence_collection`, governed `unknown`, `CORRECT_BUT_UNQUALIFIED`, evidence `NONE`, reason `BUDGET_EXHAUSTED_GATE_REJECTED:EVIDENCE_INSUFFICIENT`, payment 0, terminal `BUDGET_EXHAUSTED_GATE_REJECTED`.
- `945341c134f59204` df-06 R4 rep 1: truth `environment`, blind `verifier`, governed `unknown`, `INCORRECT_AND_REJECTED`, evidence `inspect_target_binding,inspect_source_symbols,inspect_environment,inspect_permissions,inspect_fixture_integrity,validate_semantic_model,rerun_verifier_isolated,inspect_verifier_config`, reason `BUDGET_EXHAUSTED_GATE_REJECTED:EVIDENCE_INSUFFICIENT`, payment 14, terminal `BUDGET_EXHAUSTED_GATE_REJECTED`.
- `dc71097610aa8edd` cep-05 R0 rep 1: truth `verification_plan`, blind `projection`, governed `unknown`, `INCORRECT_AND_REJECTED`, evidence `NONE`, reason `BUDGET_EXHAUSTED_GATE_REJECTED:EVIDENCE_INSUFFICIENT`, payment 0, terminal `BUDGET_EXHAUSTED_GATE_REJECTED`.
- `f64fd61970a8623d` cep-05 R3 rep 1: truth `verification_plan`, blind `projection`, governed `unknown`, `INCORRECT_AND_REJECTED`, evidence `inspect_target_binding,inspect_source_symbols,inspect_environment,inspect_permissions,inspect_fixture_integrity,validate_semantic_model,rerun_verifier_isolated,inspect_verifier_config`, reason `BUDGET_EXHAUSTED_GATE_REJECTED:EVIDENCE_INSUFFICIENT`, payment 14, terminal `BUDGET_EXHAUSTED_GATE_REJECTED`.

## Analysis validity

1. **PASS** — Primary endpoint without inference/imputation: Round-1 proposed settlement, forced-final raw decision, final gate-blind claim, and evaluator truth are recoverable.
2. **PASS** — Pre/post claims distinguishable: Round 1 precedes first delivery; calls 2+ are post-delivery.
3. **PASS** — Truth remains evaluator-side: Truth loaded only from sealed governed-keys; absent from call/result model-visible payloads.
4. **PASS** — Gate-blind/governed separable: Both are directly persisted in runner_contract.
5. **PASS** — Evidence/rendering independently recoverable: Policy, membership/hashes, and arm rendering are persisted.
6. **PASS** — Four frozen requested contrasts reconstructable: All arm numerator/denominator UID sets are deterministic.
7. **PASS** — Representation pairings exact: Case/rep mapping and three evidence equalities recover all 24 pairs.
8. **PASS** — Endpoint denominators deterministic: Round-1 proposed settlement plus sealed truth determines eligibility.
9. **PASS** — No result depends on missing intermediate state: Forced-final decision is absent from rounds but recovered exactly from committed raw bytes; final claim also directly persisted.
10. **PASS** — Reproducible without provider contact: This script reads only local immutable artifacts and sealed evaluator files.

## Instrument

| Arm | calls | calls/unit | rounds/unit | total tokens | reasoning tokens | latency ms |
|---|---:|---|---|---:|---:|---:|
| R0 | 48 | {4: 12} | {3: 12} | 248602 | 44534 | 718110.6 |
| R1 | 46 | {4: 10, 3: 2} | {3: 12} | 241505 | 50982 | 798403.4 |
| R2 | 44 | {4: 8, 3: 4} | {3: 12} | 219153 | 47072 | 712000.7 |
| R3 | 30 | {2: 9, 4: 3} | {2: 9, 3: 3} | 161059 | 41418 | 662960.9 |
| R4 | 30 | {2: 9, 4: 3} | {2: 9, 3: 3} | 166299 | 45771 | 672953.0 |

Total persisted calls: 198. All 198 raw decisions reparse under the frozen five-field shape; all 162 persisted round records are VALID_DECISION. Repair attempts, truncations, and provider retries are zero. Forced-final calls are recoverable from raw/call commits. Cost is unavailable because the provider did not report it.

The append ledgers are durable hash manifests, not externally signed hash chains. Current raw/result bytes match every ledger, sidecar, execution, and checkpoint digest; historical ordering additionally relies on the reviewed implementation, which writes and fsyncs raw before parsing/result commitment. There are no score artifacts.

Provider/model calls made by this analysis: **0/0**. FULL remains unauthorized and unexecuted.

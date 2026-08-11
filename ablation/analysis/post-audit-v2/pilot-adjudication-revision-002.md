# Revision-002 pilot corrected offline adjudication

**ANALYSIS_READY**

The frozen declarations in `eval/v2/evidence_revision.py` are authoritative. This is a six-case protocol-pilot implementation/measurement validation, not confirmatory inference and not FULL authorization.

## Integrity
PASS: 60/60 unique results, 198/198 unique committed/recoverable calls, 60/60 committed results; all raw/result/checkpoint hashes and identities match. Input run manifest: `ae2827c1ea32c2860e53f19b763955908860865266791932f1454252ffeb2a13`.

No score artifacts exist: execution stopped at `RESULT_COMMITTED`.

## Canonical frozen primary: case-level R_correct

`R_correct` is final gate-blind correctness on all cases. The primary unit is case. Two repetitions are aggregated within case by majority; a 1/1 tie is incorrect. Initial correctness never conditions this denominator.

| Arm | correct cases/6 | rate | descriptive correct reps/12 | conditional revision-to-correct |
|---|---:|---:|---:|---:|
| R0 | 0/6 | 0.0% | 1/12 (8.3%) | 1/7 (14.3%) |
| R1 | 2/6 | 33.3% | 5/12 (41.7%) | 3/8 (37.5%) |
| R2 | 1/6 | 16.7% | 3/12 (25.0%) | 3/8 (37.5%) |
| R3 | 3/6 | 50.0% | 8/12 (66.7%) | 4/8 (50.0%) |
| R4 | 3/6 | 50.0% | 8/12 (66.7%) | 7/9 (77.8%) |

### Exact case/repetition aggregation

#### R0

| Case | rep1 correct | rep2 correct | aggregated case correct |
|---|---:|---:|---:|
| cep-07 | false | false | false |
| df-11 | false | false | false |
| ag-04 | false | false | false |
| sr-10 | false | true | false |
| df-06 | false | false | false |
| cep-05 | false | false | false |

#### R1

| Case | rep1 correct | rep2 correct | aggregated case correct |
|---|---:|---:|---:|
| cep-07 | false | true | false |
| df-11 | false | false | false |
| ag-04 | false | false | false |
| sr-10 | true | true | true |
| df-06 | true | true | true |
| cep-05 | false | false | false |

#### R2

| Case | rep1 correct | rep2 correct | aggregated case correct |
|---|---:|---:|---:|
| cep-07 | false | false | false |
| df-11 | false | false | false |
| ag-04 | false | false | false |
| sr-10 | true | true | true |
| df-06 | false | true | false |
| cep-05 | false | false | false |

#### R3

| Case | rep1 correct | rep2 correct | aggregated case correct |
|---|---:|---:|---:|
| cep-07 | true | true | true |
| df-11 | false | false | false |
| ag-04 | false | true | false |
| sr-10 | true | true | true |
| df-06 | true | true | true |
| cep-05 | false | true | false |

#### R4

| Case | rep1 correct | rep2 correct | aggregated case correct |
|---|---:|---:|---:|
| cep-07 | true | true | true |
| df-11 | false | false | false |
| ag-04 | false | true | false |
| sr-10 | true | true | true |
| df-06 | false | true | false |
| cep-05 | true | true | true |

## Canonical paired comparisons

All tables use aggregated case-level R_correct. Pilot inference is NON-CONFIRMATORY.

### C1_evidence_effect: R0 vs R1
Family: `primary`. Declared differences: disclosure policy only

Both correct 0; both incorrect 4; R0-only correct 0; R1-only correct 2; discordant 2.

| Case | first correct | second correct |
|---|---:|---:|
| cep-07 | false | false |
| df-11 | false | false |
| ag-04 | false | false |
| sr-10 | false | true |
| df-06 | false | true |
| cep-05 | false | false |

### C2_legibility_governed: R1 vs R2
Family: `primary`. Declared differences: observation rendering only

Both correct 1; both incorrect 4; R1-only correct 1; R2-only correct 0; discordant 1.

| Case | first correct | second correct |
|---|---:|---:|
| cep-07 | false | false |
| df-11 | false | false |
| ag-04 | false | false |
| sr-10 | true | true |
| df-06 | true | false |
| cep-05 | false | false |

### C3_legibility_upper: R3 vs R4
Family: `primary`. Declared differences: observation rendering only

Both correct 2; both incorrect 2; R3-only correct 1; R4-only correct 1; discordant 2.

| Case | first correct | second correct |
|---|---:|---:|
| cep-07 | true | true |
| df-11 | false | false |
| ag-04 | false | false |
| sr-10 | true | true |
| df-06 | true | false |
| cep-05 | false | true |

### C4_coverage_headroom: R1 vs R3 **SECONDARY — EXEMPT_UPPER_BOUND HEADROOM; NOT SINGLE-VARIABLE ISOLATION.**
Family: `secondary`. Declared differences: TWO — disclosure AND budget_semantics. This comparison does NOT isolate a single variable and may never be described as if it did (audit A-06).

Both correct 2; both incorrect 3; R1-only correct 0; R3-only correct 1; discordant 1.

| Case | first correct | second correct |
|---|---:|---:|
| cep-07 | false | true |
| df-11 | false | false |
| ag-04 | false | false |
| sr-10 | true | true |
| df-06 | true | true |
| cep-05 | false | false |

## Frozen secondary endpoints

`revision_rate` is P(final claim differs from round-1 claim | round-1 claim wrong); it is not conditional revision-to-correct. `response_mode` is **POST-HOC**.

| Arm | revision_rate | gated_accuracy | invocation_rate | output classes | response mode POST-HOC |
|---|---:|---:|---:|---|---|
| R0 | strict concrete 1/1 (100.0%) | 0/12 (0.0%) | 0/12 (0.0%) | {'VALID_DECISION': 36} | modes=['unknown'], share=83.3%, H=0.817 bits |
| R1 | strict concrete 1/1 (100.0%) | 5/12 (41.7%) | 12/12 (100.0%) | {'VALID_DECISION': 36} | modes=['unknown'], share=58.3%, H=1.614 bits |
| R2 | strict concrete 1/1 (100.0%) | 3/12 (25.0%) | 12/12 (100.0%) | {'VALID_DECISION': 36} | modes=['unknown'], share=58.3%, H=1.781 bits |
| R3 | strict concrete 0/1 (0.0%) | 8/12 (66.7%) | 12/12 (100.0%) | {'VALID_DECISION': 27} | modes=['declared_model', 'environment', 'evidence_collection', 'implementation', 'verification_plan'], share=16.7%, H=2.752 bits |
| R4 | strict concrete 0/1 (0.0%) | 8/12 (66.7%) | 12/12 (100.0%) | {'VALID_DECISION': 27} | modes=['verification_plan'], share=25.0%, H=2.522 bits |

- `revision_rate`: **PARTIAL** — frozen prose does not resolve whether null/UNKNOWN is a wrong claim; strict-concrete and inclusive reconstructions are both in JSON.
- `response_mode`: **PARTIAL / POST-HOC** — displayed operationalization uses final gate-blind case×rep claims; alternative governed/all-round or case-aggregated choices are not uniquely frozen.
- `matched_evidence_use`: **NOT_RECONSTRUCTABLE** — holdings are persisted, but no explicit utilization/citation indicator is persisted.
- Full per-arm output-class rates and all six terminal-state categories are in the JSON artifact.

## Pilot-only transition diagnostics

These remain descriptive secondary diagnostics and never replace R_correct. Useful means eligible→true; harmful means initially true→not true. No ordinal distance between categorical wrong loci is invented.

| Arm | conditional revision→correct | any revision | useful | harmful | wrong→correct | UNKNOWN→correct | time to first correct calls |
|---|---:|---:|---:|---:|---:|---:|---|
| R0 | 1/7 | 8/12 | 1/12 | 5/12 | 0/1 | 1/6 | [3, 3, 4] |
| R1 | 3/8 | 6/12 | 3/12 | 2/12 | 0/1 | 3/7 | [3, 4, 3, 4, 4] |
| R2 | 3/8 | 10/12 | 3/12 | 4/12 | 0/1 | 3/7 | [3, 3, 3] |
| R3 | 4/8 | 6/12 | 4/12 | 0/12 | 0/1 | 4/7 | [2, 2, 2, 2, 2, 2, 2, 4] |
| R4 | 7/9 | 10/12 | 7/12 | 2/12 | 0/1 | 7/8 | [2, 2, 2, 2, 2, 2, 4, 2] |

R0 control: spontaneous revision-to-correct 1/7; any revision 8/12; wrong→different-wrong 0/1; UNKNOWN→concrete 2/6; persistent wrong 0/1; persistent UNKNOWN 4/6.

## Case×rep representation diagnostics

- R1/R2, final correctness: {'pairs': 12, 'same_outcome': 10, 'raw_better': 2, 'legible_better': 0, 'both_correct': 3, 'neither_correct': 7, 'discordant_pairs': 2}; conditional revision-to-correct: {'pairs': 12, 'same_outcome': 10, 'raw_better': 1, 'legible_better': 1, 'both_correct': 2, 'neither_correct': 8, 'discordant_pairs': 2}.
- R3/R4, final correctness: {'pairs': 12, 'same_outcome': 10, 'raw_better': 1, 'legible_better': 1, 'both_correct': 7, 'neither_correct': 3, 'discordant_pairs': 2}; conditional revision-to-correct: {'pairs': 12, 'same_outcome': 7, 'raw_better': 1, 'legible_better': 4, 'both_correct': 3, 'neither_correct': 4, 'discordant_pairs': 5}.

All R1/R2 and R3/R4 persisted evidence memberships, evidence-set hashes, and semantic hashes match for all case/rep pairs.

## Six gate/governance divergences

- `8233417f81ed8ca7` ag-04 R2 rep 2: truth `verification_plan`, blind `projection`, governed `unknown`, `INCORRECT_AND_REJECTED`, evidence `inspect_target_binding,inspect_permissions,inspect_fixture_integrity`, reason `BUDGET_EXHAUSTED_GATE_REJECTED:EVIDENCE_INSUFFICIENT`, payment 3, terminal `BUDGET_EXHAUSTED_GATE_REJECTED`.
- `c10209b7d83f8294` ag-04 R4 rep 1: truth `verification_plan`, blind `verifier`, governed `unknown`, `INCORRECT_AND_REJECTED`, evidence `inspect_target_binding,inspect_source_symbols,inspect_environment,inspect_permissions,inspect_fixture_integrity,validate_semantic_model,rerun_verifier_isolated,inspect_verifier_config`, reason `BUDGET_EXHAUSTED_GATE_REJECTED:EVIDENCE_INSUFFICIENT`, payment 14, terminal `BUDGET_EXHAUSTED_GATE_REJECTED`.
- `85cd771319143a8e` sr-10 R0 rep 2: truth `evidence_collection`, blind `evidence_collection`, governed `unknown`, `CORRECT_BUT_UNQUALIFIED`, evidence `NONE`, reason `BUDGET_EXHAUSTED_GATE_REJECTED:EVIDENCE_INSUFFICIENT`, payment 0, terminal `BUDGET_EXHAUSTED_GATE_REJECTED`.
- `945341c134f59204` df-06 R4 rep 1: truth `environment`, blind `verifier`, governed `unknown`, `INCORRECT_AND_REJECTED`, evidence `inspect_target_binding,inspect_source_symbols,inspect_environment,inspect_permissions,inspect_fixture_integrity,validate_semantic_model,rerun_verifier_isolated,inspect_verifier_config`, reason `BUDGET_EXHAUSTED_GATE_REJECTED:EVIDENCE_INSUFFICIENT`, payment 14, terminal `BUDGET_EXHAUSTED_GATE_REJECTED`.
- `dc71097610aa8edd` cep-05 R0 rep 1: truth `verification_plan`, blind `projection`, governed `unknown`, `INCORRECT_AND_REJECTED`, evidence `NONE`, reason `BUDGET_EXHAUSTED_GATE_REJECTED:EVIDENCE_INSUFFICIENT`, payment 0, terminal `BUDGET_EXHAUSTED_GATE_REJECTED`.
- `f64fd61970a8623d` cep-05 R3 rep 1: truth `verification_plan`, blind `projection`, governed `unknown`, `INCORRECT_AND_REJECTED`, evidence `inspect_target_binding,inspect_source_symbols,inspect_environment,inspect_permissions,inspect_fixture_integrity,validate_semantic_model,rerun_verifier_isolated,inspect_verifier_config`, reason `BUDGET_EXHAUSTED_GATE_REJECTED:EVIDENCE_INSUFFICIENT`, payment 14, terminal `BUDGET_EXHAUSTED_GATE_REJECTED`.

## Analysis-contract conformance

1. **PASS** — Frozen primary reconstructed without conditioning: R_correct includes all six cases per arm and uses final gate-blind claims.
2. **PASS** — Primary inferential unit is case: Two repetitions are aggregated within each case before pairing.
3. **PASS** — Frozen tie rule applied: A 1/1 split is aggregated incorrect.
4. **PASS** — Pre/post transition diagnostics separable: Conditional revision diagnostics remain distinct and secondary.
5. **PASS** — Truth remains evaluator-side: Truth is loaded only from sealed governed-keys.
6. **PASS** — Gate-blind/governed separable: Canonical primary uses gate-blind; gated accuracy uses governed truth.
7. **PASS** — Canonical C1-C4 identities exact: IDs, arm mappings, families, and declared dimensions are imported from COMPARISONS.
8. **PASS** — Representation pairings exact: All case/rep evidence memberships and hashes agree.
9. **PASS** — Frozen secondary endpoints assessed: All are reconstructed except matched_evidence_use, explicitly NOT_RECONSTRUCTABLE because utilization is unpersisted.
10. **PASS** — Reproducible without provider contact: Only local immutable artifacts and evaluator files are read.

## Instrument

| Arm | calls | calls/unit | rounds/unit | total tokens | reasoning tokens | latency ms |
|---|---:|---|---|---:|---:|---:|
| R0 | 48 | {4: 12} | {3: 12} | 248602 | 44534 | 718110.6 |
| R1 | 46 | {4: 10, 3: 2} | {3: 12} | 241505 | 50982 | 798403.4 |
| R2 | 44 | {4: 8, 3: 4} | {3: 12} | 219153 | 47072 | 712000.7 |
| R3 | 30 | {2: 9, 4: 3} | {2: 9, 3: 3} | 161059 | 41418 | 662960.9 |
| R4 | 30 | {2: 9, 4: 3} | {2: 9, 3: 3} | 166299 | 45771 | 672953.0 |

Total persisted calls: 198. All 198 raw decisions reparse; all 162 persisted round records are VALID_DECISION. Repair attempts, truncations, and provider retries are zero.

The append ledgers are durable hash manifests, not externally signed hash chains. Current raw/result bytes match every persisted digest; historical ordering additionally relies on the reviewed raw-fsync-before-parse implementation.

Provider/model calls made by this correction: **0/0**. FULL remains unauthorized and unexecuted.

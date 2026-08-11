# Current status

## Objective
Correct the offline revision-002 PILOT adjudication to use the frozen `evidence_revision.py` primary endpoint, case unit, repetition aggregation, canonical comparison IDs/families, and secondary semantics without touching the completed run or contacting a provider.

## Disposition
**ANALYSIS_READY**. Integrity remains PASS. The corrected canonical case-level `R_correct` is R0 0/6, R1 2/6, R2 1/6, R3 3/6, R4 3/6.

## Frozen-contract conformance
- Primary is final gate-blind correctness across all cases; it is never conditioned on round-1 correctness.
- Primary unit is case; two repetitions are majority aggregated and 1/1 ties are incorrect.
- Case×rep accuracy and `revision_to_correct_conditional` are descriptive only.
- Canonical mappings: C1 R0/R1 primary; C2 R1/R2 primary; C3 R3/R4 primary; C4 R1/R3 secondary.
- C4 is explicitly EXEMPT_UPPER_BOUND headroom changing disclosure and budget semantics, not a single-variable isolation.
- Frozen secondary endpoints are reported separately. `matched_evidence_use` is NOT_RECONSTRUCTABLE; `revision_rate` is PARTIAL because null/UNKNOWN membership in “wrong claim” is not operationally frozen; `response_mode` is a labelled POST-HOC/PARTIAL gate-blind case×rep operationalization.

## Verification
- 11 deterministic analysis-contract conformance tests pass.
- Complete offline suite: 377 passed, 0 failed, 0 skipped.
- Deterministic adjudication rerun is byte-identical.
- Completed-run manifest before/after: `ae2827c1ea32c2860e53f19b763955908860865266791932f1454252ffeb2a13` / same.
- Provider/model calls by correction: 0/0; experimental units: 0.
- FULL authorized/executed: false/false.

## Artifacts and commit
Corrected artifacts remain under `ablation/analysis/post-audit-v2/pilot-*revision-002*` with deterministic script `pilot_adjudication_revision_002.py`. The temporal JSONL is unchanged because its unit/call reconstruction was already correct. Correction commit: `19cacb0bf09f513e0f613fdc2d2e55531495cab0`. Previous analysis commit `a7811dd3d045a281fb0724b73a245c690b287be9` is superseded.

## Worktree
The completed run and unrelated historical untracked run directories remain untouched.

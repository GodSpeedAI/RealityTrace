# Current status

## Objective
Offline-adjudicate the completed `evidence-revision-001` revision-002 PILOT for forensic integrity and endpoint reconstructability without provider contact, experimental execution, scientific changes, or FULL authorization.

## Disposition
**ANALYSIS_READY**. All required primary inputs, requested contrasts, representation pairs, and gate-blind/governed outcomes reconstruct from the frozen local artifacts without imputation or provider contact.

## Integrity
- 60/60 unique frozen UIDs and `RESULT_COMMITTED` results verified.
- 198/198 unique provider-call identities, raw files, sidecars, call-ledger entries, execution bindings, and provenance records verified.
- All result-ledger and checkpoint hashes match current result bytes.
- Canonical matrix/case input, design, executable, runner implementation, and reviewed-commit bindings match.
- All R1/R2 and R3/R4 evidence memberships, evidence-set hashes, and semantic hashes match by case+rep.
- No score artifacts exist; the completed pilot stopped at `RESULT_COMMITTED`.
- Input run manifest SHA-256: `ae2827c1ea32c2860e53f19b763955908860865266791932f1454252ffeb2a13`.

## Primary descriptive reconstruction
- R0: 1/7 (14.3%)
- R1: 3/8 (37.5%)
- R2: 3/8 (37.5%)
- R3: 4/8 (50.0%)
- R4: 7/9 (77.8%)

Requested contrasts: C1 +23.2 pp; C2 0.0 pp; C3 +12.5 pp; C4 +27.8 pp. Protocol-pilot description only; no confirmation, threshold movement, or authorization consequence.

## Verification and calls
- Deterministic analysis rerun produced byte-identical artifacts and did not change the input run manifest.
- Complete offline suite: 366 passed, 0 failed, 0 skipped.
- Provider/model calls by analysis: 0/0.
- Experimental units run by analysis: 0.
- FULL authorized/executed: false/false.

## Artifacts and commit
- `ablation/analysis/post-audit-v2/pilot-adjudication-revision-002.md`
- `ablation/analysis/post-audit-v2/pilot-adjudication-revision-002.json`
- `ablation/analysis/post-audit-v2/pilot-unit-reconstruction-revision-002.jsonl`
- `ablation/analysis/post-audit-v2/pilot-adjudication-revision-002-hashes.json`
- `ablation/analysis/post-audit-v2/pilot_adjudication_revision_002.py`
- Analysis commit: `a7811dd3d045a281fb0724b73a245c690b287be9`.

## Worktree
The completed run and unrelated historical untracked run directories remain untouched.

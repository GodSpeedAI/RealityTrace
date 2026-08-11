# Current status

## Objective
Close BLOCKER-01 through BLOCKER-04 for `evidence-revision-001` through an offline owner-authorized scientific revision; do not run or authorize execution.

## Completed
- New design fingerprint: `3031af33611c9ae8ab0fd0bba22f0abc35147d4fe564e0e3623a174d0dfe28d9` (supersedes `7edd93a3...`).
- New executable fingerprint: `12eb26d72f74709a827059c2a6ccb693ef956a6d5aa7ca2bf6f292b645da010e` (supersedes `557c0dcd...`).
- Marker-neutral case-input v2 and pre-render probe projection; no primary exclusions.
- Exact K=3, cost-3–5 MASKED_K policy; exhaustive 43-case feasibility; zero missing/unaffordable/singleton deliveries.
- Paired evidence-policy seed; R1/R2 and R3/R4 underlying evidence equality proven over 86 pairs each.
- Revised public-only pilot: 6 cases × 5 arms × 2 repetitions = 60 units.
- Runner conformed to revised scientific/executable contracts.
- Reports: `.agents/reports/12-evidence-revision-validity-closure.{md,json}`.

## Verification
- Full offline suite: 351 collected, 351 passed, 0 failed, 0 skipped.
- Ten revised artifacts reproduce canonically.
- Mutation suite rejects marker, arm-seed, singleton, over-budget, missingness, and pair-mismatch regressions.
- Model calls: 0. Provider calls: 0.

## Authority
- Status: `FROZEN_NOT_STARTED`.
- `execution_authorized=false`.
- `additional_provider_calls_authorized=false`.
- No release token/marker, preflight, pilot, full stage, smoke call, provider call, or model call was created or run.

## Worktree
Unrelated pre-existing untracked outputs under `ablation/runs/` remain untouched.

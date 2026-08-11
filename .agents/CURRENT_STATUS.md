# Current status

## Objective
Bind the already-authorized revision-002 PILOT release to the exact reviewed payment-bearing runner implementation without changing science, executable declarations, or owner state.

## Completed
- Added canonical 15-file runner implementation manifest/hash.
- Current implementation hash: `56f4463c2b228bb31f2c142d15e5431b8bd345114bd3707e14d386b1edfcf066`.
- Reviewed bound-source commit: `cf9b28b84a415ef33519e0866536ebd7787d567d`.
- Phase A requires marker implementation hash and reviewed commit before client construction.
- Gate reconstructs reviewed commit bytes and compares them with current worktree bytes.
- Run namespace persists the implementation hash and reviewed commit.
- Report: `.agents/reports/14-evidence-revision-runner-implementation-binding.{md,json}`.

## Identity
- Design unchanged: `3031af33611c9ae8ab0fd0bba22f0abc35147d4fe564e0e3623a174d0dfe28d9`.
- Executable contract unchanged: `12eb26d72f74709a827059c2a6ccb693ef956a6d5aa7ca2bf6f292b645da010e`.
- Implementation identity is separate: `56f4463c2b228bb31f2c142d15e5431b8bd345114bd3707e14d386b1edfcf066`.

## Verification
- Full offline suite: 366 collected/passed; 0 failed; 0 skipped.
- Model calls: 0. Provider calls: 0. Pilot units run: 0.
- FULL remains unauthorized.

## Owner action required
The existing marker is stale because it predates the required implementation fields. Owner must regenerate only the marker using the existing secret token after reviewing `cf9b28b84a415ef33519e0866536ebd7787d567d`. Owner state and token do not change.

## Worktree
Unrelated pre-existing untracked outputs under `ablation/runs/` remain untouched.

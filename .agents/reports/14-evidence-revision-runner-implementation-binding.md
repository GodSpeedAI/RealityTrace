# evidence-revision-001 runner implementation release binding

## Result

The pilot release gate now carries a third, independent identity:

1. scientific design: `3031af33611c9ae8ab0fd0bba22f0abc35147d4fe564e0e3623a174d0dfe28d9`;
2. executable contract: `12eb26d72f74709a827059c2a6ccb693ef956a6d5aa7ca2bf6f292b645da010e`;
3. runner implementation: `56f4463c2b228bb31f2c142d15e5431b8bd345114bd3707e14d386b1edfcf066`.

The scientific identities are unchanged. The owner state is unchanged and remains PILOT-only with `full_execution_authorized=false`.

## Canonical implementation identity

`runner_implementation_sha256` is SHA-256 over canonical JSON containing schema `evidence-revision-runner-implementation-v1` and a sorted mapping from repository-relative path to raw-file SHA-256. Bound files:

- `ablation/eval/v2/evidence_revision_run.py`
- `ablation/eval/v2/provider.py`
- `ablation/eval/v2/controller.py`
- `ablation/eval/v2/channel.py`
- `ablation/eval/v2/dstate.py`
- `ablation/eval/v2/decision.py`
- `ablation/eval/v2/execution.py`
- `ablation/eval/v2/ledger.py`
- `ablation/eval/v2/legibility.py`
- `ablation/eval/v2/ids.py`
- `ablation/eval/v2/case_inputs_v2.py`
- `ablation/eval/v2/marker_neutralization.py`
- `ablation/eval/v2/sufficiency.py`
- `ablation/eval/v2/replication.py`
- `ablation/eval/v2/validity_revision.py`


The reviewed implementation commit is `cf9b28b84a415ef33519e0866536ebd7787d567d`. The gate reconstructs the same manifest from that Git commit with local `git show` and requires it to equal current worktree bytes. Thus a committed or uncommitted bound-source byte change fails Phase A before `_live_client()`, namespace creation, or transport.

The release marker must now contain both `runner_implementation_sha256` and `reviewed_runner_commit`. Environment variables cannot supply or override either expected value; they only locate the token and marker already checked by Phase A.

## Release disposition

The marker created before this repair is now **stale by construction** because it lacks the implementation identity fields. The owner must regenerate only the marker after reviewing `cf9b28b84a415ef33519e0866536ebd7787d567d`. The existing secret token and scientific authorization remain unchanged.

## Verification

- Full offline suite: 366 collected, 366 passed, 0 failed, 0 skipped.
- Exact reviewed implementation passes.
- One bound source-byte mutation changes the implementation hash.
- Marker/implementation mismatch refuses before client construction and output creation.
- Design and executable fingerprints remain unchanged.
- FULL remains locked.
- Network/provider/model calls: 0.

## Commits

- `cf9b28b84a415ef33519e0866536ebd7787d567d` — implementation-bound Phase-A release gate
- `e624ef39ab434d0f7984b17e9a363c8b11b3abef` — offline implementation-binding regressions

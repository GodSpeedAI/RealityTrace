# Current status

## Objective
Implement the offline-proven runner for frozen `evidence-revision-001` without provider/model calls or scientific changes.

## Worktree
Branch `main`; unrelated untracked historical outputs under `ablation/runs/` remain untouched.

## Completed
- Map: `.agents/reports/10-evidence-revision-runner-map.md`.
- Runner: `ablation/eval/v2/evidence_revision_run.py`.
- Central owner/release/stage/hash/client/matrix gate; immutable namespaces.
- Durable raw-before-parse replay, result binding, checkpoints, idempotent scoring, offline fake.
- Harness action isolation and enforced-payment conformance fixes.
- 37 new runner tests; full offline suite 291/291 passing.
- Readiness artifacts: `.agents/reports/11-evidence-revision-runner-readiness.{md,json}`.

## Verification
291 collected/passed; 0 failed; 0 skipped. Model calls 0; provider calls 0. Dry-run passed with fake only. Frozen science and historical evidence unchanged.

## Blocker
The authoritative contract lacks exact pilot case selection and repetition count. Unit count and UIDs cannot be invented. Execution remains fail-closed pending owner/Opus adjudication, independently of `FROZEN_NOT_STARTED`.

## Live-only checks
Route/credentials, returned identity/metadata, finish reason, effective ceiling, optional cost fields, and provider idempotency/reconciliation. No live preflight was run.

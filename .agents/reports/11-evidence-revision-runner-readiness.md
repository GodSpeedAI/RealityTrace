# evidence-revision-001 runner readiness

## Verdict

- Runner implemented: **yes** — `ablation/eval/v2/evidence_revision_run.py`.
- Ready for live execution: **no; deliberately fail-closed**.
- Frozen fingerprint: `7edd93a3a503b9395c0d39e79540cb20fe084fc8c020413c306db88e1029d3e3`.
- Science modified: **false**.
- Historical evidence modified: **false**.
- Model calls: **0**. Provider calls: **0**.
- Owner authorization: **false** (`FROZEN_NOT_STARTED`). FULL authorization: **false**.

## Implementation and authorization

The runner composes the frozen `ARMS`, owner state, `ConditionSpec`, `DiagnosticController`, `DiagnosticState`, typed parser/repair path, opaque observation channel, RAW/LEGIBLE renderers, `PromptLeakGuard`, provider provenance/truncation structures, `CommitLedger`, sufficiency/scoring contracts, and v2 unit IDs. It adds one central execution gate, immutable run namespaces, stage separation, frozen-input hashing, a durable replay client, raw/result/score ledgers, explicit checkpoints, idempotent scoring, and an offline-only fake path.

The single gate requires the exact fingerprint, stage-specific owner status and booleans, exact authorized stage, token-bound JSON release marker, owner-state digest, corpus/evidence/action hashes, configured client, frozen matrix, unique run ID, arm isolation, semantic binding, and operational leak guard. Environment variables cannot bypass owner state. FULL requires a separate `FULL_AUTHORIZED`; pilot completion plus a token is insufficient. The live provider import occurs only after the gate.

Two objective controller conformance gaps were fixed without changing a frozen field: model actions no longer choose evidence in harness-source arms, and enforced-budget harness delivery cannot overdraw payment or silently shrink `MASKED_K`.

## Raw-before-score and resume

Every response is written+fsynced, hashed from disk, and ledger-appended+fsynced before the replay client returns it to the parser. Provider metadata is separately bound to the exact prompt/request digest. Results verify all response hashes; scoring re-verifies calls/results and commits exactly one score. Resume replays only verified request-identical responses, rejects changed bytes/cross-unit artifacts, and skips transport for completed units.

Crash tests cover before call, after provider result, after raw commit/hash/manifest, after parse, and result-committed/before-score. A crash after provider return but before local raw commit is ambiguous without provider idempotency. A durable call intent makes the runner fail closed without retrying, preventing duplicate calls; reconciliation requires provider support and is not falsely claimed as locally provable exactly-once execution.

## Offline verification

- `cd /home/sprime01/projects/sxr/ablation && uv run --python 3.12 --with pytest python -m pytest -q`
- Collected **291**; passed **291**; failed **0**; skipped **0**.
- Offline dry-run passed with 2 deterministic fake responses; no network transport instantiated.
- Model calls **0**; provider calls **0**.

## Pilot matrix blocker

The contract owns 43 eligible base cases and arms R0–R4, but records neither the exact pilot case selection nor repetition count in the Python or emitted JSON contract. Inventing either would change science.

- eligible base: **43 cases**
- arms: **5**
- repetitions: **not specified**
- pilot unit count: **not determinable**
- pilot UIDs: **not constructible**

The safe inspection command prints the derived 43 cases, imported arms, hashes, and blocker. Arbitrary matrix files are rejected unless their identity is frozen. This is an owner/Opus contract-adjudication blocker, not a live check. The task prose called R1/R2 `SINGLE`, but the authoritative frozen contract uses `MASKED_K`; the runner preserves the authoritative definition.

## Implementation commits

- `c40f513284d43741d5055d398818a527e0fa46c4`
- `a6f87d3720d4e6ac396f95077e9ec6df815714fe`
- `cf6b6cee458e4d904aa85972c8cc96b3d8e4fe6a`
- `0ee64007225a03d61c4d61aed5858158e1ae4640`

## LIVE-PREFLIGHT-REQUIRED

No live check was attempted. Later explicit authorization must establish:

- real route/credential availability;
- provider-returned requested/resolved model identity;
- real finish-reason and usage behavior;
- requested versus effective completion ceiling;
- optional provenance/cost fields returned;
- provider idempotency/reconciliation capability.

## Commands

### Safe offline inspection

```bash
cd /home/sprime01/projects/sxr/ablation && uv run --python 3.12 python -m eval.v2.evidence_revision_run inspect-matrix --stage pilot
```

### ⛔ DO NOT RUN — OWNER AUTHORIZATION REQUIRED

```bash
cd /home/sprime01/projects/sxr/ablation && SXR_ABLATION_RELEASE='<OWNER-ISSUED-TOKEN>' SXR_ABLATION_RELEASE_MARKER='/absolute/path/to/owner-issued-evidence-revision-001-pilot-release.json' uv run --python 3.12 python -m eval.v2.evidence_revision_run run --stage pilot --matrix '/absolute/path/to/owner-frozen-evidence-revision-001-pilot-matrix.json' --run-id 'evidence-revision-001-pilot-<UNIQUE-RUN-ID>' --output '/absolute/path/to/new-run-root'
```

It currently refuses because authorization is false and the pilot matrix hash is absent.

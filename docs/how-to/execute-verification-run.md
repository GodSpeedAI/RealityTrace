# How-To: Execute a Verification Run & Register Settlement

This guide describes how to execute an automated verification run using the `cargo-test` runner, derive question-bound evidence from test telemetry, and register a settlement outcome.

---

## Goal
Execute a test suite in an enrolled repository, derive evidence from observations, and register a mathematically audited settlement.

---

## Prerequisites
- An enrolled repository (`repo_id`).
- Declared Question (`question_id`) and Threshold (`threshold_id`).
- Existing semantic snapshot (`snapshot_id`), or use `snapshot:genesis`.

---

## Procedure

### 1. Begin the Run
Initialize the run binding the snapshot, question, and threshold:

```bash
RUN_ID=$(sxr run begin \
  --repo <repo_id> \
  --snapshot <snapshot_id> \
  --question <question_id> \
  --threshold <threshold_id> \
  --json | jq -r '.run_id')
echo "Started run: $RUN_ID"
```

### 2. Create the Verification Plan
Create an executable plan naming the verifier runner (`cargo-test`) and the target declaration concept:

```bash
TRANSFORM=$(sxr plan create \
  --run "$RUN_ID" \
  --verifier cargo-test \
  --target "concept:order_service" \
  --json | jq -r '.transformation_ref')
echo "Transformation ref: $TRANSFORM"
```

### 3. Run the Verifier
Execute the planned verification step:

```bash
VERIFY_JSON=$(sxr verify run --run "$RUN_ID" --json)
OBS_ID=$(echo "$VERIFY_JSON" | jq -r '.observations[0].observation_id')
echo "Captured observation: $OBS_ID"
```

### 4. Derive Question-Bound Evidence
Derive evidence from the raw observation:

```bash
sxr evidence create --run "$RUN_ID" --observation "$OBS_ID" --derive --json
```

**Verification**: Confirm that the output contains `direction` (`supports` or `contradicts`).

### 5. Register the Settlement
Evaluate the threshold against committed evidence:

```bash
sxr settlement register \
  --run "$RUN_ID" \
  --question <question_id> \
  --threshold <threshold_id> \
  --transformation "$TRANSFORM" \
  --json
```

**Expected Result**:
```json
{
  "settlement_id": "01J7...",
  "settlement_status": "settled",
  "confidence": "high"
}
```

### 6. Finish the Run
Conclude the run context:

```bash
sxr run finish --run "$RUN_ID" --json
```

---

## Implementation Locations
- Run lifecycle: [`sxr-cli/src/commands.rs:run_begin`](file:///home/sprime01/projects/sxr/sxr-cli/src/commands.rs), `run_finish`.
- Plan creation: [`sxr-cli/src/commands.rs:plan_create`](file:///home/sprime01/projects/sxr/sxr-cli/src/commands.rs).
- Verification runner: [`sxr-verify/src/verifier.rs:CargoTestVerifier`](file:///home/sprime01/projects/sxr/sxr-verify/src/verifier.rs).
- Evidence derivation: [`sxr-cli/src/commands.rs:evidence_create`](file:///home/sprime01/projects/sxr/sxr-cli/src/commands.rs).
- Settlement evaluation: [`sxr-core/src/threshold.rs:evaluate`](file:///home/sprime01/projects/sxr/sxr-core/src/threshold.rs).

---

## Common Failure Symptoms & Troubleshooting

| Symptom | Cause | Resolution |
| :--- | :--- | :--- |
| `positive acceptance requires valid declared semantics (AC-SETTLEMENT-002)` (Exit 3) | The referenced snapshot had validation status `invalid`. | A model that failed syntax/type checking cannot be positively settled. Settle as `failed` or fix the `.sea` model. |
| `evidence create: run has spooled observations` (Exit 3) | Observations were submitted with `--spool` and remain uncommitted. | Run `sxr spool drain --repo <id>` before creating evidence. |
| `plan create: verifier cargo-test not supported` (Exit 3) | Invalid verifier name specified. | SXR Slice 1 supports `cargo-test`. |

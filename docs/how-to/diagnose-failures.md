# How-To: Diagnose Verification Failures via the 9-Locus Loop

This guide describes how to use SXR's Slice 5 diagnostic attribution loop (§12.5a) when automated verification contradicts declared semantics.

---

## Goal
Isolate the true root cause of a verification contradiction across 9 candidate failure loci, systematically excluding cheap infrastructure defects before touching the high-level domain model.

---

## Prerequisites
- An enrolled repository with an executed run where verification produced `direction = "contradicts"`.
- The `question_id` and `snapshot_id` of the run.

---

## Procedure

### 1. Compute Question Residuals
Analyze the disagreement between declared semantics ($G_{declared}$) and observed evidence ($G_{observed}$):

```bash
sxr residual compute \
  --repo <repo_id> \
  --question <question_id> \
  --snapshot <snapshot_id> \
  --json
```

**Verification**: Confirm that SXR created a `residual.computed` record with `residual_kind = "declared_contradicted"` and `attribution_status = "undiagnosed"`. Save the `residual_id`.

> [!NOTE]
> Declarations that were simply not tested are classified as `declared_unobserved` with severity `info` and never enter the failure diagnostic loop.

### 2. Enumerate Failure Hypotheses
Generate the 9 candidate failure loci hypotheses:

```bash
sxr diagnostic enumerate \
  --repo <repo_id> \
  --question <question_id> \
  --snapshot <snapshot_id> \
  --residual <residual_id> \
  --json
```

Inspect the diagnostic loop state:
```bash
sxr diagnostic status --repo <repo_id> --residual <residual_id> --json
```

### 3. Gather Discriminating Evidence
For each candidate locus, gather evidence. For example, if forensic analysis proves the failure was caused by a flaky test assertion (locus `verifier`):

Submit supporting evidence for the `verifier` hypothesis:
```bash
sxr evidence for-hypothesis \
  --run <run_id> \
  --question <question_id> \
  --hypothesis <hyp_verifier_id> \
  --direction supports \
  --json
```

Submit contradicting evidence to exclude competing cheap loci:
```bash
sxr evidence for-hypothesis \
  --run <run_id> \
  --question <question_id> \
  --hypothesis <hyp_environment_id> \
  --direction contradicts \
  --json
```

### 4. Settle the Diagnostic Hypothesis
Promote the supported hypothesis to `diagnostic.settled`:

```bash
sxr diagnostic settle \
  --repo <repo_id> \
  --question <question_id> \
  --hypothesis <hyp_verifier_id> \
  --json
```

**Verification**: Confirm the output reports `diagnostic_settlement_id` and `supported_locus: "verifier"`.

### 5. Raise a Repair Proposal
Raise an actionable proposal bound to the settlement:

```bash
sxr diagnostic propose \
  --repo <repo_id> \
  --question <question_id> \
  --hypothesis <hyp_verifier_id> \
  --locus verifier \
  --target "tests/flaky_test.rs" \
  --json
```

> [!IMPORTANT]
> If you attempt to run `sxr diagnostic propose --locus declared_model` before all cheaper loci (`verifier`, `environment`, `specification`, `contract`) have been evidence-excluded, SXR halts with exit code **3 (AC-DIAG-001)**.

---

## Implementation Locations
- Residual computation: [`sxr-cli/src/commands.rs:residual_compute`](file:///home/sprime01/projects/sxr/sxr-cli/src/commands.rs)
- Hypothesis enumeration: [`sxr-cli/src/commands.rs:diagnostic_enumerate`](file:///home/sprime01/projects/sxr/sxr-cli/src/commands.rs)
- Settlement check: [`sxr-cli/src/commands.rs:diagnostic_settle`](file:///home/sprime01/projects/sxr/sxr-cli/src/commands.rs)
- Proposal check & AC-DIAG-001: [`sxr-cli/src/commands.rs:diagnostic_propose`](file:///home/sprime01/projects/sxr/sxr-cli/src/commands.rs)

---

## Common Failure Symptoms & Troubleshooting

| Symptom | Cause | Resolution |
| :--- | :--- | :--- |
| `AC-DIAG-001: model-locus proposal requires cheap loci evidence-excluded first` (Exit 3) | Attempted to propose a model change while verifier or environment was not falsified. | Provide explicit contradicting evidence targeting the cheaper hypotheses. |
| `diagnostic settle: unmet condition` (Exit 3) | Attempted to settle without fulfilling all 4 conditions. | Check `sxr diagnostic status` to inspect which alternative hypotheses lack exclusion. |

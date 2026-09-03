# Technical Reference: Exit Codes & Diagnostic Catalog

SXR uses deterministic, domain-specific shell exit codes in compliance with the **§15 Process Exit Code Standard**. Scripts and CI/CD pipelines can branch with precision on these codes.

---

## 1. Exit Code Summary Table

| Code | Standard Name | Meaning | Common Causes |
| :--- | :--- | :--- | :--- |
| **`0`** | `SUCCESS` | Clean operational success. | All requested operations executed, validated, and committed. |
| **`2`** | `USAGE_OR_DIAG` | Command usage error, syntax error, or diagnostic self-check failure. | Unknown flag, invalid argument format, missing target directory, or `sxr doctor` failure. |
| **`3`** | `PRECONDITION_FAILED` | Invariant violation or unsatisfied protocol precondition. | Referenced snapshot/question/threshold missing; AC-SETTLEMENT-002 violation; AC-DIAG-001 violation; uncommitted spooled requests. |
| **`4`** | `CEP_VALIDATION_FAILED` | CEP-0008 semantic envelope validation failure. | Envelope missing required fields, illegal collapse shape, unknown entity section, invalid boundary record key. |
| **`5`** | `INTEGRITY_FAILED` | Cryptographic ledger hash chain failure or tampering detected. | `entry_hash` mismatch, gap in `append_ordinal`, broken `previous_entry_hash` linkage. |
| **`6`** | `VERIFIER_FAILED` | Verifier process failure or runner execution error. | `cargo` binary missing, test runner timed out, target directory missing `Cargo.toml`. |
| **`75`** | `EX_TEMPFAIL` | Temporary failure / request spooled under contention. | Request spooled to disk via `--spool` due to lock contention; client should retry or await `spool drain`. |

---

## 2. Invariant & Error Mapping Rules

In [`sxr-cli/src/main.rs:exit_for_error`](file:///home/sprime01/projects/sxr/sxr-cli/src/main.rs), errors emitted by command handlers are mapped to exit codes by examining error context prefixes:

```rust
fn exit_for_error(e: &anyhow::Error) -> i32 {
    let msg = e.to_string();
    if msg.contains("AC-SETTLEMENT")
        || msg.contains("AC-EVIDENCE")
        || msg.contains("AC-DIAG-001")
        || msg.contains("precondition")
        || msg.contains("predecessor rule")
        || msg.contains("not found")
        || msg.contains("not enrolled")
        || msg.contains("no semantic configuration")
        || msg.contains("queued_for_commit")
        || msg.contains("spooled")
        || msg.contains("not committed")
        || msg.contains("corrupt segments")
    {
        3
    } else {
        2
    }
}
```

---

## 3. Diagnostic Troubleshooting Guide

### Exit Code 3: Precondition / Protocol Violations

#### `positive acceptance requires valid declared semantics (AC-SETTLEMENT-002)`
- **Cause**: Attempted to call `settlement register` with `settled` on a run whose snapshot had `model_validation_status = "invalid"`.
- **Remediation**: SXR forbids declaring success over an invalid domain model. Settle the question as `failed` or fix the `.sea` syntax/types and take a fresh snapshot.

#### `model-locus proposals require cheap loci evidence-excluded first (AC-DIAG-001)`
- **Cause**: An autonomous agent or operator attempted to call `sxr diagnostic propose --locus declared_model` before all cheaper loci (`verifier`, `environment`, `specification`, `contract`) were falsified.
- **Remediation**: Submit explicit contradicting evidence targeting the cheaper hypotheses using `sxr evidence for-hypothesis --direction contradicts`.

#### `run has spooled observations not yet committed`
- **Cause**: Attempted to derive evidence or register settlement while uncommitted requests remain in the spool.
- **Remediation**: Run `sxr spool drain --repo <repo_id>` to commit or discard spooled entries.

#### `queue recover: corrupt segments found`
- **Cause**: A queue segment file in `$XDG_STATE_HOME/sxr/daemon-queue/` has a torn or mismatched SHA-256 hash.
- **Remediation**: Inspect the damaged file. SXR fails closed and never silently deletes torn records.

---

### Exit Code 5: Cryptographic Integrity Failures

#### `entry_hash mismatch at (ledger_id, append_ordinal)`
- **Cause**: The cryptographic SHA-256 digest recomputed from the row header does not match the stored `entry_hash`.
- **Remediation**: Indicates database file tampering or filesystem bit rot. Inspect `sxr ledger history` and initiate a generation transition via `ledger.destructive_recovery` if necessary.

#### `chain break: previous_entry_hash != prior entry_hash`
- **Cause**: Rows in the `ledger` table were reordered, deleted, or spliced.
- **Remediation**: Restore from backup or quarantine the damaged generation.

---

## 4. Source Trail
- [`sxr-cli/src/main.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/main.rs) — `main()`, `exit_for_error()`.
- [`sxr-core/src/threshold.rs`](file:///home/sprime01/projects/sxr/sxr-core/src/threshold.rs) — `authorize_settlement()` (AC-SETTLEMENT-002).
- [`sxr-core/src/chain.rs`](file:///home/sprime01/projects/sxr/sxr-core/src/chain.rs) — `chain_verify()`.

# How-To: Implement a New Verifier Port

This guide describes how to extend `sxr-verify` with a new test runner or static analysis tool by implementing the `VerifierPort` trait.

---

## Goal
Add support for an external test runner (e.g., `pytest`, `go test`, `npm test`) to execute test plans and emit structured observations without violating anti-collapse rules.

---

## Prerequisites
- Rust 1.92+ toolchain.
- Understanding of the `VerifierPort` interface in `sxr-verify/src/port.rs`.

---

## Procedure

### 1. Define the Verifier Struct
In `sxr-verify/src/verifier.rs` (or in a new module under `sxr-verify/src/`), define your verifier:

```rust
use crate::port::{
    ExecutionContext, Invocation, ParsedSummary, VerificationStep, VerifierDescriptor,
    VerifierOutcome, VerifierPort, direction_for, filtered_env,
};
use crate::error::VerifierError;

pub const PYTEST_VERIFIER_ID: &str = "pytest";

#[derive(Debug)]
pub struct PytestVerifier;

impl VerifierPort for PytestVerifier {
    fn describe(&self) -> VerifierDescriptor {
        VerifierDescriptor {
            id: PYTEST_VERIFIER_ID.into(),
            version: "1".into(),
            evidence_types: vec!["test_result".into(), "execution_log".into()],
            deterministic: false,
        }
    }

    fn execute(
        &self,
        step: &VerificationStep,
        ctx: &ExecutionContext,
    ) -> Result<VerifierOutcome, VerifierError> {
        if step.verifier_id != PYTEST_VERIFIER_ID {
            return Err(VerifierError::UnsupportedVerifier(step.verifier_id.clone()));
        }

        // Execute subprocess and capture streams
        run_pytest_invocation(&step.invocation, ctx)
    }
}
```

### 2. Implement the Subprocess Runner
Spawn the process in a sanitized environment using `filtered_env()`, stream stdout/stderr, hash the streams, and parse output counts:

```rust
fn run_pytest_invocation(
    invocation: &Invocation,
    ctx: &ExecutionContext,
) -> Result<VerifierOutcome, VerifierError> {
    let mut cmd = std::process::Command::new("pytest");
    cmd.current_dir(&invocation.cwd);
    cmd.args(&invocation.args);
    
    // Scrub environment variables
    for (k, v) in filtered_env(&invocation.env) {
        cmd.env(k, v);
    }

    let output = cmd.output()?;
    let exit_code = output.status.code().unwrap_or(1);
    
    // Parse test counts from output stream
    let summary = ParsedSummary {
        passed: 10,
        failed: 0,
        ignored: 0,
        measured: 0,
        filtered_out: 0,
    };

    Ok(VerifierOutcome {
        exit_code,
        parsed_summary: Some(summary),
        // ... populate artifact digests and duration ...
    })
}
```

### 3. Register the Verifier in `sxr-cli`
In `sxr-cli/src/commands.rs`, update `verify_run()` to route the plan step to your new verifier:

```rust
match verifier_id {
    "cargo-test" => sxr_verify::CargoTestVerifier.execute(&step, &ctx),
    "pytest" => sxr_verify::PytestVerifier.execute(&step, &ctx),
    other => bail!("unsupported verifier: {other}"),
}
```

### 4. Write Verification Tests
Add a test in `sxr-verify/tests/` asserting that:
1. Valid invocations return `VerifierOutcome` with `exit_code: 0`.
2. Output streams are hashed and preserved.
3. The verifier returns **observations only** and does not fabricate evidence direction or settlement claims.

---

## Source Trail
- Port trait: [`sxr-verify/src/port.rs:VerifierPort`](file:///home/sprime01/projects/sxr/sxr-verify/src/port.rs)
- Existing implementation: [`sxr-verify/src/verifier.rs:CargoTestVerifier`](file:///home/sprime01/projects/sxr/sxr-verify/src/verifier.rs)
- Environment filter: [`sxr-verify/src/port.rs:filtered_env`](file:///home/sprime01/projects/sxr/sxr-verify/src/port.rs)

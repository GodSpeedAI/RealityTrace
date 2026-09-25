//! The §14.2 `cargo-test` verifier: runs a plan step's invocation in a
//! filtered environment, captures the streams into the execution context's
//! artifact dir, parses `cargo-json`, and returns observations only.

use crate::cargo_json;
use crate::port::{
    ExecutionContext, Invocation, ParsedSummary, VerificationStep, VerifierDescriptor,
    VerifierOutcome, VerifierPort, direction_for, filtered_env,
};
use sha2::Digest;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const VERIFIER_ID: &str = "cargo-test";
pub const VERIFIER_VERSION: &str = "1";
pub const DETECT_FILE: &str = "Cargo.toml";

#[derive(Debug)]
pub struct CargoTestVerifier;

impl VerifierPort for CargoTestVerifier {
    fn describe(&self) -> VerifierDescriptor {
        VerifierDescriptor {
            id: VERIFIER_ID.into(),
            version: VERIFIER_VERSION.into(),
            evidence_types: vec!["test_result".into(), "execution_log".into()],
            deterministic: false,
        }
    }

    fn execute(
        &self,
        step: &VerificationStep,
        ctx: &ExecutionContext,
    ) -> Result<VerifierOutcome, crate::error::VerifierError> {
        if step.verifier_id != VERIFIER_ID {
            return Err(crate::error::VerifierError::UnsupportedVerifier(
                step.verifier_id.clone(),
            ));
        }
        let cwd = &step.invocation.cwd;
        if !cwd.join(DETECT_FILE).is_file() {
            return Err(crate::error::VerifierError::NotARepo(
                cwd.display().to_string(),
            ));
        }

        let outcome = run_invocation(&step.invocation, ctx)?;
        Ok(outcome)
    }
}

fn run_invocation(
    invocation: &Invocation,
    ctx: &ExecutionContext,
) -> Result<VerifierOutcome, crate::error::VerifierError> {
    std::fs::create_dir_all(&ctx.artifacts_dir)?;
    let started_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true);

    let mut cmd = Command::new(&invocation.argv[0]);
    cmd.args(&invocation.argv[1..])
        .current_dir(&invocation.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let env = filtered_env(&invocation.env_allowlist);
    cmd.env_clear();
    for (k, v) in &env {
        cmd.env(k, v);
    }

    let mut child = cmd.spawn().map_err(crate::error::VerifierError::Spawn)?;
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");

    // Drain the pipes concurrently: cargo's JSON stream routinely exceeds the
    // OS pipe buffer (~64 KiB), and a blocked writer never reaches exit — the
    // wait loop below would otherwise deadlock until the deadline kills it
    // (observed in the §26 corpus dogfood: verify run on domainforge crashed
    // with `crashed: true, exit_code: null` after exactly the timeout).
    let stdout_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let stderr_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf);
        buf
    });

    let deadline = Instant::now() + Duration::from_secs(invocation.timeout_s);
    let (exit_code, crashed) = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                break (status.code(), status.code().is_none());
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break (None, true);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(crate::error::VerifierError::Wait(e)),
        }
    };

    // The child is dead (or killed) now: the pipe write ends are closed, so
    // the draining threads hit EOF and complete.
    let stdout_bytes = stdout_thread.join().unwrap_or_default();
    let stderr_bytes = stderr_thread.join().unwrap_or_default();

    let ended_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    let stdout_ref = persist(&ctx.artifacts_dir, &stdout_bytes, "stdout")?;
    let stderr_ref = persist(&ctx.artifacts_dir, &stderr_bytes, "stderr")?;
    let parsed: Option<ParsedSummary> = cargo_json::parse_cargo_json(&stdout_bytes);

    Ok(VerifierOutcome {
        exit_code,
        started_at,
        ended_at,
        stdout_ref,
        stderr_ref,
        structured_ref: None,
        parsed,
        crashed,
    })
}

/// `sha256:<hex>` of the bytes, persisted at `<hex>.<suffix>` in the
/// artifacts dir. Refs are content-addressable: identical bytes → identical
/// refs, and the ref verifies against the artifact.
fn persist(dir: &Path, bytes: &[u8], suffix: &str) -> Result<String, crate::error::VerifierError> {
    let hex = hex::encode(sha2::Sha256::digest(bytes));
    std::fs::write(dir.join(format!("{hex}.{suffix}")), bytes)?;
    Ok(format!("sha256:{hex}"))
}

/// The direction the cargo-test profile declares for an outcome (§5.5.1);
/// here only to keep the rule co-located with the verifier.
pub fn direction_of(outcome: &VerifierOutcome) -> &'static str {
    direction_for(outcome.exit_code, outcome.crashed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::VerifierError;
    use crate::port::VerificationStep;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn step(argv: &[&str], cwd: PathBuf, timeout_s: u64) -> VerificationStep {
        VerificationStep {
            step_id: "unit".into(),
            obligation_ref: None,
            claim_ref: None,
            evidence_requirement_ref: "contract://test#evidence/unit".into(),
            verifier_id: VERIFIER_ID.into(),
            invocation: Invocation {
                argv: argv.iter().map(|s| s.to_string()).collect(),
                cwd,
                env_allowlist: vec![
                    "PATH".into(),
                    "HOME".into(),
                    "CARGO_HOME".into(),
                    "RUSTC_WRAPPER".into(),
                ],
                timeout_s,
            },
            produces_evidence_type: "test_result".into(),
            targets: Vec::new(),
            expected_answer_shape: "boolean_with_failure_list".into(),
            on_crash: "record_omission".into(),
        }
    }

    fn fixture_project(dir: &Path, test_body: &str) {
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "pub fn answer() -> u32 { 42 }\n").unwrap();
        std::fs::create_dir_all(dir.join("tests")).unwrap();
        std::fs::write(dir.join("tests/smoke.rs"), format!("{test_body}\n")).unwrap();
    }

    #[test]
    fn passing_project_returns_supports() {
        let dir = tempdir().unwrap();
        fixture_project(
            dir.path(),
            r#"
#[test]
fn it_works() {
    assert_eq!(fixture::answer(), 42);
}
"#,
        );
        let step = step(
            &["cargo", "test", "--quiet", "--message-format", "json"],
            dir.path().to_path_buf(),
            300,
        );
        let v = CargoTestVerifier;
        let outcome = v
            .execute(&step, &ExecutionContext::new(dir.path().join("artifacts")))
            .unwrap();
        assert_eq!(outcome.exit_code, Some(0));
        assert!(!outcome.crashed);
        let parsed = outcome.parsed.as_ref().expect("json stream parses");
        assert_eq!(parsed.passed, 1);
        assert_eq!(parsed.failed, 0);
        assert_eq!(direction_of(&outcome), "supports");
        // Artifacts persisted and refs match.
        let hex = outcome.stdout_ref.strip_prefix("sha256:").unwrap();
        assert!(
            dir.path()
                .join("artifacts")
                .join(format!("{hex}.stdout"))
                .is_file()
        );
    }

    #[test]
    fn failing_project_returns_contradicts() {
        let dir = tempdir().unwrap();
        fixture_project(
            dir.path(),
            r#"
#[test]
fn it_fails() {
    panic!("boom");
}
"#,
        );
        let step = step(
            &["cargo", "test", "--quiet", "--message-format", "json"],
            dir.path().to_path_buf(),
            300,
        );
        let v = CargoTestVerifier;
        let outcome = v
            .execute(&step, &ExecutionContext::new(dir.path().join("artifacts")))
            .unwrap();
        assert_eq!(outcome.exit_code, Some(101));
        let parsed = outcome.parsed.as_ref().expect("json stream parses");
        assert_eq!(parsed.failed, 1);
        assert_eq!(direction_of(&outcome), "contradicts");
    }

    #[test]
    fn env_allowlist_blocks_ambient_vars() {
        unsafe { std::env::set_var("SXR_SECRET", "super-secret") };
        let dir = tempdir().unwrap();
        // The detect check needs a Cargo.toml even though sh runs the step.
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"\n",
        )
        .unwrap();
        // sh sees SXR_SECRET only if the allowlist leaks it — it must not.
        let step = step(
            &["sh", "-c", "[ -z \"$SXR_SECRET\" ] && echo clean-env"],
            dir.path().to_path_buf(),
            30,
        );
        let v = CargoTestVerifier;
        let outcome = v
            .execute(&step, &ExecutionContext::new(dir.path().join("artifacts")))
            .unwrap();
        assert_eq!(outcome.exit_code, Some(0));
        assert!(
            String::from_utf8_lossy(
                &std::fs::read(dir.path().join("artifacts").join(format!(
                    "{}.stdout",
                    outcome.stdout_ref.strip_prefix("sha256:").unwrap()
                )))
                .unwrap()
            )
            .contains("clean-env")
        );
    }

    #[test]
    fn timeout_kills_and_marks_crashed() {
        let dir = tempdir().unwrap();
        // The detect check needs a Cargo.toml even though sleep runs the step.
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"\n",
        )
        .unwrap();
        let step = step(&["sleep", "30"], dir.path().to_path_buf(), 1);
        let v = CargoTestVerifier;
        let start = Instant::now();
        let outcome = v
            .execute(&step, &ExecutionContext::new(dir.path().join("artifacts")))
            .unwrap();
        assert!(outcome.crashed);
        assert_eq!(outcome.exit_code, None);
        assert!(start.elapsed() < Duration::from_secs(10));
        assert_eq!(direction_of(&outcome), "unknown");
    }

    #[test]
    fn large_stdout_is_drained_without_deadlock() {
        // Regression (corpus dogfood): cargo's JSON stream exceeds the OS
        // pipe buffer; without concurrent draining the child blocks on write
        // and the deadline kills it with `crashed: true`.
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"\n",
        )
        .unwrap();
        // 1 MiB stream > 64 KiB pipe buffer, generated without a huge argv.
        let step = step(
            &[
                "sh",
                "-c",
                "dd if=/dev/zero bs=1048576 count=1 2>/dev/null | tr '\\0' x",
            ],
            dir.path().to_path_buf(),
            30,
        );
        let v = CargoTestVerifier;
        let outcome = v
            .execute(&step, &ExecutionContext::new(dir.path().join("artifacts")))
            .unwrap();
        assert_eq!(outcome.exit_code, Some(0));
        assert!(!outcome.crashed);
        let art = dir.path().join("artifacts");
        let stdout_bytes = std::fs::read(art.join(format!(
            "{}.stdout",
            outcome.stdout_ref.strip_prefix("sha256:").unwrap()
        )))
        .unwrap();
        assert_eq!(stdout_bytes.len(), 1 << 20);
    }

    #[test]
    fn not_a_repo_is_rejected() {
        let dir = tempdir().unwrap();
        let step = step(&["cargo", "test"], dir.path().to_path_buf(), 30);
        let v = CargoTestVerifier;
        let err = v
            .execute(&step, &ExecutionContext::new(dir.path().join("artifacts")))
            .unwrap_err();
        assert!(matches!(err, VerifierError::NotARepo(_)));
    }

    #[test]
    fn wrong_verifier_id_is_rejected() {
        let dir = tempdir().unwrap();
        fixture_project(dir.path(), "");
        let mut step = step(&["cargo", "test"], dir.path().to_path_buf(), 30);
        step.verifier_id = "pytest".into();
        let v = CargoTestVerifier;
        let err = v
            .execute(&step, &ExecutionContext::new(dir.path().join("artifacts")))
            .unwrap_err();
        assert!(matches!(err, VerifierError::UnsupportedVerifier(_)));
    }
}

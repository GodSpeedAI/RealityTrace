//! DomainForge `evaluate` over the CLI binary (the same integration style as
//! `sxr-df`: a configured binary path, no library dependency).
//!
//! The only process-spawning code in this crate. A binary without the
//! `evaluate` subcommand is refused with `EVALUATE_UNAVAILABLE`; this never
//! falls back to `validate`, which cannot separate unknown from false and
//! skips operation-bound policies (E section 6).

use crate::ObservedError;
use crate::compare::{EvaluateIdentity, Evaluator};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone)]
pub struct CliEvaluator {
    binary: PathBuf,
    model: PathBuf,
    version: String,
}

impl CliEvaluator {
    /// Probe the binary: it must run, report a version and know `evaluate`.
    pub fn new(
        binary: impl Into<PathBuf>,
        model: impl Into<PathBuf>,
    ) -> Result<Self, ObservedError> {
        let binary = binary.into();
        let model = model.into();
        let out = Command::new(&binary)
            .arg("--version")
            .output()
            .map_err(|e| {
                ObservedError::new(
                    "EVALUATE_UNAVAILABLE",
                    format!("cannot run {}: {e}", binary.display()),
                )
            })?;
        let version = String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .last()
            .unwrap_or_default()
            .to_owned();
        let help = Command::new(&binary)
            .args(["evaluate", "--help"])
            .stdin(Stdio::null())
            .output()
            .map_err(|e| ObservedError::new("EVALUATE_UNAVAILABLE", e.to_string()))?;
        if !help.status.success() {
            return Err(ObservedError::new(
                "EVALUATE_UNAVAILABLE",
                format!(
                    "{} (version {version:?}) has no `evaluate` subcommand; DomainForge with `evaluate` (merged 3960326, release 0.22.0) is required and `validate` is never used instead",
                    binary.display()
                ),
            ));
        }
        if !model.is_file() {
            return Err(ObservedError::new(
                "EVALUATE_INPUT",
                format!("model {} is not a file", model.display()),
            ));
        }
        Ok(Self {
            binary,
            model,
            version,
        })
    }

    pub fn model_path(&self) -> &Path {
        &self.model
    }
}

impl Evaluator for CliEvaluator {
    fn identity(&self) -> EvaluateIdentity {
        EvaluateIdentity {
            binary: self.binary.display().to_string(),
            version: self.version.clone(),
        }
    }

    fn model_bytes(&self) -> Result<Vec<u8>, ObservedError> {
        std::fs::read(&self.model).map_err(|e| ObservedError::new("EVALUATE_INPUT", e.to_string()))
    }

    fn evaluate(&self, facts_text: &str, world_name: &str) -> Result<Value, ObservedError> {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "sxr-evaluate-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir)
            .map_err(|e| ObservedError::new("EVALUATE_INPUT", e.to_string()))?;
        let facts = dir.join("facts.json");
        std::fs::write(&facts, facts_text.as_bytes())
            .map_err(|e| ObservedError::new("EVALUATE_INPUT", e.to_string()))?;
        let out = Command::new(&self.binary)
            .arg("evaluate")
            .arg("--model")
            .arg(&self.model)
            .arg("--facts")
            .arg(&facts)
            .args(["--format", "json", "--world-name", world_name])
            .stdin(Stdio::null())
            .output();
        let _ = std::fs::remove_dir_all(&dir);
        let out = out.map_err(|e| ObservedError::new("EVALUATE_FAILED", e.to_string()))?;
        if !out.status.success() {
            return Err(ObservedError::new(
                "EVALUATE_FAILED",
                format!(
                    "evaluate exited {:?}: {}",
                    out.status.code(),
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
            ));
        }
        serde_json::from_slice(&out.stdout)
            .map_err(|e| ObservedError::new("EVALUATE_FAILED", format!("output is not JSON: {e}")))
    }
}

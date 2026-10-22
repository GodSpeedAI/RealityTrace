//! The `VerifierPort` contract (§7.1, §14.1–14.3): process runners return
//! observations only. A verifier has no vocabulary for evidence, relevance,
//! reliability, or settlement — the types here enforce that structurally.
//! The declared direction rule (§5.5.1) maps an exit code to a direction.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// `verifier-profile://cargo-test@1#exit_code_to_direction` (§5.5.1): the
/// deterministic rule the cargo-test profile declares for turning an
/// observation into a direction. 0 → supports; non-zero → contradicts;
/// crashed/unknown → unknown.
pub const DIRECTION_RULE_REF: &str = "verifier-profile://cargo-test@1#exit_code_to_direction";

/// The verifier-profile ref of a registered verifier (`verifier-profile://<id>@1`).
pub fn profile_ref(verifier_id: &str) -> String {
    format!("verifier-profile://{verifier_id}@1")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifierDescriptor {
    pub id: String,
    pub version: String,
    pub evidence_types: Vec<String>,
    pub deterministic: bool,
}

/// §14.1 one plan step: *how to obtain evidence*, never what the obligation
/// means (the projection supplies meaning; the threshold settles it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationStep {
    pub step_id: String,
    pub obligation_ref: Option<String>,
    pub claim_ref: Option<String>,
    pub evidence_requirement_ref: String,
    pub verifier_id: String,
    pub invocation: Invocation,
    pub produces_evidence_type: String,
    pub targets: Vec<String>,
    pub expected_answer_shape: String,
    pub on_crash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invocation {
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    /// Environment variable names passed through to the child. Everything
    /// else is cleared — verification runs must not inherit ambient state.
    pub env_allowlist: Vec<String>,
    pub timeout_s: u64,
    /// A prefix the verifier is run under (e.g. a sandbox command). Empty: run `argv` directly. It is
    /// recorded with the observation, so evidence states how the verifier was confined.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub launcher: Vec<String>,
}

/// Runtime context for one `execute`: where artifact bytes land. The
/// verifier persists captured stdout/stderr there and returns their
/// content hashes as refs.
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub artifacts_dir: PathBuf,
}

impl ExecutionContext {
    pub fn new(artifacts_dir: impl Into<PathBuf>) -> Self {
        Self {
            artifacts_dir: artifacts_dir.into(),
        }
    }
}

/// §7.1 `VerifierOutcome` — observations only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifierOutcome {
    /// `None` when the process was killed (signal or timeout) — never 0.
    pub exit_code: Option<i32>,
    pub started_at: String,
    pub ended_at: String,
    /// `sha256:<hex>` over the captured bytes; the bytes are persisted in
    /// the execution context's artifacts dir under `<hex>.stdout`.
    pub stdout_ref: String,
    pub stderr_ref: String,
    /// Slice 1: always None (the cargo-json stream *is* the stdout).
    pub structured_ref: Option<String>,
    pub parsed: Option<ParsedSummary>,
    pub crashed: bool,
}

/// Built-in parsers for slice 1: `cargo-json` (`none` is trivially None).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParserKind {
    CargoJson,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedSummary {
    pub summary_type: String,
    pub test_count: u64,
    pub passed: u64,
    pub failed: u64,
    pub ignored: u64,
}

/// The port: `describe` + `execute`. Adapters must not see the ledger.
pub trait VerifierPort {
    fn describe(&self) -> VerifierDescriptor;
    fn execute(
        &self,
        step: &VerificationStep,
        ctx: &ExecutionContext,
    ) -> Result<VerifierOutcome, crate::error::VerifierError>;
}

/// The §5.5.1 declared direction rule for the cargo-test profile.
pub fn direction_for(exit_code: Option<i32>, crashed: bool) -> &'static str {
    if crashed || exit_code.is_none() {
        "unknown"
    } else if exit_code == Some(0) {
        "supports"
    } else {
        "contradicts"
    }
}

/// The allowlist filter shared by verifiers: pass through exactly the named
/// variables from the ambient environment.
pub fn filtered_env(allowlist: &[String]) -> BTreeMap<String, String> {
    std::env::vars()
        .filter(|(k, _)| allowlist.iter().any(|w| w == k))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_rule_maps_exit_codes() {
        assert_eq!(direction_for(Some(0), false), "supports");
        assert_eq!(direction_for(Some(1), false), "contradicts");
        assert_eq!(direction_for(Some(101), false), "contradicts");
        assert_eq!(direction_for(None, false), "unknown");
        assert_eq!(direction_for(Some(0), true), "unknown");
        assert_eq!(direction_for(None, true), "unknown");
        assert_eq!(
            DIRECTION_RULE_REF,
            "verifier-profile://cargo-test@1#exit_code_to_direction"
        );
    }

    #[test]
    fn allowlist_passes_only_named_vars() {
        unsafe { std::env::set_var("SXR_ALLOWLIST_TEST_KEEP", "keep") };
        unsafe { std::env::set_var("SXR_ALLOWLIST_TEST_DROP", "drop") };
        let env = filtered_env(&["SXR_ALLOWLIST_TEST_KEEP".into()]);
        assert!(env.contains_key("SXR_ALLOWLIST_TEST_KEEP"));
        assert!(!env.contains_key("SXR_ALLOWLIST_TEST_DROP"));
    }
}

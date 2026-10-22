//! `VerifierPort` (§7.1, §14.1–14.3): process runners that return
//! observations only — no evidence, relevance, reliability, or settlement
//! vocabulary. Slice 1 registers the `cargo-test` verifier with the
//! `cargo-json` parser and the declared exit-code-to-direction rule.

pub mod cargo_json;
pub mod digest;
pub mod error;
pub mod port;
pub mod verifier;

pub use cargo_json::parse_cargo_json;
pub use digest::tree_digest;
pub use error::VerifierError;
pub use port::{
    DIRECTION_RULE_REF, ExecutionContext, Invocation, ParsedSummary, ParserKind, VerificationStep,
    VerifierDescriptor, VerifierOutcome, VerifierPort, direction_for, filtered_env, profile_ref,
};
pub use verifier::{
    COMMAND_VERIFIER_ID, CargoTestVerifier, CommandVerifier, VERIFIER_ID, direction_of,
    verifier_for,
};

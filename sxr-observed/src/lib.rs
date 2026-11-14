//! Observed representation: ingest classification and deterministic derive.
//! No I/O except `df_cli` (spawns the DomainForge binary); the CLI owns the
//! ledger. See README.md.

pub mod canon;
pub mod compare;
pub mod derive;
pub mod df_cli;
pub mod ingest;
pub mod tail;

use std::fmt;

/// An error carrying a stable oracle code (`CHAIN_VERIFY_FAILED`, ...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedError {
    pub code: &'static str,
    pub message: String,
}

impl ObservedError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for ObservedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ObservedError {}

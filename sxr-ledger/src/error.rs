//! Errors for the ledger layer. The `ledger verify` and append paths name the
//! `ledger_id` and `append_ordinal` of the first divergence (AC-LEDGER-001).

use rusqlite::Error as SqliteError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] SqliteError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("schema error: {0}")]
    Schema(String),

    #[error("ledger integrity: {0}")]
    Integrity(String),

    #[error("writer lock not acquired within {timeout_secs}s: {reason}")]
    LockTimeout { timeout_secs: u64, reason: String },

    #[error("generation error: {0}")]
    Generation(String),

    #[error("not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, Error>;

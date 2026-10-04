//! The `SemanticSourcePort` adapter errors (`SemanticError`).

#[derive(Debug, thiserror::Error)]
pub enum SemanticError {
    #[error("domainforge binary unavailable: {0}")]
    BinaryUnavailable(String),
    #[error("unsupported domainforge version: expected {expected}, found {found}")]
    UnsupportedVersion { expected: String, found: String },
    #[error("domainforge exited with unexpected code {code:?}: {stderr}")]
    UnexpectedExit { code: Option<i32>, stderr: String },
    #[error("domainforge output is not a CEP envelope: {0}")]
    MalformedOutput(String),
    #[error("envelope fails the pinned CEP schema (§52/§53): {0}")]
    EnvelopeSchemaViolation(String),
    #[error("hash mismatch: {0}")]
    HashMismatch(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

//! `VerifierPort` adapter errors.

#[derive(Debug, thiserror::Error)]
pub enum VerifierError {
    #[error("verifier {0} is not registered (slice 1 registers only cargo-test)")]
    UnsupportedVerifier(String),
    #[error("not a cargo project: no Cargo.toml at {0}")]
    NotARepo(String),
    #[error("failed to spawn verifier: {0}")]
    Spawn(std::io::Error),
    #[error("failed to wait for verifier: {0}")]
    Wait(std::io::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

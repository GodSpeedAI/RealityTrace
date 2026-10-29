//! `SemanticSourcePort` (§7.1) implemented by the `CliDomainForgeAdapter`
//! over a Slice 0B `domainforge` binary.

pub mod adapter;
pub mod error;

pub use adapter::{AdapterCapabilities, CliDomainForgeAdapter, SemanticContextRequest};
pub use error::SemanticError;

/// The DomainForge release this RealityTrace build is verified against.
/// Production callers opt in via `CliDomainForgeAdapter::with_required_version`.
pub const PINNED_DOMAINFORGE_VERSION: &str = "0.21.2";

#[cfg(test)]
mod pin_tests {
    /// The pin is a frozen protocol constant: this test documents the verified
    /// binding (the real-binary composition tests ran against exactly this
    /// release) so an accidental edit cannot silently re-point the enforced
    /// dependency. Deliberate bumps must change both lines together and rerun
    /// the real-binary tests.
    #[test]
    fn pinned_domainforge_version_is_the_verified_release() {
        assert_eq!(super::PINNED_DOMAINFORGE_VERSION, "0.21.2");
    }
}

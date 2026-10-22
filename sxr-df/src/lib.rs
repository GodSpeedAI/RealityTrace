//! `SemanticSourcePort` (§7.1) implemented by the `CliDomainForgeAdapter`
//! over a Slice 0B `domainforge` binary.

pub mod adapter;
pub mod error;

pub use adapter::{AdapterCapabilities, CliDomainForgeAdapter, SemanticContextRequest};
pub use error::SemanticError;

/// The DomainForge release this RealityTrace build is verified against.
/// Production callers opt in via `CliDomainForgeAdapter::with_required_version`.
pub const PINNED_DOMAINFORGE_VERSION: &str = "0.21.1";

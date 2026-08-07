//! `SemanticSourcePort` (§7.1) implemented by the `CliDomainForgeAdapter`
//! over a Slice 0B `domainforge` binary.

pub mod adapter;
pub mod error;

pub use adapter::{AdapterCapabilities, CliDomainForgeAdapter, SemanticContextRequest};
pub use error::SemanticError;

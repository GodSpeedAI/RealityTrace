//! sxr-git
//!
//! `RepositoryDiscoveryPort` (§9.1, §9.2): minted ULID identities with
//! evidence, never derived from Git properties. Discovery shells out to the
//! git CLI in the exact `rev-parse` sequence of §9.2; identity matching
//! produces typed continuity verdicts and never merges.

pub mod discovery;
pub mod identity;
pub mod matcher;

pub use discovery::{DiscoveryError, GitCli, GitRunner, RepoEvidence, discover, evidence_of, git};
pub use identity::{CheckoutIdentity, ContinuityVerdict, IdentityEvidence, mint_identities};
pub use matcher::match_continuity;

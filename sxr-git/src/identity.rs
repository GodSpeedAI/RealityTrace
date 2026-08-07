//! §9.1 identity: `repo_id` and `checkout_id` are ULIDs minted at enrollment,
//! never derived from any Git property. Identity evidence is a *set* of
//! fingerprints; each member may change, the set supports a continuity claim.

use crate::discovery::IdentityEvidenceStatus;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityEvidence {
    /// Minted at enrollment; None before enrollment (evidence is attached to
    /// the id, never the other way around).
    pub repo_id: Option<String>,
    pub checkout_id: Option<String>,
    /// Root commit(s) — ALL of them (§9.2).
    pub root_commits: Vec<String>,
    /// Normalized remote URLs.
    pub remotes: Vec<String>,
    /// Tree hashes of the N oldest commits.
    pub early_tree_hashes: Vec<String>,
    pub enrollment_path: PathBuf,
    pub first_seen_at: String,
    pub head_at_enrollment: Option<String>,
    pub evidence_status: IdentityEvidenceStatus,
    pub verdict_history: Vec<ContinuityVerdict>,
}

/// §9.1 typed continuity verdicts. A verdict is a claim with evidence, never
/// a merge decision by itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum ContinuityVerdict {
    /// Explicit sxr enrollment/export lineage token match, or local Git
    /// worktree metadata proving membership in an enrolled common-dir subject.
    SameRepository {
        repo_id: String,
    },
    /// Git fingerprints overlap exactly one enrolled repository but no
    /// explicit/local continuity exists — ask, never auto-merge.
    ProbableMatch {
        candidates: Vec<String>,
    },
    DifferentRepository,
    /// Evidence matches more than one enrolled repo; refuse (the fork case).
    Ambiguous {
        candidates: Vec<String>,
    },
    Unknown {
        reason: String,
    },
}

/// Mint a fresh ULID pair at enrollment.
pub fn mint_identities() -> (String, String) {
    let repo_id = ulid::Ulid::new().to_string();
    let checkout_id = ulid::Ulid::new().to_string();
    (repo_id, checkout_id)
}

/// The §9.1 `same_repository` reuse rule: checkout_id is reused only for one
/// unambiguous match to that checkout; a new linked worktree mints a new one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckoutIdentity {
    pub repo_id: String,
    pub checkout_id: String,
    pub is_linked_worktree: bool,
}

/// Decide whether this observed worktree is a linked worktree of the same
/// repository (git-dir ≠ git-common-dir, §9.2) — membership evidence, never
/// identity.
pub fn classify_checkout(
    git_dir: &str,
    git_common_dir: &str,
    repo_id: &str,
    minted: impl Fn() -> (String, String),
) -> CheckoutIdentity {
    if git_dir != git_common_dir {
        // Linked worktree: same repo, new checkout id.
        let (_, checkout_id) = minted();
        CheckoutIdentity {
            repo_id: repo_id.to_string(),
            checkout_id,
            is_linked_worktree: true,
        }
    } else {
        CheckoutIdentity {
            repo_id: repo_id.to_string(),
            checkout_id: ulid::Ulid::new().to_string(),
            is_linked_worktree: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minted_ids_are_ulids_not_git_derived() {
        let (repo_id, checkout_id) = mint_identities();
        assert_eq!(repo_id.len(), 26);
        assert_eq!(checkout_id.len(), 26);
        // ULID charset is Crockford base32.
        for c in repo_id.chars().chain(checkout_id.chars()) {
            assert!(c.is_ascii_alphanumeric(), "invalid ulid char {c}");
        }
    }

    #[test]
    fn minted_ids_are_unique() {
        let a = mint_identities();
        let b = mint_identities();
        assert_ne!(a, b);
    }

    #[test]
    fn linked_worktree_mints_new_checkout_same_repo() {
        let c = classify_checkout(
            "/repo/worktrees/feature/.git",
            "/repo/.git",
            "01R",
            mint_identities,
        );
        assert!(c.is_linked_worktree);
        assert_eq!(c.repo_id, "01R");
        assert_eq!(c.checkout_id.len(), 26);
    }

    #[test]
    fn plain_worktree_is_its_own_checkout() {
        let c = classify_checkout("/repo/.git", "/repo/.git", "01R", mint_identities);
        assert!(!c.is_linked_worktree);
        assert_eq!(c.repo_id, "01R");
    }
}

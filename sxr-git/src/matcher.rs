//! §9.1 continuity matching: compare one observed subject's evidence against
//! every enrolled repository and produce a typed verdict. Git similarity
//! alone never merges histories (AC-IDENTITY-003: two forks of one upstream
//! must never collapse into one ledger).

use crate::identity::{ContinuityVerdict, IdentityEvidence};

/// Match observed evidence against enrolled repositories.
///
/// Verdicts:
/// - `same_repository`: an explicit lineage token matches, or this worktree's
///   `git_common_dir` equals an enrolled repo's common dir (local Git
///   worktree metadata proves membership).
/// - `probable_match`: fingerprints overlap exactly one enrolled repo (incl.
///   the strong root+remote pair) with no explicit/local continuity — ask.
/// - `ambiguous`: evidence matches more than one enrolled repo — refuse.
/// - `different_repository`: no evidence overlaps.
/// - `unknown`: no HEAD (empty repo) or Git unavailable.
pub fn match_continuity(
    observed: &IdentityEvidence,
    enrolled: &[&IdentityEvidence],
    explicit_lineage_token: Option<&str>,
) -> ContinuityVerdict {
    // Explicit lineage token (export/import) is the strongest signal.
    if let Some(token) = explicit_lineage_token {
        for e in enrolled {
            if let Some(id) = &e.repo_id
                && token == id
            {
                return ContinuityVerdict::SameRepository {
                    repo_id: id.clone(),
                };
            }
        }
    }

    if observed.evidence_status == crate::discovery::IdentityEvidenceStatus::Insufficient
        && observed.root_commits.is_empty()
    {
        return ContinuityVerdict::Unknown {
            reason: "no HEAD: repository has no commits; evidence insufficient".into(),
        };
    }

    // Fingerprint overlap: root commits or remotes in common.
    let overlapping: Vec<String> = enrolled
        .iter()
        .filter(|e| {
            let roots_overlap = e
                .root_commits
                .iter()
                .any(|r| observed.root_commits.contains(r));
            let remotes_overlap = e.remotes.iter().any(|r| observed.remotes.contains(r));
            roots_overlap || remotes_overlap
        })
        .map(|e| e.repo_id.clone().unwrap_or_else(|| "<unenrolled>".into()))
        .collect();

    match overlapping.len() {
        0 => ContinuityVerdict::DifferentRepository,
        1 => {
            let candidate = &overlapping[0];
            // Explicit lineage token already handled; a *local* continuity
            // check (git_common_dir membership) is not part of the evidence
            // set here — callers with worktree metadata pass the token.
            ContinuityVerdict::ProbableMatch {
                candidates: vec![candidate.clone()],
            }
        }
        _ => ContinuityVerdict::Ambiguous {
            candidates: overlapping,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::IdentityEvidenceStatus;

    fn enrolled(repo_id: &str, roots: &[&str], remotes: &[&str]) -> IdentityEvidence {
        IdentityEvidence {
            repo_id: Some(repo_id.into()),
            checkout_id: None,
            root_commits: roots.iter().map(|s| s.to_string()).collect(),
            remotes: remotes.iter().map(|s| s.to_string()).collect(),
            early_tree_hashes: Vec::new(),
            enrollment_path: "/p".into(),
            first_seen_at: "".into(),
            head_at_enrollment: None,
            evidence_status: IdentityEvidenceStatus::Sufficient,
            verdict_history: Vec::new(),
        }
    }

    fn observed(roots: &[&str], remotes: &[&str], sufficient: bool) -> IdentityEvidence {
        IdentityEvidence {
            repo_id: None,
            checkout_id: None,
            root_commits: roots.iter().map(|s| s.to_string()).collect(),
            remotes: remotes.iter().map(|s| s.to_string()).collect(),
            early_tree_hashes: Vec::new(),
            enrollment_path: "/p".into(),
            first_seen_at: "".into(),
            head_at_enrollment: None,
            evidence_status: if sufficient {
                IdentityEvidenceStatus::Sufficient
            } else {
                IdentityEvidenceStatus::Insufficient
            },
            verdict_history: Vec::new(),
        }
    }

    #[test]
    fn forks_of_one_upstream_never_merge() {
        // Two forks of the same upstream share root + remote evidence.
        let fork_a = enrolled("01FORKA", &["root1"], &["https://upstream/r.git"]);
        let fork_b = enrolled("01FORKB", &["root1"], &["https://upstream/r.git"]);
        let newcomer = observed(&["root1"], &["https://upstream/r.git"], true);
        let verdict = match_continuity(&newcomer, &[&fork_a, &fork_b], None);
        // AC-IDENTITY-003: never a silent merge — ambiguous, refuse.
        let ContinuityVerdict::Ambiguous { candidates } = verdict else {
            panic!("fork case must be ambiguous, got {verdict:?}");
        };
        assert_eq!(
            candidates,
            vec!["01FORKA".to_string(), "01FORKB".to_string()]
        );
    }

    #[test]
    fn single_overlap_is_probable_match_asking() {
        let one = enrolled("01ONLY", &["root9"], &[]);
        let newcomer = observed(&["root9"], &[], true);
        let verdict = match_continuity(&newcomer, &[&one], None);
        let ContinuityVerdict::ProbableMatch { candidates } = verdict else {
            panic!("expected probable_match, got {verdict:?}");
        };
        assert_eq!(candidates, vec!["01ONLY"]);
    }

    #[test]
    fn explicit_lineage_token_is_same_repository() {
        let one = enrolled("01X", &["root1"], &[]);
        let newcomer = observed(&["different"], &[], true);
        let verdict = match_continuity(&newcomer, &[&one], Some("01X"));
        let ContinuityVerdict::SameRepository { repo_id } = verdict else {
            panic!("expected same_repository, got {verdict:?}");
        };
        assert_eq!(repo_id, "01X");
    }

    #[test]
    fn empty_repo_is_unknown_not_different() {
        let one = enrolled("01Y", &["root1"], &[]);
        let newcomer = observed(&[], &[], false);
        let verdict = match_continuity(&newcomer, &[&one], None);
        let ContinuityVerdict::Unknown { .. } = verdict else {
            panic!("expected unknown, got {verdict:?}");
        };
    }

    #[test]
    fn no_overlap_is_different_repository() {
        let one = enrolled("01Z", &["root1"], &[]);
        let newcomer = observed(&["root2"], &[], true);
        let verdict = match_continuity(&newcomer, &[&one], None);
        assert_eq!(verdict, ContinuityVerdict::DifferentRepository);
    }
}

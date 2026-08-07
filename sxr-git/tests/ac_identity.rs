//! Real-git integration tests (§9.1, §9.2, §26): test-owned fixtures only.
//! - AC-IDENTITY-004: a repository with no commits is valid, enrollable, and
//!   yields `identity_evidence_status: "insufficient"` rather than erroring.
//! - negative `.git` fixture: an empty `.git` directory is NOT a repository.
//! - worktree split: a linked worktree shares the repo ledger subject but
//!   gets its own checkout_id.
//! - fork-similarity never merges (AC-IDENTITY-003) — the matcher refuses.

use sxr_git::discovery::{GitCli, discover, evidence_of};
use sxr_git::identity::{classify_checkout, mint_identities};
use sxr_git::matcher::match_continuity;
use tempfile::tempdir;

fn git(cwd: &std::path::Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .current_dir(cwd)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .status()
        .expect("git must be installed");
    assert!(status.success(), "git {args:?} failed");
}

fn init_repo(dir: &std::path::Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "test@local"]);
    git(dir, &["config", "user.name", "test"]);
    std::fs::write(dir.join("README.md"), "# fixture\n").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-qm", "initial"]);
}

#[test]
fn empty_repo_is_enrollable_with_insufficient_evidence() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    let d = discover(&GitCli, dir.path()).unwrap();
    assert!(d.root_commits.is_empty());
    assert_eq!(
        d.identity_evidence_status,
        sxr_git::discovery::IdentityEvidenceStatus::Insufficient
    );
    // And the matcher treats it as `unknown`, never `different_repository`.
    let ev = evidence_of(&d, dir.path());
    let verdict = match_continuity(&ev, &[], None);
    assert!(matches!(
        verdict,
        sxr_git::identity::ContinuityVerdict::Unknown { .. }
    ));
}

#[test]
fn empty_git_dir_is_not_a_repository() {
    let dir = tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    let err = discover(&GitCli, dir.path()).unwrap_err();
    assert!(
        matches!(err, sxr_git::discovery::DiscoveryError::CommandFailed(..))
            || matches!(err, sxr_git::discovery::DiscoveryError::NotAWorkTree(..)),
        "empty .git dir must not be treated as a repository: {err}"
    );
}

#[test]
fn linked_worktree_shares_repo_but_gets_own_checkout() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    // Create a linked worktree (git worktree add creates .git as a FILE).
    let wt = dir.path().join("worktree");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature",
            wt.to_str().unwrap(),
        ],
    );
    assert!(
        wt.join(".git").is_file(),
        "linked worktree .git must be a file"
    );

    let main = discover(&GitCli, &repo).unwrap();
    let linked = discover(&GitCli, &wt).unwrap();
    assert_ne!(main.git_dir, linked.git_dir);
    assert_eq!(main.git_common_dir, linked.git_common_dir);

    // Same repo subject (common dir), distinct checkout.
    let (repo_id, _) = mint_identities();
    let main_checkout = classify_checkout(
        &main.git_dir,
        &main.git_common_dir,
        &repo_id,
        mint_identities,
    );
    let linked_checkout = classify_checkout(
        &linked.git_dir,
        &linked.git_common_dir,
        &repo_id,
        mint_identities,
    );
    assert!(!main_checkout.is_linked_worktree);
    assert!(linked_checkout.is_linked_worktree);
    assert_eq!(linked_checkout.repo_id, repo_id);
    assert_ne!(main_checkout.checkout_id, linked_checkout.checkout_id);
}

#[test]
fn fork_similarity_never_merges() {
    let dir = tempdir().unwrap();
    let upstream = dir.path().join("upstream");
    std::fs::create_dir_all(&upstream).unwrap();
    init_repo(&upstream);
    let upstream_id = "01FORKUPSTREAM";
    let upstream_ev = {
        let d = discover(&GitCli, &upstream).unwrap();
        let mut ev = evidence_of(&d, &upstream);
        ev.repo_id = Some(upstream_id.into());
        ev
    };

    // Two clones of the same upstream: identical root + remote evidence.
    let fork_a = dir.path().join("fork-a");
    let fork_b = dir.path().join("fork-b");
    std::fs::create_dir_all(&fork_a).unwrap();
    std::fs::create_dir_all(&fork_b).unwrap();
    git(
        &upstream,
        &[
            "clone",
            "-q",
            upstream.to_str().unwrap(),
            fork_a.to_str().unwrap(),
        ],
    );
    git(
        &upstream,
        &[
            "clone",
            "-q",
            upstream.to_str().unwrap(),
            fork_b.to_str().unwrap(),
        ],
    );
    let d_a = discover(&GitCli, &fork_a).unwrap();
    let d_b = discover(&GitCli, &fork_b).unwrap();
    assert_eq!(d_a.root_commits, d_b.root_commits);

    // Fork A has the same root commit as the enrolled upstream: the matcher
    // must never auto-merge; with one overlap it asks (probable_match).
    let ev_a = evidence_of(&d_a, &fork_a);
    let verdict = match_continuity(&ev_a, &[&upstream_ev], None);
    assert!(matches!(
        verdict,
        sxr_git::identity::ContinuityVerdict::ProbableMatch { .. }
    ));
    // Two overlapping enrolled repos (two forks) → ambiguous, refuse.
    let ev_b = evidence_of(&d_b, &fork_b);
    let fork_ev = {
        let mut e = evidence_of(&d_a, &fork_a);
        e.repo_id = Some("01FORKA".into());
        e
    };
    let verdict = match_continuity(&ev_b, &[&fork_ev, &upstream_ev], None);
    assert!(matches!(
        verdict,
        sxr_git::identity::ContinuityVerdict::Ambiguous { .. }
    ));
}

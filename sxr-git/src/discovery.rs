//! §9.2 discovery via the git CLI in the mandated order. `.git` is never
//! stat'ed and branched on `is_dir()` — a linked worktree's `.git` is a file,
//! and an empty `.git` directory satisfies `is_dir()` while `git status`
//! fails.

use crate::identity::IdentityEvidence;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("git is unavailable: {0}")]
    GitUnavailable(String),
    #[error("not inside a git work tree (bare or uninitialized): {0}")]
    NotAWorkTree(String),
    #[error("git command failed: {0}: {1}")]
    CommandFailed(String, String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// The git CLI runner; swappable for tests.
pub trait GitRunner {
    /// Run `git <args>` in `cwd`, returning trimmed stdout on success.
    fn run(&self, cwd: &Path, args: &[&str]) -> Result<String, DiscoveryError>;
}

/// Shells out to the real `git` binary.
#[derive(Debug, Clone, Default)]
pub struct GitCli;

impl GitRunner for GitCli {
    fn run(&self, cwd: &Path, args: &[&str]) -> Result<String, DiscoveryError> {
        let mut cmd = Command::new("git");
        cmd.current_dir(cwd).args(args);
        let output = cmd
            .output()
            .map_err(|e| DiscoveryError::GitUnavailable(e.to_string()))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(DiscoveryError::CommandFailed(args.join(" "), stderr));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

pub fn git<R: GitRunner>(runner: &R, cwd: &Path, args: &[&str]) -> Result<String, DiscoveryError> {
    runner.run(cwd, args)
}

/// Everything §9.2's rev-parse sequence observes about a work tree.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RepoEvidence {
    /// `rev-parse --show-toplevel`; volatile observed metadata, never identity.
    pub filesystem_path: PathBuf,
    /// `rev-parse --git-dir` (this worktree's git dir).
    pub git_dir: String,
    /// `rev-parse --git-common-dir`; differs ⇒ linked worktree.
    pub git_common_dir: String,
    /// `rev-list --max-parents=0 HEAD` — ALL root commits, as evidence.
    pub root_commits: Vec<String>,
    /// `remote -v`, normalized.
    pub remotes: Vec<String>,
    /// `rev-parse HEAD` at observation time; per-observation evidence.
    pub head_at_observation: Option<String>,
    /// No HEAD (repository with no commits) — valid, enrollable, identity
    /// evidence insufficient (§9.2, AC-IDENTITY-004).
    pub identity_evidence_status: IdentityEvidenceStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityEvidenceStatus {
    Sufficient,
    Insufficient,
}

impl RepoEvidence {
    pub fn status(&self) -> IdentityEvidenceStatus {
        self.identity_evidence_status
    }
}

/// Resolve rev-parse output to an absolute path against the invoking cwd.
fn abs_from(cwd: &Path, p: &str) -> String {
    let p = Path::new(p);
    if p.is_absolute() {
        p.to_string_lossy().into_owned()
    } else {
        cwd.join(p).to_string_lossy().into_owned()
    }
}

/// Discover one filesystem subject, using the exact §9.2 sequence:
///
/// ```text
/// git rev-parse --is-inside-work-tree
/// git rev-parse --show-toplevel
/// git rev-parse --git-dir
/// git rev-parse --git-common-dir
/// git rev-parse --verify HEAD
/// git rev-list --max-parents=0 HEAD
/// git remote -v
/// ```
///
/// Never stats `.git`. Refuses bare repositories in Slice 1.
pub fn discover<R: GitRunner>(runner: &R, path: &Path) -> Result<RepoEvidence, DiscoveryError> {
    let inside = git(runner, path, &["rev-parse", "--is-inside-work-tree"])?;
    if inside.trim() != "true" {
        return Err(DiscoveryError::NotAWorkTree(format!(
            "{} is not inside a git work tree (bare or uninitialized)",
            path.display()
        )));
    }
    let toplevel = PathBuf::from(git(runner, path, &["rev-parse", "--show-toplevel"])?);
    // §9.2's rev-parse outputs may be relative to the invoking cwd; normalize
    // so evidence is comparable across worktrees.
    let git_dir = abs_from(path, &git(runner, path, &["rev-parse", "--git-dir"])?);
    let git_common_dir = abs_from(
        path,
        &git(runner, path, &["rev-parse", "--git-common-dir"])?,
    );
    let head = git(runner, path, &["rev-parse", "--verify", "HEAD"])
        .ok()
        .filter(|h| !h.is_empty());
    let (root_commits, status) = match git(runner, path, &["rev-list", "--max-parents=0", "HEAD"]) {
        Ok(out) => {
            let roots: Vec<String> = out
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            if roots.is_empty() {
                (Vec::new(), IdentityEvidenceStatus::Insufficient)
            } else {
                (roots, IdentityEvidenceStatus::Sufficient)
            }
        }
        Err(_) => (Vec::new(), IdentityEvidenceStatus::Insufficient),
    };
    let remotes = git(runner, path, &["remote", "-v"])
        .ok()
        .map(normalize_remotes)
        .unwrap_or_default();

    Ok(RepoEvidence {
        filesystem_path: toplevel,
        git_dir,
        git_common_dir,
        root_commits,
        remotes,
        head_at_observation: head,
        identity_evidence_status: status,
    })
}

/// `origin  https://… (fetch)` → normalized unique URLs. Order preserved.
fn normalize_remotes(output: String) -> Vec<String> {
    let mut seen = Vec::new();
    for line in output.lines() {
        let mut parts = line.split_whitespace();
        let _name = parts.next();
        if let Some(url) = parts.next() {
            let url = url
                .trim_end_matches("(fetch)")
                .trim_end_matches("(push)")
                .trim();
            if !url.is_empty() && !seen.contains(&url.to_string()) {
                seen.push(url.to_string());
            }
        }
    }
    seen
}

/// Build the full identity evidence set (§9.1) from a discovery result.
pub fn evidence_of(d: &RepoEvidence, enrollment_path: &Path) -> IdentityEvidence {
    IdentityEvidence {
        repo_id: None,
        checkout_id: None,
        root_commits: d.root_commits.clone(),
        remotes: d.remotes.clone(),
        early_tree_hashes: Vec::new(),
        enrollment_path: enrollment_path.to_path_buf(),
        first_seen_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        head_at_enrollment: d.head_at_observation.clone(),
        evidence_status: d.identity_evidence_status,
        verdict_history: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scripted runner over canned outputs (no git binary needed for the
    /// sequencing contract; the CLI tests exercise the real binary).
    struct ScriptedRunner {
        outputs: std::collections::HashMap<String, String>,
        calls: std::cell::RefCell<Vec<String>>,
    }

    impl ScriptedRunner {
        fn new() -> Self {
            Self {
                outputs: std::collections::HashMap::new(),
                calls: std::cell::RefCell::new(Vec::new()),
            }
        }
        fn expect(&mut self, args: &[&str], out: &str) {
            self.outputs.insert(args.join(" "), out.to_string());
        }
    }

    impl GitRunner for ScriptedRunner {
        fn run(&self, cwd: &Path, args: &[&str]) -> Result<String, DiscoveryError> {
            let _ = cwd;
            let key = args.join(" ");
            self.calls.borrow_mut().push(key.clone());
            self.outputs
                .get(&key)
                .cloned()
                .ok_or_else(|| DiscoveryError::CommandFailed(key.clone(), "unexpected call".into()))
        }
    }

    #[test]
    fn discovery_uses_the_mandated_sequence_in_order() {
        let mut r = ScriptedRunner::new();
        r.expect(&["rev-parse", "--is-inside-work-tree"], "true");
        r.expect(&["rev-parse", "--show-toplevel"], "/repo");
        r.expect(&["rev-parse", "--git-dir"], "/repo/.git");
        r.expect(&["rev-parse", "--git-common-dir"], "/repo/.git");
        r.expect(&["rev-parse", "--verify", "HEAD"], "abc123");
        r.expect(&["rev-list", "--max-parents=0", "HEAD"], "root1\nroot2\n");
        r.expect(
            &["remote", "-v"],
            "origin\thttps://example.com/r.git (fetch)\norigin\thttps://example.com/r.git (push)\n",
        );
        let d = discover(&r, Path::new("/repo")).unwrap();
        assert_eq!(d.root_commits, vec!["root1", "root2"]);
        assert_eq!(d.remotes, vec!["https://example.com/r.git"]);
        assert_eq!(
            d.identity_evidence_status,
            IdentityEvidenceStatus::Sufficient
        );
        let calls = r.calls.borrow();
        let expected_order = [
            "rev-parse --is-inside-work-tree",
            "rev-parse --show-toplevel",
            "rev-parse --git-dir",
            "rev-parse --git-common-dir",
            "rev-parse --verify HEAD",
            "rev-list --max-parents=0 HEAD",
            "remote -v",
        ];
        for (i, e) in expected_order.iter().enumerate() {
            assert_eq!(calls.get(i).map(|s| s.as_str()), Some(*e), "step {i}");
        }
    }

    #[test]
    fn empty_repo_yields_insufficient_evidence_not_error() {
        let mut r = ScriptedRunner::new();
        r.expect(&["rev-parse", "--is-inside-work-tree"], "true");
        r.expect(&["rev-parse", "--show-toplevel"], "/empty");
        r.expect(&["rev-parse", "--git-dir"], "/empty/.git");
        r.expect(&["rev-parse", "--git-common-dir"], "/empty/.git");
        r.expect(&["rev-parse", "--verify", "HEAD"], "HEAD");
        // rev-list fails: no commits yet.
        let d = discover(&r, Path::new("/empty")).unwrap();
        assert_eq!(d.root_commits, Vec::<String>::new());
        assert_eq!(
            d.identity_evidence_status,
            IdentityEvidenceStatus::Insufficient
        );
    }

    #[test]
    fn bare_repository_is_refused() {
        let mut r = ScriptedRunner::new();
        r.expect(&["rev-parse", "--is-inside-work-tree"], "false");
        let err = discover(&r, Path::new("/bare")).unwrap_err();
        assert!(matches!(err, DiscoveryError::NotAWorkTree(_)));
    }

    #[test]
    fn relative_git_dir_is_normalized_against_cwd() {
        let mut r = ScriptedRunner::new();
        r.expect(&["rev-parse", "--is-inside-work-tree"], "true");
        r.expect(&["rev-parse", "--show-toplevel"], "/wt/repo");
        r.expect(&["rev-parse", "--git-dir"], ".git");
        r.expect(&["rev-parse", "--git-common-dir"], ".git");
        r.expect(&["rev-parse", "--verify", "HEAD"], "abc123");
        let d = discover(&r, Path::new("/wt/repo")).unwrap();
        assert_eq!(d.git_dir, "/wt/repo/.git");
        assert_eq!(d.git_common_dir, "/wt/repo/.git");
    }

    #[test]
    fn git_unavailable_is_reported() {
        struct NoGit;
        impl GitRunner for NoGit {
            fn run(&self, _cwd: &Path, _args: &[&str]) -> Result<String, DiscoveryError> {
                Err(DiscoveryError::GitUnavailable("no git binary".into()))
            }
        }
        let err = discover(&NoGit, Path::new("/x")).unwrap_err();
        assert!(matches!(err, DiscoveryError::GitUnavailable(_)));
    }
}

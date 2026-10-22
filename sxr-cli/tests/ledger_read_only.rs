//! Read-only ledger commands never create a ledger. Before this, `ledger verify` on an unknown repo
//! or on an enrolled repo whose ledger.db was lost created an empty ledger and reported `ok: true`,
//! so total data loss passed an integrity check.
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_realitytrace"))
}

fn sxr(xdg: &Path, args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .env("XDG_DATA_HOME", xdg.join("data"))
        .env("XDG_STATE_HOME", xdg.join("state"))
        .env_remove("SXR_DAEMON_SOCKET")
        .output()
        .expect("run realitytrace")
}

fn git_repo(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("README"), "fixture\n").unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "init",
        ],
    ] {
        assert!(
            Command::new("git")
                .args(&args)
                .current_dir(dir)
                .status()
                .unwrap()
                .success()
        );
    }
}

const READ_ONLY: [&str; 3] = ["verify", "history", "export"];

#[test]
fn unknown_repo_is_refused_and_nothing_is_created() {
    let tmp = tempfile::tempdir().unwrap();
    for cmd in READ_ONLY {
        let out = sxr(
            tmp.path(),
            &["ledger", cmd, "--repo", "01NOTENROLLED", "--json"],
        );
        assert!(
            !out.status.success(),
            "ledger {cmd} on an unknown repo must fail"
        );
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("not enrolled"),
            "ledger {cmd}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    assert!(
        !tmp.path().join("data/sxr/repos/01NOTENROLLED").exists(),
        "a read-only command created a repo dir"
    );
}

#[test]
fn lost_ledger_is_refused_not_verified_as_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    git_repo(&repo);
    let enrolled = sxr(
        tmp.path(),
        &["repo", "enroll", repo.to_str().unwrap(), "--json"],
    );
    assert!(
        enrolled.status.success(),
        "{}",
        String::from_utf8_lossy(&enrolled.stderr)
    );
    let repo_id = serde_json::from_slice::<Value>(&enrolled.stdout).unwrap()["repo_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Control: the intact ledger verifies and has entries.
    let ok = sxr(
        tmp.path(),
        &["ledger", "verify", "--repo", &repo_id, "--json"],
    );
    assert!(ok.status.success());
    let report: Value = serde_json::from_slice(&ok.stdout).unwrap();
    assert_eq!(report["ok"], true);
    assert!(report["verified_count"].as_u64().unwrap() >= 1, "{report}");

    // Lose the ledger (database and its WAL/SHM side files).
    let db = tmp
        .path()
        .join("data/sxr/repos")
        .join(&repo_id)
        .join("ledger.db");
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", db.display()));
    }
    for cmd in READ_ONLY {
        let out = sxr(tmp.path(), &["ledger", cmd, "--repo", &repo_id, "--json"]);
        assert!(
            !out.status.success(),
            "ledger {cmd} must fail on a lost ledger, got {}",
            String::from_utf8_lossy(&out.stdout)
        );
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("not found"),
            "ledger {cmd}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    assert!(!db.exists(), "a read-only command recreated the ledger");
}

//! `verify run` runs exactly the verifier the plan names. `command@1` runs the declared argv, cwd and
//! launcher, records a digest of what it verified, and evidence derives from that verifier's profile.
//! An unregistered verifier id is refused and records nothing.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

struct Home {
    _dir: tempfile::TempDir,
    data: PathBuf,
    state: PathBuf,
    repo: PathBuf,
    repo_id: String,
}

impl Home {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let state = dir.path().join("state");
        let repo = dir.path().join("work");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(
            repo.join("src/greet.ts"),
            "export const greet = () => 'Hello';\n",
        )
        .unwrap();
        std::fs::write(repo.join("README"), "work\n").unwrap();
        for args in [
            &["init", "-q"][..],
            &["add", "-A"],
            &[
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
                    .args(args)
                    .current_dir(&repo)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let mut h = Home {
            _dir: dir,
            data,
            state,
            repo,
            repo_id: String::new(),
        };
        let enrolled = h.ok(&["repo", "enroll", h.repo.to_str().unwrap(), "--json"]);
        h.repo_id = enrolled["repo_id"].as_str().unwrap().to_string();
        let q = h.data.join("q.json");
        std::fs::write(&q, r#"{"question_id":"q1","question_form":"is_true","question_type":"verification","target_entities":[],"expected_answer_shape":"boolean"}"#).unwrap();
        let t = h.data.join("t.json");
        std::fs::write(
            &t,
            r#"{"threshold_id":"t1","minimum_relevance":"medium","minimum_reliability":"medium"}"#,
        )
        .unwrap();
        let rid = h.repo_id.clone();
        h.ok(&[
            "question",
            "declare",
            "--repo",
            &rid,
            "--file",
            q.to_str().unwrap(),
            "--json",
        ]);
        h.ok(&[
            "threshold",
            "declare",
            "--repo",
            &rid,
            "--question",
            "q1",
            "--file",
            t.to_str().unwrap(),
            "--json",
        ]);
        h
    }

    fn cmd(&self, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_realitytrace"))
            .args(args)
            .env("XDG_DATA_HOME", &self.data)
            .env("XDG_STATE_HOME", &self.state)
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> Value {
        let o = self.cmd(args);
        assert!(
            o.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    }

    fn begin(&self) -> String {
        let rid = self.repo_id.clone();
        self.ok(&[
            "run",
            "begin",
            "--repo",
            &rid,
            "--snapshot",
            "sha256:dummy",
            "--question",
            "q1",
            "--threshold",
            "t1",
            "--json",
        ])["run_id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn ledger(&self) -> Vec<Value> {
        let rid = self.repo_id.clone();
        serde_json::from_value(self.ok(&["ledger", "export", "--repo", &rid, "--json"])).unwrap()
    }
}

/// The documented digest, recomputed here without RealityTrace's code.
fn independent_digest(root: &Path) -> String {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            let p = e.path();
            let rel = p.strip_prefix(root).unwrap().to_str().unwrap().to_string();
            let ft = e.file_type().unwrap();
            if ft.is_dir() {
                if e.file_name() != ".git" {
                    walk(root, &p, out);
                }
            } else if ft.is_file() {
                out.push(format!(
                    "{rel}\0f\0{}\n",
                    hex::encode(Sha256::digest(std::fs::read(&p).unwrap()))
                ));
            }
        }
    }
    let mut lines = Vec::new();
    walk(root, root, &mut lines);
    lines.sort();
    let mut h = Sha256::new();
    for l in &lines {
        h.update(l.as_bytes());
    }
    format!("sha256:{}", hex::encode(h.finalize()))
}

#[test]
fn command_verifier_runs_the_declared_argv_under_the_launcher_and_records_what_it_verified() {
    let h = Home::new();
    let marker = h.data.join("marker");
    let run = h.begin();
    let argv = r#"["sh","-c","pwd > \"$SXR_MARK\"; echo verified"]"#;
    let launcher = format!(r#"["env","SXR_MARK={}"]"#, marker.display());
    let plan = h.ok(&[
        "plan",
        "create",
        "--run",
        &run,
        "--verifier",
        "command",
        "--target",
        "concept:greet",
        "--argv-json",
        argv,
        "--cwd",
        "src",
        "--launcher-json",
        &launcher,
        "--json",
    ]);
    assert_eq!(plan["plan"]["steps"][0]["invocation"]["argv"][0], "sh");
    assert_eq!(plan["plan"]["steps"][0]["invocation"]["launcher"][0], "env");

    let expected_digest = independent_digest(&h.repo);
    let obs = h.ok(&["verify", "run", "--run", &run, "--json"])["observations"][0].clone();
    assert_eq!(obs["verifier_id"], "command");
    assert_eq!(obs["exit_code"], 0);
    assert_eq!(obs["target_digest"], expected_digest.as_str());
    // The declared argv ran, under the launcher (the cleared environment has no SXR_MARK otherwise), in
    // the declared cwd.
    let ran_in =
        std::fs::read_to_string(&marker).expect("the launcher-provided marker path was written");
    assert_eq!(
        Path::new(ran_in.trim()).canonicalize().unwrap(),
        h.repo.join("src").canonicalize().unwrap()
    );

    let obs_id = obs["observation_id"].as_str().unwrap();
    let ev = h.ok(&[
        "evidence",
        "create",
        "--run",
        &run,
        "--observation",
        obs_id,
        "--derive",
        "--json",
    ]);
    let rule = ev["derivations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["field"] == "direction")
        .unwrap()["rule_ref"]
        .clone();
    assert_eq!(rule, "verifier-profile://command@1#exit_code_to_direction");

    let rows = h.ledger();
    let recorded = rows
        .iter()
        .find(|r| r["record_kind"] == "observation.recorded")
        .unwrap();
    let inline = &recorded["payload"]["representations"][0]["inline_content"];
    assert_eq!(inline["verifier_id"], "command");
    assert_eq!(inline["target_digest"], expected_digest.as_str());
    assert_eq!(
        inline["invocation"]["launcher"][1],
        format!("SXR_MARK={}", marker.display())
    );
    let evidence = rows
        .iter()
        .find(|r| r["record_kind"] == "evidence.created")
        .unwrap();
    assert_eq!(evidence["payload"]["evidence"][0]["direction"], "supports");
}

#[test]
fn a_failing_declared_command_contradicts() {
    let h = Home::new();
    let run = h.begin();
    h.ok(&[
        "plan",
        "create",
        "--run",
        &run,
        "--verifier",
        "command",
        "--target",
        "concept:greet",
        "--argv-json",
        r#"["sh","-c","exit 3"]"#,
        "--json",
    ]);
    let obs = h.ok(&["verify", "run", "--run", &run, "--json"])["observations"][0].clone();
    assert_eq!(obs["exit_code"], 3);
    let ev = h.ok(&[
        "evidence",
        "create",
        "--run",
        &run,
        "--observation",
        obs["observation_id"].as_str().unwrap(),
        "--derive",
        "--json",
    ]);
    assert!(ev.to_string().contains("command@1"));
    let rows = h.ledger();
    let evidence = rows
        .iter()
        .find(|r| r["record_kind"] == "evidence.created")
        .unwrap();
    assert_eq!(
        evidence["payload"]["evidence"][0]["direction"],
        "contradicts"
    );
}

#[test]
fn verify_run_refuses_a_verifier_it_cannot_run_and_records_nothing() {
    let h = Home::new();
    let run = h.begin();
    // A label for `ingest` (e.g. pytest) is still a valid plan...
    h.ok(&[
        "plan",
        "create",
        "--run",
        &run,
        "--verifier",
        "pytest",
        "--target",
        "concept:greet",
        "--json",
    ]);
    // ...but `verify run` must not run cargo-test (or anything else) in its place.
    let o = h.cmd(&["verify", "run", "--run", &run, "--json"]);
    assert!(!o.status.success());
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("cannot be run by `verify run`"),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(
        h.ledger()
            .iter()
            .all(|r| r["record_kind"] != "observation.recorded")
    );
}

#[test]
fn command_plans_are_validated() {
    let h = Home::new();
    let run = h.begin();
    let cases: [(&[&str], &str); 5] = [
        (&["--verifier", "command"], "requires --argv-json"),
        (
            &["--verifier", "command", "--argv-json", "[]"],
            "must name a program",
        ),
        (
            &[
                "--verifier",
                "command",
                "--argv-json",
                r#"["true"]"#,
                "--cwd",
                "../elsewhere",
            ],
            "relative path inside",
        ),
        (
            &[
                "--verifier",
                "command",
                "--argv-json",
                r#"["true"]"#,
                "--cwd",
                "/tmp",
            ],
            "relative path inside",
        ),
        (
            &["--verifier", "cargo-test", "--argv-json", r#"["true"]"#],
            "apply only to --verifier command",
        ),
    ];
    for (extra, why) in cases {
        let mut args = vec![
            "plan",
            "create",
            "--run",
            run.as_str(),
            "--target",
            "concept:greet",
            "--json",
        ];
        args.extend_from_slice(extra);
        let o = h.cmd(&args);
        assert!(!o.status.success(), "{args:?} should be refused");
        assert!(
            String::from_utf8_lossy(&o.stderr).contains(why),
            "{args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
    }
}

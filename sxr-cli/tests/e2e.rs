//! Minimal e2e smoke test: verify the sxr binary can enroll, doctor, and list.
//! The full lifecycle is exercised by the proof script `/tmp/sxr-proof2.sh`.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sxr"))
}

fn run(args: &[&str], xdg_data: &str, xdg_state: &str) -> std::process::Output {
    let mut cmd = Command::new(bin());
    cmd.args(args)
        .env("XDG_DATA_HOME", xdg_data)
        .env("XDG_STATE_HOME", xdg_state);
    cmd.output().expect("spawn sxr")
}

#[test]
fn sxr_binary_is_present() {
    assert!(bin().is_file(), "sxr binary at {:?}", bin());
}

#[test]
fn doctor_returns_valid_json() {
    let d = run(
        &["doctor", "--json"],
        "/tmp/sxr-test-data",
        "/tmp/sxr-test-state",
    );
    assert!(
        d.status.success(),
        "doctor: {:?}",
        String::from_utf8_lossy(&d.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&d.stdout).unwrap();
    assert_eq!(v["sxr_output_version"], "1");
    assert!(v["filesystem_class"].as_str().is_some());
}

#[test]
fn repo_list_after_enroll() {
    let data = format!("/tmp/sxr-test-{}", std::process::id());
    let state = format!("{}-st", data);
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&state).unwrap();
    let repo = PathBuf::from(&data).join("target");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(
        repo.join("Cargo.toml"),
        "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
    )
    .unwrap();
    std::fs::write(repo.join("src/lib.rs"), "pub fn a() -> u32 { 42 }\n").unwrap();
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo)
        .status();
    let _ = Command::new("git")
        .args(["add", "-A"])
        .current_dir(&repo)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-qm", "init"])
        .current_dir(&repo)
        .status();

    let r = run(
        &["repo", "enroll", repo.to_str().unwrap(), "--json"],
        &data,
        &state,
    );
    assert!(
        r.status.success(),
        "enroll: {:?}",
        String::from_utf8_lossy(&r.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&r.stdout).unwrap();
    assert!(v["repo_id"].as_str().unwrap().len() > 10);

    let l = run(&["repo", "list", "--json"], &data, &state);
    let lv: serde_json::Value = serde_json::from_slice(&l.stdout).unwrap();
    assert_eq!(lv["repos"].as_array().unwrap().len(), 1);
}

#[test]
fn contract_import_rejects_non_work_request() {
    let data = format!("/tmp/sxr-test-ci-{}", std::process::id());
    let state = format!("{}-st", data);
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&state).unwrap();
    let repo = PathBuf::from(&data).join("target");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(
        repo.join("Cargo.toml"),
        "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
    )
    .unwrap();
    std::fs::write(repo.join("src/lib.rs"), "pub fn a() -> u32 { 42 }\n").unwrap();
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo)
        .status();
    let _ = Command::new("git")
        .args(["add", "-A"])
        .current_dir(&repo)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-qm", "init"])
        .current_dir(&repo)
        .status();

    let r = run(
        &["repo", "enroll", repo.to_str().unwrap(), "--json"],
        &data,
        &state,
    );
    assert!(r.status.success());
    let rid = serde_json::from_slice::<serde_json::Value>(&r.stdout).unwrap()["repo_id"]
        .as_str()
        .unwrap()
        .to_string();

    // A non-work_request envelope (semantic_snapshot) must be rejected by
    // `contract import` before any snapshot lookup.
    let bad = serde_json::json!({
        "envelope_id": "env-1",
        "cep_version": "0.1.0",
        "envelope_version": "1.0.0",
        "envelope_kind": "semantic_snapshot",
        "created_at": "2026-08-07T00:00:00Z",
        "created_by": "test",
        "scope": {"s": "x"},
        "boundary_record": {"scope": "x", "included_sections": [], "excluded_sections": [],
                            "known_omissions": [], "unknowns": [], "redactions": [],
                            "compression_notes": [], "out_of_scope_entities": [], "limitations": []},
        "completeness_status": "complete_for_declared_state",
        "provenance_refs": ["source-set:sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
        "omission_status": "known_omissions_recorded",
        "lineage_refs": [],
        "validation_status": "validated"
    });
    let bad_path = PathBuf::from(&data).join("bad-contract.json");
    std::fs::write(&bad_path, serde_json::to_vec(&bad).unwrap()).unwrap();
    let c = run(
        &[
            "contract",
            "import",
            "--repo",
            &rid,
            "--file",
            bad_path.to_str().unwrap(),
            "--json",
        ],
        &data,
        &state,
    );
    assert!(
        !c.status.success(),
        "non-work_request contract must be rejected"
    );
}

/// `evidence create --evidence-type` must flow into the ledger record and the
/// derivation log (§26: evidence type is part of the evidence construction).
#[test]
fn evidence_create_custom_evidence_type() {
    let data = format!("/tmp/sxr-test-ev-{}", std::process::id());
    let state = format!("{}-st", data);
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&state).unwrap();
    let repo = PathBuf::from(&data).join("target");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/lib.rs"), "pub fn a() -> u32 { 42 }\n").unwrap();
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo)
        .status();
    let _ = Command::new("git")
        .args(["add", "-A"])
        .current_dir(&repo)
        .status();
    let _ = Command::new("git")
        .args([
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "init",
        ])
        .current_dir(&repo)
        .status();

    let r = run(
        &["repo", "enroll", repo.to_str().unwrap(), "--json"],
        &data,
        &state,
    );
    assert!(r.status.success());
    let rid = serde_json::from_slice::<serde_json::Value>(&r.stdout).unwrap()["repo_id"]
        .as_str()
        .unwrap()
        .to_string();

    let qfile = PathBuf::from(&data).join("q.json");
    std::fs::write(
        &qfile,
        r#"{"question_id":"q1","question_form":"is_true","question_type":"verification","target_entities":[],"expected_answer_shape":"boolean"}"#,
    )
    .unwrap();
    assert!(
        run(
            &[
                "question",
                "declare",
                "--repo",
                &rid,
                "--file",
                qfile.to_str().unwrap(),
                "--json"
            ],
            &data,
            &state,
        )
        .status
        .success()
    );

    let tfile = PathBuf::from(&data).join("t.json");
    std::fs::write(
        &tfile,
        r#"{"threshold_id":"t1","minimum_relevance":"medium","minimum_reliability":"medium"}"#,
    )
    .unwrap();
    assert!(
        run(
            &[
                "threshold",
                "declare",
                "--repo",
                &rid,
                "--question",
                "q1",
                "--file",
                tfile.to_str().unwrap(),
                "--json"
            ],
            &data,
            &state,
        )
        .status
        .success()
    );

    let rb = run(
        &[
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
        ],
        &data,
        &state,
    );
    assert!(rb.status.success());
    let run_id = serde_json::from_slice::<serde_json::Value>(&rb.stdout).unwrap()["run_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        run(
            &[
                "plan",
                "create",
                "--run",
                &run_id,
                "--verifier",
                "pytest",
                "--target",
                "concept:x",
                "--json"
            ],
            &data,
            &state,
        )
        .status
        .success()
    );

    let mut ingest = Command::new(bin());
    ingest
        .args([
            "ingest",
            "--run",
            &run_id,
            "--verifier",
            "pytest",
            "--stdin-format",
            "pytest-progress",
        ])
        .env("XDG_DATA_HOME", &data)
        .env("XDG_STATE_HOME", &state)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped());
    let mut child = ingest.spawn().unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(b"FAILED\n").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let obs_id =
        serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["observation_id"]
            .as_str()
            .unwrap()
            .to_string();

    let ec = run(
        &[
            "evidence",
            "create",
            "--run",
            &run_id,
            "--observation",
            &obs_id,
            "--derive",
            "--evidence-type",
            "manual_observation",
            "--json",
        ],
        &data,
        &state,
    );
    assert!(ec.status.success());
    let ev: serde_json::Value = serde_json::from_slice(&ec.stdout).unwrap();
    // The derivation log must carry the caller-supplied type.
    let deriv = ev["derivations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["field"] == "evidence_type")
        .expect("evidence_type derivation");
    assert_eq!(deriv["inputs"]["evidence_types"][0], "manual_observation");

    // And the ledger record must persist it.
    let l = run(&["ledger", "export", "--repo", &rid], &data, &state);
    assert!(l.status.success());
    let rows: serde_json::Value = serde_json::from_slice(&l.stdout).unwrap();
    let ev_row = rows
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|r| r["record_kind"] == "evidence.created")
        .expect("evidence.created record");
    assert_eq!(
        ev_row["payload"]["evidence"][0]["evidence_type"],
        "manual_observation"
    );
}

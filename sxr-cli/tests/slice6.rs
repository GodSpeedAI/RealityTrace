//! Slice 6 tests (§24 Slice 6): optional daemon + durability parity.
//!
//! Covers the Slice-6 acceptance surface:
//! - AC-DAEMON-001 shape: the daemon queues the SAME validated request the
//!   CLI would commit; determinism comes from the injected seams in
//!   `determinism.rs` (proof/slice6.sh runs the full parity diff).
//! - AC-DAEMON-003 / F-19b: consumers block (exit 3) on referenced records
//!   that are not committed.
//! - AC-DAEMON-004: kill-after-receipt then `sxr queue recover` commits the
//!   fsynced segment; replay is a no-op by idempotency key.
//! - §16.4: corrupt segments are frozen, retained byte-for-byte, reported by
//!   doctor (exit 2), and recovery refuses with exit 3.
//! - F-15: `--async-mode` with the daemon unavailable falls back to a direct
//!   commit with `"daemon": false` — zero semantic difference.
//! - A second daemon on the same socket is rejected; the socket path honors
//!   `SXR_DAEMON_SOCKET`.

use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sxr"))
}

/// Run sxr with an isolated XDG root (and optional socket env).
fn run(args: &[&str], data: &Path, state: &Path, socket: Option<&str>) -> Output {
    let mut cmd = Command::new(bin());
    cmd.args(args)
        .env("XDG_DATA_HOME", data)
        .env("XDG_STATE_HOME", state);
    if let Some(s) = socket {
        cmd.env("SXR_DAEMON_SOCKET", s);
    }
    cmd.output().expect("spawn sxr")
}

fn stdout_json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout not json ({e}): {:?}\nstderr: {:?}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// A verifier stream that parses as cargo-json (1 passed).
const OK_STREAM: &[u8] = b"running 1 test\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n";

/// Spawn the daemon against an isolated root; caller must kill it.
fn spawn_daemon(data: &Path, state: &Path, socket: &str, drain_delay_ms: u64) -> Child {
    Command::new(bin())
        .args(["daemon", "--drain-delay-ms", &drain_delay_ms.to_string()])
        .env("XDG_DATA_HOME", data)
        .env("XDG_STATE_HOME", state)
        .env("SXR_DAEMON_SOCKET", socket)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn sxr daemon")
}

/// Wait until the daemon accepts connections (socket bind + READY).
fn wait_for_socket(socket: &str) {
    for _ in 0..100 {
        if std::os::unix::net::UnixStream::connect(socket).is_ok() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("daemon socket {socket} never became ready");
}

/// Kill hard (SIGKILL) — simulates the kill-after-receipt window.
fn kill_hard(child: &mut Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .arg("-9")
            .arg(child.id().to_string())
            .status();
    }
    let _ = child.wait();
}

/// Enroll a minimal cargo repo and return its repo_id.
fn enroll_repo(data: &Path, state: &Path) -> String {
    let root = data.join("repo");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"sxr-s6-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn a() -> u32 { 42 }\n#[cfg(test)]\nmod t { #[test] fn ok() { assert_eq!(2+2, 4); } }\n",
    )
    .unwrap();
    std::fs::write(root.join(".gitignore"), "target/\nCargo.lock\n").unwrap();
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&root)
        .status();
    let _ = Command::new("git")
        .args(["add", "-A"])
        .current_dir(&root)
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
        .current_dir(&root)
        .status();
    let v = stdout_json(&run(
        &["repo", "enroll", root.to_str().unwrap()],
        data,
        state,
        None,
    ));
    v["repo_id"].as_str().expect("repo_id").to_string()
}

/// Full §15.1 fixture: repo + snapshot + question + threshold + run + plan.
/// Returns (repo_id, run_id).
fn full_fixture(data: &Path, state: &Path) -> (String, String) {
    let rid = enroll_repo(data, state);
    let _ = run(
        &[
            "semantic",
            "configure",
            "--repo",
            &rid,
            "--entry",
            "src/lib.rs",
        ],
        data,
        state,
        None,
    );
    let snap = stdout_json(&run(
        &["snapshot", "create", "--repo", &rid],
        data,
        state,
        None,
    ));
    let snap_id = snap["snapshot_id"].as_str().expect("snapshot_id");
    let q = json!({"question_id": "q-s6", "question_form": "is it realized?",
                   "question_type": "verification", "target_entities": [],
                   "expected_answer_shape": "boolean"});
    let qfile = data.join("q.json");
    std::fs::write(&qfile, serde_json::to_vec(&q).unwrap()).unwrap();
    let qid = stdout_json(&run(
        &[
            "question",
            "declare",
            "--repo",
            &rid,
            "--file",
            qfile.to_str().unwrap(),
        ],
        data,
        state,
        None,
    ))["question_id"]
        .as_str()
        .expect("question_id")
        .to_string();
    let t = json!({"threshold_id": "t-s6", "minimum_relevance": "medium", "minimum_reliability": "medium"});
    let tfile = data.join("t.json");
    std::fs::write(&tfile, serde_json::to_vec(&t).unwrap()).unwrap();
    let tid = stdout_json(&run(
        &[
            "threshold",
            "declare",
            "--repo",
            &rid,
            "--question",
            &qid,
            "--file",
            tfile.to_str().unwrap(),
        ],
        data,
        state,
        None,
    ))["threshold_id"]
        .as_str()
        .expect("threshold_id")
        .to_string();
    let run_id = stdout_json(&run(
        &[
            "run",
            "begin",
            "--repo",
            &rid,
            "--snapshot",
            snap_id,
            "--question",
            &qid,
            "--threshold",
            &tid,
        ],
        data,
        state,
        None,
    ))["run_id"]
        .as_str()
        .expect("run_id")
        .to_string();
    let _ = run(
        &[
            "plan",
            "create",
            "--run",
            &run_id,
            "--verifier",
            "cargo-test",
            "--target",
            &tid,
        ],
        data,
        state,
        None,
    );
    (rid, run_id)
}

fn ingest_async(data: &Path, state: &Path, socket: Option<&str>, run_id: &str) -> Value {
    let mut cmd = Command::new(bin());
    cmd.args([
        "ingest",
        "--run",
        run_id,
        "--verifier",
        "cargo-test",
        "--stdin-format",
        "cargo-json",
        "--async-mode",
    ])
    .env("XDG_DATA_HOME", data)
    .env("XDG_STATE_HOME", state)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    if let Some(s) = socket {
        cmd.env("SXR_DAEMON_SOCKET", s);
    }
    let mut child = cmd.spawn().expect("spawn ingest");
    child.stdin.as_mut().unwrap().write_all(OK_STREAM).unwrap();
    let out = child.wait_with_output().expect("wait ingest");
    let v: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "ingest stdout not json ({e}): {:?}\nstderr: {:?}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    });
    assert!(
        out.status.success(),
        "ingest --async-mode must exit 0 (exit {:?}): {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    v
}

/// In-process: the request the CLI submits to the daemon carries the same
/// envelope, kind, subject refs and idempotency key the direct path commits
/// (§16.2/§16.3 parity), and validates against the ledger-visible request.
#[test]
fn record_request_shape_and_validation_roundtrip() {
    let built = sxr_cli::commands::IngestEnvelope {
        envelope: json!({"envelope_kind": "execution_trace",
            "representations": [{"representation_id": "rep:r",
            "representation_kind": "atomic", "representation_type": "observation",
            "inline_content": {"observation_id": "obs-1"}}]}),
        subject_refs: vec![
            "observation:obs-1".into(),
            "run:run-1".into(),
            "target:t-1".into(),
        ],
        kind: sxr_core::record_kinds::RecordKind::ObservationRecorded,
        idempotency_key: "sha256:abc".into(),
        observation_id: "obs-1".into(),
        parsed: Value::Null,
    };
    let request = sxr_cli::daemon::record_request("repo-1", &built);
    assert_eq!(request["request_kind"], "sxr.record/v1");
    assert_eq!(request["repo_id"], "repo-1");
    assert_eq!(request["record_kind"], "observation.recorded");
    assert_eq!(request["idempotency_key"], "sha256:abc");
    assert_eq!(
        request["subject_refs"],
        json!(["observation:obs-1", "run:run-1", "target:t-1"])
    );
    assert_eq!(
        request["envelope"]["representations"][0]["representation_id"],
        "rep:r"
    );

    let back = sxr_cli::daemon::validate_record_request(&request).unwrap();
    assert_eq!(
        back.kind,
        sxr_core::record_kinds::RecordKind::ObservationRecorded
    );
    assert_eq!(back.idempotency_key, "sha256:abc");
    assert_eq!(
        back.subject_refs,
        vec!["observation:obs-1", "run:run-1", "target:t-1"]
    );
}

/// In-process: the daemon rejects unknown kinds, malformed requests, and
/// envelopes that fail record-kind conformance — the CLI's fallback or spool
/// never mistakes a rejection for an acceptance (§11.1a).
#[test]
fn record_request_validation_rejects_bad_requests() {
    let built = sxr_cli::commands::IngestEnvelope {
        envelope: json!({"envelope_kind": "execution_trace",
            "representations": [{"representation_id": "rep:r",
            "representation_kind": "atomic", "representation_type": "observation",
            "inline_content": {"observation_id": "obs-1"}}]}),
        subject_refs: vec![
            "observation:obs-1".into(),
            "run:run-1".into(),
            "target:t-1".into(),
        ],
        kind: sxr_core::record_kinds::RecordKind::ObservationRecorded,
        idempotency_key: "sha256:abc".into(),
        observation_id: "obs-1".into(),
        parsed: Value::Null,
    };
    let request = sxr_cli::daemon::record_request("repo-1", &built);

    let mut unknown = request.clone();
    unknown["record_kind"] = json!("made.up.kind");
    assert!(
        sxr_cli::daemon::validate_record_request(&unknown).is_err(),
        "unknown record kind must be rejected"
    );

    let mut missing = request.clone();
    missing.as_object_mut().unwrap().remove("idempotency_key");
    assert!(sxr_cli::daemon::validate_record_request(&missing).is_err());

    let mut wrong_kind = request.clone();
    wrong_kind["record_kind"] = json!("repo.enrolled");
    assert!(
        sxr_cli::daemon::validate_record_request(&wrong_kind).is_err(),
        "non-conforming envelope must be rejected (repo.enrolled demands an enrollment shape)"
    );
}

/// AC-DAEMON-004 shape (happy path): a received+fsynced segment survives a
/// hard daemon kill and commits exactly once via `sxr queue recover`; the
/// second recover is a no-op; `await` then reports committed.
#[test]
fn kill_after_receipt_then_queue_recover_commits_once() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let state = root.path().join("state");
    let socket = root.path().join("sxr.sock").to_str().unwrap().to_string();
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&state).unwrap();

    let (rid, run_id) = full_fixture(&data, &state);

    let mut daemon = spawn_daemon(&data, &state, &socket, 60000);
    wait_for_socket(&socket);
    let v = ingest_async(&data, &state, Some(&socket), &run_id);
    assert_eq!(v["daemon"], true);
    assert_eq!(v["durability"], "queued_for_commit");
    assert!(v["receipt"]["queue_position"].as_str().unwrap().len() > 10);

    // The segment exists on disk before the kill — durability was achieved at
    // receipt time, not at drain time.
    let queue_dir = state.join("sxr").join("daemon-queue").join(&rid);
    assert!(
        std::fs::read_dir(&queue_dir).unwrap().count() >= 1,
        "segment present"
    );

    kill_hard(&mut daemon);

    let r1 = stdout_json(&run(&["queue", "recover"], &data, &state, None));
    assert_eq!(r1["recovered"], 1, "one segment committed by recovery");
    let r2 = stdout_json(&run(&["queue", "recover"], &data, &state, None));
    assert_eq!(r2["recovered"], 0, "replay by idempotency key is a no-op");

    let a = stdout_json(&run(
        &["await", "--repo", &rid, "--all", "--timeout-secs", "2"],
        &data,
        &state,
        None,
    ));
    assert_eq!(a["state"], "committed");
}

/// F-15: with no daemon listening, `--async-mode` transparently falls back to
/// the direct writer-lock commit — `"daemon": false`, `committed`, exit 0,
/// identical durability to a non-async ingest.
#[test]
fn async_falls_back_to_direct_commit_without_daemon() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let state = root.path().join("state");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&state).unwrap();
    let (_, run_id) = full_fixture(&data, &state);

    let v = ingest_async(&data, &state, None, &run_id);
    assert_eq!(v["daemon"], false, "transparent fallback is reported");
    assert_eq!(v["durability"], "committed");
    assert!(v["ledger_position"]["append_ordinal"].as_u64().is_some());
}

/// §16.4 corruption rule: a torn/foreign write after the receipt is frozen,
/// retained byte-for-byte, named by doctor (exit 2), and `queue recover`
/// refuses (exit 3) — the queue is frozen, never silently dropped.
#[test]
fn corrupt_segment_is_frozen_and_actionable() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let state = root.path().join("state");
    let socket = root.path().join("sxr.sock").to_str().unwrap().to_string();
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&state).unwrap();
    let (rid, run_id) = full_fixture(&data, &state);

    let mut daemon = spawn_daemon(&data, &state, &socket, 60000);
    wait_for_socket(&socket);
    let v = ingest_async(&data, &state, Some(&socket), &run_id);
    assert_eq!(v["durability"], "queued_for_commit");

    // Corrupt the segment AFTER the receipt: the request line hash no longer
    // matches (hash of the request bytes changed).
    let queue_dir = state.join("sxr").join("daemon-queue").join(&rid);
    let seg = std::fs::read_dir(&queue_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "queue"))
        .expect("one .queue segment");
    let mut content = std::fs::read(&seg).unwrap();
    content.extend_from_slice(b"torn write\n");
    std::fs::write(&seg, content).unwrap();

    kill_hard(&mut daemon);

    let rec = run(&["queue", "recover"], &data, &state, None);
    assert_eq!(
        rec.status.code(),
        Some(3),
        "recovery refuses: {}",
        String::from_utf8_lossy(&rec.stderr)
    );
    let err = String::from_utf8_lossy(&rec.stderr);
    assert!(err.contains("corrupt"), "names corruption: {err}");
    assert!(err.contains(&rid), "names the repo: {err}");
    assert!(seg.exists(), "corrupt segment retained byte-for-byte");

    let d = run(&["doctor"], &data, &state, None);
    assert_eq!(d.status.code(), Some(2), "doctor exits 2 on corruption");
    let dv: Value = serde_json::from_slice(&d.stdout).unwrap();
    assert!(
        dv["corrupt_queue_segments"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p.as_str().is_some()),
        "doctor reports the corrupt segment: {dv}"
    );

    let a = run(
        &["await", "--repo", &rid, "--all", "--timeout-secs", "1"],
        &data,
        &state,
        None,
    );
    assert_eq!(a.status.code(), Some(3), "await exits 3 while frozen");
}

/// The daemon is a singleton per socket: a second instance is rejected rather
/// than silently stealing the queue (writer discipline, §11.1).
#[test]
fn second_daemon_on_same_socket_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let state = root.path().join("state");
    let socket = root.path().join("sxr.sock").to_str().unwrap().to_string();
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&state).unwrap();

    let mut first = spawn_daemon(&data, &state, &socket, 60000);
    wait_for_socket(&socket);
    let second = run(&["daemon"], &data, &state, Some(&socket));
    assert!(
        !second.status.success(),
        "second daemon must be rejected: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let err = String::from_utf8_lossy(&second.stderr);
    assert!(err.contains("already serving"), "explains: {err}");
    kill_hard(&mut first);
}

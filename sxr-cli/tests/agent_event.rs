//! `ingest --stdin-format agent-event-v1`: normalized coding-agent events
//! (ADR-002 `agent-event/v1`, NDJSON on stdin) become observations in the
//! hash-chained ledger without changing any existing ingest behaviour.
//!
//! Choices under test:
//! - the batch is all-or-nothing: one invalid line (reported by number)
//!   appends nothing, mirroring spool drain's reject of an unparseable stream;
//! - dedup is per `event_id`: N distinct events are N observations, a
//!   re-delivery is a no-op, and an edited re-delivery never overwrites;
//! - builder events never share a run with non-builder events;
//! - `exit_code` rides the observation so evidence direction works.
#![cfg(unix)]

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const VERIFIER: &str = "agent-tap/claude";
const OK_STREAM: &[u8] = b"running 1 test\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n";

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_realitytrace"))
}

/// One isolated XDG home (optionally with the determinism seams on).
struct Home {
    _root: tempfile::TempDir,
    data: PathBuf,
    state: PathBuf,
    ids: Option<PathBuf>,
}

impl Home {
    fn new(deterministic: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let (data, state) = (root.path().join("data"), root.path().join("state"));
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&state).unwrap();
        let ids = deterministic.then(|| {
            let p = root.path().join("ids.txt");
            let body: String = (0..400)
                .map(|i| format!("01ARZ3NDEKTSV4RRFFQ69G{i:04}\n"))
                .collect();
            std::fs::write(&p, body).unwrap();
            p
        });
        Home {
            _root: root,
            data,
            state,
            ids,
        }
    }

    fn df_bin(&self) -> PathBuf {
        let script_path = self.data.join("mock-domainforge.sh");
        if !script_path.exists() {
            let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../sxr-df/tests/fixtures/env-valid-basic.json");
            let script = format!(
                "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo \"domainforge {}\"; exit 0; fi\ncat \"{}\"\nexit 0\n",
                sxr_df::PINNED_DOMAINFORGE_VERSION,
                fixture.canonicalize().unwrap().display()
            );
            std::fs::write(&script_path, script).unwrap();
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms).unwrap();
        }
        script_path
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let df = self.df_bin();
        let mut c = Command::new(bin());
        c.args(args)
            .env("XDG_DATA_HOME", &self.data)
            .env("XDG_STATE_HOME", &self.state)
            .env("REALITYTRACE_DOMAINFORGE_BIN", &df)
            .env("SXR_DOMAINFORGE_BIN", &df);
        if let Some(ids) = &self.ids {
            c.env("SXR_TEST_ID_STREAM", ids)
                .env("SXR_TEST_FIXED_CLOCK", "2026-01-02T03:04:05Z");
        }
        c
    }

    fn run(&self, args: &[&str]) -> Output {
        self.cmd(args).output().expect("spawn")
    }

    fn json(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).expect("stdout json")
    }

    /// `ingest` with `stdin`; returns (exit code, stdout json or Null, stderr).
    fn ingest(
        &self,
        run_id: &str,
        format: &str,
        stdin: &[u8],
        extra: &[&str],
    ) -> (i32, Value, String) {
        let verifier = if format == "agent-event-v1" {
            VERIFIER
        } else {
            "cargo-test"
        };
        let mut args = vec![
            "ingest",
            "--run",
            run_id,
            "--verifier",
            verifier,
            "--stdin-format",
            format,
        ];
        args.extend_from_slice(extra);
        let mut child = self
            .cmd(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let _ = child.stdin.as_mut().unwrap().write_all(stdin);
        drop(child.stdin.take());
        let out = child.wait_with_output().unwrap();
        (
            out.status.code().unwrap_or(-1),
            serde_json::from_slice(&out.stdout).unwrap_or(Value::Null),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    fn observations(&self, rid: &str) -> Vec<Value> {
        self.json(&["ledger", "export", "--repo", rid])
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["record_kind"] == "observation.recorded")
            .cloned()
            .collect()
    }
}

/// A minimal enrolled cargo repo; shared by the two replay homes.
fn make_repo(parent: &Path) -> PathBuf {
    let root = parent.join("repo");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"agent-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn a() -> u32 { 42 }\n").unwrap();
    std::fs::write(root.join(".gitignore"), "target/\nCargo.lock\n").unwrap();
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(&root)
            .status()
            .unwrap();
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&[
        "-c",
        "user.email=t@t",
        "-c",
        "user.name=t",
        "commit",
        "-qm",
        "init",
    ]);
    root
}

struct Fixture {
    rid: String,
    snap: String,
    qid: String,
    tid: String,
}

/// Enroll, snapshot, declare question + threshold.
fn declare(h: &Home, repo: &Path) -> Fixture {
    let rid = h.json(&["repo", "enroll", repo.to_str().unwrap()])["repo_id"]
        .as_str()
        .unwrap()
        .to_string();
    let _ = h.run(&[
        "semantic",
        "configure",
        "--repo",
        &rid,
        "--entry",
        "src/lib.rs",
    ]);
    let snap = h.json(&["snapshot", "create", "--repo", &rid])["snapshot_id"]
        .as_str()
        .unwrap()
        .to_string();
    let q = h.data.join("q.json");
    std::fs::write(
        &q,
        json!({"question_id": "q-agent", "question_form": "is it realized?",
               "question_type": "verification", "target_entities": [],
               "expected_answer_shape": "boolean"})
        .to_string(),
    )
    .unwrap();
    let qid = h.json(&[
        "question",
        "declare",
        "--repo",
        &rid,
        "--file",
        q.to_str().unwrap(),
    ])["question_id"]
        .as_str()
        .unwrap()
        .to_string();
    let t = h.data.join("t.json");
    std::fs::write(
        &t,
        json!({"threshold_id": "t-agent", "minimum_relevance": "medium", "minimum_reliability": "medium"})
            .to_string(),
    )
    .unwrap();
    let tid = h.json(&[
        "threshold",
        "declare",
        "--repo",
        &rid,
        "--question",
        &qid,
        "--file",
        t.to_str().unwrap(),
    ])["threshold_id"]
        .as_str()
        .unwrap()
        .to_string();
    Fixture {
        rid,
        snap,
        qid,
        tid,
    }
}

fn begin_run(h: &Home, f: &Fixture) -> String {
    let run_id = h.json(&[
        "run",
        "begin",
        "--repo",
        &f.rid,
        "--snapshot",
        &f.snap,
        "--question",
        &f.qid,
        "--threshold",
        &f.tid,
    ])["run_id"]
        .as_str()
        .unwrap()
        .to_string();
    h.json(&[
        "plan",
        "create",
        "--run",
        &run_id,
        "--verifier",
        "cargo-test",
        "--target",
        &f.tid,
    ]);
    run_id
}

/// Full fixture: (home, repo_id, run_id).
fn setup() -> (Home, String, String) {
    let h = Home::new(false);
    let repo = make_repo(&h.data);
    let f = declare(&h, &repo);
    let run_id = begin_run(&h, &f);
    (h, f.rid, run_id)
}

fn event(n: u32, kind: &str, exit_code: Value, role: &str) -> Value {
    json!({
        "schema": "agent-event/v1",
        "event_id": format!("{n:064x}"),
        "source": "hook",
        "runtime": "claude",
        "native_session_id": "sess-1",
        "native_event_ref": format!("{n}"),
        "kind": kind,
        "ts": "2026-10-29T10:00:00Z",
        "observed_at": "2026-10-29T10:00:01Z",
        "case_id": "case:run-42",
        "unit_id": "unknown",
        "te_session_id": "te-1",
        "role": role,
        "repo": "/work/repo",
        "worktree": null,
        "branch": "main",
        "tokens": {"input": 10, "output": 5, "cache_write": 0, "cache_read": 3},
        "token_quality": "EXACT",
        "owner": "native",
        "payload_ref": format!("file:/spool/claude.ndjson#{n}"),
        "exit_code": exit_code,
    })
}

fn ndjson(events: &[Value]) -> Vec<u8> {
    let mut out = Vec::new();
    for e in events {
        out.extend_from_slice(e.to_string().as_bytes());
        out.push(b'\n');
    }
    out
}

fn batch() -> Vec<Value> {
    vec![
        event(1, "session_start", Value::Null, "user"),
        event(2, "tool_post", json!(0), "user"),
        event(3, "tool_fail", json!(1), "user"),
    ]
}

#[test]
fn valid_batch_makes_one_observation_per_event() {
    let (h, rid, run_id) = setup();
    let (code, out, err) = h.ingest(&run_id, "agent-event-v1", &ndjson(&batch()), &[]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out["events"], 3);
    assert_eq!(out["new_observations"], 3);
    assert_eq!(out["already_committed"], false);
    let rows = h.observations(&rid);
    assert_eq!(rows.len(), 3);
    for r in &rows {
        let ae = &r["payload"]["extensions"]["agent_event"];
        assert_eq!(ae["agent_event_version"], 1);
        assert_eq!(ae["verifier"], VERIFIER);
        assert_eq!(ae["case_id"], "case:run-42");
        assert_eq!(ae["tokens"]["cache_read"], 3);
        assert_eq!(ae["token_quality"], "EXACT");
        assert_eq!(ae["owner"], "native");
        assert_eq!(
            r["payload"]["representations"][0]["inline_content"]["verifier_id"],
            VERIFIER
        );
    }
}

#[test]
fn reingest_is_a_noop() {
    let (h, rid, run_id) = setup();
    let input = ndjson(&batch());
    h.ingest(&run_id, "agent-event-v1", &input, &[]);
    let (code, out, err) = h.ingest(&run_id, "agent-event-v1", &input, &[]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out["new_observations"], 0);
    assert_eq!(out["already_committed_count"], 3);
    assert_eq!(out["already_committed"], true);
    assert_eq!(h.observations(&rid).len(), 3);
    // a partially overlapping batch only adds the new event
    let mut more = batch();
    more.push(event(4, "stop", Value::Null, "user"));
    let (_, out, _) = h.ingest(&run_id, "agent-event-v1", &ndjson(&more), &[]);
    assert_eq!(out["new_observations"], 1);
    assert_eq!(h.observations(&rid).len(), 4);
}

#[test]
fn invalid_line_is_reported_by_number_and_appends_nothing() {
    let (h, rid, run_id) = setup();
    let mut bad = event(2, "tool_post", json!(0), "user");
    bad["tokens"] = json!({"input": -1, "output": 0, "cache_write": 0, "cache_read": 0});
    let input = ndjson(&[event(1, "session_start", Value::Null, "user"), bad]);
    let (code, _, err) = h.ingest(&run_id, "agent-event-v1", &input, &[]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("line 2"), "names the line: {err}");
    assert!(h.observations(&rid).is_empty(), "all-or-nothing");

    let mut not_json = ndjson(&[event(1, "session_start", Value::Null, "user")]);
    not_json.extend_from_slice(b"{broken\n");
    let (code, _, err) = h.ingest(&run_id, "agent-event-v1", &not_json, &[]);
    assert_eq!(code, 2);
    assert!(err.contains("line 2"), "{err}");

    // each required field, a bad enum, a short event_id, and a long case_id are all refused
    let mut cases: Vec<Value> = Vec::new();
    for field in [
        "schema",
        "event_id",
        "source",
        "runtime",
        "native_session_id",
        "native_event_ref",
        "kind",
        "ts",
        "case_id",
        "unit_id",
        "te_session_id",
        "role",
        "token_quality",
        "owner",
        "tokens",
    ] {
        let mut e = event(1, "session_start", Value::Null, "user");
        e.as_object_mut().unwrap().remove(field);
        cases.push(e);
    }
    let mut e = event(1, "session_start", Value::Null, "user");
    e["event_id"] = json!("abc");
    cases.push(e);
    let mut e = event(1, "session_start", Value::Null, "user");
    e["kind"] = json!("bogus");
    cases.push(e);
    let mut e = event(1, "session_start", Value::Null, "user");
    e["case_id"] = json!("c".repeat(201));
    cases.push(e);
    let mut e = event(1, "tool_post", Value::Null, "user");
    e["exit_code"] = Value::Null; // tool events must say how they ended
    cases.push(e);
    for c in cases {
        let (code, _, err) = h.ingest(&run_id, "agent-event-v1", &ndjson(&[c.clone()]), &[]);
        assert_eq!(code, 2, "{c} should be refused: {err}");
        assert!(err.contains("line 1"), "{err}");
    }
    // opaque case_id up to 200 chars, and the explicit sentinel, are accepted
    let mut ok = event(9, "session_start", Value::Null, "user");
    ok["case_id"] = json!("c".repeat(200));
    let (code, ..) = h.ingest(&run_id, "agent-event-v1", &ndjson(&[ok]), &[]);
    assert_eq!(code, 0);
    assert!(h.observations(&rid).len() == 1);
}

#[test]
fn raw_transcript_text_is_never_accepted() {
    let (h, rid, run_id) = setup();
    for (k, v) in [
        ("text", json!("hello")),
        ("content", json!("x")),
        ("messages", json!([])),
    ] {
        let mut e = event(1, "session_start", Value::Null, "user");
        e[k] = v;
        let (code, _, err) = h.ingest(&run_id, "agent-event-v1", &ndjson(&[e]), &[]);
        assert_eq!(code, 2, "{k}: {err}");
        assert!(err.contains("payload_ref"), "{err}");
    }
    let mut e = event(1, "session_start", Value::Null, "user");
    e["payload_ref"] = json!("line one\nline two of a transcript");
    let (code, ..) = h.ingest(&run_id, "agent-event-v1", &ndjson(&[e]), &[]);
    assert_eq!(code, 2);
    assert!(h.observations(&rid).is_empty());
}

#[test]
fn old_format_is_unchanged_and_unknown_format_is_refused() {
    let (h, rid, run_id) = setup();
    let (code, first, err) = h.ingest(&run_id, "cargo-json", OK_STREAM, &[]);
    assert_eq!(code, 0, "{err}");
    // the legacy key: sha256(run + "+" + verifier + "+ingest+1")
    let mut hasher = Sha256::new();
    hasher.update(format!("{run_id}+cargo-test+ingest+1"));
    assert_eq!(first["idempotency_key"], hex::encode(hasher.finalize()));
    assert_eq!(first["already_committed"], false);
    assert_eq!(first["parsed"]["passed"], 1);
    assert!(first.get("events").is_none(), "old output shape");
    // another legacy spelling lands on the same key: still one observation per run+verifier
    let (_, dup, _) = h.ingest(&run_id, "cargo-test-json", OK_STREAM, &[]);
    assert_eq!(dup["already_committed"], true);
    let rows = h.observations(&rid);
    assert_eq!(rows.len(), 1);
    let p = &rows[0]["payload"];
    assert!(p.get("extensions").is_none());
    assert_eq!(
        p["representations"][0]["inline_content"],
        json!({"exit_code": null, "crashed": false, "stderr_ref": null,
               "parsed": {"test_count": 1, "passed": 1, "failed": 0, "ignored": 0}})
    );
    assert_eq!(
        p["representations"][0]["content_hash"],
        format!("sha256:{}", hex::encode(Sha256::digest(OK_STREAM)))
    );

    // unknown format: clear error, stable non-zero exit, nothing recorded or spooled
    for bad in ["nope", "agent-event-v2", ""] {
        let (code, _, err) = h.ingest(&run_id, bad, OK_STREAM, &[]);
        assert_eq!(code, 2, "{bad:?}");
        assert!(err.contains("unknown --stdin-format"), "{err}");
    }
    let (code, ..) = h.ingest(&run_id, "nope", OK_STREAM, &["--spool"]);
    assert_eq!(code, 2);
    assert_eq!(h.observations(&rid).len(), 1);
}

#[test]
fn builder_and_non_builder_events_never_share_a_run() {
    let (h, rid, run_id) = setup();
    let mixed = ndjson(&[
        event(1, "session_start", Value::Null, "user"),
        event(2, "session_start", Value::Null, "builder"),
    ]);
    let (code, _, err) = h.ingest(&run_id, "agent-event-v1", &mixed, &[]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("builder"), "{err}");
    assert!(h.observations(&rid).is_empty());

    // across batches too: the run's first role class sticks
    let (code, ..) = h.ingest(
        &run_id,
        "agent-event-v1",
        &ndjson(&[event(3, "stop", Value::Null, "user")]),
        &[],
    );
    assert_eq!(code, 0);
    let (code, _, err) = h.ingest(
        &run_id,
        "agent-event-v1",
        &ndjson(&[event(4, "stop", Value::Null, "builder")]),
        &[],
    );
    assert_eq!(code, 2, "{err}");
    assert_eq!(h.observations(&rid).len(), 1);
}

#[test]
fn exit_code_sets_evidence_direction() {
    let (h, rid, run_id) = setup();
    let (_, out, _) = h.ingest(&run_id, "agent-event-v1", &ndjson(&batch()), &[]);
    let ids: Vec<String> = out["observation_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids.len(), 3);
    // session_start (null) -> unknown, tool_post 0 -> supports, tool_fail 1 -> contradicts
    for (obs, want) in ids.iter().zip(["unknown", "supports", "contradicts"]) {
        h.json(&[
            "evidence",
            "create",
            "--run",
            &run_id,
            "--observation",
            obs,
            "--derive",
        ]);
        let ev: Vec<Value> = h
            .json(&["ledger", "export", "--repo", &rid])
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["record_kind"] == "evidence.created")
            .cloned()
            .collect();
        let last = ev.last().unwrap();
        assert_eq!(last["payload"]["evidence"][0]["direction"], want, "{obs}");
    }
}

#[test]
fn ledger_verifies_after_agent_ingest() {
    let (h, rid, run_id) = setup();
    h.ingest(&run_id, "agent-event-v1", &ndjson(&batch()), &[]);
    let v = h.json(&["ledger", "verify", "--repo", &rid]);
    assert_eq!(v["ok"], true, "{v}");
}

#[test]
fn edited_redelivery_does_not_overwrite_a_committed_event() {
    let (h, rid, run_id) = setup();
    h.ingest(&run_id, "agent-event-v1", &ndjson(&batch()), &[]);
    let mut forged = batch();
    forged[1]["tokens"] =
        json!({"input": 999999, "output": 999999, "cache_write": 0, "cache_read": 0});
    let (code, out, err) = h.ingest(&run_id, "agent-event-v1", &ndjson(&forged), &[]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out["new_observations"], 0);
    assert_eq!(
        out["conflicting_redeliveries"].as_array().unwrap().len(),
        1,
        "the edit is surfaced, not silently absorbed"
    );
    let rows = h.observations(&rid);
    assert_eq!(rows.len(), 3);
    assert!(
        rows.iter()
            .all(|r| r["payload"]["extensions"]["agent_event"]["tokens"]["input"] != 999999)
    );
    assert_eq!(h.json(&["ledger", "verify", "--repo", &rid])["ok"], true);
}

#[test]
fn spooled_batch_drains_to_one_observation_per_event() {
    let (h, rid, run_id) = setup();
    let (code, out, err) = h.ingest(&run_id, "agent-event-v1", &ndjson(&batch()), &["--spool"]);
    assert_eq!(code, 75, "{err}");
    assert_eq!(out["durability"], "spooled");
    let d = h.json(&["spool", "drain", "--repo", &rid]);
    assert_eq!(d["drained"], 3, "{d}");
    assert_eq!(h.observations(&rid).len(), 3);
    // an invalid spooled batch is rejected at drain with its line number
    let (code, ..) = h.ingest(
        &run_id,
        "agent-event-v1",
        b"{\"schema\":\"agent-event/v1\"}\n",
        &["--spool"],
    );
    assert_eq!(code, 75);
    let d = h.json(&["spool", "drain", "--repo", &rid]);
    assert!(d["rejected"][0].as_str().unwrap().contains("line 1"), "{d}");
    assert_eq!(h.observations(&rid).len(), 3);
}

/// Two fresh homes, same NDJSON, id stream and clock seams on: the canonical
/// projection export is byte-identical. No field is excluded.
#[test]
fn replay_is_byte_identical_across_fresh_homes() {
    let shared = tempfile::tempdir().unwrap();
    let repo = make_repo(shared.path());
    let input = ndjson(&batch());
    let mut exports = Vec::new();
    for _ in 0..2 {
        let h = Home::new(true);
        let f = declare(&h, &repo);
        let run_id = begin_run(&h, &f);
        let (code, out, err) = h.ingest(&run_id, "agent-event-v1", &input, &[]);
        assert_eq!(code, 0, "{err}");
        for obs in out["observation_ids"].as_array().unwrap() {
            h.json(&[
                "evidence",
                "create",
                "--run",
                &run_id,
                "--observation",
                obs.as_str().unwrap(),
                "--derive",
            ]);
        }
        h.json(&["projections", "rebuild", "--repo", &f.rid, "--all"]);
        let ex = h.run(&[
            "projections",
            "export",
            "--repo",
            &f.rid,
            "--canonical-json",
        ]);
        assert!(
            ex.status.success(),
            "{}",
            String::from_utf8_lossy(&ex.stderr)
        );
        assert!(ex.stdout.len() > 10);
        exports.push(ex.stdout);
    }
    assert_eq!(exports[0], exports[1], "replay must be byte-identical");
    println!("REPLAY_SHA256={}", hex::encode(Sha256::digest(&exports[0])));
}

#[test]
fn run_begin_require_declared_is_opt_in() {
    let h = Home::new(false);
    let repo = make_repo(&h.data);
    let f = declare(&h, &repo);
    let begin = |q: &str, t: &str, strict: bool| {
        let mut a = vec![
            "run",
            "begin",
            "--repo",
            &f.rid,
            "--snapshot",
            &f.snap,
            "--question",
            q,
            "--threshold",
            t,
        ];
        if strict {
            a.push("--require-declared");
        }
        h.run(&a)
    };
    // default unchanged: an undeclared question still begins
    assert!(begin("q-ghost", "t-ghost", false).status.success());
    // opt-in refuses the missing question, then the missing threshold
    let out = begin("q-ghost", "t-agent", true);
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("question q-ghost"));
    let out = begin(&f.qid, "t-ghost", true);
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("threshold t-ghost"));
    // a threshold declared for another question does not count
    let out = begin("q-ghost", &f.tid, true);
    assert_eq!(out.status.code(), Some(3));
    // declared question + threshold pass
    assert!(begin(&f.qid, &f.tid, true).status.success());
}

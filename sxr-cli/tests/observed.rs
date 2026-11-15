//! WP-16 CLI end to end: `observed ingest` records accepted and rejected
//! events, `observed derive` is read-only, byte-identical, window-bounded and
//! refuses a tampered ledger with CHAIN_VERIFY_FAILED.
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use sxr_ledger::ledger::{Ledger, LedgerConfig, PendingEntry};

const WORLD: &str =
    "world:sop@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const EPISODE: &str =
    "episode:case1:step1:1@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const KEY: &str = "concept:6ba7b810-9dad-51d1-80b4-00c04fd430c8";

fn sxr(xdg: &Path, args: &[&str]) -> Output {
    Command::new(PathBuf::from(env!("CARGO_BIN_EXE_realitytrace")))
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

fn ext(record_id: &str, seq: u64) -> Value {
    json!({
        "world_ref": WORLD, "semantic_ref": "sop::Step", "declaration_key": KEY,
        "actor": {"principal": "p1", "role": "clerk"}, "operation": "approve", "resource": "invoice-1",
        "context": {"case_id": "case1", "plan_item_id": "step1", "episode_ref": EPISODE},
        "correlation": {"case_id": "case1"}, "authority": {"decision_id": "not_required"},
        "outcome": {"status": "succeeded"},
        "source": {"system": "sea-forge", "record_id": record_id, "sequence": seq, "recorded_at": "2026-11-14T10:00:00Z"},
        "executor_provenance": "program", "kind": "evaluated", "expectation_refs": ["question:q1"]
    })
}

fn line(e: &Value) -> String {
    json!({"envelope_kind": "execution_trace", "extensions": {"godspeed.interaction_event": e}})
        .to_string()
}

fn derive(xdg: &Path, repo_id: &str, extra: &[&str]) -> Output {
    let mut a = vec![
        "observed",
        "derive",
        "--repo",
        repo_id,
        "--world",
        WORLD,
        "--scope",
        "case:case1",
        "--episode",
        EPISODE,
        "--kind",
        "evaluated",
    ];
    a.extend_from_slice(extra);
    sxr(xdg, &a)
}

#[test]
fn ingest_records_rejections_and_derive_is_deterministic_bounded_and_tamper_evident() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    git_repo(&repo);
    let enrolled = sxr(
        tmp.path(),
        &["repo", "enroll", repo.to_str().unwrap(), "--json"],
    );
    assert!(enrolled.status.success());
    let repo_id = serde_json::from_slice::<Value>(&enrolled.stdout).unwrap()["repo_id"]
        .as_str()
        .unwrap()
        .to_string();
    let db = tmp
        .path()
        .join("data/sxr/repos")
        .join(&repo_id)
        .join("ledger.db");

    // Evidence records exist in the ledger for r1 and r2 only.
    {
        let mut l = Ledger::open(&db, &repo_id, LedgerConfig::default()).unwrap();
        for r in ["r1", "r2"] {
            l.append(&PendingEntry {
                record_kind: "evidence.recorded".into(),
                idempotency_key: Some(format!("ev-{r}")),
                subject_refs: vec![],
                payload: json!({"evidence": [{"evidence_id": format!("evidence:recorded:{r}")}]})
                    .to_string(),
                entry_ulid: None,
                committed_at: None,
            })
            .unwrap();
        }
    }
    let mut no_world = ext("r1", 9);
    no_world.as_object_mut().unwrap().remove("world_ref");
    let input = [
        line(&ext("r1", 1)),
        line(&no_world),
        line(&ext("r3", 3)),
        line(&ext("r2", 2)),
    ]
    .join("\n");
    let f = tmp.path().join("events.ndjson");
    std::fs::write(&f, input).unwrap();
    let ing = sxr(
        tmp.path(),
        &[
            "observed",
            "ingest",
            "--repo",
            &repo_id,
            "--file",
            f.to_str().unwrap(),
        ],
    );
    assert_eq!(
        ing.status.code(),
        Some(4),
        "{}",
        String::from_utf8_lossy(&ing.stderr)
    );
    let out: Value = serde_json::from_slice(&ing.stdout).unwrap();
    let results = out["ingest"]["results"].as_array().unwrap();
    let status: Vec<(&str, &str)> = results
        .iter()
        .map(|r| {
            (
                r["status"].as_str().unwrap(),
                r["code"].as_str().unwrap_or("-"),
            )
        })
        .collect();
    assert_eq!(
        status,
        [
            ("accepted", "-"),
            ("rejected", "INGEST_REJECTED_IDENTITY"),
            ("rejected", "INGEST_REJECTED_EVIDENCE"),
            ("accepted", "-")
        ]
    );
    assert_eq!(results[0]["signature_status"], "unsigned");

    // Re-delivery is a ledger no-op.
    let again = sxr(
        tmp.path(),
        &[
            "observed",
            "ingest",
            "--repo",
            &repo_id,
            "--file",
            f.to_str().unwrap(),
        ],
    );
    let n_rows = |db: &Path| {
        Ledger::open(db, &repo_id, LedgerConfig::default())
            .unwrap()
            .count()
            .unwrap()
    };
    let rows_after_first = n_rows(&db);
    assert_eq!(again.status.code(), Some(4));
    assert_eq!(n_rows(&db), rows_after_first, "re-delivery must not append");

    // Derive: twice identical; rejected counted, never an observation; read-only.
    let a = derive(tmp.path(), &repo_id, &["--independent-system", "sea-forge"]);
    assert!(a.status.success(), "{}", String::from_utf8_lossy(&a.stderr));
    let b = derive(tmp.path(), &repo_id, &["--independent-system", "sea-forge"]);
    assert_eq!(a.stdout, b.stdout);
    assert_eq!(n_rows(&db), rows_after_first, "derive must not write");
    let doc: Value = serde_json::from_slice(&a.stdout).unwrap();
    let rep = &doc["representation"];
    assert_eq!(rep["nodes"][0]["frequency"], 2);
    assert_eq!(rep["nodes"][0]["independent_occurrence_count"], 2);
    assert_eq!(rep["uncertainty"]["rejected_event_count"], 2);
    assert_eq!(rep["uncertainty"]["unverified_event_count"], 0);

    // --until excludes later records and changes the hash.
    let obs: Vec<u64> = Ledger::open(&db, &repo_id, LedgerConfig::default())
        .unwrap()
        .read_all()
        .unwrap()
        .iter()
        .filter(|r| r.record_kind == "observation.recorded")
        .map(|r| r.append_ordinal)
        .collect();
    assert_eq!(obs.len(), 2);
    let cut = derive(tmp.path(), &repo_id, &["--until", &obs[0].to_string()]);
    assert!(
        cut.status.success(),
        "{}",
        String::from_utf8_lossy(&cut.stderr)
    );
    let cut: Value = serde_json::from_slice(&cut.stdout).unwrap();
    assert_eq!(cut["window"]["last_ordinal"], obs[0]);
    assert_eq!(cut["representation"]["nodes"][0]["frequency"], 1);
    assert_ne!(cut["representation_id"], doc["representation_id"]);

    // --out writes the same bytes the stdout form prints.
    let o = tmp.path().join("rep.json");
    let plain = derive(tmp.path(), &repo_id, &[]);
    let w = derive(tmp.path(), &repo_id, &["--out", o.to_str().unwrap()]);
    assert!(w.status.success());
    assert_eq!(std::fs::read(&o).unwrap(), plain.stdout);

    // Tamper: delete one occurrence from the ledger copy.
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute_batch(&format!(
            "DELETE FROM ledger WHERE append_ordinal = {}",
            obs[0]
        ))
        .unwrap();
    let t = derive(tmp.path(), &repo_id, &[]);
    assert_eq!(t.status.code(), Some(5));
    assert!(
        String::from_utf8_lossy(&t.stderr).contains("CHAIN_VERIFY_FAILED"),
        "{}",
        String::from_utf8_lossy(&t.stderr)
    );
}

//! WP-17 CLI end to end: ingest, derive, compare. Compare is read-only, byte
//! identical on a re-run, refuses a profile that is not the pinned one, exits 5
//! on a truncated tail, and (with a DomainForge binary that has `evaluate`)
//! decides a policy expectation. `REALITYTRACE_DOMAINFORGE_BIN` unset: the
//! DomainForge part prints NOT_RUN. Set to a non-file: the test fails.
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use sxr_core::canonical_json::canonical_json;
use sxr_ledger::ledger::{Ledger, LedgerConfig, PendingEntry};

const KEY_F: &str = "concept:6ba7b810-9dad-51d1-80b4-00c04fd430c8";
const KEY_M: &str = "concept:6ba7b811-9dad-51d1-80b4-00c04fd430c8";
const KEY_U: &str = "concept:6ba7b814-9dad-51d1-80b4-00c04fd430c8";
const KEY_C: &str = "concept:6ba7b813-9dad-51d1-80b4-00c04fd430c8";

fn sxr(xdg: &Path, args: &[&str]) -> Output {
    Command::new(PathBuf::from(env!("CARGO_BIN_EXE_realitytrace")))
        .args(args)
        .env("XDG_DATA_HOME", xdg.join("data"))
        .env("XDG_STATE_HOME", xdg.join("state"))
        .env_remove("SXR_DAEMON_SOCKET")
        .env_remove("REALITYTRACE_DOMAINFORGE_BIN")
        .env_remove("SXR_DOMAINFORGE_BIN")
        .output()
        .expect("run realitytrace")
}

fn git_repo(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("README"), "fixture\n").unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["add", "-A"],
        vec!["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "init"],
    ] {
        assert!(Command::new("git").args(&args).current_dir(dir).status().unwrap().success());
    }
}

fn sha_hex(v: &Value) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(canonical_json(v).as_bytes()))
}

fn ev(world: &str, episode: &str, rid: &str, seq: u64, key: &str, op: &str, resource: &str) -> Value {
    json!({"envelope_kind": "execution_trace", "extensions": {"godspeed.interaction_event": {
        "world_ref": world, "semantic_ref": "sop::Element", "declaration_key": key,
        "actor": {"principal": "p1", "role": "clerk"}, "operation": op, "resource": resource,
        "context": {"case_id": "case1", "plan_item_id": "step1", "episode_ref": episode},
        "correlation": {"case_id": "case1"}, "authority": {"decision_id": "not_required"},
        "outcome": {"status": "succeeded"},
        "source": {"system": "sea-forge", "record_id": rid, "sequence": seq, "recorded_at": "2026-11-14T10:00:00Z"},
        "executor_provenance": "program", "kind": "evaluated", "expectation_refs": ["question:q1"]}}})
}

fn profile() -> Value {
    let entry = |kind: &str, comparison: &str, extra: Value| {
        let mut m = json!({"true": "confirmed", "false": "contradicted", "unknown": "insufficient"});
        for (k, v) in extra.as_object().unwrap() {
            m[k] = v.clone();
        }
        json!({"kind": kind, "comparison": comparison, "quantifier": "universal", "min_support": 1,
               "allow_vacuous_truth": false, "deadline": null, "class_mapping": m})
    };
    json!({"contract": "comparison-profile.v1", "profile_id": "profile:s1", "kinds": [
        entry("policy_evaluable", "episode", json!({})),
        entry("structural", "structural", json!({"undeclared": "observed_undeclared"}))]})
}

/// Enroll a repo and ingest the events through `observed ingest`.
struct Setup {
    tmp: tempfile::TempDir,
    repo_id: String,
    db: PathBuf,
    episode: String,
    reference: PathBuf,
    profile: PathBuf,
}

fn setup(world: &str, close_case: bool) -> Setup {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    git_repo(&repo);
    let enrolled = sxr(tmp.path(), &["repo", "enroll", repo.to_str().unwrap(), "--json"]);
    assert!(enrolled.status.success());
    let repo_id = serde_json::from_slice::<Value>(&enrolled.stdout).unwrap()["repo_id"].as_str().unwrap().to_string();
    let db = tmp.path().join("data/sxr/repos").join(&repo_id).join("ledger.db");
    let p = profile();
    let body = json!({
        "case_id": "case1", "plan_item_id": "step1", "n": 1, "world_ref": world, "kind": "evaluated",
        "executor_provenance": "program",
        "criteria": {"intention": "i", "expected_change": "c", "invariants": [], "predicted_irrelevancies": [],
                     "success_region": "s", "failure_region": "f", "falsifiers": ["x"]},
        "comparison_profile": {"id": "profile:s1", "sha256": sha_hex(&p)},
        "decision_maker": {"principal": "p", "role": "r"},
        "evidence_sources": [{"source_id": "src-sf", "system": "sea-forge", "purpose": "p"}],
        "coverage_manifest": [{"emitter": "sea-forge", "event_kinds": ["move"]}],
    });
    let episode = format!("episode:case1:step1:1@sha256:{}", sha_hex(&body));
    let reference = tmp.path().join("reference.json");
    std::fs::write(&reference, json!({"episode_ref": episode, "body": body}).to_string()).unwrap();
    let profile_path = tmp.path().join("profile.json");
    std::fs::write(&profile_path, p.to_string()).unwrap();

    let mut events = vec![
        ("r1", 1, KEY_F, "freeze", "case1"),
        ("r2", 2, KEY_M, "move", "case1"),
        ("r3", 3, KEY_U, "purge", "case1"),
    ];
    if close_case {
        events.push(("r4", 4, KEY_C, "close_case", "open"));
    }
    {
        let mut l = Ledger::open(&db, &repo_id, LedgerConfig::default()).unwrap();
        for (r, ..) in &events {
            l.append(&PendingEntry {
                record_kind: "evidence.recorded".into(),
                idempotency_key: Some(format!("ev-{r}")),
                subject_refs: vec![],
                payload: json!({"evidence": [{"evidence_id": format!("evidence:recorded:{r}")}]}).to_string(),
                entry_ulid: None,
                committed_at: None,
            })
            .unwrap();
        }
    }
    let nd = events.iter().map(|(r, s, k, o, res)| ev(world, &episode, r, *s, k, o, res).to_string()).collect::<Vec<_>>().join("\n");
    let f = tmp.path().join("events.ndjson");
    std::fs::write(&f, nd).unwrap();
    let ing = sxr(tmp.path(), &["observed", "ingest", "--repo", &repo_id, "--file", f.to_str().unwrap()]);
    assert!(ing.status.success(), "{}", String::from_utf8_lossy(&ing.stderr));
    Setup { tmp, repo_id, db, episode, reference, profile: profile_path }
}

fn derive(s: &Setup, world: &str, extra: &[&str]) -> Output {
    let declared = s.tmp.path().join("declared.json");
    std::fs::write(&declared, json!([KEY_F, KEY_M, KEY_C]).to_string()).unwrap();
    let mut a = vec!["observed", "derive", "--repo", &s.repo_id, "--world", world, "--scope", "case:case1",
        "--episode", &s.episode, "--kind", "evaluated", "--profile", "profile:s1",
        "--independent-system", "sea-forge", "--declared", declared.to_str().unwrap()];
    a.extend_from_slice(extra);
    sxr(s.tmp.path(), &a)
}

fn compare(s: &Setup, world: &str, observed: &Path, mapping: &Path, extra: &[&str]) -> Output {
    let mut a = vec!["observed", "compare", "--repo", &s.repo_id, "--observed", observed.to_str().unwrap(),
        "--reference", s.reference.to_str().unwrap(), "--profile", s.profile.to_str().unwrap(),
        "--mapping", mapping.to_str().unwrap(), "--world", world, "--declared", "snapshot:sop-1"];
    a.extend_from_slice(extra);
    sxr(s.tmp.path(), &a)
}

fn mapping(policy: bool) -> Value {
    let mut ex = vec![json!({"id": "flow", "kind": "structural", "relation": "flow", "ref": "FrozenExpectation",
        "from": KEY_F, "to": KEY_M, "question": "question:q1"})];
    let mut m = json!({"contract": "compare-mapping.v1", "undeclared_question": "question:q1", "operations": {}});
    if policy {
        ex.push(json!({"id": "open", "kind": "policy_evaluable", "policy": "case_must_be_open",
            "operation": "close_case", "node": KEY_C, "question": "question:q1"}));
        m["operations"] = json!({"close_case": {"node": KEY_C, "input": {"case_status": {"kind": "string", "pointer": "/resource"}}}});
    }
    m["expectations"] = Value::Array(ex);
    m
}

const WORLD: &str = "world:sop@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn compare_is_read_only_deterministic_pinned_and_checks_the_tail() {
    let s = setup(WORLD, false);
    let rows_before = std::fs::read(&s.db).unwrap();
    let rep = s.tmp.path().join("rep.json");
    let d = derive(&s, WORLD, &["--out", rep.to_str().unwrap()]);
    assert!(d.status.success(), "{}", String::from_utf8_lossy(&d.stderr));
    let map = s.tmp.path().join("mapping.json");
    std::fs::write(&map, mapping(false).to_string()).unwrap();

    let a = compare(&s, WORLD, &rep, &map, &[]);
    assert!(a.status.success(), "{}", String::from_utf8_lossy(&a.stderr));
    let b = compare(&s, WORLD, &rep, &map, &[]);
    assert_eq!(a.stdout, b.stdout, "byte-identical re-run");
    let doc: Value = serde_json::from_slice(&a.stdout).unwrap();
    let classes: Vec<&str> = doc["residuals"]["structural"].as_array().unwrap().iter().map(|r| r["class"].as_str().unwrap()).collect();
    assert!(classes.contains(&"confirmed") && classes.contains(&"observed_undeclared"), "{classes:?}");
    assert_eq!(doc["tail_verification"]["status"], "unverified");
    assert_eq!(doc["residuals"]["episode"], json!([]));
    assert_eq!(std::fs::read(&s.db).unwrap(), rows_before, "compare and derive must not write the ledger");

    // A profile that is not the pinned one is refused.
    let mut other = profile();
    other["kinds"][0]["min_support"] = json!(3);
    std::fs::write(&s.profile, other.to_string()).unwrap();
    let bad = compare(&s, WORLD, &rep, &map, &[]);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("PROFILE_DIGEST_MISMATCH"));
    std::fs::write(&s.profile, profile().to_string()).unwrap();

    // Tail: with the anchored head the whole ledger verifies; delete the last row and exit 5.
    let rows = Ledger::open(&s.db, &s.repo_id, LedgerConfig::default()).unwrap().read_all().unwrap();
    let (n, head) = (rows.len().to_string(), rows.last().unwrap().entry_hash.clone());
    let ok = compare(&s, WORLD, &rep, &map, &["--expected-head", &head, "--expected-record-count", &n]);
    assert!(ok.status.success(), "{}", String::from_utf8_lossy(&ok.stderr));
    assert_eq!(serde_json::from_slice::<Value>(&ok.stdout).unwrap()["tail_verification"]["status"], "verified_against_expected_head");
    rusqlite::Connection::open(&s.db).unwrap()
        .execute_batch("DELETE FROM ledger WHERE append_ordinal = (SELECT MAX(append_ordinal) FROM ledger)").unwrap();
    let t = derive(&s, WORLD, &["--expected-head", &head]);
    assert_eq!(t.status.code(), Some(5), "{}", String::from_utf8_lossy(&t.stderr));
    assert!(String::from_utf8_lossy(&t.stderr).contains("TAIL_TRUNCATED"));
    let t2 = compare(&s, WORLD, &rep, &map, &["--expected-head", &head]);
    assert_eq!(t2.status.code(), Some(5));
    // The same ledger without a head: still derivable, and the output says the tail is unverified.
    let u = derive(&s, WORLD, &[]);
    assert!(u.status.success());
    assert_eq!(serde_json::from_slice::<Value>(&u.stdout).unwrap()["window"]["tail_verification"]["status"], "unverified");
}

#[test]
fn a_configured_binary_that_is_missing_fails_and_never_falls_back() {
    let s = setup(WORLD, true);
    let rep = s.tmp.path().join("rep.json");
    assert!(derive(&s, WORLD, &["--out", rep.to_str().unwrap()]).status.success());
    let map = s.tmp.path().join("mapping.json");
    std::fs::write(&map, mapping(true).to_string()).unwrap();
    let model = s.tmp.path().join("model.sea");
    std::fs::write(&model, "@namespace \"x\"\n").unwrap();
    let o = compare(&s, WORLD, &rep, &map, &["--domainforge-bin", "/nonexistent/domainforge", "--model", model.to_str().unwrap()]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("EVALUATE_UNAVAILABLE"), "{}", String::from_utf8_lossy(&o.stderr));
    // No binary at all: the policy expectation has occurrences and nothing may decide it.
    let n = compare(&s, WORLD, &rep, &map, &[]);
    assert!(String::from_utf8_lossy(&n.stderr).contains("EVALUATOR_REQUIRED"), "{}", String::from_utf8_lossy(&n.stderr));
}

#[test]
fn compare_with_domainforge_evaluate_decides_a_policy_expectation() {
    let Some(bin) = std::env::var_os("REALITYTRACE_DOMAINFORGE_BIN").filter(|v| !v.is_empty()) else {
        eprintln!("NOT_RUN: REALITYTRACE_DOMAINFORGE_BIN is not set");
        return;
    };
    let bin = PathBuf::from(bin);
    assert!(bin.is_file(), "REALITYTRACE_DOMAINFORGE_BIN={} is configured but is not a file", bin.display());
    let model = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sxr-observed/tests/fixtures/compare/model.sea");
    let facts = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(facts.path(), r#"{"schema":"domainforge.evaluate-facts.v1"}"#).unwrap();
    let w = Command::new(&bin).args(["evaluate", "--model", model.to_str().unwrap(), "--facts", facts.path().to_str().unwrap(), "--format", "json", "--world-name", "evalfix"]).output().unwrap();
    assert!(w.status.success(), "{}", String::from_utf8_lossy(&w.stderr));
    let world = serde_json::from_slice::<Value>(&w.stdout).unwrap()["world_ref"].as_str().unwrap().to_owned();

    let s = setup(&world, true);
    let rep = s.tmp.path().join("rep.json");
    assert!(derive(&s, &world, &["--out", rep.to_str().unwrap()]).status.success());
    let map = s.tmp.path().join("mapping.json");
    std::fs::write(&map, mapping(true).to_string()).unwrap();
    let facts_out = s.tmp.path().join("facts.json");
    let o = compare(&s, &world, &rep, &map, &["--domainforge-bin", bin.to_str().unwrap(), "--model", model.to_str().unwrap(), "--facts-out", facts_out.to_str().unwrap()]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let doc: Value = serde_json::from_slice(&o.stdout).unwrap();
    let r = &doc["residuals"]["episode"][0];
    assert_eq!((r["class"].as_str(), r["basis"].as_str()), (Some("confirmed"), Some("policy_true")));
    assert!(!doc["residuals"]["structural"].as_array().unwrap().is_empty(), "both lists are filled");
    use sha2::Digest;
    let on_disk = format!("sha256:{}", hex::encode(sha2::Sha256::digest(std::fs::read(&facts_out).unwrap())));
    assert_eq!(doc["evaluate"]["facts_sha256"], json!(on_disk), "the facts file is the one hashed");
    assert_eq!(doc["evaluate"]["world_ref"], json!(world));
}

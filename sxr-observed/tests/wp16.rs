//! WP-16 tests: ingest classification (NT-02) and derive (AT-06, AT-08 derive
//! half, NT-03 derive half, NT-03b) over a real SQLite hash-chained ledger.

use jsonschema::JSONSchema;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use sxr_core::LedgerRow;
use sxr_ledger::ledger::{Ledger, LedgerConfig, PendingEntry};
use sxr_observed::derive::{Options, Until, derive};
use sxr_observed::ingest::{
    EXTENSION_KEY, EventValidator, EvidenceIndex, SignatureCheck, SignatureStatus, UnsignedOnly,
    classify, schema_file_sha256,
};

const WORLD: &str =
    "world:sop@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const EPISODE: &str =
    "episode:case1:step1:1@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const KEY_A: &str = "concept:6ba7b810-9dad-51d1-80b4-00c04fd430c8";
const KEY_B: &str = "concept:6ba7b811-9dad-51d1-80b4-00c04fd430c8";

fn event(record_id: &str, seq: u64, key: &str, status: &str) -> Value {
    json!({
        "world_ref": WORLD, "semantic_ref": "sop::Step", "declaration_key": key,
        "actor": {"principal": "p1", "role": "clerk"},
        "operation": "approve", "resource": "invoice-1",
        "context": {"case_id": "case1", "plan_item_id": "step1", "episode_ref": EPISODE},
        "correlation": {"case_id": "case1"},
        "authority": {"decision_id": "not_required"},
        "outcome": {"status": status},
        "source": {"system": "sea-forge", "record_id": record_id, "sequence": seq,
                   "recorded_at": "2026-11-14T10:00:00Z"},
        "executor_provenance": "program", "kind": "evaluated", "expectation_refs": ["question:q1"]
    })
}

fn envelope(ext: Value) -> Value {
    json!({"envelope_kind": "execution_trace", "extensions": {EXTENSION_KEY: ext}})
}

fn evidence_payload(record_id: &str) -> String {
    json!({"evidence": [{"evidence_id": format!("evidence:recorded:{record_id}")}]}).to_string()
}

fn obs_payload(ext: &Value) -> String {
    json!({"extensions": {EXTENSION_KEY: ext, "sxr.ingest": {"accepted": true, "signature_status": "unsigned"}}})
        .to_string()
}

struct Fixture {
    dir: tempfile::TempDir,
    n: u32,
}

impl Fixture {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
            n: 0,
        }
    }
    fn path(&self) -> PathBuf {
        self.dir.path().join("ledger.db")
    }
    fn append(&mut self, kind: &str, payload: String) {
        let mut l = Ledger::open(&self.path(), "led1", LedgerConfig::default()).unwrap();
        self.n += 1;
        l.append(&PendingEntry {
            record_kind: kind.into(),
            idempotency_key: Some(format!("k{}", self.n)),
            subject_refs: vec![],
            payload,
            entry_ulid: None,
            committed_at: Some(format!("2026-11-14T10:00:{:02}Z", self.n)),
        })
        .unwrap();
    }
    /// Evidence row, then the observation that cites it.
    fn event(&mut self, ext: &Value) {
        let rid = ext["source"]["record_id"].as_str().unwrap().to_owned();
        self.append("evidence.recorded", evidence_payload(&rid));
        self.append("observation.recorded", obs_payload(ext));
    }
    /// Raw read: `Ledger::open` itself refuses a tampered chain at startup, and
    /// the derive gate must hold for rows however they were obtained.
    fn rows(&self) -> Vec<LedgerRow> {
        let c = rusqlite::Connection::open(self.path()).unwrap();
        let mut stmt = c
            .prepare(
                "SELECT ledger_id, append_ordinal, entry_ulid, record_kind, record_version,
                        idempotency_key, subject_refs, canonicalization, hash_algorithm,
                        payload, payload_hash, previous_entry_hash, entry_hash,
                        committed_at, writer_identity_ref FROM ledger ORDER BY append_ordinal",
            )
            .unwrap();
        stmt.query_map([], |r| {
            Ok(LedgerRow {
                ledger_id: r.get(0)?,
                append_ordinal: r.get::<_, i64>(1)? as u64,
                entry_ulid: r.get(2)?,
                record_kind: r.get(3)?,
                record_version: r.get(4)?,
                idempotency_key: r.get(5)?,
                subject_refs: serde_json::from_str(&r.get::<_, String>(6)?).unwrap(),
                canonicalization: r.get(7)?,
                hash_algorithm: r.get(8)?,
                payload: r.get(9)?,
                payload_hash: r.get(10)?,
                previous_entry_hash: r.get(11)?,
                entry_hash: r.get(12)?,
                committed_at: r.get(13)?,
                writer_identity_ref: r.get(14)?,
            })
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
    }
}

/// Three events: A, B (caused by A), A again.
fn base() -> Fixture {
    let mut f = Fixture::new();
    f.event(&event("r1", 1, KEY_A, "succeeded"));
    let mut b = event("r2", 2, KEY_B, "succeeded");
    b["causation_id"] = json!("r1");
    f.event(&b);
    f.event(&event("r3", 3, KEY_A, "succeeded"));
    f
}

fn opts() -> Options {
    Options {
        world_ref: WORLD.into(),
        scope: "case:case1".into(),
        episode_ref: EPISODE.into(),
        kind: "evaluated".into(),
        profile_id: "profile:s1".into(),
        until: Until::Head,
        declared_keys: None,
        coverage_manifest: None,
        independent_systems: BTreeSet::new(),
    }
}

fn contract_schema() -> JSONSchema {
    let p =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/observed-representation.v1.json");
    JSONSchema::compile(&serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap())
        .unwrap()
}

fn occurrence_ids(v: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for n in v["nodes"].as_array().unwrap() {
        for id in n["occurrence_ids"].as_array().unwrap() {
            out.insert(id.as_str().unwrap().to_owned());
        }
    }
    out
}

#[test]
fn vendored_extension_schema_is_the_wp11_file() {
    assert_eq!(
        schema_file_sha256(),
        "sha256:edc73a14eb665bd1300dbf61245511b7cc180805cfc848ffc8478035d52fbbc2"
    );
}

// ---- NT-02: no verified conclusion without identity or evidence ----

fn index_with(ids: &[&str]) -> EvidenceIndex {
    let mut f = Fixture::new();
    for id in ids {
        f.append("evidence.recorded", evidence_payload(id));
    }
    EvidenceIndex::from_rows(&f.rows())
}

#[test]
fn nt02_a_missing_world_ref_or_declaration_key_is_identity_rejected() {
    let v = EventValidator::new();
    let idx = index_with(&["r1"]);
    for field in ["world_ref", "declaration_key"] {
        let mut e = event("r1", 1, KEY_A, "succeeded");
        e.as_object_mut().unwrap().remove(field);
        let r = classify(&v, &envelope(e), &idx, &UnsignedOnly).unwrap_err();
        assert_eq!(r.code.as_str(), "INGEST_REJECTED_IDENTITY", "{field}");
    }
}

#[test]
fn nt02_b_valid_world_ref_without_evidence_record_is_evidence_rejected() {
    let v = EventValidator::new();
    let r = classify(
        &v,
        &envelope(event("r9", 1, KEY_A, "succeeded")),
        &index_with(&["r1"]),
        &UnsignedOnly,
    )
    .unwrap_err();
    assert_eq!(r.code.as_str(), "INGEST_REJECTED_EVIDENCE");
}

#[test]
fn valid_event_is_accepted_and_unsigned_is_recorded_as_unsigned() {
    let v = EventValidator::new();
    let a = classify(
        &v,
        &envelope(event("r1", 1, KEY_A, "succeeded")),
        &index_with(&["r1"]),
        &UnsignedOnly,
    )
    .unwrap();
    assert_eq!(a.occurrence_id, "sea-forge:r1");
    assert_eq!(a.signature, SignatureStatus::Unsigned);
    assert_ne!(a.signature.as_str(), "verified");
}

#[test]
fn present_signature_without_a_verifier_is_unverified_and_a_failing_hook_rejects() {
    struct Refuse;
    impl SignatureCheck for Refuse {
        fn check(&self, _: &Value) -> Result<SignatureStatus, String> {
            Err("bad signature".into())
        }
    }
    let v = EventValidator::new();
    let idx = index_with(&["r1"]);
    let mut e = event("r1", 1, KEY_A, "succeeded");
    e["emitter"] = json!({"signature": {"key_id": "k", "alg": "ed25519", "sig": "s"}});
    let a = classify(&v, &envelope(e.clone()), &idx, &UnsignedOnly).unwrap();
    assert_eq!(a.signature.as_str(), "unverified");
    let r = classify(&v, &envelope(e), &idx, &Refuse).unwrap_err();
    assert_eq!(r.code.as_str(), "INGEST_REJECTED_SIGNATURE");
}

#[test]
fn schema_violations_are_schema_rejected() {
    let v = EventValidator::new();
    let idx = index_with(&["r1"]);
    let mut e = event("r1", 1, KEY_A, "succeeded");
    e.as_object_mut().unwrap().remove("expectation_refs"); // evaluated needs it
    assert_eq!(
        classify(&v, &envelope(e), &idx, &UnsignedOnly)
            .unwrap_err()
            .code
            .as_str(),
        "INGEST_REJECTED_SCHEMA"
    );
    let mut e = event("r1", 1, KEY_A, "succeeded");
    e["expectation_refs"] = json!(["question:q1", "question:q1"]);
    assert_eq!(
        classify(&v, &envelope(e), &idx, &UnsignedOnly)
            .unwrap_err()
            .code
            .as_str(),
        "INGEST_REJECTED_SCHEMA"
    );
    let not_trace = json!({"envelope_kind": "work_request", "extensions": {}});
    assert_eq!(
        classify(&v, &not_trace, &idx, &UnsignedOnly)
            .unwrap_err()
            .code
            .as_str(),
        "INGEST_REJECTED_SCHEMA"
    );
}

// ---- AT-06: computed, deterministic, occurrence ids resolve ----

#[test]
fn at06_two_derives_are_byte_identical_and_list_real_occurrences() {
    let f = base();
    let rows = f.rows();
    let a = derive(&rows, &opts()).unwrap();
    let b = derive(&f.rows(), &opts()).unwrap();
    assert_eq!(a.canonical, b.canonical);
    assert_eq!(a.representation_id, b.representation_id);
    assert!(
        contract_schema().is_valid(&a.representation),
        "representation violates the contract"
    );

    // Every listed occurrence id exists in the ledger, and no real one is omitted.
    let in_ledger: BTreeSet<String> = rows
        .iter()
        .filter(|r| r.record_kind == "observation.recorded")
        .map(|r| {
            let p: Value = serde_json::from_str(&r.payload).unwrap();
            let s = &p["extensions"][EXTENSION_KEY]["source"];
            format!(
                "{}:{}",
                s["system"].as_str().unwrap(),
                s["record_id"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(occurrence_ids(&a.representation), in_ledger);
    for e in a.representation["edges"].as_array().unwrap() {
        for id in e["occurrence_ids"].as_array().unwrap() {
            assert!(in_ledger.contains(id.as_str().unwrap()));
        }
    }
    // Coverage and profile are present; the window is explicit; hashes agree.
    assert!(!a.representation["coverage"].as_array().unwrap().is_empty());
    assert_eq!(a.representation["profile_id"], "profile:s1");
    assert_eq!(a.document["window"]["last_ordinal"], 6);
    assert_eq!(
        a.document["input_set_sha256"],
        a.representation["input_set_hash"]
    );
    // Shape: A twice, B once; A->B by causation, B->A by sequence, A->A absent.
    let nodes = a.representation["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0]["frequency"], 2);
    let edges = a.representation["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 2);
    assert_eq!(
        edges[0]["ordering"],
        json!({"via_sequence": 0, "via_causation": 1})
    );
    assert_eq!(
        edges[1]["ordering"],
        json!({"via_sequence": 1, "via_causation": 0})
    );
    // Nothing is independent unless declared.
    assert_eq!(nodes[0]["independent_occurrence_count"], 0);
}

#[test]
fn independence_and_undeclared_and_coverage_options_are_applied() {
    let f = base();
    let mut o = opts();
    o.independent_systems = ["sea-forge".to_string()].into();
    o.declared_keys = Some([KEY_A.to_string()].into());
    o.coverage_manifest = Some(vec![
        ("sea-forge".into(), "approve".into()),
        ("cognate".into(), "pay".into()),
    ]);
    let d = derive(&f.rows(), &o).unwrap();
    assert!(contract_schema().is_valid(&d.representation));
    let nodes = d.representation["nodes"].as_array().unwrap();
    assert!(nodes.iter().any(
        |n| n["node_id"].as_str().unwrap().starts_with("undeclared:")
            && n.get("semantic_ref").is_none()
    ));
    assert_eq!(
        nodes.iter().find(|n| n["node_id"] == KEY_A).unwrap()["independent_occurrence_count"],
        2
    );
    let cov = d.representation["coverage"].as_array().unwrap();
    assert_eq!(cov[0]["status"], "silent"); // cognate/pay
    assert_eq!(cov[1]["status"], "covered");
}

// ---- AT-08 (derive half): one real input more, and --until ----

#[test]
fn at08_one_more_input_changes_the_hash_and_until_excludes_later_records() {
    let mut f = base();
    let before = derive(&f.rows(), &opts()).unwrap();
    let head_before = f.rows().last().unwrap().append_ordinal;
    f.event(&event("r4", 4, KEY_B, "failed"));
    let after = derive(&f.rows(), &opts()).unwrap();
    assert_ne!(before.representation_id, after.representation_id);
    assert_ne!(
        before.representation["input_set_hash"],
        after.representation["input_set_hash"]
    );
    assert!(occurrence_ids(&after.representation).contains("sea-forge:r4"));
    assert!(!occurrence_ids(&before.representation).contains("sea-forge:r4"));

    let mut o = opts();
    o.until = Until::Ordinal(head_before);
    let cut = derive(&f.rows(), &o).unwrap();
    assert_eq!(
        occurrence_ids(&cut.representation),
        occurrence_ids(&before.representation)
    );
    assert_eq!(cut.representation["nodes"], before.representation["nodes"]);
    assert_ne!(cut.representation_id, after.representation_id);
    // timestamp and record forms select the same window
    let rows = f.rows();
    for u in [
        rows[head_before as usize - 1].committed_at.clone(),
        rows[head_before as usize - 1].entry_ulid.clone(),
    ] {
        o.until = Until::parse(&u).unwrap();
        let d = derive(&rows, &o).unwrap();
        assert_eq!(d.document["window"]["last_ordinal"], head_before);
    }
}

// ---- NT-03 (derive half): changed occurrence content changes the result ----

#[test]
fn nt03_changed_occurrence_content_changes_the_hash_within_one_process() {
    let a = base();
    let mut b = Fixture::new();
    b.event(&event("r1", 1, KEY_A, "failed")); // the one changed occurrence: a violating value
    b.event(&{
        let mut e = event("r2", 2, KEY_B, "succeeded");
        e["causation_id"] = json!("r1");
        e
    });
    b.event(&event("r3", 3, KEY_A, "succeeded"));
    // Same case id and selectors, called back to back: no result may be reused.
    let da = derive(&a.rows(), &opts()).unwrap();
    let db = derive(&b.rows(), &opts()).unwrap();
    assert_ne!(da.representation_id, db.representation_id);
    assert_ne!(da.representation["nodes"], db.representation["nodes"]);
    let failed = |d: &Value| d["nodes"][0]["outcome_status"]["failed"].as_u64().unwrap();
    assert_eq!(
        (failed(&da.representation), failed(&db.representation)),
        (0, 1)
    );
}

// ---- NT-03b: missing or altered record fails the chain ----

fn tamper(f: &Fixture, sql: &str) {
    let c = rusqlite::Connection::open(f.path()).unwrap();
    c.execute_batch(sql).unwrap();
}

#[test]
fn nt03b_deleted_occurrence_is_chain_verify_failed() {
    let f = base();
    derive(&f.rows(), &opts()).unwrap(); // intact ledger derives
    tamper(&f, "DELETE FROM ledger WHERE append_ordinal = 4"); // the second observation
    let err = derive(&f.rows(), &opts()).unwrap_err();
    assert_eq!(err.code, "CHAIN_VERIFY_FAILED", "{err}");
}

#[test]
fn nt03b_altered_occurrence_is_chain_verify_failed() {
    let f = base();
    tamper(
        &f,
        "UPDATE ledger SET payload = replace(payload, 'succeeded', 'failed') WHERE append_ordinal = 2",
    );
    let err = derive(&f.rows(), &opts()).unwrap_err();
    assert_eq!(err.code, "CHAIN_VERIFY_FAILED", "{err}");
}

// ---- rejected and unresolved events are never observations ----

#[test]
fn rejected_and_unresolved_events_are_counted_as_uncertainty_not_observations() {
    let mut f = base();
    let note = json!({"code": "INGEST_REJECTED_EVIDENCE", "reason": "x", "candidate_sha256": "sha256:00",
        "hints": {"world_ref": WORLD, "case_id": "case1", "episode_ref": EPISODE, "kind": "evaluated"}});
    f.append(
        "ingest.rejected",
        json!({"extensions": {"sxr.ingest_rejection": note}}).to_string(),
    );
    // an observation whose source record never resolves
    f.append(
        "observation.recorded",
        obs_payload(&event("ghost", 9, KEY_B, "succeeded")),
    );
    let d = derive(&f.rows(), &opts()).unwrap();
    assert_eq!(
        d.representation["uncertainty"],
        json!({"rejected_event_count": 1, "unverified_event_count": 1})
    );
    assert!(!occurrence_ids(&d.representation).contains("sea-forge:ghost"));
    assert!(contract_schema().is_valid(&d.representation));
}

#[test]
fn canonicalisation_refuses_values_outside_the_ijson_domain() {
    assert_eq!(
        sxr_observed::canon::canonical(&json!({"a": 1.5}))
            .unwrap_err()
            .code,
        "CANONICALIZATION_REFUSED"
    );
    assert_eq!(
        sxr_observed::canon::canonical(&json!({"a": 9007199254740993_u64}))
            .unwrap_err()
            .code,
        "CANONICALIZATION_REFUSED"
    );
    assert_eq!(
        sxr_observed::canon::canonical(&json!({"b": 1, "a": "é"})).unwrap(),
        "{\"a\":\"é\",\"b\":1}"
    );
}

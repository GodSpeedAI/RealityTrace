//! WP-17 tests: `observed compare` over a real SQLite hash-chained ledger,
//! built by ingest classification (not hand-written residuals).
//!
//! Tests that need the DomainForge binary use `REALITYTRACE_DOMAINFORGE_BIN`.
//! Unset: the test prints `NOT_RUN` and returns. Set to a path that is not a
//! file, or to a binary without `evaluate`: the test fails loudly.

use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use sxr_core::LedgerRow;
use sxr_core::canonical_json::sha256_hex;
use sxr_ledger::ledger::{Ledger, LedgerConfig, PendingEntry};
use sxr_observed::canon;
use sxr_observed::compare::{CompareInput, Compared, Evaluator, compare};
use sxr_observed::derive::{Derived, Options, Until, derive};
use sxr_observed::df_cli::CliEvaluator;
use sxr_observed::ingest::{EXTENSION_KEY, EventValidator, EvidenceIndex, UnsignedOnly, classify};
use sxr_observed::tail::ExpectedHead;

const WORLD: &str =
    "world:sop@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const KEY_F: &str = "concept:6ba7b810-9dad-51d1-80b4-00c04fd430c8";
const KEY_M: &str = "concept:6ba7b811-9dad-51d1-80b4-00c04fd430c8";
const KEY_V: &str = "concept:6ba7b812-9dad-51d1-80b4-00c04fd430c8";
const KEY_C: &str = "concept:6ba7b813-9dad-51d1-80b4-00c04fd430c8";
const KEY_U: &str = "concept:6ba7b814-9dad-51d1-80b4-00c04fd430c8"; // never declared
const SYS: &str = "sea-forge";

// ------------------------------------------------------------- ledger

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
            committed_at: Some(format!(
                "2026-11-14T10:{:02}:{:02}Z",
                self.n / 60,
                self.n % 60
            )),
        })
        .unwrap();
    }
    /// Evidence row, then the classified observation, as `observed ingest` writes it.
    fn ingest(&mut self, ext: &Value) {
        let rid = ext["source"]["record_id"].as_str().unwrap().to_owned();
        self.append(
            "evidence.recorded",
            json!({"evidence": [{"evidence_id": format!("evidence:recorded:{rid}")}]}).to_string(),
        );
        let env = json!({"envelope_kind": "execution_trace", "extensions": {EXTENSION_KEY: ext}});
        let idx = EvidenceIndex::from_rows(&self.rows());
        let acc = classify(&EventValidator::new(), &env, &idx, &UnsignedOnly)
            .unwrap_or_else(|r| panic!("fixture event rejected: {}", r.reason));
        self.append(
            "observation.recorded",
            json!({"extensions": {EXTENSION_KEY: acc.event,
                "sxr.ingest": {"accepted": true, "signature_status": acc.signature.as_str(),
                               "occurrence_id": acc.occurrence_id}}})
            .to_string(),
        );
    }
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
    /// Raw delete of the last row: the chain over what is left still verifies.
    fn delete_last_row(&self) {
        let c = rusqlite::Connection::open(self.path()).unwrap();
        c.execute("DROP TRIGGER IF EXISTS ledger_no_delete", [])
            .ok();
        let n = c
            .execute(
                "DELETE FROM ledger WHERE append_ordinal = (SELECT MAX(append_ordinal) FROM ledger)",
                [],
            )
            .unwrap();
        assert_eq!(n, 1);
    }
}

// ----------------------------------------------------- reference, profile

fn profile() -> Value {
    let entry = |kind: &str, comparison: &str, quantifier: &str, extra: Value| {
        let mut m =
            json!({"true": "confirmed", "false": "contradicted", "unknown": "insufficient"});
        for (k, v) in extra.as_object().unwrap() {
            m[k] = v.clone();
        }
        json!({"kind": kind, "comparison": comparison, "quantifier": quantifier, "min_support": 1,
               "allow_vacuous_truth": false, "deadline": null, "class_mapping": m})
    };
    json!({"contract": "comparison-profile.v1", "profile_id": "profile:s1", "kinds": [
        entry("policy_evaluable", "episode", "universal", json!({})),
        entry("structural", "structural", "universal", json!({"undeclared": "observed_undeclared"})),
        entry("absence", "structural", "universal", json!({"trigger_observed_record_absent": "declared_unobserved"})),
    ]})
}

fn reference(
    kind: &str,
    sources: &[(&str, &str)],
    world: &str,
    profile: &Value,
) -> (Value, String) {
    let body = json!({
        "case_id": "case1", "plan_item_id": "step1", "n": 1, "world_ref": world, "kind": kind,
        "executor_provenance": "program",
        "criteria": {"intention": "i", "expected_change": "c", "invariants": [], "predicted_irrelevancies": [],
                     "success_region": "s", "failure_region": "f", "falsifiers": ["x"]},
        "comparison_profile": {"id": "profile:s1", "sha256": canon::digest(profile).unwrap().trim_start_matches("sha256:")},
        "decision_maker": {"principal": "p", "role": "r"},
        "evidence_sources": sources.iter().map(|(id, sys)| json!({"source_id": id, "system": sys, "purpose": "p"})).collect::<Vec<_>>(),
        "coverage_manifest": [{"emitter": SYS, "event_kinds": ["verify"]}],
    });
    let hex = canon::digest(&body)
        .unwrap()
        .trim_start_matches("sha256:")
        .to_owned();
    let ep = format!("episode:case1:step1:1@sha256:{hex}");
    (json!({"episode_ref": ep, "body": body}), ep)
}

struct Ev<'a> {
    rid: &'a str,
    seq: u64,
    key: &'a str,
    op: &'a str,
    resource: &'a str,
    system: &'a str,
}

fn event(e: &Ev, episode: &str, kind: &str, world: &str) -> Value {
    json!({
        "world_ref": world, "semantic_ref": "sop::Element", "declaration_key": e.key,
        "actor": {"principal": "p1", "role": "clerk"},
        "operation": e.op, "resource": e.resource,
        "context": {"case_id": "case1", "plan_item_id": "step1", "episode_ref": episode},
        "correlation": {"case_id": "case1"},
        "authority": {"decision_id": "not_required"},
        "outcome": {"status": "succeeded"},
        "source": {"system": e.system, "record_id": e.rid, "sequence": e.seq,
                   "recorded_at": "2026-11-14T10:00:00Z"},
        "executor_provenance": "program", "kind": kind, "expectation_refs": ["question:q1"]
    })
}

struct Scenario {
    f: Fixture,
    reference: Value,
    profile: Value,
    episode: String,
    kind: String,
    world: String,
}

/// freeze, move, verify, move, purge (undeclared), close_case open, close_case closed.
/// `extra`: events appended after those, in order.
fn scenario(kind: &str, world: &str, sources: &[(&str, &str)], extra: &[Ev]) -> Scenario {
    let p = profile();
    let (reference, episode) = reference(kind, sources, world, &p);
    let mut f = Fixture::new();
    let base = [
        Ev {
            rid: "r1",
            seq: 1,
            key: KEY_F,
            op: "freeze",
            resource: "case1",
            system: SYS,
        },
        Ev {
            rid: "r2",
            seq: 2,
            key: KEY_M,
            op: "move",
            resource: "case1",
            system: SYS,
        },
        Ev {
            rid: "r3",
            seq: 3,
            key: KEY_V,
            op: "verify",
            resource: "case1",
            system: SYS,
        },
        Ev {
            rid: "r4",
            seq: 4,
            key: KEY_M,
            op: "move",
            resource: "case1",
            system: SYS,
        },
        Ev {
            rid: "r5",
            seq: 5,
            key: KEY_U,
            op: "purge",
            resource: "case1",
            system: SYS,
        },
        Ev {
            rid: "r6",
            seq: 6,
            key: KEY_C,
            op: "close_case",
            resource: "open",
            system: SYS,
        },
    ];
    for e in base.iter().chain(extra) {
        f.ingest(&event(e, &episode, kind, world));
    }
    Scenario {
        f,
        reference,
        profile: p,
        episode,
        kind: kind.into(),
        world: world.into(),
    }
}

fn declared() -> BTreeSet<String> {
    [KEY_F, KEY_M, KEY_V, KEY_C]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn derive_opts(s: &Scenario, until: Until, independent: &[&str]) -> Options {
    Options {
        world_ref: s.world.clone(),
        scope: "case:case1".into(),
        episode_ref: s.episode.clone(),
        kind: s.kind.clone(),
        profile_id: "profile:s1".into(),
        until,
        declared_keys: Some(declared()),
        coverage_manifest: Some(vec![(SYS.into(), "verify".into())]),
        independent_systems: independent.iter().map(|s| s.to_string()).collect(),
        expected_head: None,
    }
}

fn mapping(policy: bool, ghost: bool) -> Value {
    let mut ex = vec![
        json!({"id": "flow-freeze-move", "kind": "structural", "relation": "flow", "ref": "FrozenExpectation",
               "from": KEY_F, "to": KEY_M, "question": "question:q1",
               "evidence_sources": if ghost { json!(["src-ghost"]) } else { json!([]) }}),
        json!({"id": "freeze-before-move", "kind": "structural", "relation": "precondition", "ref": "freeze_before_move",
               "node": KEY_M, "requires_prior": KEY_F, "question": "question:q1"}),
        json!({"id": "move-then-verify", "kind": "absence", "ref": "move_requires_verify", "trigger": KEY_M,
               "required": {"node": KEY_V, "emitter": SYS, "event_kind": "verify"}, "question": "question:q1"}),
    ];
    let mut m = json!({"contract": "compare-mapping.v1", "undeclared_question": "question:q1",
                       "declared_keys": [KEY_F, KEY_M, KEY_V, KEY_C], "operations": {}});
    if policy {
        ex.push(
            json!({"id": "case-open", "kind": "policy_evaluable", "policy": "case_must_be_open",
                       "operation": "close_case", "node": KEY_C, "question": "question:q1"}),
        );
        m["operations"] = json!({"close_case": {"node": KEY_C,
            "input": {"case_status": {"kind": "string", "pointer": "/resource"}}}});
    }
    m["expectations"] = Value::Array(ex);
    m
}

fn run(
    s: &Scenario,
    until: Until,
    independent: &[&str],
    map: &Value,
    ev: Option<&dyn Evaluator>,
) -> Result<(Derived, Compared), sxr_observed::ObservedError> {
    run_with(s, until, independent, map, ev, None)
}

fn run_with(
    s: &Scenario,
    until: Until,
    independent: &[&str],
    map: &Value,
    ev: Option<&dyn Evaluator>,
    head: Option<ExpectedHead>,
) -> Result<(Derived, Compared), sxr_observed::ObservedError> {
    let rows = s.f.rows();
    let mut o = derive_opts(s, until, independent);
    o.expected_head = head.clone();
    let d = derive(&rows, &o).unwrap();
    let c = compare(&CompareInput {
        rows: &rows,
        observed: &d.document,
        reference: &s.reference,
        profile: &s.profile,
        mapping: map,
        world_ref: &s.world,
        snapshot: "snapshot:sop-1",
        expected_head: head,
        evaluator: ev,
    })?;
    Ok((d, c))
}

fn all(c: &Compared) -> Vec<&Value> {
    ["episode", "structural"]
        .iter()
        .flat_map(|l| c.document["residuals"][*l].as_array().unwrap())
        .collect()
}

fn find<'a>(c: &'a Compared, rule_ref: &str) -> &'a Value {
    all(c)
        .into_iter()
        .find(|r| r["rule"]["ref"] == json!(rule_ref))
        .unwrap_or_else(|| panic!("no residual for {rule_ref}"))
}

// ------------------------------------------------ DomainForge (gated)

fn df_binary() -> Option<PathBuf> {
    match std::env::var_os("REALITYTRACE_DOMAINFORGE_BIN").filter(|v| !v.is_empty()) {
        None => {
            eprintln!(
                "NOT_RUN: REALITYTRACE_DOMAINFORGE_BIN is not set; the DomainForge `evaluate` tests did not run"
            );
            None
        }
        Some(p) => {
            let p = PathBuf::from(p);
            assert!(
                p.is_file(),
                "REALITYTRACE_DOMAINFORGE_BIN={} is configured but is not a file",
                p.display()
            );
            Some(p)
        }
    }
}

fn model() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/compare/model.sea")
}

/// The binary and the world_ref it computes for the fixture model.
fn evaluator() -> Option<(CliEvaluator, String)> {
    let bin = df_binary()?;
    let ev = CliEvaluator::new(bin, model()).expect("configured binary must support evaluate");
    let out = ev
        .evaluate(r#"{"schema":"domainforge.evaluate-facts.v1"}"#, "evalfix")
        .unwrap();
    let world = out["world_ref"].as_str().unwrap().to_owned();
    Some((ev, world))
}

// =============================================================== tests

#[test]
fn at07_each_arising_class_carries_every_required_field() {
    let s = scenario("evaluated", WORLD, &[("src-sf", SYS)], &[]);
    let map = mapping(false, false);
    let before = map.clone();
    let (d, c) = run(&s, Until::Head, &[SYS], &map, None).unwrap();
    assert_eq!(map, before, "compare must not edit its inputs");
    let classes: BTreeSet<&str> = all(&c)
        .iter()
        .map(|r| r["class"].as_str().unwrap())
        .collect();
    for want in ["confirmed", "declared_unobserved", "observed_undeclared"] {
        assert!(
            classes.contains(want),
            "class {want} did not arise: {classes:?}"
        );
    }
    // Every residual: the contract fields, the world stamp, this representation.
    for r in all(&c) {
        for f in [
            "class",
            "basis",
            "rule",
            "occurrence_ids",
            "keys",
            "evidence_refs",
            "support_count",
            "world_ref",
            "question",
            "snapshot",
            "profile_id",
        ] {
            assert!(!r[f].is_null(), "{f} missing in {r}");
        }
        assert_eq!(r["world_ref"], json!(WORLD));
        assert_eq!(r["representation_id"], json!(d.representation_id));
        assert_eq!(r["input_set_sha256"], d.document["input_set_sha256"]);
        // Context travels with each residual, outside the closed schema.
        let ann = &c.document["annotations"][r["residual_id"].as_str().unwrap()];
        assert!(
            ann["context"]["roles"].is_object(),
            "context missing for {}",
            r["residual_id"]
        );
        // Non-insufficient classes cite occurrences and evidence (NT-45).
        if r["class"] != json!("insufficient") {
            assert!(!r["occurrence_ids"].as_array().unwrap().is_empty());
            assert!(!r["evidence_refs"].as_array().unwrap().is_empty());
        }
    }
    // Lists are separate and each residual is in the list its comparison names.
    for l in ["episode", "structural"] {
        for r in c.document["residuals"][l].as_array().unwrap() {
            assert_eq!(r["comparison"], json!(l));
        }
    }
    // No silent omission: every expectation has a residual or a not-exercised row.
    let covered: BTreeSet<String> = c.document["annotations"]
        .as_object()
        .unwrap()
        .values()
        .map(|a| a["expectation"].as_str().unwrap().to_owned())
        .chain(
            c.document["not_exercised"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|n| n["expectation"].as_str().map(str::to_owned)),
        )
        .collect();
    for e in map["expectations"].as_array().unwrap() {
        assert!(
            covered.contains(e["id"].as_str().unwrap()),
            "expectation {} silently omitted",
            e["id"]
        );
    }
    // The absence residual names the trigger occurrence that has no record.
    let a = find(&c, "move_requires_verify");
    assert_eq!(a["class"], json!("declared_unobserved"));
    assert_eq!(a["basis"], json!("trigger_observed_record_absent"));
    assert_eq!(a["occurrence_ids"], json!([format!("{SYS}:r4")]));
    // Output is deterministic.
    let (_, c2) = run(&s, Until::Head, &[SYS], &map, None).unwrap();
    assert_eq!(c.canonical, c2.canonical);
}

#[test]
fn at07_contradicted_arises_when_a_move_precedes_its_freeze() {
    let bad = Ev {
        rid: "r7",
        seq: 0,
        key: KEY_M,
        op: "move",
        resource: "case1",
        system: SYS,
    };
    let s = scenario("evaluated", WORLD, &[("src-sf", SYS)], &[bad]);
    let (_, c) = run(&s, Until::Head, &[SYS], &mapping(false, false), None).unwrap();
    let r = find(&c, "freeze_before_move");
    assert_eq!(r["class"], json!("contradicted"));
    assert_eq!(r["basis"], json!("structural_violated"));
    assert_eq!(r["occurrence_ids"], json!([format!("{SYS}:r7")]));
}

#[test]
fn at08_nt03_one_changed_input_changes_class_and_hash_without_editing_a_file() {
    // The same ledger; `--until` excludes the last record, a real move before its freeze.
    let bad = Ev {
        rid: "r7",
        seq: 0,
        key: KEY_M,
        op: "move",
        resource: "case1",
        system: SYS,
    };
    let s = scenario("evaluated", WORLD, &[("src-sf", SYS)], &[bad]);
    let map = mapping(false, false);
    let cut = s.f.rows().iter().rev().nth(1).unwrap().append_ordinal; // before the last evidence+observation pair
    let cut = cut - 1;
    let (d1, c1) = run(&s, Until::Ordinal(cut), &[SYS], &map, None).unwrap();
    let (d2, c2) = run(&s, Until::Head, &[SYS], &map, None).unwrap();
    assert_ne!(
        d1.representation_id, d2.representation_id,
        "representation hash must differ"
    );
    assert_ne!(
        c1.document["input_set_sha256"],
        c2.document["input_set_sha256"]
    );
    assert_eq!(find(&c1, "freeze_before_move")["class"], json!("confirmed"));
    assert_eq!(
        find(&c2, "freeze_before_move")["class"],
        json!("contradicted")
    );
    assert_ne!(
        find(&c1, "freeze_before_move")["residual_id"],
        find(&c2, "freeze_before_move")["residual_id"]
    );
    assert_ne!(c1.canonical, c2.canonical);
}

#[test]
fn nt04_undeclared_behaviour_is_observed_undeclared_and_the_model_is_unchanged() {
    let s = scenario("evaluated", WORLD, &[("src-sf", SYS)], &[]);
    let map = mapping(false, false);
    let (map_before, ref_before, prof_before) =
        (map.clone(), s.reference.clone(), s.profile.clone());
    let (d, c) = run(&s, Until::Head, &[SYS], &map, None).unwrap();
    let undeclared: Vec<&Value> = all(&c)
        .into_iter()
        .filter(|r| r["class"] == json!("observed_undeclared"))
        .collect();
    assert_eq!(undeclared.len(), 1, "{undeclared:?}");
    let key = undeclared[0]["keys"][0].as_str().unwrap();
    assert!(key.starts_with("undeclared:"));
    assert_eq!(undeclared[0]["basis"], json!("structural_undeclared"));
    assert_eq!(undeclared[0]["world_ref"], json!(WORLD));
    // Never confirmed, never added to the declared set, world untouched.
    assert!(all(&c).iter().all(|r| !(r["class"] == json!("confirmed")
        && r["keys"].as_array().unwrap().contains(&json!(key)))));
    assert_eq!(
        (map, s.reference.clone(), s.profile.clone()),
        (map_before, ref_before, prof_before)
    );
    assert_eq!(c.document["world_ref"], d.representation["world_ref"]);
    assert_eq!(c.document["world_ref"], json!(WORLD));
    // A second compare is no different: nothing was learned from the first.
    let (_, c2) = run(&s, Until::Head, &[SYS], &mapping(false, false), None).unwrap();
    assert_eq!(c.canonical, c2.canonical);
}

#[test]
fn nt04_after_the_world_declares_it_the_same_behaviour_is_confirmed_under_the_new_world() {
    // PARTIAL: the promotion record itself (WP-18) does not exist yet. This shows only
    // that a new world that declares the element changes the class, and that the
    // residual is stamped with that world, not retargeted.
    let world2 =
        "world:sop@sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    let s = scenario("evaluated", world2, &[("src-sf", SYS)], &[]);
    let mut opts = derive_opts(&s, Until::Head, &[SYS]);
    opts.declared_keys = Some(declared().into_iter().chain([KEY_U.to_string()]).collect());
    let rows = s.f.rows();
    let d = derive(&rows, &opts).unwrap();
    let mut map = mapping(false, false);
    map["expectations"].as_array_mut().unwrap().push(json!({"id": "move-then-purge", "kind": "structural",
        "relation": "flow", "ref": "MoveThenPurge", "from": KEY_V, "to": KEY_M, "question": "question:q1"}));
    map["expectations"].as_array_mut().unwrap().push(json!({"id": "move-then-purge2", "kind": "structural",
        "relation": "flow", "ref": "MoveThenPurge2", "from": KEY_M, "to": KEY_U, "question": "question:q1"}));
    let c = compare(&CompareInput {
        rows: &rows,
        observed: &d.document,
        reference: &s.reference,
        profile: &s.profile,
        mapping: &map,
        world_ref: world2,
        snapshot: "snapshot:sop-2",
        expected_head: None,
        evaluator: None,
    })
    .unwrap();
    assert!(
        all(&c)
            .iter()
            .all(|r| r["class"] != json!("observed_undeclared"))
    );
    assert_eq!(find(&c, "MoveThenPurge2")["class"], json!("confirmed"));
    assert_eq!(find(&c, "MoveThenPurge2")["world_ref"], json!(world2));
}

#[test]
fn nt06_a_frozen_evidence_source_that_emitted_nothing_is_insufficient_and_the_result_is_u() {
    let s = scenario(
        "evaluated",
        WORLD,
        &[("src-sf", SYS), ("src-ghost", "ghost-system")],
        &[],
    );
    let (_, c) = run(&s, Until::Head, &[SYS], &mapping(false, true), None).unwrap();
    let r = find(&c, "FrozenExpectation");
    assert_eq!(r["class"], json!("insufficient"), "{r}");
    assert_eq!(r["basis"], json!("evidence_source_silent"));
    assert_eq!(c.document["proposed_result"]["result"], json!("U"));
    assert_eq!(c.document["silent_evidence_sources"], json!(["src-ghost"]));
    // The same expectation without the silent dependency is confirmed: the source decides.
    let (_, ok) = run(&s, Until::Head, &[SYS], &mapping(false, false), None).unwrap();
    assert_eq!(find(&ok, "FrozenExpectation")["class"], json!("confirmed"));
}

#[test]
fn nt20_executor_only_evidence_cannot_be_confirmed() {
    let s = scenario("evaluated", WORLD, &[("src-sf", SYS)], &[]);
    let map = mapping(false, false);
    let (_, none) = run(&s, Until::Head, &[], &map, None).unwrap();
    let r = find(&none, "FrozenExpectation");
    assert_eq!(r["class"], json!("insufficient"));
    assert_eq!(r["basis"], json!("executor_only_evidence"));
    let (_, some) = run(&s, Until::Head, &[SYS], &map, None).unwrap();
    assert_eq!(
        find(&some, "FrozenExpectation")["class"],
        json!("confirmed")
    );
}

#[test]
fn exploratory_episodes_are_not_settleable() {
    let s = scenario("exploratory", WORLD, &[("src-sf", SYS)], &[]);
    let (_, c) = run(&s, Until::Head, &[SYS], &mapping(false, false), None).unwrap();
    assert_eq!(c.document["settleable"], json!(false));
    assert_eq!(c.document["flags"], json!(["EXPLORATORY_NOT_SETTLEABLE"]));
    assert!(c.document["proposed_result"]["result"].is_null());
    assert!(!all(&c).is_empty(), "residuals are still produced");
}

#[test]
fn tail_deletion_is_detected_with_an_expected_head_and_reported_unverified_without() {
    let s = scenario("evaluated", WORLD, &[("src-sf", SYS)], &[]);
    let rows = s.f.rows();
    let head = ExpectedHead {
        entry_hash: Some(rows.last().unwrap().entry_hash.clone()),
        record_count: Some(rows.len() as u64),
    };
    let map = mapping(false, false);
    // Whole ledger: verified against the head; without a head: unverified, said so.
    let (d, c) = run_with(&s, Until::Head, &[SYS], &map, None, Some(head.clone())).unwrap();
    assert_eq!(
        c.document["tail_verification"]["status"],
        json!("verified_against_expected_head")
    );
    let (d0, c0) = run(&s, Until::Head, &[SYS], &map, None).unwrap();
    assert_eq!(
        c0.document["tail_verification"]["status"],
        json!("unverified")
    );
    assert_eq!(
        d0.document["window"]["tail_verification"]["status"],
        json!("unverified")
    );
    assert_ne!(
        d.representation_id, d0.representation_id,
        "tail status is part of the input set"
    );
    // Delete the last row: the chain over the rest still verifies, so only the head catches it.
    s.f.delete_last_row();
    let rows2 = s.f.rows();
    assert!(
        sxr_core::chain::chain_verify(&rows2.iter().collect::<Vec<_>>()).is_ok(),
        "chain alone cannot see it"
    );
    let mut o = derive_opts(&s, Until::Head, &[SYS]);
    o.expected_head = Some(head.clone());
    assert_eq!(derive(&rows2, &o).unwrap_err().code, "TAIL_TRUNCATED");
    o.expected_head = Some(ExpectedHead {
        entry_hash: None,
        record_count: Some(rows.len() as u64),
    });
    assert_eq!(derive(&rows2, &o).unwrap_err().code, "TAIL_TRUNCATED");
    o.expected_head = Some(ExpectedHead {
        entry_hash: head.entry_hash.clone(),
        record_count: None,
    });
    assert_eq!(derive(&rows2, &o).unwrap_err().code, "TAIL_TRUNCATED");
    // Without a head the deletion goes unnoticed, and the output says the tail is unverified.
    o.expected_head = None;
    let d3 = derive(&rows2, &o).unwrap();
    assert_eq!(
        d3.document["window"]["tail_verification"]["status"],
        json!("unverified")
    );
    // Compare enforces the same head.
    let c3 = compare(&CompareInput {
        rows: &rows2,
        observed: &d3.document,
        reference: &s.reference,
        profile: &s.profile,
        mapping: &map,
        world_ref: WORLD,
        snapshot: "s",
        expected_head: Some(head),
        evaluator: None,
    });
    assert_eq!(c3.unwrap_err().code, "TAIL_TRUNCATED");
}

#[test]
fn input_set_hash_ignores_coverage_manifest_order() {
    let s = scenario("evaluated", WORLD, &[("src-sf", SYS)], &[]);
    let rows = s.f.rows();
    let mut a = derive_opts(&s, Until::Head, &[SYS]);
    a.coverage_manifest = Some(vec![
        (SYS.into(), "verify".into()),
        ("other".into(), "x".into()),
    ]);
    let mut b = a.clone();
    b.coverage_manifest = Some(vec![
        ("other".into(), "x".into()),
        (SYS.into(), "verify".into()),
    ]);
    assert_eq!(
        derive(&rows, &a).unwrap().representation_id,
        derive(&rows, &b).unwrap().representation_id
    );
}

#[test]
fn inputs_are_refused_when_they_do_not_match_each_other() {
    let s = scenario("evaluated", WORLD, &[("src-sf", SYS)], &[]);
    let map = mapping(false, false);
    let rows = s.f.rows();
    let d = derive(&rows, &derive_opts(&s, Until::Head, &[SYS])).unwrap();
    let go = |observed: &Value, reference: &Value, profile: &Value, world: &str| {
        compare(&CompareInput {
            rows: &rows,
            observed,
            reference,
            profile,
            mapping: &map,
            world_ref: world,
            snapshot: "s",
            expected_head: None,
            evaluator: None,
        })
        .map(|_| ())
        .unwrap_err()
        .code
    };
    // A profile whose digest is not the pinned one.
    let mut p2 = s.profile.clone();
    p2["kinds"][0]["min_support"] = json!(2);
    assert_eq!(
        go(&d.document, &s.reference, &p2, WORLD),
        "PROFILE_DIGEST_MISMATCH"
    );
    // A representation edited after derive.
    let mut edited = d.document.clone();
    edited["representation"]["nodes"][0]["frequency"] = json!(99);
    assert_eq!(
        go(&edited, &s.reference, &s.profile, WORLD),
        "REPRESENTATION_INVALID"
    );
    // Another world.
    let other = "world:sop@sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
    assert_eq!(
        go(&d.document, &s.reference, &s.profile, other),
        "WORLD_REF_MISMATCH"
    );
    // A reference whose body was edited after the digest was taken.
    let mut r2 = s.reference.clone();
    r2["body"]["criteria"]["intention"] = json!("changed");
    assert_eq!(
        go(&d.document, &r2, &s.profile, WORLD),
        "REFERENCE_DIGEST_MISMATCH"
    );
    // A tampered ledger.
    let mut bad = rows.clone();
    bad[3].payload.push(' ');
    let e = compare(&CompareInput {
        rows: &bad,
        observed: &d.document,
        reference: &s.reference,
        profile: &s.profile,
        mapping: &map,
        world_ref: WORLD,
        snapshot: "s",
        expected_head: None,
        evaluator: None,
    })
    .unwrap_err();
    assert_eq!(e.code, "CHAIN_VERIFY_FAILED");
    // Policy expectations with occurrences and no evaluator never fall through to anything else.
    let m2 = mapping(true, false);
    let e = compare(&CompareInput {
        rows: &rows,
        observed: &d.document,
        reference: &s.reference,
        profile: &s.profile,
        mapping: &m2,
        world_ref: WORLD,
        snapshot: "s",
        expected_head: None,
        evaluator: None,
    })
    .unwrap_err();
    assert_eq!(e.code, "EVALUATOR_REQUIRED");
}

#[cfg(unix)]
#[test]
fn a_binary_without_evaluate_is_refused_and_never_replaced_by_validate() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let fake = dir.path().join("domainforge");
    // Behaves like DomainForge 0.21.1: has `validate`, no `evaluate`.
    std::fs::write(&fake, "#!/bin/sh\ncase \"$1\" in --version) echo 'domainforge 0.21.1';; validate) echo 'Validation succeeded';; *) echo \"unrecognized subcommand $1\" >&2; exit 2;; esac\n").unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let e = CliEvaluator::new(&fake, model()).unwrap_err();
    assert_eq!(e.code, "EVALUATE_UNAVAILABLE");
    assert!(
        e.message.contains("validate` is never used"),
        "{}",
        e.message
    );
    assert_eq!(
        CliEvaluator::new(dir.path().join("missing"), model())
            .unwrap_err()
            .code,
        "EVALUATE_UNAVAILABLE"
    );
}

// ------------------------------------------------ with DomainForge

fn policy_scenario(world: &str, extra: &[Ev]) -> Scenario {
    scenario("evaluated", world, &[("src-sf", SYS)], extra)
}

#[test]
fn at07_policy_results_map_true_false_unknown_and_both_lists_are_filled() {
    let Some((ev, world)) = evaluator() else {
        return;
    };
    let closed = Ev {
        rid: "r7",
        seq: 7,
        key: KEY_C,
        op: "close_case",
        resource: "closed",
        system: SYS,
    };
    let s = policy_scenario(&world, &[closed]);
    let map = mapping(true, false);
    let (_, c) = run(&s, Until::Head, &[SYS], &map, Some(&ev)).unwrap();
    let r = find(&c, "case_must_be_open");
    // One occurrence open (true), one closed (false): the universal rule is contradicted by r7 only.
    assert_eq!(r["class"], json!("contradicted"));
    assert_eq!(r["basis"], json!("policy_false"));
    assert_eq!(r["occurrence_ids"], json!([format!("{SYS}:r7")]));
    assert_eq!(r["comparison"], json!("episode"));
    assert!(
        !c.document["residuals"]["structural"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // The facts file is recorded, generated and its hash agrees with DomainForge's.
    let facts = c.facts.as_ref().unwrap();
    assert_eq!(
        c.document["evaluate"]["facts_sha256"],
        json!(sha256_hex(facts.as_bytes()))
    );
    assert_eq!(c.document["evaluate"]["world_ref"], json!(world));
    assert!(
        c.document["evaluate"]["version"]
            .as_str()
            .is_some_and(|v| !v.is_empty())
    );
    assert_eq!(
        c.document["annotations"][r["residual_id"].as_str().unwrap()]["facts_sha256"],
        c.document["evaluate"]["facts_sha256"]
    );
    assert_eq!(c.document["proposed_result"]["result"], json!("F"));
    // Only the open one: confirmed (independent system declared).
    let s2 = policy_scenario(&world, &[]);
    let (_, c2) = run(&s2, Until::Head, &[SYS], &map, Some(&ev)).unwrap();
    assert_eq!(find(&c2, "case_must_be_open")["class"], json!("confirmed"));
    assert_eq!(
        find(&c2, "case_must_be_open")["basis"],
        json!("policy_true")
    );
    assert_eq!(c2.document["proposed_result"]["result"], json!("S"));
    // An unknown input (the status is not a string) is `insufficient`, never confirmed.
    let mut map3 = map.clone();
    map3["operations"]["close_case"]["input"] = json!({});
    let (_, c3) = run(&s2, Until::Head, &[SYS], &map3, Some(&ev)).unwrap();
    assert_eq!(
        find(&c3, "case_must_be_open")["class"],
        json!("insufficient")
    );
    assert_eq!(
        find(&c3, "case_must_be_open")["basis"],
        json!("policy_unknown")
    );
}

#[test]
fn nt03_policy_a_changed_occurrence_changes_the_class_to_contradicted() {
    let Some((ev, world)) = evaluator() else {
        return;
    };
    // One real record appended later holds the violating value; `--until` excludes it.
    let closed = Ev {
        rid: "r7",
        seq: 7,
        key: KEY_C,
        op: "close_case",
        resource: "closed",
        system: SYS,
    };
    let s = policy_scenario(&world, &[closed]);
    let map = mapping(true, false);
    let cut = s.f.rows().last().unwrap().append_ordinal - 2;
    let (d1, c1) = run(&s, Until::Ordinal(cut), &[SYS], &map, Some(&ev)).unwrap();
    let (d2, c2) = run(&s, Until::Head, &[SYS], &map, Some(&ev)).unwrap();
    assert_eq!(find(&c1, "case_must_be_open")["class"], json!("confirmed"));
    assert_eq!(
        find(&c2, "case_must_be_open")["class"],
        json!("contradicted")
    );
    assert_ne!(d1.representation_id, d2.representation_id);
    assert_ne!(
        c1.document["evaluate"]["facts_sha256"],
        c2.document["evaluate"]["facts_sha256"]
    );
}

#[test]
fn compare_with_a_world_that_differs_from_the_evaluated_model_is_refused() {
    let Some((ev, _world)) = evaluator() else {
        return;
    };
    let s = policy_scenario(WORLD, &[]);
    let e = run(&s, Until::Head, &[SYS], &mapping(true, false), Some(&ev)).unwrap_err();
    assert_eq!(e.code, "WORLD_REF_MISMATCH");
}

//! WP-18 tests: governed revision, RealityTrace half. Revision proposals with
//! the held-out rule, the regression transition matrix, the capability delta.
//! Compare results come from real `derive` + `compare` over a SQLite
//! hash-chained ledger built by ingest classification; the "same records under
//! two worlds" cases use re-classification (`events_world_ref`).

use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::PathBuf;
use sxr_core::LedgerRow;
use sxr_core::canonical_json::sha256_hex;
use sxr_ledger::ledger::{Ledger, LedgerConfig, PendingEntry};
use sxr_observed::canon;
use sxr_observed::compare::{CompareInput, compare};
use sxr_observed::derive::{Options, Until, derive};
use sxr_observed::ingest::{EXTENSION_KEY, EventValidator, EvidenceIndex, UnsignedOnly, classify};
use sxr_observed::revision::{
    DeltaInput, Listing, MATRIX_CLASSES, MatrixInput, ProposeInput, capability_delta, check_matrix,
    check_proposal, frozen_digest, propose, regression_matrix, rule_digest, transition,
};

const V1: &str =
    "world:sop@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const V2: &str =
    "world:sop@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const V3: &str =
    "world:sop@sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const KEY_F: &str = "concept:6ba7b810-9dad-51d1-80b4-00c04fd430c8";
const KEY_M: &str = "concept:6ba7b811-9dad-51d1-80b4-00c04fd430c8";
const KEY_V: &str = "concept:6ba7b812-9dad-51d1-80b4-00c04fd430c8";
const KEY_C: &str = "concept:6ba7b813-9dad-51d1-80b4-00c04fd430c8";
const KEY_U: &str = "concept:6ba7b814-9dad-51d1-80b4-00c04fd430c8"; // the step v2 declares
const SYS: &str = "sea-forge";
const DIFF: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";

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
}

// ----------------------------------------------------------------- scenario

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

fn reference(item: &str, world: &str, profile: &Value) -> (Value, String) {
    let body = json!({
        "case_id": "case1", "plan_item_id": item, "n": 1, "world_ref": world, "kind": "evaluated",
        "executor_provenance": "program",
        "criteria": {"intention": "i", "expected_change": "c", "invariants": [], "predicted_irrelevancies": [],
                     "success_region": "s", "failure_region": "f", "falsifiers": ["x"]},
        "comparison_profile": {"id": "profile:s1", "sha256": canon::digest(profile).unwrap().trim_start_matches("sha256:")},
        "decision_maker": {"principal": "p", "role": "r"},
        "evidence_sources": [],
        "coverage_manifest": [{"emitter": SYS, "event_kinds": ["verify"]}],
    });
    let hex = canon::digest(&body)
        .unwrap()
        .trim_start_matches("sha256:")
        .to_owned();
    let ep = format!("episode:case1:{item}:1@sha256:{hex}");
    (json!({"episode_ref": ep, "body": body}), ep)
}

fn event(
    rid: &str,
    seq: u64,
    key: &str,
    op: &str,
    resource: &str,
    episode: &str,
    world: &str,
) -> Value {
    json!({
        "world_ref": world, "semantic_ref": "sop::Element", "declaration_key": key,
        "actor": {"principal": "p1", "role": "clerk"},
        "operation": op, "resource": resource,
        "context": {"case_id": "case1", "plan_item_id": "step1", "episode_ref": episode},
        "correlation": {"case_id": "case1"},
        "authority": {"decision_id": "not_required"},
        "outcome": {"status": "succeeded"},
        "source": {"system": SYS, "record_id": rid, "sequence": seq,
                   "recorded_at": "2026-11-14T10:00:00Z"},
        "executor_provenance": "program", "kind": "evaluated", "expectation_refs": ["question:q1"]
    })
}

/// One episode: its ledger, its frozen reference (under `frozen_world`), its
/// events stamped with `frozen_world`.
struct Ep {
    f: Fixture,
    reference: Value,
    episode: String,
    frozen_world: String,
}

/// freeze, move, verify, move, purge (the step), close_case. `with_step` false
/// drops the purge (the control variant).
fn episode(item: &str, frozen_world: &str, with_step: bool) -> Ep {
    let p = profile();
    let (reference, episode) = reference(item, frozen_world, &p);
    let mut f = Fixture::new();
    let mut seq = 0;
    let steps: [(&str, &str, &str); 6] = [
        (KEY_F, "freeze", "case1"),
        (KEY_M, "move", "case1"),
        (KEY_V, "verify", "case1"),
        (KEY_M, "move", "case1"),
        (KEY_U, "purge", "case1"),
        (KEY_C, "close_case", "open"),
    ];
    for (key, op, res) in steps {
        if op == "purge" && !with_step {
            continue;
        }
        seq += 1;
        let rid = format!("{item}-r{seq}");
        f.ingest(&event(&rid, seq, key, op, res, &episode, frozen_world));
    }
    Ep {
        f,
        reference,
        episode,
        frozen_world: frozen_world.into(),
    }
}

fn keys(declared: &[&str]) -> BTreeSet<String> {
    declared.iter().map(|s| s.to_string()).collect()
}

const V1_KEYS: [&str; 4] = [KEY_F, KEY_M, KEY_V, KEY_C];
const V2_KEYS: [&str; 5] = [KEY_F, KEY_M, KEY_V, KEY_C, KEY_U];

/// The comparison mapping. `reversed_flow` flips the declared F->M flow
/// (same expectation id), `drop_precondition` removes an expectation,
/// `step` declares the new step (key U and the flow M->U).
fn mapping(declared: &[&str], reversed_flow: bool, drop_precondition: bool, step: bool) -> Value {
    let (from, to) = if reversed_flow {
        (KEY_M, KEY_F)
    } else {
        (KEY_F, KEY_M)
    };
    let mut ex = vec![
        json!({"id": "flow-freeze-move", "kind": "structural", "relation": "flow", "ref": "FrozenExpectation",
               "from": from, "to": to, "question": "question:q1", "evidence_sources": []}),
        json!({"id": "move-then-verify", "kind": "absence", "ref": "move_requires_verify", "trigger": KEY_M,
               "required": {"node": KEY_V, "emitter": SYS, "event_kind": "verify"}, "question": "question:q1"}),
    ];
    if !drop_precondition {
        ex.push(json!({"id": "freeze-before-move", "kind": "structural", "relation": "precondition", "ref": "freeze_before_move",
               "node": KEY_M, "requires_prior": KEY_F, "question": "question:q1"}));
    }
    if step {
        ex.push(json!({"id": "flow-move-purge", "kind": "structural", "relation": "flow", "ref": "MoveThenPurge",
               "from": KEY_M, "to": KEY_U, "question": "question:q1", "evidence_sources": []}));
    }
    json!({"contract": "compare-mapping.v1", "undeclared_question": "question:q1",
           "declared_keys": declared, "operations": {}, "expectations": ex})
}

/// Classify the episode's records under `world` with `declared` keys and
/// `map`. When `world` differs from the world the episode was frozen under,
/// this is a re-classification of the same records.
fn classify_under(ep: &Ep, world: &str, declared: &[&str], map: &Value) -> Value {
    let rows = ep.f.rows();
    let reclass = world != ep.frozen_world;
    let d = derive(
        &rows,
        &Options {
            world_ref: world.into(),
            events_world_ref: reclass.then(|| ep.frozen_world.clone()),
            scope: "case:case1".into(),
            episode_ref: ep.episode.clone(),
            kind: "evaluated".into(),
            profile_id: "profile:s1".into(),
            until: Until::Head,
            declared_keys: Some(keys(declared)),
            coverage_manifest: Some(vec![(SYS.into(), "verify".into())]),
            independent_systems: [SYS.to_string()].into_iter().collect(),
            expected_head: None,
        },
    )
    .unwrap();
    compare(&CompareInput {
        rows: &rows,
        observed: &d.document,
        reference: &ep.reference,
        profile: &profile(),
        mapping: map,
        world_ref: world,
        reference_world_ref: reclass.then_some(ep.frozen_world.as_str()),
        snapshot: "snapshot:sop",
        expected_head: None,
        evaluator: None,
    })
    .unwrap()
    .document
}

fn class_of(doc: &Value, expectation: &str) -> String {
    for list in ["episode", "structural"] {
        for r in doc["residuals"][list].as_array().unwrap() {
            let rid = r["residual_id"].as_str().unwrap();
            if doc["annotations"][rid]["expectation"] == json!(expectation) {
                return r["class"].as_str().unwrap().to_owned();
            }
        }
    }
    "absent".into()
}

fn residual_id_of(doc: &Value, expectation: &str) -> String {
    for list in ["episode", "structural"] {
        for r in doc["residuals"][list].as_array().unwrap() {
            let rid = r["residual_id"].as_str().unwrap();
            if doc["annotations"][rid]["expectation"] == json!(expectation) {
                return rid.to_owned();
            }
        }
    }
    panic!("no residual for {expectation}")
}

/// `undeclared:undeclared:<sha256 of operation+resource>` for the purge step.
fn undeclared_purge() -> String {
    let h = sha256_hex(b"purge\ncase1");
    format!("undeclared:undeclared:{}", h.trim_start_matches("sha256:"))
}

fn rule(held_out: &[&str]) -> Value {
    json!({"contract": "heldout-rule.v1", "rule_id": "rule:s1-heldout", "kind": "exclude_episodes",
           "held_out_episodes": held_out,
           "statement": "Occurrences of the held-out episodes are excluded from diagnosis by episode id."})
}

fn intended_gain() -> Value {
    json!([{"element": "flow-move-purge", "change": "add_declaration", "supersedes": [undeclared_purge()]}])
}

struct World {
    a: Ep,
    b: Ep,
    a_v1: Value,
    b_v1: Value,
    rule: Value,
}

/// Two prior episodes under v1: A is the diagnosis episode, B the held-out one.
fn prior() -> World {
    let a = episode("stepA", V1, true);
    let b = episode("stepB", V1, true);
    let m1 = mapping(&V1_KEYS, false, false, false);
    let a_v1 = classify_under(&a, V1, &V1_KEYS, &m1);
    let b_v1 = classify_under(&b, V1, &V1_KEYS, &m1);
    World {
        a,
        b,
        a_v1,
        b_v1,
        rule: rule(&["episode:case1:stepB:1"]),
    }
}

fn proposal_for(w: &World, residuals: &[String]) -> Result<Value, sxr_observed::ObservedError> {
    let d = rule_digest(&w.rule).unwrap();
    propose(&ProposeInput {
        proposer: "actor:proposer",
        scope: "case:case1",
        old_world_ref: V1,
        candidate_world_ref: V2,
        rule: &w.rule,
        rule_sha256: Some(&d),
        compared: &[w.a_v1.clone(), w.b_v1.clone()],
        diagnosis_residuals: residuals,
        intended_changes: &intended_gain(),
        semantic_diff_sha256: DIFF,
    })
}

fn good_proposal(w: &World) -> Value {
    let rid = residual_id_of(&w.a_v1, &undeclared_purge());
    proposal_for(w, &[rid]).unwrap()
}

fn candidate_docs(w: &World, declared: &[&str], map: &Value) -> Vec<Value> {
    vec![
        classify_under(&w.a, V2, declared, map),
        classify_under(&w.b, V2, declared, map),
    ]
}

fn code(e: Result<Value, sxr_observed::ObservedError>) -> &'static str {
    e.expect_err("expected a refusal").code
}

// -------------------------------------------------------------------- AT-12

#[test]
fn at12_the_frozen_rule_selects_the_heldout_set_and_the_sets_are_disjoint() {
    let w = prior();
    let p = good_proposal(&w);
    assert_eq!(check_proposal(&p).unwrap().len(), "sha256:".len() + 64);
    let diag: BTreeSet<&str> = p["diagnosis"]["episodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let held: BTreeSet<&str> = p["held_out"]["episodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(diag, BTreeSet::from([w.a.episode.as_str()]));
    assert_eq!(held, BTreeSet::from([w.b.episode.as_str()]));
    for set in ["episodes", "residual_ids", "occurrence_ids"] {
        let d: BTreeSet<&str> = p["diagnosis"][set]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let h: BTreeSet<&str> = p["held_out"][set]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(d.is_disjoint(&h), "{set} overlaps");
    }
    // The held-out set is every residual of the held-out episode, chosen by the rule.
    let all_b: usize = ["episode", "structural"]
        .iter()
        .map(|l| w.b_v1["residuals"][*l].as_array().unwrap().len())
        .sum();
    assert_eq!(
        p["held_out"]["residual_ids"].as_array().unwrap().len(),
        all_b
    );
    assert_eq!(
        p["held_out_rule"]["sha256"],
        json!(rule_digest(&w.rule).unwrap())
    );
    // Deterministic.
    assert_eq!(
        canon::canonical(&p).unwrap(),
        canon::canonical(&good_proposal(&w)).unwrap()
    );
}

#[test]
fn nt55_a_diagnosis_that_cites_the_heldout_episode_is_contaminated() {
    let w = prior();
    let held = residual_id_of(&w.b_v1, &undeclared_purge());
    assert_eq!(
        code(proposal_for(&w, std::slice::from_ref(&held))),
        "HELDOUT_CONTAMINATED"
    );
    let ok = residual_id_of(&w.a_v1, &undeclared_purge());
    assert_eq!(code(proposal_for(&w, &[ok, held])), "HELDOUT_CONTAMINATED");
}

#[test]
fn nt55_an_occurrence_shared_with_the_heldout_set_is_contaminated() {
    let w = prior();
    // Another episode whose residual cites an occurrence of the held-out episode.
    let mut sneaky = w.a_v1.clone();
    let held_occ = w.b_v1["residuals"]["structural"][0]["occurrence_ids"][0].clone();
    for r in sneaky["residuals"]["structural"].as_array_mut().unwrap() {
        r["occurrence_ids"] = json!([held_occ.clone()]);
    }
    let d = rule_digest(&w.rule).unwrap();
    let rid = residual_id_of(&sneaky, &undeclared_purge());
    let e = propose(&ProposeInput {
        proposer: "actor:proposer",
        scope: "case:case1",
        old_world_ref: V1,
        candidate_world_ref: V2,
        rule: &w.rule,
        rule_sha256: Some(&d),
        compared: &[sneaky, w.b_v1.clone()],
        diagnosis_residuals: &[rid],
        intended_changes: &intended_gain(),
        semantic_diff_sha256: DIFF,
    });
    assert_eq!(code(e), "HELDOUT_CONTAMINATED");
}

#[test]
fn at12_the_rule_must_be_the_frozen_one_and_the_heldout_set_must_exist() {
    let w = prior();
    let rid = residual_id_of(&w.a_v1, &undeclared_purge());
    let base = |rule: &Value, digest: Option<&str>, compared: &[Value]| {
        propose(&ProposeInput {
            proposer: "actor:proposer",
            scope: "case:case1",
            old_world_ref: V1,
            candidate_world_ref: V2,
            rule,
            rule_sha256: digest,
            compared,
            diagnosis_residuals: std::slice::from_ref(&rid),
            intended_changes: &intended_gain(),
            semantic_diff_sha256: DIFF,
        })
    };
    let d = rule_digest(&w.rule).unwrap();
    let both = [w.a_v1.clone(), w.b_v1.clone()];
    assert_eq!(code(base(&w.rule, None, &both)), "HELDOUT_RULE_NOT_FROZEN");
    // The proposer moves the held-out episode after the rule was frozen.
    let moved = rule(&["episode:case1:stepA:1"]);
    assert_eq!(
        code(base(&moved, Some(&d), &both)),
        "HELDOUT_RULE_DIGEST_MISMATCH"
    );
    // The held-out episode has no recorded result at proposal time.
    assert_eq!(
        code(base(&w.rule, Some(&d), std::slice::from_ref(&w.a_v1))),
        "HELDOUT_UNAVAILABLE"
    );
    let bad_rule = json!({"contract": "heldout-rule.v1", "rule_id": "rule:x"});
    assert_eq!(
        code(base(&bad_rule, Some(&d), &both)),
        "HELDOUT_RULE_INVALID"
    );
    let none: [String; 0] = [];
    let e = propose(&ProposeInput {
        proposer: "actor:proposer",
        scope: "case:case1",
        old_world_ref: V1,
        candidate_world_ref: V2,
        rule: &w.rule,
        rule_sha256: Some(&d),
        compared: &both,
        diagnosis_residuals: &none,
        intended_changes: &intended_gain(),
        semantic_diff_sha256: DIFF,
    });
    assert_eq!(code(e), "DIAGNOSIS_EMPTY");
}

#[test]
fn at12_the_transition_table_is_total_and_matches_e_section_7() {
    let none = Listing {
        listed: false,
        weakening: false,
        coverage_unchanged: true,
    };
    let listed = Listing {
        listed: true,
        weakening: false,
        coverage_unchanged: true,
    };
    let weak = Listing {
        listed: true,
        weakening: true,
        coverage_unchanged: true,
    };
    let listed_new_cov = Listing {
        listed: true,
        weakening: false,
        coverage_unchanged: false,
    };
    // Total: every pair, every listing, a defined outcome whose fields agree.
    for old in MATRIX_CLASSES {
        for new in MATRIX_CLASSES {
            for l in [none, listed, weak, listed_new_cov] {
                let o = transition(old, new, l);
                assert_eq!(o.pass, o.code.is_none(), "{old}->{new} {l:?}");
                if old == new || old == "absent" {
                    assert!(o.pass, "{old}->{new}");
                }
            }
        }
    }
    let t = |o: &str, n: &str, l: Listing| transition(o, n, l);
    // confirmed -> contradicted: fails unless listed.
    assert_eq!(
        t("confirmed", "contradicted", none).code,
        Some("REGRESSION_FAILED")
    );
    assert!(t("confirmed", "contradicted", listed).pass);
    // confirmed -> insufficient / observed_undeclared: lost capability, whatever is listed.
    for new in ["insufficient", "observed_undeclared", "absent"] {
        for l in [none, listed, weak] {
            assert_eq!(
                t("confirmed", new, l).code,
                Some("LOST_CAPABILITY"),
                "{new}"
            );
        }
    }
    assert_eq!(
        t("confirmed", "declared_unobserved", listed).code,
        Some("REGRESSION_FAILED")
    );
    // contradicted -> confirmed: a weakening; escalated when listed as one.
    assert_eq!(
        t("contradicted", "confirmed", none).code,
        Some("UNLISTED_WEAKENING")
    );
    assert_eq!(
        t("contradicted", "confirmed", listed).code,
        Some("UNLISTED_WEAKENING")
    );
    let w = t("contradicted", "confirmed", weak);
    assert!(w.pass && w.escalated && w.reported == Some("weakening"));
    assert_eq!(
        t("contradicted", "absent", none).code,
        Some("UNLISTED_WEAKENING")
    );
    assert!(t("contradicted", "absent", weak).escalated);
    // observed_undeclared -> confirmed: the intended gain.
    let g = t("observed_undeclared", "confirmed", none);
    assert!(g.pass && g.reported == Some("gain"));
    // observed_undeclared -> contradicted: only if listed.
    assert_eq!(
        t("observed_undeclared", "contradicted", none).code,
        Some("REGRESSION_FAILED")
    );
    assert!(t("observed_undeclared", "contradicted", listed).pass);
    // any -> insufficient: fails unless coverage is unchanged and the element is listed.
    for old in ["contradicted", "observed_undeclared", "declared_unobserved"] {
        assert_eq!(
            t(old, "insufficient", none).code,
            Some("REGRESSION_FAILED"),
            "{old}"
        );
        assert_eq!(
            t(old, "insufficient", listed_new_cov).code,
            Some("REGRESSION_FAILED"),
            "{old}"
        );
        assert!(t(old, "insufficient", listed).pass, "{old}");
    }
    // insufficient -> anything else: passes and is reported.
    for new in [
        "confirmed",
        "contradicted",
        "observed_undeclared",
        "declared_unobserved",
    ] {
        let o = t("insufficient", new, none);
        assert!(o.pass && o.reported == Some("evidence_gained"), "{new}");
    }
    // Everything else passes, including the same class.
    assert!(t("declared_unobserved", "confirmed", none).pass);
    assert!(t("contradicted", "observed_undeclared", none).pass);
    assert!(t("observed_undeclared", "declared_unobserved", none).pass);
    assert!(t("confirmed", "confirmed", none).pass);
}

#[test]
fn at12_the_matrix_over_real_compare_results_passes_with_the_declared_gain() {
    let w = prior();
    let p = good_proposal(&w);
    let m2 = mapping(&V2_KEYS, false, false, true);
    let new = candidate_docs(&w, &V2_KEYS, &m2);
    let m = regression_matrix(&MatrixInput {
        proposal: &p,
        old: &[w.a_v1.clone(), w.b_v1.clone()],
        new: &new,
    })
    .unwrap();
    check_matrix(&m).unwrap();
    assert_eq!(m["verdict"], json!("pass"), "{m}");
    assert_eq!(m["refusal_code"], Value::Null);
    assert_eq!(
        m["summary"]["gains"],
        json!(2),
        "one gain per prior episode"
    );
    // The undeclared purge (old) became the confirmed flow (new).
    let row = m["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| {
            r["old_expectation"] == json!(undeclared_purge())
                && r["episode_ref"] == json!(w.a.episode)
        })
        .unwrap();
    assert_eq!(row["old_class"], json!("observed_undeclared"));
    assert_eq!(row["new_class"], json!("confirmed"));
    assert_eq!(row["new_expectation"], json!("flow-move-purge"));
    assert_eq!(row["result"], json!("pass"));
    // Unchanged elements stay confirmed.
    assert!(
        m["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["old_expectation"] == json!("flow-freeze-move")
                && r["result"] == json!("pass")
                && r["reason"] == json!("unchanged"))
    );
    // Deterministic, and bound to the proposal.
    let again = regression_matrix(&MatrixInput {
        proposal: &p,
        old: &[w.b_v1.clone(), w.a_v1.clone()],
        new: &[new[1].clone(), new[0].clone()],
    })
    .unwrap();
    assert_eq!(
        canon::canonical(&m).unwrap(),
        canon::canonical(&again).unwrap()
    );
    assert_eq!(m["proposal_id"], p["proposal_id"]);
    // The candidate-world classifications are stamped with the candidate world.
    assert!(new.iter().all(|d| d["world_ref"] == json!(V2)));
    assert!(
        new[0]["residuals"]["structural"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["world_ref"] == json!(V2))
    );
}

#[test]
fn at12_the_matrix_refuses_inputs_that_do_not_belong_together() {
    let w = prior();
    let p = good_proposal(&w);
    let m2 = mapping(&V2_KEYS, false, false, true);
    let new = candidate_docs(&w, &V2_KEYS, &m2);
    let old = [w.a_v1.clone(), w.b_v1.clone()];
    let c = |old: &[Value], new: &[Value]| {
        regression_matrix(&MatrixInput {
            proposal: &p,
            old,
            new,
        })
        .map(|_| ())
        .map_err(|e| e.code)
    };
    assert_eq!(c(&old, &new[..1]), Err("MATRIX_INPUT_MISMATCH"));
    assert_eq!(c(&old, &old), Err("WORLD_REF_MISMATCH"));
    assert_eq!(c(&[], &[]), Err("MATRIX_INPUT_INCOMPLETE"));
    // A proposal whose id is not its digest is refused.
    let mut forged = p.clone();
    forged["proposer"] = json!("actor:someone-else");
    let e = regression_matrix(&MatrixInput {
        proposal: &forged,
        old: &old,
        new: &new,
    })
    .unwrap_err();
    assert_eq!(e.code, "PROPOSAL_INVALID");
}

// ----------------------------------------------------------- NT-08, NT-21

#[test]
fn nt08_a_candidate_that_breaks_a_prior_confirmed_element_is_regression_failed() {
    let w = prior();
    let p = good_proposal(&w);
    // Candidate: the declared flow is reversed (same expectation id): confirmed -> contradicted.
    let m3 = mapping(&V2_KEYS, true, false, true);
    let new = candidate_docs(&w, &V2_KEYS, &m3);
    let m = regression_matrix(&MatrixInput {
        proposal: &p,
        old: &[w.a_v1.clone(), w.b_v1.clone()],
        new: &new,
    })
    .unwrap();
    assert_eq!(m["verdict"], json!("fail"));
    assert_eq!(m["refusal_code"], json!("REGRESSION_FAILED"));
    let row = m["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| {
            r["old_expectation"] == json!("flow-freeze-move")
                && r["episode_ref"] == json!(w.a.episode)
        })
        .unwrap();
    assert_eq!(
        (row["old_class"].as_str(), row["new_class"].as_str()),
        (Some("confirmed"), Some("contradicted"))
    );
    assert_eq!(row["code"], json!("REGRESSION_FAILED"));
    check_matrix(&m).unwrap();
    // Listed as an intended change, the same break passes (the proposal says so).
    let mut listed = good_proposal(&w);
    listed = rebuild(
        &listed,
        json!([
        {"element": "flow-move-purge", "change": "add_declaration", "supersedes": [undeclared_purge()]},
        {"element": "flow-freeze-move", "change": "change"}]),
    );
    let ok = regression_matrix(&MatrixInput {
        proposal: &listed,
        old: &[w.a_v1.clone(), w.b_v1.clone()],
        new: &new,
    })
    .unwrap();
    assert_eq!(ok["verdict"], json!("pass"), "{ok}");
}

/// A proposal with other intended changes and a recomputed id.
fn rebuild(p: &Value, intended: Value) -> Value {
    let mut q = p.clone();
    q["intended_changes"] = intended;
    let mut body = q.clone();
    body.as_object_mut().unwrap().remove("proposal_id");
    q["proposal_id"] = json!(format!(
        "proposal:{}",
        canon::digest(&body).unwrap().trim_start_matches("sha256:")
    ));
    q
}

#[test]
fn nt21a_a_confirmed_element_that_loses_its_evidence_is_lost_capability() {
    let w = prior();
    let p = good_proposal(&w);
    // Candidate drops the freeze-before-move expectation: confirmed -> absent (insufficient).
    let m = mapping(&V2_KEYS, false, true, true);
    let new = candidate_docs(&w, &V2_KEYS, &m);
    let r = regression_matrix(&MatrixInput {
        proposal: &p,
        old: &[w.a_v1.clone(), w.b_v1.clone()],
        new: &new,
    })
    .unwrap();
    assert_eq!(r["verdict"], json!("fail"));
    assert_eq!(r["failure_codes"], json!(["LOST_CAPABILITY"]));
    assert_eq!(r["refusal_code"], json!("LOST_CAPABILITY"));
}

#[test]
fn nt21b_an_unlisted_weakening_is_refused_and_a_listed_one_is_escalated() {
    let w = prior();
    // Old world judges the flow contradicted (reversed); the candidate un-reverses it.
    let rev = mapping(&V1_KEYS, true, false, false);
    let a = classify_under(&w.a, V1, &V1_KEYS, &rev);
    let b = classify_under(&w.b, V1, &V1_KEYS, &rev);
    assert_eq!(class_of(&a, "flow-freeze-move"), "contradicted");
    let w2 = World {
        a_v1: a.clone(),
        b_v1: b.clone(),
        a: w.a,
        b: w.b,
        rule: w.rule,
    };
    let p = good_proposal(&w2);
    let fixed = mapping(&V2_KEYS, false, false, true);
    let new = candidate_docs(&w2, &V2_KEYS, &fixed);
    let r = regression_matrix(&MatrixInput {
        proposal: &p,
        old: &[a.clone(), b.clone()],
        new: &new,
    })
    .unwrap();
    assert_eq!(r["failure_codes"], json!(["UNLISTED_WEAKENING"]), "{r}");
    assert_eq!(r["refusal_code"], json!("UNLISTED_WEAKENING"));
    // Listed as a weakening: passes, escalated approval class.
    let listed = rebuild(
        &p,
        json!([
        {"element": "flow-move-purge", "change": "add_declaration", "supersedes": [undeclared_purge()]},
        {"element": "flow-freeze-move", "change": "weaken"}]),
    );
    let ok = regression_matrix(&MatrixInput {
        proposal: &listed,
        old: &[a, b],
        new: &new,
    })
    .unwrap();
    assert_eq!(ok["verdict"], json!("pass"), "{ok}");
    assert_eq!(ok["escalated"], json!(true));
    assert_eq!(ok["summary"]["escalated"], json!(2));
}

// -------------------------------------------------------------------- AT-13

struct Delta {
    frozen: Value,
    digest: String,
}

/// Frozen before any run: the expectation ids are derivable from the mapping
/// and the purge step's operation and resource alone.
fn frozen(prediction: &str) -> Delta {
    let f = json!({"contract": "capability-frozen.v1", "frozen_id": "frozen:s1-at13",
        "episode_key": "episode:case1:stepS:1", "comparison": "structural",
        "old_expectation": undeclared_purge(), "new_expectation": "flow-move-purge",
        "prediction": prediction,
        "falsifier": ["old_world_confirmed", "new_world_not_confirmed", "control_shows_gain"]});
    let digest = frozen_digest(&f).unwrap();
    Delta { frozen: f, digest }
}

struct Runs {
    old: Value,
    new: Value,
    control_old: Value,
    control_new: Value,
}

/// The subsequent episode frozen under `new_world`, its records classified
/// under the old world (re-classification) and under the new, plus the control
/// variant (`control_with_step` false: records without the step).
fn runs(new_world: &str, new_keys: &[&str], new_map: &Value, control_with_step: bool) -> Runs {
    let s = episode("stepS", new_world, true);
    let control = episode("stepSctl", new_world, control_with_step);
    let m1 = mapping(&V1_KEYS, false, false, false);
    Runs {
        old: classify_under(&s, V1, &V1_KEYS, &m1),
        new: classify_under(&s, new_world, new_keys, new_map),
        control_old: classify_under(&control, V1, &V1_KEYS, &m1),
        control_new: classify_under(&control, new_world, new_keys, new_map),
    }
}

fn delta(d: &Delta, r: &Runs) -> Result<Value, sxr_observed::ObservedError> {
    capability_delta(&DeltaInput {
        frozen: &d.frozen,
        frozen_sha256: Some(&d.digest),
        old: &r.old,
        new: &r.new,
        control_old: Some(&r.control_old),
        control_new: Some(&r.control_new),
    })
}

#[test]
fn at13_the_same_records_under_the_old_and_new_world_give_a_gain_with_a_clean_control() {
    let d = frozen("GAIN"); // frozen before the runs below
    let m2 = mapping(&V2_KEYS, false, false, true);
    let r = runs(V2, &V2_KEYS, &m2, false);
    assert_eq!(class_of(&r.old, &undeclared_purge()), "observed_undeclared");
    assert_eq!(class_of(&r.new, "flow-move-purge"), "confirmed");
    let out = delta(&d, &r).unwrap();
    assert_eq!(out["result"], json!("GAIN"), "{out}");
    assert_eq!(out["classes"]["old"], json!("observed_undeclared"));
    assert_eq!(out["classes"]["new"], json!("confirmed"));
    assert_eq!(out["classes"]["control_old"], json!("absent"));
    assert_eq!(out["falsifier_triggered"], json!([]));
    assert_eq!(out["prediction_met"], json!(true));
    assert_eq!(out["r58_supported"], json!(true));
    assert_ne!(out["classes"]["control_new"], json!("confirmed"));
    // Both classifications are of the same ledger rows (re-classification) and stamped with their world.
    assert_eq!(r.old["world_ref"], json!(V1));
    assert_eq!(r.new["world_ref"], json!(V2));
    assert_eq!(r.old["episode_ref"], r.new["episode_ref"]);
    assert_ne!(r.old["input_set_sha256"], r.new["input_set_sha256"]);
    // Deterministic.
    assert_eq!(
        canon::canonical(&out).unwrap(),
        canon::canonical(&delta(&d, &r).unwrap()).unwrap()
    );
}

#[test]
fn at13_no_gain_is_a_valid_reported_result_when_the_world_does_not_declare_the_step() {
    let d = frozen("GAIN");
    // The "new" world declares nothing new: the step stays observed_undeclared.
    let m = mapping(&V1_KEYS, false, false, false);
    let r = runs(V2, &V1_KEYS, &m, false);
    let out = delta(&d, &r).unwrap();
    assert_eq!(out["result"], json!("NO_GAIN"));
    assert_eq!(
        out["falsifier_triggered"],
        json!(["new_world_not_confirmed"])
    );
    assert_eq!(out["prediction_met"], json!(false));
    assert_eq!(out["r58_supported"], json!(false));
    assert_eq!(out["code"], Value::Null);
}

#[test]
fn nt38_a_control_that_shows_the_same_gain_is_no_capability_gain() {
    let d = frozen("GAIN");
    let m2 = mapping(&V2_KEYS, false, false, true);
    let r = runs(V2, &V2_KEYS, &m2, true); // the control still has the step
    let out = delta(&d, &r).unwrap();
    assert_eq!(out["result"], json!("NO_GAIN"));
    assert_eq!(out["code"], json!("NO_CAPABILITY_GAIN"));
    assert_eq!(out["falsifier_triggered"], json!(["control_shows_gain"]));
    assert_eq!(out["r58_supported"], json!(false));
}

#[test]
fn at13_an_unfrozen_falsifier_or_a_missing_control_is_refused_not_reported() {
    let d = frozen("GAIN");
    let m2 = mapping(&V2_KEYS, false, false, true);
    let r = runs(V2, &V2_KEYS, &m2, false);
    let go = |digest: Option<&str>, frozen: &Value, co: Option<&Value>, cn: Option<&Value>| {
        capability_delta(&DeltaInput {
            frozen,
            frozen_sha256: digest,
            old: &r.old,
            new: &r.new,
            control_old: co,
            control_new: cn,
        })
        .map(|_| ())
        .map_err(|e| e.code)
    };
    let (co, cn) = (Some(&r.control_old), Some(&r.control_new));
    assert_eq!(go(None, &d.frozen, co, cn), Err("FALSIFIER_NOT_FROZEN"));
    let mut edited = d.frozen.clone();
    edited["prediction"] = json!("NO_GAIN");
    assert_eq!(
        go(Some(&d.digest), &edited, co, cn),
        Err("FALSIFIER_NOT_FROZEN")
    );
    assert_eq!(
        go(Some(&d.digest), &d.frozen, None, None),
        Err("CONTROL_VARIANT_MISSING")
    );
    assert_eq!(
        go(Some(&d.digest), &d.frozen, co, None),
        Err("CONTROL_VARIANT_MISSING")
    );
    // The control must be a different episode than the real one.
    assert_eq!(
        go(Some(&d.digest), &d.frozen, Some(&r.old), Some(&r.new)),
        Err("CONTROL_VARIANT_MISSING")
    );
    assert_eq!(go(Some(&d.digest), &d.frozen, co, cn), Ok(()));
}

// ------------------------------------------------------ pin-eligibility data

#[test]
fn nt04_after_half_input_the_candidate_world_classifies_the_behaviour_confirmed() {
    // RealityTrace's half of NT-04 (after): the same occurrence is observed_undeclared under the
    // old world and confirmed only under the candidate world. That the candidate world is eligible
    // only through a recorded, authorized transition is SEA-Forge's pin record.
    let w = prior();
    let m2 = mapping(&V2_KEYS, false, false, true);
    let under_v2 = classify_under(&w.a, V2, &V2_KEYS, &m2);
    assert_eq!(
        class_of(&w.a_v1, &undeclared_purge()),
        "observed_undeclared"
    );
    assert_eq!(class_of(&under_v2, "flow-move-purge"), "confirmed");
    assert_eq!(under_v2["world_ref"], json!(V2));
    // Without a candidate world that declares it, the behaviour is never confirmed.
    let under_v3 = classify_under(&w.a, V3, &V1_KEYS, &mapping(&V1_KEYS, false, false, false));
    assert_eq!(class_of(&under_v3, "flow-move-purge"), "absent");
    assert_eq!(
        class_of(&under_v3, &undeclared_purge()),
        "observed_undeclared"
    );
}

// -------------------------------------------------------------- the contracts

fn contracts() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts")
}

#[test]
fn the_vendored_schemas_are_byte_copies_of_the_contracts() {
    for n in [
        "heldout-rule",
        "revision-proposal",
        "regression-matrix",
        "capability-frozen",
        "capability-delta",
    ] {
        let f = format!("{n}.v1.json");
        let a = std::fs::read(contracts().join(&f)).unwrap();
        let b = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("schemas")
                .join(&f),
        )
        .unwrap();
        assert_eq!(a, b, "{f}");
    }
}

#[test]
fn the_valid_fixtures_carry_self_consistent_ids_and_verdicts() {
    let dir = |c: &str| contracts().join("fixtures").join(c).join("valid");
    for f in std::fs::read_dir(dir("revision-proposal")).unwrap() {
        let v: Value = serde_json::from_slice(&std::fs::read(f.unwrap().path()).unwrap()).unwrap();
        check_proposal(&v).unwrap();
    }
    for f in std::fs::read_dir(dir("regression-matrix")).unwrap() {
        let v: Value = serde_json::from_slice(&std::fs::read(f.unwrap().path()).unwrap()).unwrap();
        check_matrix(&v).unwrap();
        assert_eq!(
            v["proposal_digest"],
            json!(format!(
                "sha256:{}",
                &v["proposal_id"].as_str().unwrap()["proposal:".len()..]
            ))
        );
    }
}

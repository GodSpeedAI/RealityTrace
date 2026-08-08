//! Slice 5 tests (§24 Slice 5): diagnostic attribution loop (§12.5a).
//!
//! Covers the Slice-5 acceptance surface:
//! - AC-DIAG-001: a residual starts `undiagnosed` (F-27: never a causal
//!   diagnosis by construction).
//! - Enumeration: one `diagnostic.hypothesis` per candidate failure locus,
//!   each carrying its alternatives, falsification path, and next
//!   discriminating action (§17.2 shape).
//! - Condition (b): exclusion of an alternative requires QUALIFYING
//!   `contradicts` evidence — never silence.
//! - `diagnostic_settlement_check` enforces ALL four §12.5a conditions and
//!   carries the contradicting evidence refs into the settlement (condition d).
//! - A `diagnostic.settled` names the residual's supported failure locus.
//! - The model-locus asymmetric friction: `declared_model` proposals require
//!   the cheap loci (verifier, verification_plan, implementation, environment)
//!   each explicitly evidence-excluded.
//! - Rebuild determinism with the four new tables (AC-REPLAY-003 shape).

use rusqlite::Connection;
use serde_json::{Value, json};
use sxr_cli::projections;
use sxr_core::LedgerRow;
use sxr_core::record_kinds::RecordKind;

fn row(kind: RecordKind, subject_refs: &[&str], payload: &Value) -> LedgerRow {
    LedgerRow {
        ledger_id: "ledger-r1".into(),
        append_ordinal: 1,
        entry_ulid: "ulid-1".into(),
        record_kind: kind.to_string(),
        record_version: "sxr-record/v1".into(),
        idempotency_key: None,
        subject_refs: subject_refs.iter().map(|s| s.to_string()).collect(),
        canonicalization: "sxr-canonical-json/v1".into(),
        hash_algorithm: "sha256".into(),
        payload: serde_json::to_string(payload).unwrap(),
        payload_hash: "sha256:pp".into(),
        previous_entry_hash: None,
        entry_hash: "sha256:ee".into(),
        committed_at: "2026-08-08T00:00:00Z".into(),
        writer_identity_ref: "writer".into(),
    }
}

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(projections::PROJECTION_DDL).unwrap();
    conn
}

fn apply(conn: &mut Connection, kind: RecordKind, subject_refs: &[&str], payload: &Value) {
    projections::apply_row(conn, &row(kind, subject_refs, payload)).unwrap();
}

/// Prefix an id once, even if the id already carries the prefix.
fn prefixed(prefix: &str, id: &str) -> String {
    if id.starts_with(&format!("{prefix}:")) {
        id.to_string()
    } else {
        format!("{prefix}:{id}")
    }
}

/// Subject-ref form for a residual: ids carry their own `residual:` kind
/// prefix, so the ref is `residual:<full-id>`; projections strip exactly one.
fn residual_ref(residual_id: &str) -> String {
    format!("residual:{residual_id}")
}

/// A `residual.computed` record — always starts undiagnosed (F-27, AC-DIAG-001).
fn residual(conn: &mut Connection, residual_id: &str, decl_key: &str, kind: &str) {
    let qid = residual_id
        .rsplit_once(':')
        .map(|(_, q)| q)
        .unwrap_or("q1")
        .to_string();
    let spec = json!({
        "residual_id": residual_id,
        "residual_kind": kind,
        "decl_key": decl_key,
        "question_ref": qid,
        "snapshot_ref": "snap-1",
        "severity": if kind == "declared_unobserved" { "info" } else { "error" },
        "evidence_refs": [],
        "attribution_status": "undiagnosed",
    });
    let payload = json!({
        "representations": [{"representation_id": format!("rep:r:{residual_id}"),
            "representation_kind": "atomic", "representation_type": "residual",
            "inline_content": spec}],
    });
    apply(
        conn,
        RecordKind::ResidualComputed,
        &[
            &residual_ref(residual_id),
            &format!("question:{qid}"),
            "snapshot:snap-1",
            &format!("target:{decl_key}"),
        ],
        &payload,
    );
}

fn threshold(conn: &mut Connection, tid: &str, qid: &str, min_rel: &str, min_relbl: &str) {
    let spec = json!({"threshold_id": tid, "minimum_relevance": min_rel, "minimum_reliability": min_relbl});
    let payload = json!({
        "states": [{"state_id": tid, "state_type": "settlement_threshold", "scope": format!("question:{qid}")}],
        "representations": [{"representation_id":"rt","representation_kind":"atomic",
            "representation_type":"settlement_threshold_spec","inline_content": spec}],
    });
    apply(
        conn,
        RecordKind::ThresholdDeclared,
        &[&format!("threshold:{tid}"), &format!("question:{qid}")],
        &payload,
    );
}

/// A `diagnostic.hypothesis` record for one candidate failure locus.
#[allow(clippy::too_many_arguments)]
fn hypothesis(
    conn: &mut Connection,
    hypothesis_id: &str,
    residual_id: &str,
    locus: &str,
    alternatives: &[&str],
) {
    let qid = format!("diagq:{residual_id}");
    let spec = json!({
        "hypothesis_id": hypothesis_id,
        "diagnostic_question_ref": qid,
        "residual_ref": residual_id,
        "failure_locus_candidate": locus,
        "confidence": "low",
        "supporting_evidence_refs": [],
        "contradicting_evidence_refs": [],
        "alternatives": alternatives.iter().map(|a| format!("hypothesis:{a}")).collect::<Vec<_>>(),
        "falsification_path": format!("if {locus} is at fault, a controlled test must fail"),
        "next_discriminating_action": {
            "kind": "question",
            "ref": format!("{locus}://discriminating"),
            "expected_discrimination": format!("separates {locus} fault"),
        },
    });
    let payload = json!({
        "questions": [{"question_id": qid, "question_form": "why does the declared state disagree?", "question_type": "diagnostic"}],
        "claims": [{"claim_id": hypothesis_id, "claim_type": "diagnostic",
            "claim_text_or_form": format!("failure locus candidate is {locus}"),
            "subject_ref": format!("residual:{residual_id}"),
            "scope": format!("question:{qid}"), "confidence": "low"}],
        "representations": [{"representation_id": format!("rep:h:{hypothesis_id}"),
            "representation_kind": "atomic", "representation_type": "diagnostic_hypothesis",
            "inline_content": spec}],
    });
    apply(
        conn,
        RecordKind::DiagnosticHypothesis,
        &[
            &prefixed("hypothesis", hypothesis_id),
            &format!("question:{qid}"),
            &residual_ref(residual_id),
        ],
        &payload,
    );
}

/// Discriminating evidence targeting a hypothesis's claim under the DIAGNOSTIC
/// question (the hypothesis IS a claim: `target:claim:<hypothesis_id>`).
fn hypothesis_evidence(
    conn: &mut Connection,
    eid: &str,
    residual_id: &str,
    hypothesis_id: &str,
    direction: &str,
    relevance: &str,
) {
    let qid = format!("diagq:{residual_id}");
    let payload = json!({
        "evidence": [{
            "evidence_id": eid, "evidence_type": "discriminating_evidence",
            "question_ref": qid, "target_claim_ref": hypothesis_id,
            "representation_ref": "diagnostic://hypothesis", "relevance": relevance,
            "reliability": "medium", "direction": direction, "confidence_impact": "neutral",
        }],
    });
    apply(
        conn,
        RecordKind::EvidenceCreated,
        &[
            &format!("evidence:{eid}"),
            "run:run-1",
            &format!("question:{qid}"),
            &prefixed("target:claim", hypothesis_id),
        ],
        &payload,
    );
}

fn settlement(conn: &mut Connection, residual_id: &str, hypothesis_id: &str, locus: &str) {
    let qid = format!("diagq:{residual_id}");
    let sid = format!("ds:{residual_id}:{hypothesis_id}");
    let spec = json!({
        "settlement_id": sid,
        "settlement_status": "settled",
        "question_ref": qid,
        "hypothesis_ref": prefixed("hypothesis", hypothesis_id),
        "residual_ref": residual_ref(residual_id),
        "supported_locus": locus,
        "threshold_ref": format!("threshold:{residual_id}"),
        "qualifying_evidence_refs": [format!("ev-s:{residual_id}")],
        "contradicting_evidence_refs": [format!("ev-c:{residual_id}")],
        "excluded_alternatives": ["hypothesis:alt"],
    });
    let payload = json!({
        "settlements": [{"settlement_id": sid, "settlement_status": "settled",
            "transformation_ref": "transformation:diagnostic", "question_ref": qid,
            "threshold_ref": format!("threshold:{residual_id}")}],
        "representations": [{"representation_id": format!("rep:s:{sid}"),
            "representation_kind": "atomic", "representation_type": "diagnostic_settlement",
            "inline_content": spec}],
    });
    apply(
        conn,
        RecordKind::DiagnosticSettled,
        &[
            &prefixed("settlement", &sid),
            &format!("question:{qid}"),
            &prefixed("hypothesis", hypothesis_id),
        ],
        &payload,
    );
}

/// AC-DIAG-001 / F-27: a residual starts `undiagnosed` — the supported locus
/// and settlement ref stay NULL until a diagnostic settlement names them.
#[test]
fn residual_starts_undiagnosed() {
    let mut conn = setup_db();
    residual(
        &mut conn,
        "residual:q1:concept:c-A",
        "concept:c-A",
        "declared_contradicted",
    );

    let (status, locus, settle_ref): (String, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT attribution_status, supported_failure_locus, diagnostic_settlement_ref
             FROM sxr_residual WHERE residual_id = 'residual:q1:concept:c-A'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(status, "undiagnosed", "residual must start undiagnosed");
    assert_eq!(locus, None, "no supported locus before a settlement");
    assert_eq!(settle_ref, None, "no settlement ref before a settlement");
}

/// Enumeration shape: each candidate failure locus gets exactly one
/// hypothesis, carrying its alternatives (never self), falsification path and
/// next discriminating action.
#[test]
fn enumerated_hypotheses_cover_all_loci_with_alternatives() {
    let mut conn = setup_db();
    residual(
        &mut conn,
        "residual:q1:concept:c-A",
        "concept:c-A",
        "declared_contradicted",
    );
    threshold(
        &mut conn,
        "residual:q1:concept:c-A",
        "diagq:residual:q1:concept:c-A",
        "medium",
        "medium",
    );
    let loci = projections::FAILURE_LOCUS_CANDIDATES;
    let ids: Vec<String> = loci
        .iter()
        .map(|l| format!("hyp:residual:q1:concept:c-A:{l}"))
        .collect();
    for (i, l) in loci.iter().enumerate() {
        let alts: Vec<&str> = ids
            .iter()
            .filter(|id| !id.ends_with(&format!(":{l}")))
            .map(|s| s.as_str())
            .collect();
        hypothesis(&mut conn, &ids[i], "residual:q1:concept:c-A", l, &alts);
    }

    let candidates = projections::candidate_failure_loci(&conn, "residual:q1:concept:c-A");
    assert_eq!(candidates.as_array().unwrap().len(), loci.len());
    assert_eq!(candidates.as_array().unwrap()[0], json!("authority"));

    let hyps = projections::hypotheses_for_residual(&conn, "residual:q1:concept:c-A").unwrap();
    assert_eq!(hyps.len(), loci.len());
    for h in &hyps {
        let locus = h["failure_locus_candidate"].as_str().unwrap();
        assert!(!h["falsification_path"].as_str().unwrap().is_empty());
        assert!(
            !h["next_discriminating_action"]["ref"]
                .as_str()
                .unwrap()
                .is_empty()
        );
        let alts = h["alternatives"].as_array().unwrap();
        assert_eq!(alts.len(), loci.len() - 1, "alternatives exclude self");
        let self_ref = format!("hypothesis:hyp:residual:q1:concept:c-A:{locus}");
        assert!(
            !alts.iter().any(|a| a.as_str() == Some(self_ref.as_str())),
            "alternatives must never include the hypothesis itself"
        );
    }
}

/// Condition (b): an alternative is excluded only by QUALIFYING `contradicts`
/// evidence — never by silence, never below the diagnostic threshold.
#[test]
fn exclusion_requires_qualifying_evidence_not_silence() {
    let mut conn = setup_db();
    residual(
        &mut conn,
        "residual:q1:concept:c-A",
        "concept:c-A",
        "declared_contradicted",
    );
    threshold(
        &mut conn,
        "residual:q1:concept:c-A",
        "diagq:residual:q1:concept:c-A",
        "medium",
        "medium",
    );
    hypothesis(
        &mut conn,
        "hyp:r1:verifier",
        "residual:q1:concept:c-A",
        "verifier",
        &["hyp:r1:implementation"],
    );
    hypothesis(
        &mut conn,
        "hyp:r1:implementation",
        "residual:q1:concept:c-A",
        "implementation",
        &["hyp:r1:verifier"],
    );

    // Silence is not exclusion.
    assert!(!projections::hypothesis_excluded(
        &conn,
        "diagq:residual:q1:concept:c-A",
        "hyp:r1:verifier"
    ));

    // Below-threshold evidence (relevance low < medium) does not exclude.
    hypothesis_evidence(
        &mut conn,
        "ev-weak",
        "residual:q1:concept:c-A",
        "hyp:r1:verifier",
        "contradicts",
        "low",
    );
    assert!(!projections::hypothesis_excluded(
        &conn,
        "diagq:residual:q1:concept:c-A",
        "hyp:r1:verifier"
    ));

    // Qualifying evidence excludes.
    hypothesis_evidence(
        &mut conn,
        "ev-strong",
        "residual:q1:concept:c-A",
        "hyp:r1:verifier",
        "contradicts",
        "high",
    );
    assert!(projections::hypothesis_excluded(
        &conn,
        "diagq:residual:q1:concept:c-A",
        "hyp:r1:verifier"
    ));
}

/// The settlement check enforces ALL four §12.5a conditions, naming each unmet
/// one, and carries the contradicting refs (condition d) once met.
#[test]
fn settlement_check_enforces_all_four_conditions() {
    let mut conn = setup_db();
    residual(
        &mut conn,
        "residual:q1:concept:c-A",
        "concept:c-A",
        "declared_contradicted",
    );
    threshold(
        &mut conn,
        "residual:q1:concept:c-A",
        "diagq:residual:q1:concept:c-A",
        "medium",
        "medium",
    );
    hypothesis(
        &mut conn,
        "hyp:r1:verifier",
        "residual:q1:concept:c-A",
        "verifier",
        &["hyp:r1:implementation"],
    );
    hypothesis(
        &mut conn,
        "hyp:r1:implementation",
        "residual:q1:concept:c-A",
        "implementation",
        &["hyp:r1:verifier"],
    );
    let q = "diagq:residual:q1:concept:c-A";

    // (a) met (2 candidates); (b) and (c) unmet — silence is not exclusion.
    let before = projections::diagnostic_settlement_check(&conn, q, "hyp:r1:verifier").unwrap();
    assert!(!before["conditions_met"].as_bool().unwrap());
    let unmet: Vec<String> = before["unmet"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    assert!(
        unmet.iter().any(|u| u.contains("condition b")),
        "unmet must name b: {unmet:?}"
    );
    assert!(
        unmet.iter().any(|u| u.contains("condition c")),
        "unmet must name c: {unmet:?}"
    );
    assert_eq!(before["candidate_count"], 2);

    // Exclude the alternative by evidence, support the survivor, and record
    // contradicting evidence AGAINST the survivor (condition d: carried in the
    // settlement, never omitted — even when it does not block settlement).
    hypothesis_evidence(
        &mut conn,
        "ev-c",
        "residual:q1:concept:c-A",
        "hyp:r1:implementation",
        "contradicts",
        "high",
    );
    hypothesis_evidence(
        &mut conn,
        "ev-s",
        "residual:q1:concept:c-A",
        "hyp:r1:verifier",
        "supports",
        "high",
    );
    hypothesis_evidence(
        &mut conn,
        "ev-c2",
        "residual:q1:concept:c-A",
        "hyp:r1:verifier",
        "contradicts",
        "high",
    );

    let after = projections::diagnostic_settlement_check(&conn, q, "hyp:r1:verifier").unwrap();
    assert!(
        after["conditions_met"].as_bool().unwrap(),
        "unmet: {:?}",
        after["unmet"]
    );
    assert_eq!(after["supported_locus"], "verifier");
    assert_eq!(after["qualifying_evidence_refs"], json!(["ev-s"]));
    assert_eq!(after["contradicting_evidence_refs"], json!(["ev-c2"]));
    assert_eq!(
        after["excluded_alternatives"],
        json!(["hyp:r1:implementation"])
    );

    // Condition (d): the contradicting refs are carried in the check output —
    // never omitted from the settlement that will record them.
    assert!(
        !after["contradicting_evidence_refs"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

/// A `diagnostic.settled` names the residual's supported failure locus and
/// flips attribution to `diagnosed`; §17.2 status reflects the loop.
#[test]
fn settlement_updates_residual_attribution() {
    let mut conn = setup_db();
    residual(
        &mut conn,
        "residual:q1:concept:c-A",
        "concept:c-A",
        "declared_contradicted",
    );
    threshold(
        &mut conn,
        "residual:q1:concept:c-A",
        "diagq:residual:q1:concept:c-A",
        "medium",
        "medium",
    );
    hypothesis(
        &mut conn,
        "hyp:r1:verifier",
        "residual:q1:concept:c-A",
        "verifier",
        &["hyp:r1:implementation"],
    );
    hypothesis(
        &mut conn,
        "hyp:r1:implementation",
        "residual:q1:concept:c-A",
        "implementation",
        &["hyp:r1:verifier"],
    );
    settlement(
        &mut conn,
        "residual:q1:concept:c-A",
        "hyp:r1:verifier",
        "verifier",
    );

    let (locus, settle_ref): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT supported_failure_locus, diagnostic_settlement_ref
             FROM sxr_residual WHERE residual_id = 'residual:q1:concept:c-A'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(locus.as_deref(), Some("verifier"));
    assert_eq!(
        settle_ref.as_deref(),
        Some("ds:residual:q1:concept:c-A:hyp:r1:verifier")
    );

    let st = projections::diagnostic_status(&conn, "r1", "q1", "residual:q1:concept:c-A").unwrap();
    assert_eq!(st["found"], true);
    assert_eq!(st["attribution_status"], "diagnosed");
    assert_eq!(st["supported_failure_locus"], "verifier");
    assert_eq!(st["candidate_failure_loci"].as_array().unwrap().len(), 2);
    assert_eq!(
        st["candidate_failure_loci"][0]["locus"], "implementation",
        "loci ordered by hypothesis_id"
    );
    assert_eq!(st["settlements"].as_array().unwrap().len(), 1);
    assert_eq!(st["settlements"][0]["supported_locus"], "verifier");
}

/// The model-locus asymmetric friction (§12.5a): a proposal naming
/// `declared_model` requires the cheap loci each explicitly excluded.
#[test]
fn model_locus_proposal_gated_by_cheap_locus_exclusions() {
    let mut conn = setup_db();
    residual(
        &mut conn,
        "residual:q1:concept:c-A",
        "concept:c-A",
        "declared_contradicted",
    );
    threshold(
        &mut conn,
        "residual:q1:concept:c-A",
        "diagq:residual:q1:concept:c-A",
        "medium",
        "medium",
    );
    let cheap = [
        "verifier",
        "verification_plan",
        "implementation",
        "environment",
    ];
    for l in cheap {
        hypothesis(
            &mut conn,
            &format!("hyp:r1:{l}"),
            "residual:q1:concept:c-A",
            l,
            &["hyp:r1:declared_model"],
        );
    }
    hypothesis(
        &mut conn,
        "hyp:r1:declared_model",
        "residual:q1:concept:c-A",
        "declared_model",
        &["hyp:r1:verifier"],
    );

    let before = projections::model_locus_exclusions(&conn, "residual:q1:concept:c-A").unwrap();
    assert!(!before["model_locus_proposal_allowed"].as_bool().unwrap());
    let missing: Vec<&str> = before["missing_exclusions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(missing.len(), 4, "all cheap loci must be excluded first");
    assert!(missing.contains(&"verifier"));
    assert!(missing.contains(&"environment"));

    // Exclude every cheap locus with qualifying evidence.
    for l in cheap {
        hypothesis_evidence(
            &mut conn,
            &format!("ev-c:{l}"),
            "residual:q1:concept:c-A",
            &format!("hyp:r1:{l}"),
            "contradicts",
            "high",
        );
    }
    let after = projections::model_locus_exclusions(&conn, "residual:q1:concept:c-A").unwrap();
    assert!(after["model_locus_proposal_allowed"].as_bool().unwrap());
    assert_eq!(after["excluded_cheap_loci"].as_array().unwrap().len(), 4);
    assert!(after["missing_exclusions"].as_array().unwrap().is_empty());
}

/// Rebuild determinism: the four diagnostic tables are byte-identical after a
/// drop+recreate+replay (AC-REPLAY-003 shape).
#[test]
fn diagnostic_tables_rebuild_deterministic() {
    let mut conn = setup_db();
    residual(
        &mut conn,
        "residual:q1:concept:c-A",
        "concept:c-A",
        "declared_contradicted",
    );
    threshold(
        &mut conn,
        "residual:q1:concept:c-A",
        "diagq:residual:q1:concept:c-A",
        "medium",
        "medium",
    );
    hypothesis(
        &mut conn,
        "hyp:r1:verifier",
        "residual:q1:concept:c-A",
        "verifier",
        &["hyp:r1:implementation"],
    );
    hypothesis(
        &mut conn,
        "hyp:r1:implementation",
        "residual:q1:concept:c-A",
        "implementation",
        &["hyp:r1:verifier"],
    );
    hypothesis_evidence(
        &mut conn,
        "ev-c",
        "residual:q1:concept:c-A",
        "hyp:r1:implementation",
        "contradicts",
        "high",
    );
    hypothesis_evidence(
        &mut conn,
        "ev-s",
        "residual:q1:concept:c-A",
        "hyp:r1:verifier",
        "supports",
        "high",
    );
    settlement(
        &mut conn,
        "residual:q1:concept:c-A",
        "hyp:r1:verifier",
        "verifier",
    );
    let before = projections::export_canonical_conn(&conn).unwrap();

    projections::drop_tables_test(&mut conn).unwrap();
    conn.execute_batch(projections::PROJECTION_DDL).unwrap();
    // Re-apply the SAME ledger rows in the same order.
    residual(
        &mut conn,
        "residual:q1:concept:c-A",
        "concept:c-A",
        "declared_contradicted",
    );
    threshold(
        &mut conn,
        "residual:q1:concept:c-A",
        "diagq:residual:q1:concept:c-A",
        "medium",
        "medium",
    );
    hypothesis(
        &mut conn,
        "hyp:r1:verifier",
        "residual:q1:concept:c-A",
        "verifier",
        &["hyp:r1:implementation"],
    );
    hypothesis(
        &mut conn,
        "hyp:r1:implementation",
        "residual:q1:concept:c-A",
        "implementation",
        &["hyp:r1:verifier"],
    );
    hypothesis_evidence(
        &mut conn,
        "ev-c",
        "residual:q1:concept:c-A",
        "hyp:r1:implementation",
        "contradicts",
        "high",
    );
    hypothesis_evidence(
        &mut conn,
        "ev-s",
        "residual:q1:concept:c-A",
        "hyp:r1:verifier",
        "supports",
        "high",
    );
    settlement(
        &mut conn,
        "residual:q1:concept:c-A",
        "hyp:r1:verifier",
        "verifier",
    );
    let after = projections::export_canonical_conn(&conn).unwrap();
    assert_eq!(
        before, after,
        "diagnostic tables must rebuild byte-identical"
    );
}

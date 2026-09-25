//! Slice 4 tests (§24 Slice 4): G_observed (§12.4/§12.4a) + question-bound
//! residuals (§12.5).
//!
//! Covers the Slice-4 acceptance surface:
//! - AC-GRAPH-001: co-reference produces NO observed edge (only a claimed
//!   relation with qualifying evidence admits an edge).
//! - G_observed nodes derive only from qualified history (evidence).
//! - `sxr delta --question` produces typed residuals; `declared_unobserved` is
//!   never rendered as failure (severity "info"); contradictions surface.
//! - Rebuild determinism over the G_observed tables (AC-REPLAY-003 shape).

use realitytrace::projections;
use rusqlite::Connection;
use serde_json::{Value, json};
use sxr_core::LedgerRow;
use sxr_core::record_kinds::RecordKind;

fn snapshot_payload(semantic_hash: &str, decls: &[&str]) -> Value {
    let d = json!({
        "schema_version": "domainforge-semantic-envelope/v1",
        "self_hash": "sha256:aaaa",
        "semantic_closure_hash": semantic_hash,
        "inputs": {"source_set_hash": "sha256:bbbb", "semantic_pack_set_hash": "sha256:cccc",
                   "language_schema_version": "a", "interpretation_version": "b"},
        "producer": {"name": "domainforge-core", "version": "0.16.0"},
        "envelope": {
            "modules": [{"logical_id": "m.sea", "semantic_content_hash": "x", "namespace": "default"}],
            "import_graph": [],
            "namespace_bindings": [{"namespace": "default", "logical_id": "m.sea"}],
            "semantic_packs": [],
            "semantic_declarations": decls.iter().map(|name| json!({
                "id": {"kind": "concept", "data": {"id": format!("c-{name}")}},
                "logical_module_id": "m.sea",
                "declaration": {"kind": "entity", "data": {"name": name}}
            })).collect::<Vec<_>>(),
            "resolved_references": [],
            "application_contract": {}
        }
    });
    json!({
        "envelope_id": "env-1", "cep_version": "0.1.0", "envelope_version": "1.0.0",
        "envelope_kind": "semantic_snapshot", "created_at": "2026-08-07T00:00:00Z",
        "created_by": "x", "scope": {"semantic_root": "m.sea", "entry": "m.sea"},
        "boundary_record": {"scope":"m.sea","included_sections":[],"excluded_sections":[],
            "known_omissions":[],"unknowns":[],"redactions":[],"compression_notes":[],
            "out_of_scope_entities":[],"limitations":[]},
        "completeness_status": "complete_for_declared_state",
        "provenance_refs": [], "omission_status": "known_omissions_recorded",
        "lineage_refs": [], "validation_status": "validated",
        "representations": [{"representation_id":"r","representation_kind":"derived",
            "representation_type":"domainforge_semantic_envelope_document","scope":"m.sea",
            "source":"x","inline_content": serde_json::to_string(&d).unwrap(),
            "content_hash":"sha256:aaaa","semantic_hash":semantic_hash,
            "content_availability":"available","validation_status":"valid"}],
        "extensions": {"domainforge": {"model_validation_status": "valid", "diagnostics": []}}
    })
}

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
        committed_at: "2026-08-07T00:00:00Z".into(),
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

fn snapshot(conn: &mut Connection, snapshot_id: &str, decls: &[&str]) {
    let p = snapshot_payload("sha256:s1", decls);
    apply(
        conn,
        RecordKind::SemanticSnapshot,
        &[&format!("snapshot:{snapshot_id}"), "repo:r1", "checkout:c1"],
        &p,
    );
}

fn question(conn: &mut Connection, qid: &str, targets: &[&str]) {
    let spec = json!({
        "question_id": qid,
        "question_form": "is the relation realized?",
        "question_type": "verification",
        "target_entities": targets,
    });
    let payload = json!({
        "questions": [{"question_id": qid, "question_form": "is the relation realized?", "question_type": "verification"}],
        "representations": [{"representation_id":"rq","representation_kind":"atomic",
            "representation_type":"question_spec","inline_content": spec}],
    });
    apply(
        conn,
        RecordKind::QuestionDeclared,
        &[&format!("question:{qid}")],
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

fn evidence(conn: &mut Connection, eid: &str, qid: &str, target: &str, direction: &str) {
    let payload = json!({
        "evidence": [{
            "evidence_id": eid, "evidence_type": "test_result",
            "question_ref": qid, "target_entity_ref": target,
            "representation_ref": "rep", "relevance": "high",
            "reliability": "medium", "direction": direction, "confidence_impact": "neutral",
        }],
    });
    let target_ref = format!("target:{target}");
    apply(
        conn,
        RecordKind::EvidenceCreated,
        &[
            &format!("evidence:{eid}"),
            "run:run-1",
            &format!("question:{qid}"),
            &target_ref,
        ],
        &payload,
    );
}

fn claim(conn: &mut Connection, cid: &str, qid: &str, src: &str, dst: &str, kind: &str) {
    let relation_form = format!("relation {src} -> {dst} kind:{kind}");
    let payload = json!({
        "claims": [{"claim_id": cid, "claim_type": "relation",
                    "claim_text_or_form": relation_form,
                    "subject_ref": src, "scope": format!("question:{qid}"),
                    "confidence": "unsettled", "question_refs": [qid]}],
    });
    apply(
        conn,
        RecordKind::ClaimMade,
        &[
            &format!("claim:{cid}"),
            &format!("question:{qid}"),
            "run:run-1",
        ],
        &payload,
    );
}

fn evidence_claim(conn: &mut Connection, eid: &str, qid: &str, cid: &str, direction: &str) {
    let payload = json!({
        "evidence": [{
            "evidence_id": eid, "evidence_type": "relation_claim",
            "question_ref": qid, "target_claim_ref": cid,
            "representation_ref": "claim", "relevance": "high",
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
            &format!("target:claim:{cid}"),
        ],
        &payload,
    );
}

fn settlement(conn: &mut Connection, sid: &str, qid: &str, status: &str) {
    let payload = json!({
        "settlements": [{"settlement_id": sid, "settlement_status": status,
            "transformation_ref": "tx", "question_ref": qid, "threshold_ref": "t1"}],
    });
    apply(
        conn,
        RecordKind::SettlementRegistered,
        &[
            &format!("settlement:{sid}"),
            &format!("question:{qid}"),
            "run:run-1",
        ],
        &payload,
    );
}

/// AC-GRAPH-001: evidence on A and evidence on B (co-reference, same question)
/// produces NO observed edge. Only a claimed relation admits an edge.
#[test]
fn co_reference_produces_no_observed_edge_ac_graph_001() {
    let mut conn = setup_db();
    snapshot(&mut conn, "snap-1", &["A", "B"]);
    question(&mut conn, "q1", &["concept:c-A", "concept:c-B"]);
    threshold(&mut conn, "t1", "q1", "medium", "medium");
    // Evidence on A and evidence on B under the same question — co-reference.
    evidence(&mut conn, "ev-a", "q1", "concept:c-A", "supports");
    evidence(&mut conn, "ev-b", "q1", "concept:c-B", "supports");
    projections::materialize_g_observed(&conn).unwrap();

    let edges: i64 = conn
        .query_row("SELECT COUNT(*) FROM sxr_obs_edge", [], |r| r.get(0))
        .unwrap();
    assert_eq!(edges, 0, "co-reference must NOT create an observed edge");

    // Now assert a claimed relation R(A,B) with qualifying evidence on the claim.
    claim(
        &mut conn,
        "cl-1",
        "q1",
        "concept:c-A",
        "concept:c-B",
        "relation",
    );
    evidence_claim(&mut conn, "ev-cl", "q1", "cl-1", "supports");
    settlement(&mut conn, "s1", "q1", "settled");
    projections::materialize_g_observed(&conn).unwrap();

    let edges: i64 = conn
        .query_row("SELECT COUNT(*) FROM sxr_obs_edge", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        edges, 1,
        "a claimed relation with qualifying evidence admits an edge"
    );

    // The edge names src, dst, kind and carries settlement status.
    let (src, dst, kind, status): (String, String, String, String) = conn
        .query_row(
            "SELECT src_decl_key, dst_decl_key, edge_kind, settlement_status FROM sxr_obs_edge",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(src, "concept:c-A");
    assert_eq!(dst, "concept:c-B");
    assert_eq!(kind, "relation");
    assert_eq!(status, "settled");
}

/// G_observed nodes derive only from declarations with qualifying evidence.
#[test]
fn g_observed_nodes_from_qualified_history() {
    let mut conn = setup_db();
    snapshot(&mut conn, "snap-1", &["A", "B"]);
    question(&mut conn, "q1", &["concept:c-A"]);
    evidence(&mut conn, "ev-a", "q1", "concept:c-A", "supports");
    projections::materialize_g_observed(&conn).unwrap();

    let nodes: i64 = conn
        .query_row("SELECT COUNT(*) FROM sxr_obs_node", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        nodes, 1,
        "only evidenced declarations become observed nodes"
    );
    let evidenced: String = conn
        .query_row("SELECT decl_key FROM sxr_obs_node", [], |r| r.get(0))
        .unwrap();
    assert_eq!(evidenced, "concept:c-A");
}

/// §12.5: declared_unobserved is never rendered as failure (severity "info").
#[test]
fn declared_unobserved_is_never_failure() {
    let mut conn = setup_db();
    snapshot(&mut conn, "snap-1", &["A", "B"]);
    question(&mut conn, "q1", &["concept:c-A", "concept:c-B"]);
    threshold(&mut conn, "t1", "q1", "medium", "medium");
    // Only A has evidence; B is declared but unobserved.
    evidence(&mut conn, "ev-a", "q1", "concept:c-A", "supports");
    projections::materialize_g_observed(&conn).unwrap();

    let d = projections::delta_question(&conn, "r1", "q1", "snap-1").unwrap();
    let residuals = d["residuals"].as_array().unwrap();
    let b = residuals
        .iter()
        .find(|r| r["decl_key"] == "concept:c-B")
        .expect("B in scope");
    assert_eq!(b["residual_kind"], "declared_unobserved");
    assert_eq!(
        b["severity"], "info",
        "declared_unobserved must never be coloured red"
    );
}

/// §12.5: contradictions surface as `declared_contradicted` (severity "error").
#[test]
fn contradictions_surface_in_delta() {
    let mut conn = setup_db();
    snapshot(&mut conn, "snap-1", &["A"]);
    question(&mut conn, "q1", &["concept:c-A"]);
    threshold(&mut conn, "t1", "q1", "medium", "medium");
    evidence(&mut conn, "ev-1", "q1", "concept:c-A", "contradicts");
    projections::materialize_g_observed(&conn).unwrap();

    let d = projections::delta_question(&conn, "r1", "q1", "snap-1").unwrap();
    let a = &d["residuals"][0];
    assert_eq!(a["residual_kind"], "declared_contradicted");
    assert_eq!(a["severity"], "error");
    assert_eq!(a["contradicts"], true);
}

/// §12.5: both supports + contradicts above threshold → `contradictory`.
#[test]
fn contradictory_residual_when_supports_and_contradicts() {
    let mut conn = setup_db();
    snapshot(&mut conn, "snap-1", &["A"]);
    question(&mut conn, "q1", &["concept:c-A"]);
    threshold(&mut conn, "t1", "q1", "medium", "medium");
    evidence(&mut conn, "ev-s", "q1", "concept:c-A", "supports");
    evidence(&mut conn, "ev-c", "q1", "concept:c-A", "contradicts");
    projections::materialize_g_observed(&conn).unwrap();

    let d = projections::delta_question(&conn, "r1", "q1", "snap-1").unwrap();
    let a = &d["residuals"][0];
    assert_eq!(a["residual_kind"], "contradictory");
    assert_eq!(a["severity"], "error");
}

/// observed_undeclared: a G_observed edge with no declared counterpart surfaces
/// as a candidate model gap (§12.5).
#[test]
fn observed_undeclared_surfaces_model_gap() {
    let mut conn = setup_db();
    snapshot(&mut conn, "snap-1", &["A", "B"]);
    question(&mut conn, "q1", &[]);
    threshold(&mut conn, "t1", "q1", "medium", "medium");
    // A claimed relation A->C where C is NOT declared in this snapshot.
    claim(
        &mut conn,
        "cl-1",
        "q1",
        "concept:c-A",
        "concept:c-C",
        "relation",
    );
    evidence_claim(&mut conn, "ev-cl", "q1", "cl-1", "supports");
    settlement(&mut conn, "s1", "q1", "settled");
    projections::materialize_g_observed(&conn).unwrap();

    let d = projections::delta_question(&conn, "r1", "q1", "snap-1").unwrap();
    let undeclared = d["observed_undeclared"].as_array().unwrap();
    assert!(
        !undeclared.is_empty(),
        "an observed edge with no declared counterpart must surface"
    );
    assert_eq!(undeclared[0]["dst_decl_key"], "concept:c-C");
}

/// Rebuild determinism: G_observed tables are byte-identical after a
/// drop+recreate+materialize (AC-REPLAY-003 shape).
#[test]
fn g_observed_rebuild_deterministic() {
    let mut conn = setup_db();
    snapshot(&mut conn, "snap-1", &["A", "B"]);
    question(&mut conn, "q1", &["concept:c-A", "concept:c-B"]);
    threshold(&mut conn, "t1", "q1", "medium", "medium");
    evidence(&mut conn, "ev-a", "q1", "concept:c-A", "supports");
    claim(
        &mut conn,
        "cl-1",
        "q1",
        "concept:c-A",
        "concept:c-B",
        "relation",
    );
    evidence_claim(&mut conn, "ev-cl", "q1", "cl-1", "supports");
    settlement(&mut conn, "s1", "q1", "settled");
    projections::materialize_g_observed(&conn).unwrap();
    let before = projections::export_canonical_conn(&conn).unwrap();

    projections::drop_tables_test(&mut conn).unwrap();
    conn.execute_batch(projections::PROJECTION_DDL).unwrap();
    // re-apply the SAME ledger rows in the same order.
    snapshot(&mut conn, "snap-1", &["A", "B"]);
    question(&mut conn, "q1", &["concept:c-A", "concept:c-B"]);
    threshold(&mut conn, "t1", "q1", "medium", "medium");
    evidence(&mut conn, "ev-a", "q1", "concept:c-A", "supports");
    claim(
        &mut conn,
        "cl-1",
        "q1",
        "concept:c-A",
        "concept:c-B",
        "relation",
    );
    evidence_claim(&mut conn, "ev-cl", "q1", "cl-1", "supports");
    settlement(&mut conn, "s1", "q1", "settled");
    projections::materialize_g_observed(&conn).unwrap();
    let after = projections::export_canonical_conn(&conn).unwrap();
    assert_eq!(before, after, "G_observed rebuild must be byte-identical");
}

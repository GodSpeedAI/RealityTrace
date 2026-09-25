//! Slice 3 tests (§24 Slice 3): G_declared nodes/edges + FTS retrieval.
//!
//! Covers the Slice-3 acceptance surface:
//! - Edge count matches `resolved_references` (§12.3).
//! - FTS recall on a seeded corpus (§12.2).
//! - Rebuild determinism (AC-REPLAY-002/003 shape).
//! - `decl_key` scoping by `(repo_id, semantic_closure_hash)` (AC-IDENTITY-002).

use realitytrace::projections;
use rusqlite::Connection;
use serde_json::{Value, json};
use sxr_core::LedgerRow;
use sxr_core::record_kinds::RecordKind;

/// A minimal CEP semantic_snapshot envelope whose inline D carries two entity
/// declarations, one flow declaration, one record declaration, and a set of
/// resolved_references (mimicking the flagship shape).
fn snapshot_payload(
    semantic_hash: &str,
    decl_kinds: &[(&str, &str)],
    references: &[(&str, &str, &str)],
) -> Value {
    let d = json!({
        "schema_version": "domainforge-semantic-envelope/v1",
        "self_hash": "sha256:aaaa",
        "semantic_closure_hash": semantic_hash,
        "inputs": {
            "source_set_hash": "sha256:bbbb",
            "semantic_pack_set_hash": "sha256:cccc",
            "language_schema_version": "domainforge-ast/v3",
            "interpretation_version": "domainforge-interpretation/v1"
        },
        "producer": {"name": "domainforge-core", "version": "0.16.0"},
        "envelope": {
            "language_schema_version": "domainforge-ast/v3",
            "canonicalization_version": "domainforge-canonicalization/v1",
            "compiler_interpretation_version": "domainforge-interpretation/v1",
            "modules": [{"logical_id": "model.sea", "semantic_content_hash": "sha256:dddd", "namespace": "default"}],
            "import_graph": [],
            "namespace_bindings": [{"namespace": "default", "logical_id": "model.sea"}],
            "semantic_packs": [],
            "semantic_declarations": decl_kinds.iter().map(|(dk, kind)| {
                let id = match *dk {
                    "entity" => json!({"kind": "concept", "data": {"id": "concept-a"}}),
                    "flow" => json!({"kind": "occurrence", "data": {"kind": "flow", "content_hash": "sha256:flow1", "duplicate_index": 0}}),
                    "record" => json!({"kind": "application", "data": {"id": "record-a"}}),
                    _ => json!({"kind": "concept", "data": {"id": "concept-x"}}),
                };
                json!({
                    "id": id,
                    "logical_module_id": "model.sea",
                    "declaration": {"kind": kind, "data": {"name": format!("{}_{}", dk, kind)}}
                })
            }).collect::<Vec<_>>(),
            "resolved_references": references.iter().map(|(src, dst, path)| {
                json!({
                    "source": decl_ref_id(src),
                    "field_path": path,
                    "target": {
                        "kind": "declaration",
                        "data": {"id": decl_ref_id(dst)}
                    }
                })
            }).collect::<Vec<_>>(),
            "application_contract": {}
        }
    });

    json!({
        "envelope_id": "env-1",
        "cep_version": "0.1.0",
        "envelope_version": "1.0.0",
        "envelope_kind": "semantic_snapshot",
        "created_at": "2026-08-07T00:00:00Z",
        "created_by": "domainforge-core 0.16.0",
        "scope": {"semantic_root": "model.sea", "entry": "model.sea"},
        "boundary_record": {
            "scope": "model.sea", "included_sections": [], "excluded_sections": [],
            "known_omissions": [], "unknowns": [], "redactions": [],
            "compression_notes": [], "out_of_scope_entities": [], "limitations": []
        },
        "completeness_status": "complete_for_declared_state",
        "provenance_refs": ["source-set:sha256:bbbb"],
        "omission_status": "known_omissions_recorded",
        "lineage_refs": [],
        "validation_status": "validated",
        "representations": [{
            "representation_id": "representation:sha256:aaaa",
            "representation_kind": "derived",
            "representation_type": "domainforge_semantic_envelope_document",
            "scope": "model.sea",
            "source": "domainforge-core 0.16.0",
            "inline_content": serde_json::to_string(&d).unwrap(),
            "content_hash": "sha256:aaaa",
            "semantic_hash": semantic_hash,
            "content_availability": "available",
            "validation_status": "valid"
        }],
        "extensions": {
            "domainforge": {"model_validation_status": "valid", "diagnostics": []}
        }
    })
}

/// decl_key string for a synthetic declaration-reference id.
fn decl_ref_id(tag: &str) -> Value {
    match tag {
        "entity" => json!({"kind": "concept", "data": {"id": "concept-a"}}),
        "flow" => {
            json!({"kind": "occurrence", "data": {"kind": "flow", "content_hash": "sha256:flow1", "duplicate_index": 0}})
        }
        "record" => json!({"kind": "application", "data": {"id": "record-a"}}),
        _ => json!({"kind": "concept", "data": {"id": "concept-x"}}),
    }
}

/// CAS-carried variant of `snapshot_payload`: the D document is persisted as
/// a content-addressed object under `cas_root` and the representation carries
/// `content_ref` instead of `inline_content` (§10.7a). Returns
/// `(payload, content_hash)`.
fn snapshot_payload_cas(
    cas_root: &std::path::Path,
    semantic_hash: &str,
    decl_kinds: &[(&str, &str)],
    references: &[(&str, &str, &str)],
) -> (Value, String) {
    let mut payload = snapshot_payload(semantic_hash, decl_kinds, references);
    let rep = payload["representations"][0].as_object_mut().unwrap();
    let d: Value = serde_json::from_str(rep["inline_content"].as_str().unwrap()).unwrap();
    let mut selfless = d.clone();
    selfless.as_object_mut().unwrap().remove("self_hash");
    let content_hash = sxr_core::canonical_json::sha256_hex(
        sxr_core::canonical_json::canonical_json(&selfless).as_bytes(),
    );
    let hex = content_hash.trim_start_matches("sha256:");
    std::fs::create_dir_all(cas_root).unwrap();
    std::fs::write(
        cas_root.join(hex),
        sxr_core::canonical_json::canonical_json(&d),
    )
    .unwrap();
    rep.remove("inline_content");
    rep.insert("content_ref".into(), format!("cas://{content_hash}").into());
    rep.insert("content_hash".into(), content_hash.clone().into());
    (payload, content_hash)
}

fn row(kind: RecordKind, snapshot_id: &str, repo_id: &str, payload: &Value) -> LedgerRow {
    LedgerRow {
        ledger_id: format!("ledger-{repo_id}"),
        append_ordinal: 1,
        entry_ulid: "ulid-1".into(),
        record_kind: kind.to_string(),
        record_version: "sxr-record/v1".into(),
        idempotency_key: None,
        subject_refs: vec![
            format!("snapshot:{snapshot_id}"),
            format!("repo:{repo_id}"),
            format!("checkout:checkout-{repo_id}"),
        ],
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

#[test]
fn g_declared_edge_count_matches_resolved_references() {
    let mut conn = setup_db();
    let snapshot_id = "snap-1";
    let payload = snapshot_payload(
        "sha256:s1",
        &[("entity", "entity"), ("flow", "flow"), ("record", "record")],
        &[
            ("entity", "flow", "resource"),
            ("entity", "record", "input"),
        ],
    );
    projections::apply_row(
        &mut conn,
        &row(RecordKind::SemanticSnapshot, snapshot_id, "r1", &payload),
    )
    .unwrap();
    // 2 resolved_references → 2 G_declared 'reference' edges.
    let refs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sxr_decl_edge WHERE edge_kind='reference'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        refs, 2,
        "reference edge count must match resolved_references"
    );
}

#[test]
fn g_declared_nodes_and_reference_edges_are_populated() {
    let mut conn = setup_db();
    let snapshot_id = "snap-2";
    let payload = snapshot_payload(
        "sha256:s2",
        &[("entity", "entity"), ("flow", "flow"), ("record", "record")],
        &[
            ("entity", "flow", "resource"),
            ("entity", "record", "input"),
        ],
    );
    projections::apply_row(
        &mut conn,
        &row(RecordKind::SemanticSnapshot, snapshot_id, "r1", &payload),
    )
    .unwrap();

    // 3 declaration nodes.
    let nodes: i64 = conn
        .query_row("SELECT COUNT(*) FROM sxr_decl_node", [], |r| r.get(0))
        .unwrap();
    assert_eq!(nodes, 3);

    // 2 reference edges (matches resolved_references count).
    let refs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sxr_decl_edge WHERE edge_kind='reference'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(refs, 2);

    // The flow also contributes internal refs (resource/from_entity/to_entity)
    // that are not in resolved_references — but the synthetic flow has no data
    // refs, so only the 2 reference edges are present.
    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM sxr_decl_edge", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total, 2);
}

#[test]
fn fts_recall_on_seeded_corpus() {
    let mut conn = setup_db();
    let snapshot_id = "snap-3";
    let payload = snapshot_payload("sha256:s3", &[("entity", "entity"), ("flow", "flow")], &[]);
    projections::apply_row(
        &mut conn,
        &row(RecordKind::SemanticSnapshot, snapshot_id, "r1", &payload),
    )
    .unwrap();

    // Seed FTS directly via the projection's content table.
    let hits = projections::fts_search(&conn, "entity_entity", 10).unwrap();
    assert!(!hits.is_empty(), "FTS must recall a seeded declaration");
    let kind: String = hits[0]["kind"].as_str().unwrap().to_string();
    assert_eq!(kind, "declaration");
}

#[test]
fn rebuild_drops_and_recreates_identically() {
    let mut conn = setup_db();
    let snapshot_id = "snap-4";
    let payload = snapshot_payload(
        "sha256:s4",
        &[("entity", "entity"), ("flow", "flow")],
        &[("entity", "flow", "resource")],
    );
    projections::apply_row(
        &mut conn,
        &row(RecordKind::SemanticSnapshot, snapshot_id, "r1", &payload),
    )
    .unwrap();

    let snapshot_before = projections::export_canonical_conn(&conn).unwrap();
    // Drop + recreate via the same DDL path.
    projections::drop_tables_test(&mut conn).unwrap();
    conn.execute_batch(projections::PROJECTION_DDL).unwrap();
    projections::apply_row(
        &mut conn,
        &row(RecordKind::SemanticSnapshot, snapshot_id, "r1", &payload),
    )
    .unwrap();
    let snapshot_after = projections::export_canonical_conn(&conn).unwrap();
    assert_eq!(
        snapshot_before, snapshot_after,
        "rebuild must be byte-identical"
    );
}

#[test]
fn cas_carried_snapshot_projects_decl_nodes_and_edges() {
    let tmp = tempfile::tempdir().unwrap();
    let cas_root = tmp.path().join("cas");
    let mut conn = setup_db();
    let snapshot_id = "snap-cas-1";
    let (payload, content_hash) = snapshot_payload_cas(
        &cas_root,
        "sha256:s9",
        &[("entity", "entity"), ("flow", "flow"), ("record", "record")],
        &[
            ("entity", "flow", "resource"),
            ("entity", "record", "input"),
        ],
    );
    projections::apply_row_with_cas(
        &mut conn,
        &row(RecordKind::SemanticSnapshot, snapshot_id, "r1", &payload),
        Some(&cas_root),
    )
    .unwrap();

    // Inline and CAS-carried snapshots must project identically: 3 nodes,
    // 2 reference edges, FTS rows, and a stored content_hash.
    let nodes: i64 = conn
        .query_row("SELECT COUNT(*) FROM sxr_decl_node", [], |r| r.get(0))
        .unwrap();
    assert_eq!(nodes, 3);
    let refs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sxr_decl_edge WHERE edge_kind='reference'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(refs, 2);
    let fts: i64 = conn
        .query_row("SELECT COUNT(*) FROM sxr_fts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(fts, 3);
    let stored: String = conn
        .query_row(
            "SELECT content_hash FROM sxr_snapshot WHERE snapshot_id = ?1",
            [snapshot_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored, content_hash);
}

#[test]
fn cas_carried_snapshot_missing_object_fails_loudly() {
    let tmp = tempfile::tempdir().unwrap();
    let mut conn = setup_db();
    let snapshot_id = "snap-cas-2";
    let (payload, _) = snapshot_payload_cas(
        &tmp.path().join("cas"),
        "sha256:s10",
        &[("entity", "entity")],
        &[],
    );
    // Remove the CAS object to simulate a corrupt/incomplete store.
    std::fs::remove_dir_all(tmp.path().join("cas")).unwrap();
    let err = projections::apply_row_with_cas(
        &mut conn,
        &row(RecordKind::SemanticSnapshot, snapshot_id, "r1", &payload),
        Some(&tmp.path().join("cas")),
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("read CAS object"),
        "missing CAS object must fail loudly: {err}"
    );
}

/// Two repos with the same declaration identity must not merge.
#[test]
fn decl_key_scoping_by_repo_and_hash_ac_identity_002() {
    use std::collections::HashSet;
    let mut conn = setup_db();
    let payload_a = snapshot_payload("sha256:same", &[("entity", "entity")], &[]);
    let payload_b = snapshot_payload("sha256:same", &[("entity", "entity")], &[]);
    projections::apply_row(
        &mut conn,
        &row(RecordKind::SemanticSnapshot, "snap-a", "repo-a", &payload_a),
    )
    .unwrap();
    projections::apply_row(
        &mut conn,
        &row(RecordKind::SemanticSnapshot, "snap-b", "repo-b", &payload_b),
    )
    .unwrap();

    // Same decl_key in repo-a and repo-b, distinct snapshot rows.
    let snap_a: String = conn
        .query_row(
            "SELECT snapshot_id FROM sxr_decl_node WHERE decl_key='concept:concept-a' AND snapshot_id='snap-a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let snap_b: String = conn
        .query_row(
            "SELECT snapshot_id FROM sxr_decl_node WHERE decl_key='concept:concept-a' AND snapshot_id='snap-b'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(snap_a, "snap-a");
    assert_eq!(snap_b, "snap-b");

    // The scope is (repo_id, semantic_hash, decl_key). Each repo resolves the
    // same decl_key to ITS OWN snapshot — the two must not collapse.
    let a = projections::resolve_decl_key(&conn, "repo-a", "concept:concept-a")
        .unwrap()
        .expect("repo-a resolves");
    let b = projections::resolve_decl_key(&conn, "repo-b", "concept:concept-a")
        .unwrap()
        .expect("repo-b resolves");
    assert_eq!(a.0, b.0, "decl_key string is identical across repos");
    assert_ne!(
        a.1, b.1,
        "repos must resolve to distinct snapshots (scoped scope)"
    );
    // The full scope is distinct: repo_id + snapshot together.
    let mut scopes = HashSet::new();
    scopes.insert(("repo-a".to_string(), a.1.clone()));
    scopes.insert(("repo-b".to_string(), b.1.clone()));
    assert_eq!(
        scopes.len(),
        2,
        "full (repo_id, semantic_hash, decl_key) scope stays distinct"
    );
}

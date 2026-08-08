//! Slice-1 projections (§12): derived relational tables rebuilt from the
//! ledger. The projector interprets each envelope payload by record kind and
//! upserts rows; `rebuild --all` drops and re-applies in predecessor-chain
//! order so `export --canonical-json` is byte-identical across rebuilds
//! (AC-REPLAY-002).

use anyhow::{Context, Result, anyhow};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use sxr_core::LedgerRow;
use sxr_core::record_kinds::RecordKind;

pub const PROJECTION_VERSION: &str = "sxr-projection/v1";

pub const PROJECTION_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS sxr_snapshot (
    snapshot_id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    semantic_closure_hash TEXT,
    content_hash TEXT,
    invalid_declared_checkpoint_hash TEXT,
    model_validation_status TEXT NOT NULL,
    representation_omission_json TEXT,
    source_set_hash TEXT NOT NULL,
    entry_logical_path TEXT NOT NULL,
    registry_content_hash TEXT,
    resolution_inputs_json TEXT NOT NULL,
    drift_class TEXT,
    first_seen_ledger_id TEXT NOT NULL,
    first_seen_append_ordinal INTEGER NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS sxr_run (
    run_id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    checkout_id TEXT NOT NULL,
    git_commit TEXT,
    dirty INTEGER NOT NULL,
    snapshot_id TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    status TEXT NOT NULL,
    plan_id TEXT
) STRICT;
CREATE TABLE IF NOT EXISTS sxr_question (
    question_id TEXT PRIMARY KEY,
    question_form TEXT NOT NULL,
    question_type TEXT NOT NULL,
    scope_json TEXT NOT NULL,
    ledger_id TEXT NOT NULL,
    append_ordinal INTEGER NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS sxr_threshold (
    threshold_id TEXT PRIMARY KEY,
    question_id TEXT NOT NULL,
    ledger_id TEXT NOT NULL,
    append_ordinal INTEGER NOT NULL,
    spec_json TEXT NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS sxr_evidence (
    evidence_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    question_id TEXT NOT NULL,
    evidence_type TEXT NOT NULL,
    target_kind TEXT NOT NULL,
    target_ref TEXT NOT NULL,
    relevance TEXT NOT NULL,
    reliability TEXT NOT NULL,
    direction TEXT NOT NULL,
    confidence_impact TEXT NOT NULL,
    representation_ref TEXT NOT NULL,
    ledger_id TEXT NOT NULL,
    append_ordinal INTEGER NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS sxr_evidence_target (
    evidence_id TEXT NOT NULL,
    decl_key TEXT NOT NULL,
    PRIMARY KEY (evidence_id, decl_key)
) STRICT;
CREATE TABLE IF NOT EXISTS sxr_settlement (
    settlement_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    question_id TEXT NOT NULL,
    transformation_ref TEXT NOT NULL,
    threshold_id TEXT NOT NULL,
    settlement_status TEXT NOT NULL,
    confidence TEXT NOT NULL,
    debt_id TEXT,
    ledger_id TEXT NOT NULL,
    append_ordinal INTEGER NOT NULL
) STRICT;
/* Slice 3: G_declared (§12.3) — derived solely from semantic.snapshot records.
   Nodes are scoped by (snapshot_id, decl_key); the snapshot belongs to one
   repo, so (repo_id, semantic_closure_hash, decl_key) is the full scope
   (AC-IDENTITY-002). */
CREATE TABLE IF NOT EXISTS sxr_decl_node (
    snapshot_id TEXT NOT NULL,
    decl_key TEXT NOT NULL,
    decl_kind TEXT NOT NULL,
    name TEXT NOT NULL,
    namespace TEXT NOT NULL,
    logical_module_id TEXT NOT NULL,
    PRIMARY KEY (snapshot_id, decl_key)
) STRICT;
CREATE TABLE IF NOT EXISTS sxr_decl_edge (
    snapshot_id TEXT NOT NULL,
    src_decl_key TEXT NOT NULL,
    dst_decl_key TEXT NOT NULL,
    edge_kind TEXT NOT NULL,
    field_path TEXT NOT NULL,
    PRIMARY KEY (snapshot_id, src_decl_key, dst_decl_key, edge_kind, field_path)
) STRICT;
/* Slice 3: FTS5 index (§12.2). Contentless-external-content: the FTS index
   owns no truth — the authoritative text lives in sxr_fts_content, which is
   itself derived from the ledger and recreated on every rebuild. FTS5 reads
   that content for indexing; a hit returns a rowid that maps back to the
   content row, so every result names its source record (§12.6). */
CREATE TABLE IF NOT EXISTS sxr_fts_content (
    rowid INTEGER PRIMARY KEY,
    subject_key TEXT NOT NULL,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL
) STRICT;
CREATE VIRTUAL TABLE sxr_fts USING fts5(
    subject_key, kind, title, body,
    content = 'sxr_fts_content',
    content_rowid = 'rowid',
    tokenize = 'porter unicode61 remove_diacritics 2'
);
/* Slice 4: G_observed (§12.4) — derived ONLY from qualified history.
   nodes: every declaration with qualifying evidence. A declared-but-unobserved
   decl has no observed node (absence = not observed, rendered as such).
   edges: only relations admitted under §12.4a (a claim.made asserts the
   relation; ≥1 evidence targets that claim meeting the question's min
   relevance/reliability; settlement status recorded). Co-reference never
   creates an observed edge (AC-GRAPH-001). */
CREATE TABLE IF NOT EXISTS sxr_claim (
    claim_id TEXT PRIMARY KEY,
    question_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    src_decl_key TEXT NOT NULL,
    dst_decl_key TEXT NOT NULL,
    edge_kind TEXT NOT NULL,
    ledger_id TEXT NOT NULL,
    append_ordinal INTEGER NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS sxr_obs_node (
    decl_key TEXT PRIMARY KEY,
    first_evidenced_ordinal INTEGER NOT NULL,
    last_evidenced_ordinal INTEGER NOT NULL,
    evidence_count INTEGER NOT NULL,
    settled_count INTEGER NOT NULL,
    unsettled_count INTEGER NOT NULL,
    debt_count INTEGER NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS sxr_obs_edge (
    src_decl_key TEXT NOT NULL,
    dst_decl_key TEXT NOT NULL,
    edge_kind TEXT NOT NULL,
    claim_id TEXT NOT NULL,
    confidence TEXT NOT NULL,
    contradiction_count INTEGER NOT NULL,
    settlement_status TEXT NOT NULL,
    evidence_refs_json TEXT NOT NULL,
    settlement_refs_json TEXT NOT NULL,
    PRIMARY KEY (src_decl_key, dst_decl_key, edge_kind)
) STRICT;
"#;

fn string_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn parse_kind(kind: &str) -> Option<RecordKind> {
    kind.parse().ok()
}

/// Apply one ledger row to the projection tables. `conn` is a connection to
/// the repo's ledger DB (projection tables live in the same DB file).
pub fn apply_row(conn: &mut Connection, row: &LedgerRow) -> Result<()> {
    let Some(record_kind) = parse_kind(&row.record_kind) else {
        return Ok(());
    };
    let payload: Value = serde_json::from_str(&row.payload).context("parse payload")?;
    match record_kind {
        RecordKind::QuestionDeclared => apply_question(conn, row, &payload)?,
        RecordKind::ThresholdDeclared => apply_threshold(conn, row, &payload)?,
        RecordKind::SemanticSnapshot => apply_snapshot(conn, row, &payload)?,
        RecordKind::RunStarted => apply_run_started(conn, row, &payload)?,
        RecordKind::RunFinished => apply_run_finished(conn, row, &payload)?,
        RecordKind::ObservationRecorded => {}
        RecordKind::EvidenceCreated => apply_evidence(conn, row, &payload)?,
        RecordKind::SettlementRegistered => apply_settlement(conn, row, &payload)?,
        RecordKind::ClaimMade => apply_claim(conn, row, &payload)?,
        _ => {}
    }
    Ok(())
}

fn first_section<'a>(payload: &'a Value, section: &str) -> Option<&'a Value> {
    payload
        .get(section)
        .and_then(Value::as_array)
        .and_then(|a| a.first())
}

/// Insert one row into the FTS index (contentless-external-content). The
/// authoritative text lives in `sxr_fts_content`; the FTS index reads it for
/// search. `kind`/`subject_key` are stored so a hit names its source (§12.6).
fn fts_insert(
    conn: &Connection,
    subject_key: &str,
    kind: &str,
    title: &str,
    body: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO sxr_fts_content(subject_key, kind, title, body) VALUES (?1,?2,?3,?4)",
        params![subject_key, kind, title, body],
    )?;
    let rowid: i64 = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO sxr_fts(rowid, subject_key, kind, title, body) VALUES (?1,?2,?3,?4,?5)",
        params![rowid, subject_key, kind, title, body],
    )?;
    Ok(())
}

fn apply_question(conn: &mut Connection, row: &LedgerRow, payload: &Value) -> Result<()> {
    let q = first_section(payload, "questions");
    let question_id = q
        .and_then(|v| string_field(v, "question_id"))
        .unwrap_or_default();
    if question_id.is_empty() {
        return Ok(());
    }
    let form = q
        .and_then(|v| string_field(v, "question_form"))
        .unwrap_or_default();
    let qtype = q
        .and_then(|v| string_field(v, "question_type"))
        .unwrap_or_default();
    // §12.5: delta resolves the Question's selected declaration set from its
    // target_entities/target_transformations. These live in the question spec
    // representation's inline_content; merge them into scope_json.
    let mut scope = payload.get("scope").cloned().unwrap_or(Value::Null);
    let inline = payload
        .get("representations")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|r| r.get("inline_content"))
        .cloned();
    if let Some(spec) = inline {
        if let Some(obj) = scope.as_object_mut() {
            for key in ["target_entities", "target_transformations", "scope"] {
                if let Some(v) = spec.get(key) {
                    obj.insert(key.to_string(), v.clone());
                }
            }
        } else {
            scope = spec;
        }
    }
    conn.execute(
        "INSERT OR REPLACE INTO sxr_question(question_id, question_form, question_type, scope_json, ledger_id, append_ordinal)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![question_id, form, qtype, serde_json::to_string(&scope)?, row.ledger_id, row.append_ordinal],
    )?;
    // FTS: question form is indexed context (§12.2).
    fts_insert(conn, &question_id, "question", &form, &qtype)?;
    Ok(())
}

fn apply_threshold(conn: &mut Connection, row: &LedgerRow, payload: &Value) -> Result<()> {
    let t = first_section(payload, "states");
    let threshold_id = t
        .and_then(|v| string_field(v, "state_id"))
        .unwrap_or_default();
    if threshold_id.is_empty() {
        return Ok(());
    }
    let question_id = row
        .subject_refs
        .iter()
        .find(|r| r.starts_with("question:"))
        .map(|r| r.trim_start_matches("question:").to_string())
        .unwrap_or_default();
    conn.execute(
        "INSERT OR REPLACE INTO sxr_threshold(threshold_id, question_id, ledger_id, append_ordinal, spec_json)
         VALUES (?1,?2,?3,?4,?5)",
        params![threshold_id, question_id, row.ledger_id, row.append_ordinal, {
            let spec = payload
                .get("representations")
                .and_then(Value::as_array)
                .and_then(|a| a.iter().find(|r| r.get("representation_type").and_then(Value::as_str) == Some("settlement_threshold_spec")))
                .and_then(|r| r.get("inline_content"))
                .cloned()
                .unwrap_or(Value::Null);
            serde_json::to_string(&spec)?
        }],
    )?;
    Ok(())
}

fn apply_snapshot(conn: &mut Connection, row: &LedgerRow, payload: &Value) -> Result<()> {
    let rep = first_section(payload, "representations");
    let snapshot_id = row
        .subject_refs
        .iter()
        .find(|r| r.starts_with("snapshot:"))
        .map(|r| r.trim_start_matches("snapshot:").to_string())
        .unwrap_or_default();
    if snapshot_id.is_empty() {
        return Ok(());
    }
    let repo_id = row
        .subject_refs
        .iter()
        .find(|r| r.starts_with("repo:"))
        .map(|r| r.trim_start_matches("repo:").to_string())
        .unwrap_or_default();
    let semantic_closure_hash = rep.and_then(|v| string_field(v, "semantic_hash"));
    let content_hash = rep.and_then(|v| string_field(v, "content_hash"));
    let df_ext = payload.get("extensions").and_then(|e| e.get("domainforge"));
    let invalid_checkpoint =
        df_ext.and_then(|d| string_field(d, "invalid_declared_checkpoint_hash"));
    let model_status = match df_ext
        .and_then(|d| string_field(d, "model_validation_status"))
        .as_deref()
    {
        Some("invalid") => "invalid",
        _ => {
            if invalid_checkpoint.is_some() {
                "invalid"
            } else if semantic_closure_hash.is_some() && content_hash.is_some() {
                "valid"
            } else {
                "invalid"
            }
        }
    };
    let omission_json = payload
        .get("omissions")
        .map(serde_json::to_string)
        .transpose()?;
    let source_set_hash = rep
        .and_then(|v| string_field(v, "source_set_hash"))
        .unwrap_or_default();
    let entry = payload
        .get("scope")
        .and_then(|s| string_field(s, "semantic_root").or_else(|| string_field(s, "entry")))
        .unwrap_or_default();
    let resolution_inputs = payload.get("scope").cloned().unwrap_or(Value::Null);
    conn.execute(
        "INSERT OR REPLACE INTO sxr_snapshot(snapshot_id, repo_id, semantic_closure_hash, content_hash, invalid_declared_checkpoint_hash, model_validation_status, representation_omission_json, source_set_hash, entry_logical_path, registry_content_hash, resolution_inputs_json, drift_class, first_seen_ledger_id, first_seen_append_ordinal)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
        params![
            snapshot_id, repo_id, semantic_closure_hash, content_hash, invalid_checkpoint,
            model_status, omission_json, source_set_hash, entry, None::<String>,
            serde_json::to_string(&resolution_inputs)?, None::<String>, row.ledger_id, row.append_ordinal
        ],
    )?;
    // Slice 3: populate G_declared (nodes + edges) + FTS from the inline D
    // document when a valid representation is present.
    if model_status == "valid" {
        apply_g_declared(conn, &snapshot_id, payload)?;
    }
    Ok(())
}

/// Parse the inline canonical D document from a snapshot's representation and
/// populate `sxr_decl_node`, `sxr_decl_edge`, and the FTS index (§12.3/§12.2).
fn apply_g_declared(conn: &mut Connection, snapshot_id: &str, payload: &Value) -> Result<()> {
    let rep = first_section(payload, "representations");
    let inline = rep
        .and_then(|v| string_field(v, "inline_content"))
        .unwrap_or_default();
    if inline.is_empty() {
        return Ok(());
    }
    let d: Value = serde_json::from_str(&inline).context("parse inline D")?;
    let env = d.get("envelope").cloned().unwrap_or(Value::Null);

    // module → namespace from namespace_bindings.
    let ns_of: std::collections::HashMap<String, String> = env
        .get("namespace_bindings")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|b| {
                    Some((
                        b.get("logical_id")?.as_str()?.to_string(),
                        b.get("namespace")?.as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();

    // declarations → decl_key + metadata.
    let declarations = env.get("semantic_declarations");
    let mut decl_keys: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    if let Some(arr) = declarations.and_then(Value::as_array) {
        for sd in arr {
            let Some(decl_key) = decl_key_of(sd.get("id")) else {
                continue;
            };
            let logical = sd
                .get("logical_module_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let ns = ns_of.get(&logical).cloned().unwrap_or_default();
            let decl_kind = sd
                .get("declaration")
                .and_then(|x| x.get("kind"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let name = sd
                .get("declaration")
                .and_then(|x| x.get("data"))
                .and_then(|x| x.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            conn.execute(
                "INSERT OR REPLACE INTO sxr_decl_node(snapshot_id, decl_key, decl_kind, name, namespace, logical_module_id)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![snapshot_id, decl_key, decl_kind, name, ns, logical],
            )?;
            // FTS row for this declaration (name + namespace).
            fts_insert(conn, &decl_key, "declaration", &name, &ns)?;
            decl_keys.insert(decl_key.to_string(), sd.clone());
        }
    }

    // Edges from resolved_references (decl → decl, kind "reference").
    if let Some(arr) = env.get("resolved_references").and_then(Value::as_array) {
        for rr in arr {
            let Some(src) = decl_key_of(rr.get("source")) else {
                continue;
            };
            let field_path = rr
                .get("field_path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let target = rr.get("target");
            let mut dst = None;
            if target.and_then(|t| t.get("kind")).and_then(Value::as_str) == Some("declaration") {
                dst = target
                    .and_then(|t| t.get("data"))
                    .and_then(|d| decl_key_of(d.get("id")));
            }
            if let Some(dst) = dst {
                conn.execute(
                    "INSERT OR REPLACE INTO sxr_decl_edge(snapshot_id, src_decl_key, dst_decl_key, edge_kind, field_path)
                     VALUES (?1,?2,?3,'reference',?4)",
                    params![snapshot_id, src, dst, field_path],
                )?;
            }
        }
    }

    // Edges from declaration-internal Flow references (resource/from_entity/
    // to_entity) — §12.3, not present in resolved_references.
    for sd in decl_keys.values() {
        let Some(decl_key) = decl_key_of(sd.get("id")) else {
            continue;
        };
        let data = sd
            .get("declaration")
            .and_then(|x| x.get("data"))
            .cloned()
            .unwrap_or(Value::Null);
        for field in ["resource", "from_entity", "to_entity"] {
            if let Some(dst) = data
                .get(field)
                .and_then(|x| x.get("data"))
                .and_then(|x| x.get("id"))
                .and_then(|x| decl_key_of(Some(x)))
            {
                conn.execute(
                    "INSERT OR REPLACE INTO sxr_decl_edge(snapshot_id, src_decl_key, dst_decl_key, edge_kind, field_path)
                     VALUES (?1,?2,?3,?4,?5)",
                    params![snapshot_id, decl_key, dst, field, field],
                )?;
            }
        }
    }
    Ok(())
}

/// Convert a `CanonicalDeclarationId` JSON (`{kind, data}`) to a `decl_key`
/// string (`concept:<uuid>` / `occurrence:<kind>:<hash>:<idx>` /
/// `application:<sym>`), matching the §12.1 spine.
fn decl_key_of(id: Option<&Value>) -> Option<String> {
    let id = id?;
    match id.get("kind").and_then(Value::as_str)? {
        "concept" => Some(format!("concept:{}", id.get("data")?.get("id")?.as_str()?)),
        "application" => Some(format!(
            "application:{}",
            id.get("data")?.get("id")?.as_str()?
        )),
        "occurrence" => {
            let data = id.get("data")?;
            Some(format!(
                "occurrence:{}:{}:{}",
                data.get("kind")?.as_str()?,
                data.get("content_hash")?.as_str()?,
                data.get("duplicate_index")?.as_u64()?
            ))
        }
        _ => None,
    }
}

fn apply_run_started(conn: &mut Connection, row: &LedgerRow, payload: &Value) -> Result<()> {
    let run_id = row
        .subject_refs
        .iter()
        .find(|r| r.starts_with("run:"))
        .map(|r| r.trim_start_matches("run:").to_string())
        .unwrap_or_default();
    let repo_id = row
        .subject_refs
        .iter()
        .find(|r| r.starts_with("repo:"))
        .map(|r| r.trim_start_matches("repo:").to_string())
        .unwrap_or_default();
    let checkout_id = row
        .subject_refs
        .iter()
        .find(|r| r.starts_with("checkout:"))
        .map(|r| r.trim_start_matches("checkout:").to_string())
        .unwrap_or_default();
    let snapshot_id = row
        .subject_refs
        .iter()
        .find(|r| r.starts_with("snapshot:"))
        .map(|r| r.trim_start_matches("snapshot:").to_string())
        .unwrap_or_default();
    let scope = payload.get("scope").cloned().unwrap_or(Value::Null);
    let git_commit = scope
        .get("git_commit")
        .and_then(Value::as_str)
        .map(str::to_string);
    let dirty = scope.get("dirty").and_then(Value::as_bool).unwrap_or(false) as i64;
    let started_at = payload
        .get("created_at")
        .and_then(Value::as_str)
        .map(str::to_string);
    conn.execute(
        "INSERT OR REPLACE INTO sxr_run(run_id, repo_id, checkout_id, git_commit, dirty, snapshot_id, started_at, finished_at, status, plan_id)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'started',?9)",
        params![run_id, repo_id, checkout_id, git_commit, dirty, snapshot_id, started_at, None::<String>, None::<String>],
    )?;
    Ok(())
}

fn apply_run_finished(conn: &mut Connection, row: &LedgerRow, payload: &Value) -> Result<()> {
    let run_id = row
        .subject_refs
        .iter()
        .find(|r| r.starts_with("run:"))
        .map(|r| r.trim_start_matches("run:").to_string())
        .unwrap_or_default();
    let finished_at = payload
        .get("created_at")
        .and_then(Value::as_str)
        .map(str::to_string);
    conn.execute(
        "UPDATE sxr_run SET status='finished', finished_at=?2 WHERE run_id=?1",
        params![run_id, finished_at],
    )?;
    Ok(())
}

fn apply_evidence(conn: &mut Connection, row: &LedgerRow, payload: &Value) -> Result<()> {
    let ev = first_section(payload, "evidence");
    let evidence_id = ev
        .and_then(|v| string_field(v, "evidence_id"))
        .unwrap_or_default();
    if evidence_id.is_empty() {
        return Ok(());
    }
    let question_id = row
        .subject_refs
        .iter()
        .find(|r| r.starts_with("question:"))
        .map(|r| r.trim_start_matches("question:").to_string())
        .unwrap_or_default();
    let run_id = row
        .subject_refs
        .iter()
        .find(|r| r.starts_with("run:"))
        .map(|r| r.trim_start_matches("run:").to_string())
        .unwrap_or_default();
    let target = row
        .subject_refs
        .iter()
        .find(|r| r.starts_with("target:"))
        .cloned()
        .unwrap_or_default();
    let target_ref = target.trim_start_matches("target:").to_string();
    // A target may be a declaration (concept:/occurrence:/application:) or a
    // claim (claim:<id>) — §12.4a condition 2 admits edges from evidence
    // targeting the relation claim itself.
    let target_kind = if target_ref.starts_with("concept:") {
        "concept"
    } else if target_ref.starts_with("claim:") {
        "claim"
    } else {
        "target"
    };
    let evidence_type = ev
        .and_then(|v| string_field(v, "evidence_type"))
        .unwrap_or_default();
    let relevance = ev
        .and_then(|v| string_field(v, "relevance"))
        .unwrap_or_default();
    let reliability = ev
        .and_then(|v| string_field(v, "reliability"))
        .unwrap_or_default();
    let direction = ev
        .and_then(|v| string_field(v, "direction"))
        .unwrap_or_default();
    let confidence_impact = ev
        .and_then(|v| string_field(v, "confidence_impact"))
        .unwrap_or_default();
    let representation_ref = ev
        .and_then(|v| string_field(v, "representation_ref"))
        .unwrap_or_default();
    conn.execute(
        "INSERT OR REPLACE INTO sxr_evidence(evidence_id, run_id, question_id, evidence_type, target_kind, target_ref, relevance, reliability, direction, confidence_impact, representation_ref, ledger_id, append_ordinal)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![evidence_id, run_id, question_id, evidence_type, target_kind, target_ref, relevance, reliability, direction, confidence_impact, representation_ref, row.ledger_id, row.append_ordinal],
    )?;
    // Map the evidence target into sxr_evidence_target. For a claim target,
    // the decl_key is `claim:<id>` so G_observed edge admission can join claims
    // to their qualifying evidence (§12.4a condition 2).
    let decl_key = target_ref;
    conn.execute(
        "INSERT OR REPLACE INTO sxr_evidence_target(evidence_id, decl_key) VALUES (?1,?2)",
        params![evidence_id, decl_key],
    )?;
    // FTS: evidence type + direction is searchable context (§12.2).
    fts_insert(conn, &evidence_id, "evidence", &evidence_type, &direction)?;
    Ok(())
}

fn apply_settlement(conn: &mut Connection, row: &LedgerRow, payload: &Value) -> Result<()> {
    let s = first_section(payload, "settlements");
    let settlement_id = s
        .and_then(|v| string_field(v, "settlement_id"))
        .unwrap_or_default();
    if settlement_id.is_empty() {
        return Ok(());
    }
    let question_id = row
        .subject_refs
        .iter()
        .find(|r| r.starts_with("question:"))
        .map(|r| r.trim_start_matches("question:").to_string())
        .unwrap_or_default();
    let transformation_ref = row
        .subject_refs
        .iter()
        .find(|r| r.starts_with("transformation:"))
        .map(|r| r.trim_start_matches("transformation:").to_string())
        .unwrap_or_default();
    let run_id = row
        .subject_refs
        .iter()
        .find(|r| r.starts_with("run:"))
        .map(|r| r.trim_start_matches("run:").to_string())
        .unwrap_or_default();
    let status = s
        .and_then(|v| string_field(v, "settlement_status"))
        .unwrap_or_default();
    let confidence = s
        .and_then(|v| string_field(v, "confidence"))
        .unwrap_or_else(|| "unknown".into());
    let threshold_id = s
        .and_then(|v| string_field(v, "threshold_ref"))
        .unwrap_or_default();
    conn.execute(
        "INSERT OR REPLACE INTO sxr_settlement(settlement_id, run_id, question_id, transformation_ref, threshold_id, settlement_status, confidence, debt_id, ledger_id, append_ordinal)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![settlement_id, run_id, question_id, transformation_ref, threshold_id, status, confidence, None::<String>, row.ledger_id, row.append_ordinal],
    )?;
    // FTS: settlement status is searchable context (§12.2).
    fts_insert(conn, &settlement_id, "settlement", &status, &confidence)?;
    Ok(())
}

/// Slice 4: a `claim.made` asserts a relation R(src, dst, edge_kind) under a
/// question (§12.4a condition 1). Stored for G_observed edge admission. The
/// CEP claim schema fixes the allowed fields, so the relation endpoints are
/// encoded in `claim_text_or_form` as `relation <src> -> <dst> kind:<kind>`.
fn apply_claim(conn: &mut Connection, row: &LedgerRow, payload: &Value) -> Result<()> {
    let c = first_section(payload, "claims");
    let claim_id = c
        .and_then(|v| string_field(v, "claim_id"))
        .unwrap_or_default();
    if claim_id.is_empty() {
        return Ok(());
    }
    let question_id = row
        .subject_refs
        .iter()
        .find(|r| r.starts_with("question:"))
        .map(|r| r.trim_start_matches("question:").to_string())
        .unwrap_or_default();
    let run_id = row
        .subject_refs
        .iter()
        .find(|r| r.starts_with("run:"))
        .map(|r| r.trim_start_matches("run:").to_string())
        .unwrap_or_default();
    // Parse the relation form: "relation <src> -> <dst> kind:<kind>".
    let form = c
        .and_then(|v| string_field(v, "claim_text_or_form"))
        .unwrap_or_default();
    let (src, dst, edge_kind) = parse_relation_form(&form);
    if src.is_empty() || dst.is_empty() {
        return Ok(());
    }
    conn.execute(
        "INSERT OR REPLACE INTO sxr_claim(claim_id, question_id, run_id, src_decl_key, dst_decl_key, edge_kind, ledger_id, append_ordinal)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![claim_id, question_id, run_id, src, dst, edge_kind, row.ledger_id, row.append_ordinal],
    )?;
    Ok(())
}

/// Parse a relation claim form `"relation <src> -> <dst> kind:<kind>"`.
fn parse_relation_form(form: &str) -> (String, String, String) {
    let rest = form.strip_prefix("relation ").unwrap_or(form);
    let (pair, kind) = match rest.split_once(" kind:") {
        Some((p, k)) => (p, k.to_string()),
        None => (rest, "relation".to_string()),
    };
    if let Some((s, d)) = pair.split_once(" -> ") {
        (s.trim().to_string(), d.trim().to_string(), kind)
    } else {
        (String::new(), String::new(), kind)
    }
}

/// Slice 4: materialize `G_observed` (§12.4) from qualified history. Called as
/// a post-rebuild pass because nodes/edges aggregate across many ledger rows.
///
/// **Nodes** (§12.4): every declaration with ≥1 evidence record. Fields:
/// first/last evidenced ordinal, evidence/settled/unsettled/debt counts. A
/// declared-but-unobserved decl has no observed node (absence = not observed).
///
/// **Edges** (§12.4a admission rule): an edge `(src, dst, kind)` enters
/// `G_observed` only if ALL hold:
/// 1. a `claim.made` asserts the relation (sxr_claim);
/// 2. ≥1 evidence targets that claim (`target:claim:<id>`, in
///    sxr_evidence_target as `claim:<id>`);
/// 3. that evidence meets the question's minimum relevance/reliability;
/// 4. the relation's settlement status is recorded on the edge.
///
/// Co-reference (evidence on A and on B, no claim) produces NO edge
/// (AC-GRAPH-001).
pub fn materialize_g_observed(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM sxr_obs_node", [])?;
    conn.execute("DELETE FROM sxr_obs_edge", [])?;

    // ── Nodes: aggregate evidence + settlement per targeted declaration ──
    // Only non-claim targets (concept:/occurrence:/application:/...) are
    // observed nodes; claim targets feed edges, not nodes.
    {
        let mut stmt = conn.prepare(
            "SELECT t.decl_key,
                    MIN(e.append_ordinal), MAX(e.append_ordinal),
                    COUNT(DISTINCT e.evidence_id)
             FROM sxr_evidence_target t
             JOIN sxr_evidence e ON e.evidence_id = t.evidence_id
             WHERE t.decl_key NOT LIKE 'claim:%'
             GROUP BY t.decl_key",
        )?;
        let node_rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for (decl_key, first_ord, last_ord, ev_count) in node_rows {
            // Settled/unsettled counts: settlements for questions whose evidence
            // targets this decl.
            let settled: i64 = conn.query_row(
                "SELECT COUNT(DISTINCT s.settlement_id)
                 FROM sxr_settlement s
                 JOIN sxr_evidence e ON e.question_id = s.question_id
                 JOIN sxr_evidence_target t ON t.evidence_id = e.evidence_id
                 WHERE t.decl_key = ?1 AND s.settlement_status = 'settled'",
                [&decl_key],
                |r| r.get(0),
            )?;
            let unsettled: i64 = conn.query_row(
                "SELECT COUNT(DISTINCT s.settlement_id)
                 FROM sxr_settlement s
                 JOIN sxr_evidence e ON e.question_id = s.question_id
                 JOIN sxr_evidence_target t ON t.evidence_id = e.evidence_id
                 WHERE t.decl_key = ?1 AND s.settlement_status = 'unsettled'",
                [&decl_key],
                |r| r.get(0),
            )?;
            let debt: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sxr_settlement WHERE debt_id IS NOT NULL",
                [],
                |r| r.get(0),
            )?;
            conn.execute(
                "INSERT OR REPLACE INTO sxr_obs_node(decl_key, first_evidenced_ordinal, last_evidenced_ordinal, evidence_count, settled_count, unsettled_count, debt_count)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![decl_key, first_ord, last_ord, ev_count, settled, unsettled, debt],
            )?;
        }
    }

    // ── Edges: §12.4a admission over claimed relations ──
    {
        let mut stmt = conn.prepare(
            "SELECT cl.claim_id, cl.question_id, cl.src_decl_key, cl.dst_decl_key, cl.edge_kind
             FROM sxr_claim cl",
        )?;
        let claims = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for (claim_id, question_id, src, dst, edge_kind) in claims {
            // Condition 2: ≥1 evidence targets this claim.
            // Condition 3: meets the question's minimum relevance/reliability
            // (threshold spec_json). If no threshold is declared, require the
            // evidence to be present (admission still needs qualifying evidence).
            let claim_target = format!("claim:{claim_id}");
            let min_relevance = question_min_relevance(conn, &question_id);
            let min_reliability = question_min_reliability(conn, &question_id);
            let mut ev_q = conn.prepare(
                "SELECT e.evidence_id, e.direction, e.relevance, e.reliability
                 FROM sxr_evidence_target t
                 JOIN sxr_evidence e ON e.evidence_id = t.evidence_id
                 WHERE t.decl_key = ?1",
            )?;
            let qualifying: Vec<(String, String)> = ev_q
                .query_map([&claim_target], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                })?
                .filter_map(|r| r.ok())
                .filter(|(_, _dir, rel, relbl)| {
                    meets_bar(rel.as_str(), min_relevance.as_deref())
                        && meets_bar(relbl.as_str(), min_reliability.as_deref())
                })
                .map(|(eid, dir, _, _)| (eid, dir))
                .collect();
            if qualifying.is_empty() {
                continue;
            }
            // Condition 4: settlement status for the relation's question.
            let (settlement_status, settlement_refs, confidence, contradiction_count) =
                relation_settlement(conn, &question_id, &qualifying);
            let evidence_refs: Vec<String> = qualifying.iter().map(|(e, _)| e.clone()).collect();
            conn.execute(
                "INSERT OR REPLACE INTO sxr_obs_edge(src_decl_key, dst_decl_key, edge_kind, claim_id, confidence, contradiction_count, settlement_status, evidence_refs_json, settlement_refs_json)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    src,
                    dst,
                    edge_kind,
                    claim_id,
                    confidence,
                    contradiction_count,
                    settlement_status,
                    serde_json::to_string(&evidence_refs)?,
                    serde_json::to_string(&settlement_refs)?,
                ],
            )?;
        }
    }
    Ok(())
}

/// The question's minimum relevance, read from its settlement threshold spec.
fn question_min_relevance(conn: &Connection, question_id: &str) -> Option<String> {
    conn.query_row(
        "SELECT spec_json FROM sxr_threshold WHERE question_id = ?1 LIMIT 1",
        [question_id],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .and_then(|s| serde_json::from_str::<Value>(&s).ok())
    .and_then(|v| string_field(&v, "minimum_relevance"))
}

/// The question's minimum reliability, read from its settlement threshold spec.
fn question_min_reliability(conn: &Connection, question_id: &str) -> Option<String> {
    conn.query_row(
        "SELECT spec_json FROM sxr_threshold WHERE question_id = ?1 LIMIT 1",
        [question_id],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .and_then(|s| serde_json::from_str::<Value>(&s).ok())
    .and_then(|v| string_field(&v, "minimum_reliability"))
}

/// Ordinal rank of a relevance/reliability label (higher = stronger).
fn label_rank(label: &str) -> i64 {
    match label {
        "critical" => 5,
        "high" => 4,
        "medium" => 3,
        "low" => 2,
        "unknown" => 1,
        _ => 0,
    }
}

/// Whether an evidence label meets (≥) a declared minimum bar.
fn meets_bar(label: &str, min: Option<&str>) -> bool {
    match min {
        Some(m) if !m.is_empty() => label_rank(label) >= label_rank(m),
        _ => true, // no threshold declared → admit if evidence exists
    }
}

/// Settlement status + refs for a claimed relation, derived from its
/// qualifying evidence and the question's settlements.
fn relation_settlement(
    conn: &Connection,
    question_id: &str,
    qualifying: &[(String, String)],
) -> (String, Vec<String>, String, i64) {
    let contradiction_count = qualifying
        .iter()
        .filter(|(_, d)| d == "contradicts")
        .count() as i64;
    let supports = qualifying.iter().any(|(_, d)| d == "supports");
    let mut settlement_refs: Vec<String> = Vec::new();
    let mut stmt = match conn.prepare(
        "SELECT settlement_id, settlement_status, confidence FROM sxr_settlement WHERE question_id = ?1",
    ) {
        Ok(s) => s,
        Err(_) => {
            let status = if supports { "settled" } else { "unsettled" };
            return (status.into(), settlement_refs, "unknown".into(), contradiction_count);
        }
    };
    let rows = stmt
        .query_map([question_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .ok();
    let mut status = "unsettled".to_string();
    let mut confidence = "unknown".to_string();
    if let Some(rows) = rows {
        for r in rows.flatten() {
            settlement_refs.push(r.0.clone());
            status = r.1;
            confidence = r.2;
        }
    }
    (status, settlement_refs, confidence, contradiction_count)
}

/// Rebuild all projections from scratch: drop the derived tables, re-create,
/// re-apply every ledger row in ordinal order.
pub fn rebuild(ctx: &crate::state::Ctx, repo_id: &str) -> Result<()> {
    let dir = ctx.ensure_repo_dir(repo_id)?;
    let db_path = dir.join("ledger.db");
    let mut conn = rusqlite::Connection::open(&db_path).context("open ledger db")?;
    drop_tables(&mut conn)?;
    conn.execute_batch(PROJECTION_DDL)
        .context("create projection tables")?;
    let rows = {
        // Reuse the ledger read path for hash-safe reads.
        let ledger = sxr_ledger::ledger::Ledger::open(
            &db_path,
            repo_id,
            sxr_ledger::ledger::LedgerConfig::default(),
        )
        .map_err(|e| anyhow!("open ledger: {e}"))?;
        ledger.read_all().map_err(|e| anyhow!("read all: {e}"))?
    };
    for row in &rows {
        apply_row(&mut conn, row).with_context(|| format!("project row {}", row.append_ordinal))?;
    }
    // Slice 4: G_observed is derived from qualified history (evidence +
    // settlement + claims), which spans multiple rows, so it is materialized
    // as a post-rebuild pass (§12.4/§12.4a).
    materialize_g_observed(&conn)?;
    // Set watermark to the head.
    if let Some(head) = rows.last() {
        conn.execute(
            "INSERT OR REPLACE INTO projection_watermark(name, last_ledger_id, last_append_ordinal, projection_version, updated_at)
             VALUES ('sxr_slice1', ?1, ?2, ?3, '')",
            params![head.ledger_id, head.append_ordinal, PROJECTION_VERSION],
        )?;
    }
    Ok(())
}

fn drop_tables(conn: &mut Connection) -> Result<()> {
    for t in [
        "sxr_snapshot",
        "sxr_run",
        "sxr_question",
        "sxr_threshold",
        "sxr_evidence",
        "sxr_evidence_target",
        "sxr_settlement",
        "sxr_claim",
        "sxr_obs_node",
        "sxr_obs_edge",
        "sxr_decl_node",
        "sxr_decl_edge",
    ] {
        conn.execute(&format!("DROP TABLE IF EXISTS {t}"), [])?;
    }
    conn.execute("DROP TABLE IF EXISTS sxr_fts", [])?;
    conn.execute("DROP TABLE IF EXISTS sxr_fts_content", [])?;
    Ok(())
}

/// Export projection rows as a canonical-ordered JSON array (byte-stable for
/// AC-REPLAY-002). Tables iterated in a fixed order; rows sorted by PK.
pub fn export_canonical(ctx: &crate::state::Ctx, repo_id: &str) -> Result<String> {
    let dir = ctx.ensure_repo_dir(repo_id)?;
    let conn = rusqlite::Connection::open(dir.join("ledger.db")).context("open ledger db")?;
    let mut out: Vec<Value> = Vec::new();
    for table in [
        "sxr_snapshot",
        "sxr_run",
        "sxr_question",
        "sxr_threshold",
        "sxr_evidence",
        "sxr_evidence_target",
        "sxr_settlement",
        "sxr_claim",
        "sxr_obs_node",
        "sxr_obs_edge",
        "sxr_decl_node",
        "sxr_decl_edge",
    ] {
        let order = match table {
            "sxr_evidence_target" => "evidence_id, decl_key",
            "sxr_decl_node" => "snapshot_id, decl_key",
            "sxr_decl_edge" => "snapshot_id, src_decl_key, dst_decl_key, edge_kind, field_path",
            "sxr_obs_edge" => "src_decl_key, dst_decl_key, edge_kind",
            _ => "1",
        };
        let sql = format!("SELECT * FROM {table} ORDER BY {order}");
        let mut stmt = conn
            .prepare(&sql)
            .with_context(|| format!("select {table}"))?;
        let ncols = stmt.column_count();
        let cols: Vec<String> = (0..ncols)
            .map(|i| stmt.column_name(i).unwrap_or("c").to_string())
            .collect();
        let rows = stmt.query_map([], |r| {
            let mut obj = serde_json::Map::new();
            for (i, c) in cols.iter().enumerate() {
                let val: Value = match r.get_ref(i).map_err(|e| anyhow!("{e}")) {
                    Ok(v) => match v {
                        rusqlite::types::ValueRef::Null => Value::Null,
                        rusqlite::types::ValueRef::Integer(n) => Value::from(n),
                        rusqlite::types::ValueRef::Real(x) => Value::from(x),
                        rusqlite::types::ValueRef::Text(t) => {
                            Value::String(String::from_utf8_lossy(t).into_owned())
                        }
                        rusqlite::types::ValueRef::Blob(b) => Value::String(hex::encode(b)),
                    },
                    Err(_) => Value::Null,
                };
                obj.insert(c.clone(), val);
            }
            Ok(Value::Object(obj))
        })?;
        for r in rows {
            out.push(r?);
        }
    }
    // stable serialization
    serde_json::to_string(&out).context("serialize canonical export")
}

/// Report projection lag + counts per table.
pub fn status(ctx: &crate::state::Ctx, repo_id: &str) -> Result<Value> {
    let dir = ctx.ensure_repo_dir(repo_id)?;
    let conn = rusqlite::Connection::open(dir.join("ledger.db")).context("open ledger db")?;
    let mut tables = serde_json::Map::new();
    for t in [
        "sxr_snapshot",
        "sxr_run",
        "sxr_question",
        "sxr_threshold",
        "sxr_evidence",
        "sxr_evidence_target",
        "sxr_settlement",
        "sxr_claim",
        "sxr_obs_node",
        "sxr_obs_edge",
        "sxr_decl_node",
        "sxr_decl_edge",
    ] {
        let n: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))
            .unwrap_or(0);
        tables.insert(t.into(), serde_json::json!(n));
    }
    let fts: i64 = conn
        .query_row("SELECT COUNT(*) FROM sxr_fts", [], |r| r.get(0))
        .unwrap_or(0);
    tables.insert("sxr_fts".into(), serde_json::json!(fts));
    let lag: i64 = conn
        .query_row(
            "SELECT (SELECT COALESCE(MAX(append_ordinal),0) FROM ledger) - COALESCE((SELECT last_append_ordinal FROM projection_watermark WHERE name='sxr_slice1'),0)",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    Ok(serde_json::json!({
        "projection_version": PROJECTION_VERSION,
        "tables": tables,
        "lag": lag,
    }))
}

// ── Slice 3: G_declared + FTS query support (§12.3, §12.2, §12.7) ──────────

/// Open the repo's ledger DB (projection tables live there).
pub fn open(ctx: &crate::state::Ctx, repo_id: &str) -> Result<Connection> {
    let dir = ctx.ensure_repo_dir(repo_id)?;
    rusqlite::Connection::open(dir.join("ledger.db")).context("open ledger db")
}

/// The latest snapshot_id for a repo that produced a valid `G_declared`
/// (has declaration nodes). Falls back to the newest snapshot row.
pub fn latest_g_declared_snapshot(conn: &Connection, repo_id: &str) -> Result<Option<String>> {
    let best = conn
        .query_row(
            "SELECT s.snapshot_id FROM sxr_snapshot s
             WHERE s.repo_id = ?1 AND s.model_validation_status = 'valid'
               AND EXISTS (SELECT 1 FROM sxr_decl_node n WHERE n.snapshot_id = s.snapshot_id)
             ORDER BY s.first_seen_append_ordinal DESC LIMIT 1",
            [repo_id],
            |r| r.get(0),
        )
        .optional()?;
    if best.is_some() {
        return Ok(best);
    }
    // Fallback: newest valid snapshot even without nodes.
    let any = conn
        .query_row(
            "SELECT snapshot_id FROM sxr_snapshot
             WHERE repo_id = ?1 AND model_validation_status = 'valid'
             ORDER BY first_seen_append_ordinal DESC LIMIT 1",
            [repo_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(any)
}

/// Resolve an entity argument to a scoped `decl_key` in the repo's current
/// `G_declared` snapshot. Accepts a raw `decl_key` (`concept:…`,
/// `occurrence:…`, `application:…`), `concept:<uuid>`, or `name:<Name>` /
/// `name::<Name>` / `name:<Namespace::<Name>>` looked up by declaration name.
/// Returns `(decl_key, snapshot_id, semantic_closure_hash)` or None.
pub fn resolve_decl_key(
    conn: &Connection,
    repo_id: &str,
    entity: &str,
) -> Result<Option<(String, String, String)>> {
    let Some(snapshot_id) = latest_g_declared_snapshot(conn, repo_id)? else {
        return Ok(None);
    };
    let semantic_hash: String = conn
        .query_row(
            "SELECT semantic_closure_hash FROM sxr_snapshot WHERE snapshot_id = ?1",
            [&snapshot_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten()
        .unwrap_or_default();

    // 1. Direct decl_key forms.
    let direct = if entity.starts_with("concept:")
        || entity.starts_with("occurrence:")
        || entity.starts_with("application:")
    {
        Some(entity.to_string())
    } else if entity.starts_with("name:") {
        None
    } else {
        // bare: treat as a name lookup fallback
        None
    };

    if let Some(dk) = direct {
        let found: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sxr_decl_node WHERE snapshot_id = ?1 AND decl_key = ?2)",
                params![snapshot_id, dk],
                |r| r.get(0),
            )
            .unwrap_or(false);
        if found {
            return Ok(Some((dk, snapshot_id, semantic_hash)));
        }
        return Ok(None);
    }

    // 2. name lookup: strip `name:` prefix, then match by name (and optional
    // namespace:: prefix).
    let name_arg = entity.strip_prefix("name:").unwrap_or(entity);
    let (want_ns, want_name) = if let Some((ns, n)) = name_arg.split_once("::") {
        (Some(ns.to_string()), n.to_string())
    } else {
        (None, name_arg.to_string())
    };
    let found = conn
        .query_row(
            "SELECT decl_key FROM sxr_decl_node
             WHERE snapshot_id = ?1 AND name = ?2
               AND (?3 IS NULL OR namespace = ?3)
             ORDER BY decl_key LIMIT 1",
            params![snapshot_id, want_name, want_ns],
            |r| r.get::<_, String>(0),
        )
        .optional()?;
    Ok(found.map(|dk| (dk, snapshot_id, semantic_hash)))
}

/// §12.7: answer "what evidence and settlements exist for entity X?" from
/// `G_declared` + evidence joins, with an honest `not_observed` reason when the
/// declaration exists in `G_declared` but has no qualifying evidence.
pub fn query_evidence_declared(conn: &Connection, repo_id: &str, entity: &str) -> Result<Value> {
    let Some((decl_key, snapshot_id, semantic_hash)) = resolve_decl_key(conn, repo_id, entity)?
    else {
        return Ok(json!({
            "decl_key": entity,
            "presence": "not_in_declared_graph",
            "evidence": [],
        }));
    };

    let mut evidence: Vec<Value> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT e.evidence_id, e.evidence_type, e.direction, e.relevance, e.reliability,
                    e.run_id, e.question_id, q.question_form,
                    s.settlement_id, s.settlement_status, s.confidence
             FROM sxr_evidence_target t
             JOIN sxr_evidence e ON e.evidence_id = t.evidence_id
             JOIN sxr_question q ON q.question_id = e.question_id
             LEFT JOIN sxr_settlement s ON s.question_id = e.question_id AND s.run_id = e.run_id
             WHERE t.decl_key = ?1
             ORDER BY e.append_ordinal",
        )?;
        let rows = stmt.query_map([&decl_key], |r| {
            Ok(json!({
                "evidence_id": r.get::<_, String>(0)?,
                "evidence_type": r.get::<_, String>(1)?,
                "direction": r.get::<_, String>(2)?,
                "relevance": r.get::<_, String>(3)?,
                "reliability": r.get::<_, String>(4)?,
                "run_id": r.get::<_, String>(5)?,
                "question_id": r.get::<_, String>(6)?,
                "question_form": r.get::<_, String>(7)?,
                "settlement_id": r.get::<_, Option<String>>(8)?,
                "settlement_status": r.get::<_, Option<String>>(9)?,
                "confidence": r.get::<_, Option<String>>(10)?,
            }))
        })?;
        for r in rows {
            evidence.push(r?);
        }
    }

    if !evidence.is_empty() {
        return Ok(json!({
            "repo_id": repo_id,
            "decl_key": decl_key,
            "snapshot_id": snapshot_id,
            "semantic_closure_hash": semantic_hash,
            "presence": "observed",
            "evidence": evidence,
        }));
    }

    // Declared but no qualifying evidence → honest not_observed (§12.4).
    let node = conn
        .query_row(
            "SELECT decl_kind, name, namespace FROM sxr_decl_node WHERE snapshot_id = ?1 AND decl_key = ?2",
            params![snapshot_id, decl_key],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
        )
        .optional()?;
    let (kind, name, ns) = node.unwrap_or_default();
    Ok(json!({
        "repo_id": repo_id,
        "decl_key": decl_key,
        "snapshot_id": snapshot_id,
        "semantic_closure_hash": semantic_hash,
        "decl_kind": kind,
        "name": name,
        "namespace": ns,
        "presence": "not_observed",
        "reason": "not_collected",
        "evidence": [],
    }))
}

/// Slice 4 §12.5: `G_delta` — question-bound typed residuals, NOT subtraction.
/// Computes the residual for each declaration in the Question's selected scope
/// over the given snapshot's `G_declared`, classifying each against qualified
/// history (evidence direction + settlement status).
///
/// Residual kinds (§12.5): `declared_unobserved` (in scope, in G_declared, no
/// qualifying evidence — NOT an error, severity "info"), `declared_contradicted`
/// (evidence contradicts), `contradictory` (both supports + contradicts above
/// threshold), `declared_unsettled` (evidence but threshold unmet),
/// `settled_with_debt` (settled + debt open), `observed_consistent` (evidence
/// supports, no residual), and `observed_undeclared` (G_observed edge with no
/// declared counterpart — candidate model gap). `unknown` when representation
/// is insufficient.
pub fn delta_question(
    conn: &Connection,
    repo_id: &str,
    question_id: &str,
    snapshot_id: &str,
) -> Result<Value> {
    let scope_json: Option<String> = conn
        .query_row(
            "SELECT scope_json FROM sxr_question WHERE question_id = ?1",
            [question_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten();
    let scope: Value = scope_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(Value::Null);

    // Resolve the Question's selected declaration set (§12.5).
    let mut target_decls: Vec<String> = Vec::new();
    if let Some(arr) = scope.get("target_entities").and_then(Value::as_array) {
        for v in arr {
            if let Some(s) = v.as_str() {
                target_decls.push(s.to_string());
            }
        }
    }
    // If the question names no targets, default to the snapshot's full declared
    // declaration set (§12.5: "the declaration set that Question selects").
    if target_decls.is_empty() {
        let mut stmt = conn.prepare(
            "SELECT decl_key FROM sxr_decl_node WHERE snapshot_id = ?1 ORDER BY decl_key",
        )?;
        target_decls = stmt
            .query_map([snapshot_id], |r| r.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
    }

    let min_relevance = question_min_relevance(conn, question_id);
    let min_reliability = question_min_reliability(conn, question_id);

    let mut residuals: Vec<Value> = Vec::new();
    for decl_key in &target_decls {
        residuals.push(classify_declared(
            conn,
            question_id,
            decl_key,
            &min_relevance,
            &min_reliability,
        )?);
    }

    // observed_undeclared: G_observed edges whose relation has no declared
    // counterpart in this snapshot (candidate model gap) — §12.5.
    let mut undeclared: Vec<Value> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT e.src_decl_key, e.dst_decl_key, e.edge_kind
             FROM sxr_obs_edge e
             WHERE NOT EXISTS (
                 SELECT 1 FROM sxr_decl_edge d
                 WHERE d.snapshot_id = ?1
                   AND d.src_decl_key = e.src_decl_key
                   AND d.dst_decl_key = e.dst_decl_key
             )",
        )?;
        let rows = stmt.query_map([snapshot_id], |r| {
            Ok(json!({
                "src_decl_key": r.get::<_, String>(0)?,
                "dst_decl_key": r.get::<_, String>(1)?,
                "edge_kind": r.get::<_, String>(2)?,
            }))
        })?;
        for v in rows.flatten() {
            let mut u = v;
            u["residual_kind"] = json!("observed_undeclared");
            u["severity"] = json!("warning");
            undeclared.push(u);
        }
    }

    Ok(json!({
        "repo_id": repo_id,
        "question_id": question_id,
        "snapshot_id": snapshot_id,
        "scope_size": target_decls.len(),
        "minimum_relevance": min_relevance,
        "minimum_reliability": min_reliability,
        "residuals": residuals,
        "observed_undeclared": undeclared,
    }))
}

/// Classify one declared declaration against qualified history (§12.5).
fn classify_declared(
    conn: &Connection,
    question_id: &str,
    decl_key: &str,
    min_relevance: &Option<String>,
    min_reliability: &Option<String>,
) -> Result<Value> {
    let mut stmt = conn.prepare(
        "SELECT e.evidence_id, e.direction, e.relevance, e.reliability
         FROM sxr_evidence_target t
         JOIN sxr_evidence e ON e.evidence_id = t.evidence_id
         WHERE t.decl_key = ?1 AND e.question_id = ?2",
    )?;
    let mut supports = false;
    let mut contradicts = false;
    let mut any_evidence = false;
    let ev_rows = stmt
        .query_map(params![decl_key, question_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (_eid, dir, rel, relbl) in &ev_rows {
        if meets_bar(rel, min_relevance.as_deref()) && meets_bar(relbl, min_reliability.as_deref())
        {
            any_evidence = true;
            if dir == "supports" {
                supports = true;
            }
            if dir == "contradicts" {
                contradicts = true;
            }
        }
    }

    // Settlement + debt status for this question.
    let (settled, debt_open) = settlement_state(conn, question_id);

    let (kind, severity) = if !any_evidence {
        // §12.5: declared_unobserved is the default state of an honest system
        // and must never be coloured red.
        ("declared_unobserved", "info")
    } else if supports && contradicts {
        ("contradictory", "error")
    } else if contradicts {
        ("declared_contradicted", "error")
    } else if settled && debt_open {
        ("settled_with_debt", "warning")
    } else if !settled {
        ("declared_unsettled", "warning")
    } else {
        ("observed_consistent", "info")
    };

    Ok(json!({
        "decl_key": decl_key,
        "residual_kind": kind,
        "severity": severity,
        "supports": supports,
        "contradicts": contradicts,
        "evidence_count": ev_rows.len(),
    }))
}

/// (settled?, debt_open?) for a question's settlements.
fn settlement_state(conn: &Connection, question_id: &str) -> (bool, bool) {
    let settled: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sxr_settlement WHERE question_id = ?1 AND settlement_status = 'settled')",
            [question_id],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n != 0)
        .unwrap_or(false);
    let debt_open: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sxr_settlement WHERE question_id = ?1 AND debt_id IS NOT NULL)",
            [question_id],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n != 0)
        .unwrap_or(false);
    (settled, debt_open)
}

/// FTS5 search over the seeded corpus (§12.2). Contentless-external-content:
/// the index stores no text, so values are read back from `sxr_fts_content`
/// via the rowid, keeping every result traceable to its source record (§12.6).
pub fn fts_search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<Value>> {
    // Quote the term so FTS5 does not parse `-`, `:`, `.` etc. as operators;
    // append `*` for a prefix match.
    let q = format!(r#""{query}"*"#);
    let mut stmt = conn.prepare(
        "SELECT c.subject_key, c.kind, c.title, c.body
         FROM sxr_fts f JOIN sxr_fts_content c ON c.rowid = f.rowid
         WHERE sxr_fts MATCH ?1
         ORDER BY rank LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![q, limit as i64], |r| {
        Ok(json!({
            "subject_key": r.get::<_, String>(0)?,
            "kind": r.get::<_, String>(1)?,
            "title": r.get::<_, String>(2)?,
            "body": r.get::<_, String>(3)?,
        }))
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// 1-and-2-hop neighbours of a decl_key in `G_declared`, annotated with their
/// observed status (§17.1 graph retriever). Returns neighbour records.
pub fn neighbours(
    conn: &Connection,
    snapshot_id: &str,
    decl_key: &str,
    hop: u32,
) -> Result<Vec<Value>> {
    let mut out: Vec<Value> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut frontier: Vec<(String, u32)> = vec![(decl_key.to_string(), 0)];
    while let Some((cur, cur_hop)) = frontier.pop() {
        if cur_hop >= hop {
            continue;
        }
        // outgoing edges
        let mut stmt = conn.prepare(
            "SELECT dst_decl_key, edge_kind FROM sxr_decl_edge
             WHERE snapshot_id = ?1 AND src_decl_key = ?2",
        )?;
        let rows = stmt.query_map(params![snapshot_id, cur], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for r in rows {
            let (dst, edge) = r?;
            let key = format!("{dst}:{edge}");
            if seen.insert(key) {
                let presence = observed_presence(conn, &dst);
                out.push(json!({
                    "decl_key": dst,
                    "hop": cur_hop + 1,
                    "edge_kind": edge,
                    "presence": presence.0,
                    "reason": presence.1,
                }));
                frontier.push((dst, cur_hop + 1));
            }
        }
        // incoming edges
        let mut istmt = conn.prepare(
            "SELECT src_decl_key, edge_kind FROM sxr_decl_edge
             WHERE snapshot_id = ?1 AND dst_decl_key = ?2",
        )?;
        let irows = istmt.query_map(params![snapshot_id, cur], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for r in irows {
            let (src, edge) = r?;
            let key = format!("{src}:{edge}");
            if seen.insert(key) {
                let presence = observed_presence(conn, &src);
                out.push(json!({
                    "decl_key": src,
                    "hop": cur_hop + 1,
                    "edge_kind": edge,
                    "presence": presence.0,
                    "reason": presence.1,
                }));
                frontier.push((src, cur_hop + 1));
            }
        }
    }
    Ok(out)
}

/// Presence of a decl_key in the evidence tables: `observed` if any evidence
/// targets it, else `not_observed` with reason `not_collected`.
fn observed_presence(conn: &Connection, decl_key: &str) -> (String, String) {
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sxr_evidence_target WHERE decl_key = ?1",
            [decl_key],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if n > 0 {
        ("observed".to_string(), String::new())
    } else {
        ("not_observed".to_string(), "not_collected".to_string())
    }
}

/// Canonical byte-stable export over a caller-owned connection (test helper +
/// reusable by `export_canonical`).
#[allow(dead_code)]
pub fn export_canonical_conn(conn: &Connection) -> Result<String> {
    let mut out: Vec<Value> = Vec::new();
    for table in [
        "sxr_snapshot",
        "sxr_run",
        "sxr_question",
        "sxr_threshold",
        "sxr_evidence",
        "sxr_evidence_target",
        "sxr_settlement",
        "sxr_claim",
        "sxr_obs_node",
        "sxr_obs_edge",
        "sxr_decl_node",
        "sxr_decl_edge",
    ] {
        let order = match table {
            "sxr_evidence_target" => "evidence_id, decl_key",
            "sxr_decl_node" => "snapshot_id, decl_key",
            "sxr_decl_edge" => "snapshot_id, src_decl_key, dst_decl_key, edge_kind, field_path",
            "sxr_obs_edge" => "src_decl_key, dst_decl_key, edge_kind",
            _ => "1",
        };
        let sql = format!("SELECT * FROM {table} ORDER BY {order}");
        let mut stmt = conn.prepare(&sql)?;
        let ncols = stmt.column_count();
        let cols: Vec<String> = (0..ncols)
            .map(|i| stmt.column_name(i).unwrap_or("c").to_string())
            .collect();
        let rows = stmt.query_map([], |r| {
            let mut obj = serde_json::Map::new();
            for (i, c) in cols.iter().enumerate() {
                let val: Value = match r.get_ref(i) {
                    Ok(rusqlite::types::ValueRef::Null) => Value::Null,
                    Ok(rusqlite::types::ValueRef::Integer(n)) => Value::from(n),
                    Ok(rusqlite::types::ValueRef::Real(x)) => Value::from(x),
                    Ok(rusqlite::types::ValueRef::Text(t)) => {
                        Value::String(String::from_utf8_lossy(t).into_owned())
                    }
                    Ok(rusqlite::types::ValueRef::Blob(b)) => Value::String(hex::encode(b)),
                    Err(_) => Value::Null,
                };
                obj.insert(c.clone(), val);
            }
            Ok(Value::Object(obj))
        })?;
        for r in rows {
            out.push(r?);
        }
    }
    serde_json::to_string(&out).context("serialize canonical export")
}

/// Drop the Slice-3 projection tables (test helper; `rebuild` re-applies).
#[allow(dead_code)]
pub fn drop_tables_test(conn: &mut Connection) -> Result<()> {
    drop_tables(conn)
}

/// Slice 4: export sxr's records as an external anti-collapse corpus
/// (`cep/anti-collapse-corpus@1`) for independent evaluation by
/// `cep conformance anti-collapse --corpus <path>`.
///
/// Each delta residual becomes a `submitted_answer` that faithfully encodes
/// sxr's honest declared↔observed position in the dialect the CEP evaluator
/// reads (gap_status, gap_summary, declared/observed state refs, evidence_refs,
/// confidence, recommended action, limitations). sxr does NOT evaluate its own
/// semantics; it only translates its records into the corpus transport shape.
/// The CEP evaluator is the semantic authority.
pub fn export_anti_collapse_corpus(
    conn: &Connection,
    repo_id: &str,
    question_id: &str,
    snapshot_id: &str,
) -> Result<Value> {
    let delta = delta_question(conn, repo_id, question_id, snapshot_id)?;
    let residuals = delta["residuals"].as_array().cloned().unwrap_or_default();

    let mut records: Vec<Value> = Vec::new();
    for r in &residuals {
        let decl_key = r["decl_key"].as_str().unwrap_or("");
        let kind = r["residual_kind"].as_str().unwrap_or("unknown");
        let contradicts = r["contradicts"].as_bool().unwrap_or(false);
        let supports = r["supports"].as_bool().unwrap_or(false);

        // Faithful mapping of sxr's residual → the anti-collapse answer dialect.
        // declared_unobserved: honestly "gap detected, evidence missing" — NOT
        // "no gap". declared_contradicted: honestly a contradiction. These must
        // not collapse declared↔observed or treat unknown as false.
        let (gap_status, gap_summary, confidence_value, action_type) = match kind {
            "declared_unobserved" => (
                "gap_detected",
                format!(
                    "Declared state {decl_key} has no qualifying observed evidence; \
                     declared and observed differ."
                ),
                "low",
                "collect_evidence",
            ),
            "declared_contradicted" | "contradictory" => (
                "gap_detected",
                format!(
                    "Declared state {decl_key} is contradicted by observed evidence; \
                     declared and observed disagree."
                ),
                "high",
                "investigate_contradiction",
            ),
            "declared_unsettled" => (
                "gap_detected",
                format!("Declared state {decl_key} has evidence but the threshold is unmet."),
                "medium",
                "collect_evidence",
            ),
            "settled_with_debt" => (
                "no_gap_detected",
                format!("Declared state {decl_key} is settled with open debt."),
                "medium",
                "resolve_debt",
            ),
            _ => (
                "no_gap_detected",
                format!("Declared state {decl_key} is observed and consistent."),
                "high",
                "no_action_required",
            ),
        };

        // Evidence refs that actually exist (qualifying), so the evaluator's
        // claim_as_evidence check has real evidence ids to match against.
        let ev_refs: Vec<String> = if supports || contradicts {
            evidence_ids_for(conn, question_id, decl_key)
        } else {
            Vec::new()
        };
        let valid_evidence_ids = ev_refs.clone();

        records.push(json!({
            "record_id": format!("residual:{decl_key}"),
            "record_ref": format!("sxr://delta/{question_id}/{decl_key}"),
            "accepted_gap_statuses": ["gap_detected"],
            "valid_evidence_ids": valid_evidence_ids,
            "submitted_answer": {
                "answer_id": format!("ans-{decl_key}"),
                "gap_status": gap_status,
                "gap_summary": gap_summary,
                "declared_state_refs": [format!("declared:{decl_key}")],
                "observed_state_refs": ev_refs
                    .iter()
                    .map(|e| format!("observed:{e}"))
                    .collect::<Vec<_>>(),
                "evidence_refs": ev_refs,
                "confidence": {
                    "value": confidence_value,
                    "basis": "sxr residual classification from qualified history",
                },
                "recommended_next_action": {
                    "type": action_type,
                    "description": "sxr residual recommendation; no ungrounded claim.",
                },
                "limitations": [
                    "The observed evidence does not prove the declared state.",
                ],
            },
        }));
    }

    Ok(json!({
        "$schema": "cep/anti-collapse-corpus@1",
        "corpus_id": format!("sxr-slice4-{repo_id}-{question_id}"),
        "records": records,
    }))
}

/// Evidence ids targeting a declaration under a question (qualifying).
fn evidence_ids_for(conn: &Connection, question_id: &str, decl_key: &str) -> Vec<String> {
    let mut stmt = match conn.prepare(
        "SELECT e.evidence_id FROM sxr_evidence_target t
         JOIN sxr_evidence e ON e.evidence_id = t.evidence_id
         WHERE t.decl_key = ?1 AND e.question_id = ?2",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map(params![decl_key, question_id], |r| r.get::<_, String>(0))
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
}

//! Slice-1 projections (§12): derived relational tables rebuilt from the
//! ledger. The projector interprets each envelope payload by record kind and
//! upserts rows; `rebuild --all` drops and re-applies in predecessor-chain
//! order so `export --canonical-json` is byte-identical across rebuilds
//! (AC-REPLAY-002).

use anyhow::{Context, Result, anyhow};
use rusqlite::{Connection, params};
use serde_json::Value;
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
    let scope = payload.get("scope").cloned().unwrap_or(Value::Null);
    conn.execute(
        "INSERT OR REPLACE INTO sxr_question(question_id, question_form, question_type, scope_json, ledger_id, append_ordinal)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![question_id, form, qtype, serde_json::to_string(&scope)?, row.ledger_id, row.append_ordinal],
    )?;
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
    let _st = first_section(payload, "states");
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
    Ok(())
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
    let target_kind = if target_ref.starts_with("concept:") {
        "concept"
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
    let decl_key = target_ref;
    conn.execute(
        "INSERT OR REPLACE INTO sxr_evidence_target(evidence_id, decl_key) VALUES (?1,?2)",
        params![evidence_id, decl_key],
    )?;
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
    Ok(())
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
    ] {
        conn.execute(&format!("DROP TABLE IF EXISTS {t}"), [])?;
    }
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
    ] {
        let order = match table {
            "sxr_evidence_target" => "evidence_id, decl_key",
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
    ] {
        let n: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))
            .unwrap_or(0);
        tables.insert(t.into(), serde_json::json!(n));
    }
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

//! Open a repo's SQLite ledger under the writer lock and append validated
//! envelope records. All writes go through the single durable writer.

use anyhow::{Context, Result, anyhow};
use sxr_core::LedgerPosition;
use sxr_core::record_kinds::RecordKind;
use sxr_ledger::AppendOutcome;
use sxr_ledger::ledger::{Ledger, LedgerConfig, PendingEntry};

use crate::state::Ctx;

/// A held writer lock + open ledger for one repo.
pub struct RepoLedger {
    pub ledger: Ledger,
    pub _lock: sxr_ledger::writer_lock::WriterLock,
}

/// Open the repo ledger, acquiring the writer flock (blocking, 30 s).
pub fn open_repo_ledger(ctx: &Ctx, repo_id: &str) -> Result<RepoLedger> {
    let dir = ctx.ensure_repo_dir(repo_id)?;
    let lock_path = dir.join("writer.lock");
    let lock = sxr_ledger::writer_lock::WriterLock::acquire(&lock_path, 30)
        .map_err(|e| anyhow!("write lock on {}: {e}", lock_path.display()))?;
    let db_path = dir.join("ledger.db");
    let ledger = Ledger::open(&db_path, repo_id, LedgerConfig::default())
        .map_err(|e| anyhow!("open ledger for {repo_id}: {e}"))?;
    Ok(RepoLedger {
        ledger,
        _lock: lock,
    })
}

/// Append a single validated envelope record. Returns the committed position.
pub fn append_record(
    repo: &mut RepoLedger,
    kind: RecordKind,
    idempotency_key: Option<String>,
    subject_refs: Vec<String>,
    payload: &serde_json::Value,
) -> Result<LedgerPosition> {
    let payload_json = serde_json::to_string(payload).context("serialize envelope payload")?;
    let entry = PendingEntry {
        record_kind: kind.as_str().into(),
        idempotency_key,
        subject_refs,
        payload: payload_json,
        entry_ulid: None,
        committed_at: None,
    };
    match repo
        .ledger
        .append(&entry)
        .map_err(|e| anyhow!("append {}: {e}", kind.as_str()))?
    {
        AppendOutcome::Committed { position, .. } => Ok(position),
        AppendOutcome::AlreadyCommitted { position } => Ok(position),
    }
}

/// Read all ledger rows for a repo.
pub fn read_all(ctx: &Ctx, repo_id: &str) -> Result<Vec<sxr_core::LedgerRow>> {
    let dir = ctx.ensure_repo_dir(repo_id)?;
    let db_path = dir.join("ledger.db");
    let ledger = Ledger::open(&db_path, repo_id, LedgerConfig::default())
        .map_err(|e| anyhow!("open: {e}"))?;
    ledger.read_all().map_err(|e| anyhow!("read: {e}"))
}

pub fn ledger_position_json(pos: &LedgerPosition) -> serde_json::Value {
    serde_json::json!({
        "ledger_id": pos.ledger_id,
        "append_ordinal": pos.append_ordinal,
    })
}

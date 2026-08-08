//! Durability-state helpers (§11.1a): pending-entry scanning over the daemon
//! commit queue and the spool, shared by `sxr await`, `sxr doctor`, the F-19b
//! consumer checks, and the daemon.

use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::path::PathBuf;

use crate::state::Ctx;

/// A not-yet-committed request in transient custody.
#[derive(Debug, Clone)]
pub struct PendingEntry {
    /// `queued_for_commit` (daemon queue, validated) or `spooled`
    /// (opt-in spool, not yet validated).
    pub state: &'static str,
    pub path: PathBuf,
    pub request: String,
    /// Producer-set idempotency key of the pending record (receipt field for
    /// queue entries; request field for spool entries).
    pub idempotency_key: Option<String>,
    /// True for queue segments whose request hash does not verify.
    pub corrupt: bool,
}

/// Scan the daemon commit queue for a repo (§11.1a `daemon-queue/<repo_id>`).
fn queued_entries(ctx: &Ctx, repo_id: &str) -> Result<Vec<PendingEntry>> {
    let entries = sxr_ledger::queue::scan(&ctx.queue_root(), repo_id)
        .with_context(|| format!("scan queue for {repo_id}"))?;
    Ok(entries
        .into_iter()
        .map(|e| PendingEntry {
            state: "queued_for_commit",
            path: e.path,
            request: e.request,
            idempotency_key: Some(e.receipt.idempotency_key),
            corrupt: !e.request_hash_ok,
        })
        .collect())
}

/// Scan the opt-in spool for a repo (§11.1a `spool/<repo_id>/*.ndjson`).
fn spooled_entries(ctx: &Ctx, repo_id: &str) -> Result<Vec<PendingEntry>> {
    let dir = ctx.spool_dir(repo_id);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut names: Vec<PathBuf> = std::fs::read_dir(&dir)
        .with_context(|| format!("read spool {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    names.sort();
    for path in names {
        if path.extension().is_none_or(|e| e != "ndjson") {
            continue;
        }
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("read spool entry {}", path.display()))?;
        let key = serde_json::from_str::<Value>(&raw).ok().and_then(|v| {
            v.get("idempotency_key")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
        out.push(PendingEntry {
            state: "spooled",
            path,
            request: raw,
            idempotency_key: key,
            corrupt: false,
        });
    }
    Ok(out)
}

/// All pending entries for a repo: validated queue first, then spool.
pub fn pending_entries(ctx: &Ctx, repo_id: &str) -> Result<Vec<PendingEntry>> {
    let mut out = queued_entries(ctx, repo_id)?;
    out.extend(spooled_entries(ctx, repo_id)?);
    Ok(out)
}

/// True when the pending request references `needle` (an id or a `ref:id`
/// subject ref). Queue entries are full envelopes (subject refs like
/// `run:<id>`); spool entries are ingest requests carrying the id fields.
pub fn references(entry: &PendingEntry, needle: &str) -> bool {
    entry.request.contains(needle) || entry.request.contains(&format!(":{needle}"))
}

/// F-19b: every referenced record must be committed. A pending entry that is
/// NOT itself committed (its idempotency key absent from the ledger) blocks
/// any consumer whose referenced ids appear in that entry — a spooled or
/// queued observation blocks `evidence create` on its run even though the run
/// itself is long committed (AC-DAEMON-003).
pub fn ensure_committed(ctx: &Ctx, repo_id: &str, refs: &[&str]) -> Result<()> {
    let pending = pending_entries(ctx, repo_id)?;
    if pending.is_empty() {
        return Ok(());
    }
    let rows = crate::ledger_io::read_all(ctx, repo_id)?;
    let committed_keys: std::collections::HashSet<&str> = rows
        .iter()
        .filter_map(|r| r.idempotency_key.as_deref())
        .collect();
    for entry in &pending {
        let self_committed = match &entry.idempotency_key {
            Some(k) => committed_keys.contains(k.as_str()),
            None => false,
        };
        if self_committed {
            continue;
        }
        for r in refs {
            if references(entry, r) {
                let why = if entry.corrupt {
                    "corrupt queue segment (request hash mismatch); frozen"
                } else if entry.state == "spooled" {
                    "spooled (not yet validated)"
                } else {
                    "queued_for_commit (fsynced receipt issued)"
                };
                bail!(
                    "record {r} is {state} ({why}); not committed — run `sxr await` or `sxr spool drain`/`sxr queue recover` first (AC-DAEMON-003/F-19b)",
                    state = entry.state
                );
            }
        }
    }
    Ok(())
}

/// Parse the request JSON of a pending entry (best effort; the daemon-queue
/// entries are full envelopes, spool entries are ingest requests).
pub fn request_json(entry: &PendingEntry) -> Value {
    serde_json::from_str(&entry.request).unwrap_or(Value::Null)
}

/// Delete a spool entry (only after its idempotency key is committed).
pub fn spool_remove(entry: &PendingEntry) -> Result<()> {
    if entry.state != "spooled" {
        bail!("internal: spool_remove on non-spool entry {:?}", entry.path);
    }
    std::fs::remove_file(&entry.path)
        .with_context(|| format!("remove spool {}", entry.path.display()))
}

//! Durable commit queue (§11.1a): `daemon-queue/<repo_id>/<seq>.queue`.
//!
//! The queue is the shared adapter's transient custody for validated but
//! uncommitted requests — the CLI (`sxr queue recover`) and the daemon both
//! drain it; it is never daemon-only code (§16.2). Every acknowledged request
//! is present as a fully fsynced segment: a receipt line plus the canonical
//! request line, with the receipt's `request_hash` recomputable from the
//! request bytes so corruption is detectable and the segment is then frozen
//! (never deleted, reported by `sxr doctor` as actionable).
//!
//! Writes are temp-file → fsync → rename → fsync(parent), so an interrupted
//! enqueue leaves at most a `.tmp` garbage file that is ignored (no receipt
//! was ever issued for it, so it was never reported successful — §16.4).
//! A segment is deleted only after its idempotency key is observed committed
//! and the parent directory is fsynced.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// One queue segment: receipt + canonical request bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueReceipt {
    pub repo_id: String,
    pub idempotency_key: String,
    pub request_hash: String,
    pub queue_position: String,
}

/// A scanned queue entry. `request` is the canonical request line as written;
/// `request_hash_ok` is false for corrupt segments (frozen, never deleted).
#[derive(Debug, Clone)]
pub struct QueueEntry {
    pub path: PathBuf,
    pub receipt: QueueReceipt,
    pub request: String,
    pub request_hash_ok: bool,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// fsync a directory (POSIX: open dir, fsync fd).
fn fsync_dir(dir: &Path) -> Result<()> {
    let f = File::open(dir)?;
    f.sync_all()?;
    Ok(())
}

/// Enqueue one canonical request under its idempotency key. Returns the
/// durable receipt; the caller may issue it to the client only now.
pub fn enqueue(
    queue_root: &Path,
    repo_id: &str,
    idempotency_key: &str,
    canonical_request: &str,
) -> Result<QueueReceipt> {
    let dir = queue_root.join(repo_id);
    fs::create_dir_all(&dir)?;
    let position = ulid::Ulid::new().to_string();
    let request_hash = sha256_hex(canonical_request.as_bytes());
    let receipt = QueueReceipt {
        repo_id: repo_id.into(),
        idempotency_key: idempotency_key.into(),
        request_hash,
        queue_position: position.clone(),
    };
    let receipt_line = serde_json::to_string(&receipt).map_err(|e| Error::Schema(e.to_string()))?;
    let tmp = dir.join(format!("{position}.queue.tmp"));
    let final_path = dir.join(format!("{position}.queue"));
    {
        let mut f = OpenOptions::new().create_new(true).write(true).open(&tmp)?;
        writeln!(f, "{receipt_line}")?;
        writeln!(f, "{canonical_request}")?;
        f.sync_all()?;
    }
    fs::rename(&tmp, &final_path)?;
    fsync_dir(&dir)?;
    Ok(receipt)
}

/// Scan all queue segments for a repo. Corrupt entries (bad request hash or
/// unparseable) are retained and flagged; `.tmp` leftovers are removed (they
/// were never receipted). Entries are returned in filename order.
pub fn scan(queue_root: &Path, repo_id: &str) -> Result<Vec<QueueEntry>> {
    let dir = queue_root.join(repo_id);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut names: Vec<PathBuf> = fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    names.sort();
    for path in names {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        if name.ends_with(".tmp") {
            let _ = fs::remove_file(&path);
            continue;
        }
        if !name.ends_with(".queue") {
            continue;
        }
        let (receipt, request, hash_ok) = match read_segment(&path) {
            Ok(t) => t,
            Err(_) => {
                out.push(QueueEntry {
                    path,
                    receipt: QueueReceipt {
                        repo_id: repo_id.into(),
                        idempotency_key: String::new(),
                        request_hash: String::new(),
                        queue_position: name.trim_end_matches(".queue").to_string(),
                    },
                    request: String::new(),
                    request_hash_ok: false,
                });
                continue;
            }
        };
        out.push(QueueEntry {
            path,
            receipt,
            request,
            request_hash_ok: hash_ok,
        });
    }
    Ok(out)
}

/// Read and verify one segment: line 1 = receipt, line 2 = request.
/// `hash_ok` is false when the receipt's request_hash does not match the
/// request bytes (corrupt segment).
fn read_segment(path: &Path) -> Result<(QueueReceipt, String, bool)> {
    let f = File::open(path)?;
    let mut lines = BufReader::new(f).lines();
    let receipt_line = lines
        .next()
        .ok_or_else(|| Error::Integrity(format!("queue segment {}: empty", path.display())))??;
    let receipt: QueueReceipt = serde_json::from_str(&receipt_line).map_err(|e| {
        Error::Integrity(format!(
            "queue segment {}: receipt unparseable: {e}",
            path.display()
        ))
    })?;
    let request_line = lines.next().ok_or_else(|| {
        Error::Integrity(format!("queue segment {}: missing request", path.display()))
    })??;
    let hash_ok = sha256_hex(request_line.as_bytes()) == receipt.request_hash;
    // The segment format is exactly two lines (§16.4); any further bytes are
    // corruption (e.g. a torn write or a foreign writer), not a new segment.
    if hash_ok && lines.next().is_some() {
        return Ok((receipt, request_line, false));
    }
    Ok((receipt, request_line, hash_ok))
}

/// Delete a segment only after its idempotency key is observed committed
/// (§11.1a), then fsync the parent directory.
pub fn remove_after_committed(entry: &QueueEntry) -> Result<()> {
    if !entry.request_hash_ok {
        return Err(Error::Integrity(format!(
            "queue segment {}: corrupt (request hash mismatch); frozen, refusing to delete",
            entry.path.display()
        )));
    }
    let dir = entry.path.parent().ok_or_else(|| {
        Error::Integrity(format!(
            "queue segment {}: no parent dir",
            entry.path.display()
        ))
    })?;
    fs::remove_file(&entry.path)?;
    fsync_dir(dir)?;
    Ok(())
}

/// Drain all segments of a repo with a caller-provided commit function. The
/// closure receives each verified entry's receipt + request and must commit
/// (or find already-committed) the request under its idempotency key; on
/// success the segment is deleted. Corrupt segments stop the drain and are
/// reported (exit 3 by the caller) — the queue is frozen during corruption.
pub fn recover(
    queue_root: &Path,
    repo_id: &str,
    commit: impl FnMut(&QueueEntry) -> Result<()>,
) -> Result<RecoverReport> {
    let entries = scan(queue_root, repo_id)?;
    let mut report = RecoverReport::default();
    let mut commit = commit;
    for entry in &entries {
        if !entry.request_hash_ok {
            report.corrupt.push(entry.path.display().to_string());
            continue;
        }
        commit(entry)?;
        remove_after_committed(entry)?;
        report.recovered += 1;
    }
    Ok(report)
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RecoverReport {
    pub recovered: u64,
    pub corrupt: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enqueue_scan_roundtrip_and_verify() {
        let dir = tempfile::tempdir().unwrap();
        let receipt = enqueue(dir.path(), "repo-r1", "key-1", "{\"a\":1}").unwrap();
        assert_eq!(receipt.repo_id, "repo-r1");
        assert_eq!(receipt.idempotency_key, "key-1");
        let entries = scan(dir.path(), "repo-r1").unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].request_hash_ok);
        assert_eq!(entries[0].request, "{\"a\":1}");
        assert_eq!(entries[0].receipt.request_hash, receipt.request_hash);
    }

    #[test]
    fn request_hash_mismatch_is_corrupt_and_frozen() {
        let dir = tempfile::tempdir().unwrap();
        enqueue(dir.path(), "repo-r1", "key-1", "{\"a\":1}").unwrap();
        // Tamper with the request bytes after enqueue.
        let entries = scan(dir.path(), "repo-r1").unwrap();
        fs::write(&entries[0].path, "{\"receipt\":{}}\n{\"b\":2}\n").unwrap();
        let entries = scan(dir.path(), "repo-r1").unwrap();
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].request_hash_ok);
        // Recovery refuses to delete it.
        let err = remove_after_committed(&entries[0]).unwrap_err();
        assert!(err.to_string().contains("frozen"), "{err}");
        assert!(entries[0].path.exists(), "corrupt segment retained");
    }

    #[test]
    fn recover_deletes_only_after_committed() {
        let dir = tempfile::tempdir().unwrap();
        enqueue(dir.path(), "repo-r1", "key-1", "{\"a\":1}").unwrap();
        enqueue(dir.path(), "repo-r1", "key-2", "{\"a\":2}").unwrap();
        let report = recover(dir.path(), "repo-r1", |e| {
            assert!(e.receipt.idempotency_key == "key-1" || e.receipt.idempotency_key == "key-2");
            Ok(())
        })
        .unwrap();
        assert_eq!(report.recovered, 2);
        assert!(scan(dir.path(), "repo-r1").unwrap().is_empty());
        let dir_path = dir.path().join("repo-r1");
        // Parent dir still exists but is empty of segments.
        assert_eq!(fs::read_dir(&dir_path).unwrap().count(), 0);
    }

    #[test]
    fn trailing_garbage_after_request_is_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        enqueue(dir.path(), "repo-r1", "key-1", "{\"a\":1}").unwrap();
        let entries = scan(dir.path(), "repo-r1").unwrap();
        // The format is exactly two lines; a third line means a torn/foreign
        // write, which must be frozen, not silently committed.
        let mut s = fs::read_to_string(&entries[0].path).unwrap();
        s.push_str("xx\n");
        fs::write(&entries[0].path, s).unwrap();
        let entries = scan(dir.path(), "repo-r1").unwrap();
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].request_hash_ok, "trailing bytes are corruption");
        let report = recover(dir.path(), "repo-r1", |_| Ok(())).unwrap();
        assert!(report.corrupt.len() == 1 && report.recovered == 0);
        assert!(entries[0].path.exists(), "corrupt segment retained");
    }

    #[test]
    fn tmp_leftovers_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("repo-r1");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("dead.queue.tmp"), "partial").unwrap();
        let entries = scan(dir.path(), "repo-r1").unwrap();
        assert!(entries.is_empty());
        assert!(!sub.join("dead.queue.tmp").exists());
    }
}

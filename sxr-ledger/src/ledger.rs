//! The generation-local SQLite ledger (§10.3, §11.1): append-only rows,
//! dense ordinals, idempotent append, hash-chain verification, WAL pragmas,
//! and the `user_version` schema guard.

use crate::error::{Error, Result};
use crate::schema::{SCHEMA_VERSION, full_ddl};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use std::path::{Path, PathBuf};
use sxr_core::chain;
use sxr_core::types::{
    LedgerHead, LedgerPosition, LedgerRow, SXR_CANONICALIZATION, SXR_HASH_ALGORITHM,
    SXR_RECORD_VERSION,
};

/// Writer-side configuration (defaults per §11.1).
#[derive(Debug, Clone)]
pub struct LedgerConfig {
    pub busy_timeout_ms: u32,
    pub wal_autocheckpoint: u32,
    pub writer_identity_ref: String,
    /// Verify the last N ordinals on open (§11.3 startup check).
    pub tail_verify_on_open: u64,
}

impl Default for LedgerConfig {
    fn default() -> Self {
        Self {
            busy_timeout_ms: 5000,
            wal_autocheckpoint: 1000,
            writer_identity_ref: writer_identity_default(),
            tail_verify_on_open: 1000,
        }
    }
}

/// `sxr@<host>/<pid>` (§10.3). Hostname is observed metadata, never identity.
pub fn writer_identity_default() -> String {
    let host = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".into());
    format!("sxr@{host}/{}", std::process::id())
}

/// An entry ready for append: payload is the complete canonical CEP envelope
/// (I-LED: valid, complete, `validation_status` validated — enforced by the
/// producer; see `sxr_core::envelope::build_envelope`).
#[derive(Debug, Clone)]
pub struct PendingEntry {
    pub record_kind: String,
    pub idempotency_key: Option<String>,
    pub subject_refs: Vec<String>,
    pub payload: String,
    /// Override for tests/import; minted when None.
    pub entry_ulid: Option<String>,
    pub committed_at: Option<String>,
}

/// Outcome of an append: committed (new row) or already committed under the
/// same idempotency key (AC-INGEST-002: re-ingest returns the existing
/// position).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppendOutcome {
    Committed {
        row: Box<LedgerRow>,
        position: LedgerPosition,
    },
    AlreadyCommitted {
        position: LedgerPosition,
    },
}

/// The ledger report of §10.6 verification.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VerifyReport {
    pub ledger_id: String,
    pub verified_count: u64,
    pub first_divergence: Option<LedgerPosition>,
    pub complete: bool,
    pub limitations: Vec<String>,
}

/// One generation's append-only SQLite ledger. Serializes writes with the
/// caller-owned writer flock; SQLite's own WAL locking covers readers.
pub struct Ledger {
    conn: Connection,
    path: PathBuf,
    ledger_id: String,
    config: LedgerConfig,
}

impl std::fmt::Debug for Ledger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ledger")
            .field("path", &self.path)
            .field("ledger_id", &self.ledger_id)
            .finish_non_exhaustive()
    }
}

impl Ledger {
    /// Open an existing generation ledger, or create it at `path` if absent.
    /// Applies the frozen v1 schema, the WAL pragmas, `quick_check`, and the
    /// tail-chain startup check. Refuses `user_version > 1` (§10.5).
    pub fn open(path: &Path, ledger_id: &str, config: LedgerConfig) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let mut ledger = Self {
            conn,
            path: path.to_path_buf(),
            ledger_id: ledger_id.to_string(),
            config,
        };
        ledger.apply_pragmas()?;
        ledger.ensure_schema()?;
        ledger.guard_ledger_id()?;
        ledger.quick_check()?;
        if ledger.config.tail_verify_on_open > 0 {
            let report = ledger.verify_tail(ledger.config.tail_verify_on_open)?;
            if let Some(diverge) = report.first_divergence {
                return Err(Error::Integrity(format!(
                    "startup tail-chain verification failed at {}:{}",
                    diverge.ledger_id, diverge.append_ordinal
                )));
            }
        }
        Ok(ledger)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn ledger_id(&self) -> &str {
        &self.ledger_id
    }

    /// SQLite connection (outbox drain, projectors, tests).
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Mutable SQLite connection (outbox drain in one transaction).
    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// WAL, synchronous=FULL, busy_timeout, foreign_keys, autocheckpoint
    /// (§11.1).
    fn apply_pragmas(&mut self) -> Result<()> {
        self.conn
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| {
                Error::Schema(format!(
                    "journal_mode=WAL unavailable on {}: {e} (see §10.2a probes)",
                    self.path.display()
                ))
            })?;
        let mode: String = self
            .conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(Error::Schema(format!(
                "journal_mode=WAL refused (got {mode}) on {}; capability class must be safe",
                self.path.display()
            )));
        }
        self.conn
            .pragma_update(None, "synchronous", "FULL")
            .map_err(|e| Error::Schema(format!("synchronous=FULL failed: {e}")))?;
        self.conn
            .pragma_update(None, "busy_timeout", self.config.busy_timeout_ms)?;
        self.conn.pragma_update(None, "foreign_keys", "ON")?;
        self.conn
            .pragma_update(None, "wal_autocheckpoint", self.config.wal_autocheckpoint)?;
        Ok(())
    }

    /// §10.5: schema version guard + frozen DDL. user_version 0 → apply v1;
    /// higher → refuse (forward incompatibility is an error).
    fn ensure_schema(&mut self) -> Result<()> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(Error::Schema(format!(
                "ledger {} has user_version {version}, newer than this build understands ({SCHEMA_VERSION}); refusing to open (forward incompatibility, §10.5)",
                self.path.display()
            )));
        }
        if version == 0 {
            self.conn.execute_batch(&full_ddl())?;
            self.conn
                .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(())
    }

    /// Every row in one generation DB belongs to the same `ledger_id`.
    fn guard_ledger_id(&self) -> Result<()> {
        let stored: Option<String> = self
            .conn
            .query_row(
                "SELECT ledger_id FROM ledger ORDER BY append_ordinal DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(stored) = stored
            && stored != self.ledger_id
        {
            return Err(Error::Generation(format!(
                "ledger at {} holds generation {stored}, but was opened as {}",
                self.path.display(),
                self.ledger_id
            )));
        }
        Ok(())
    }

    pub fn quick_check(&self) -> Result<()> {
        let result: String = self
            .conn
            .query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if result != "ok" {
            return Err(Error::Integrity(format!(
                "PRAGMA quick_check on {} returned: {result}",
                self.path.display()
            )));
        }
        Ok(())
    }

    pub fn count(&self) -> Result<u64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM ledger", [], |r| r.get(0))?)
    }

    pub fn head(&self) -> Result<Option<LedgerHead>> {
        let row = self
            .conn
            .query_row(
                "SELECT ledger_id, append_ordinal, entry_hash FROM ledger ORDER BY append_ordinal DESC LIMIT 1",
                [],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)? as u64,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        Ok(
            row.map(|(ledger_id, append_ordinal, entry_hash)| LedgerHead {
                position: LedgerPosition::new(ledger_id, append_ordinal),
                entry_hash,
            }),
        )
    }

    /// Append one entry in its own transaction (manual mode: one batch is one
    /// txn; `append_batch` is the multi-entry path).
    pub fn append(&mut self, entry: &PendingEntry) -> Result<AppendOutcome> {
        let mut outcomes = self.append_batch(std::slice::from_ref(entry), &[])?;
        Ok(outcomes.remove(0))
    }

    /// Append N entries plus outbox rows in one transaction (§11.2).
    /// Idempotent: an `idempotency_key` that already exists returns the
    /// existing position without inserting (AC-INGEST-002).
    pub fn append_batch(
        &mut self,
        entries: &[PendingEntry],
        outbox_projections: &[String],
    ) -> Result<Vec<AppendOutcome>> {
        let mut outcomes = Vec::with_capacity(entries.len());
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for entry in entries {
            // Idempotency first: re-ingest must return the existing position.
            if let Some(key) = &entry.idempotency_key
                && let Some((ledger_id, ordinal)) = tx
                    .query_row(
                        "SELECT ledger_id, append_ordinal FROM ledger WHERE idempotency_key = ?1",
                        [key],
                        |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)),
                    )
                    .optional()?
            {
                outcomes.push(AppendOutcome::AlreadyCommitted {
                    position: LedgerPosition::new(ledger_id, ordinal),
                });
                continue;
            }
            let head: Option<String> = tx
                .query_row(
                    "SELECT entry_hash FROM ledger ORDER BY append_ordinal DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            let ordinal: i64 = tx.query_row(
                "SELECT COALESCE(MAX(append_ordinal), 0) + 1 FROM ledger",
                [],
                |r| r.get(0),
            )?;
            if ordinal <= 0 {
                return Err(Error::Integrity("append_ordinal overflow".into()));
            }
            let entry_ulid = entry
                .entry_ulid
                .clone()
                .unwrap_or_else(|| ulid::Ulid::new().to_string());
            let committed_at = entry.committed_at.clone().unwrap_or_else(|| {
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
            });
            let subject_refs_json =
                serde_json::to_string(&entry.subject_refs).expect("Vec<String> is serializable");
            let payload_hash = chain::payload_hash(&entry.payload);
            let previous_entry_hash = head.clone();
            let row = LedgerRow {
                ledger_id: self.ledger_id.clone(),
                append_ordinal: ordinal as u64,
                entry_ulid,
                record_kind: entry.record_kind.clone(),
                record_version: SXR_RECORD_VERSION.into(),
                idempotency_key: entry.idempotency_key.clone(),
                subject_refs: entry.subject_refs.clone(),
                canonicalization: SXR_CANONICALIZATION.into(),
                hash_algorithm: SXR_HASH_ALGORITHM.into(),
                payload: entry.payload.clone(),
                payload_hash,
                previous_entry_hash,
                entry_hash: String::new(),
                committed_at,
                writer_identity_ref: self.config.writer_identity_ref.clone(),
            };
            let entry_hash = chain::entry_hash(&row);
            let row = LedgerRow { entry_hash, ..row };
            tx.execute(
                "INSERT INTO ledger (
                    ledger_id, append_ordinal, entry_ulid, record_kind, record_version,
                    idempotency_key, subject_refs, canonicalization, hash_algorithm,
                    payload, payload_hash, previous_entry_hash, entry_hash,
                    committed_at, writer_identity_ref
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                rusqlite::params![
                    row.ledger_id,
                    row.append_ordinal as i64,
                    row.entry_ulid,
                    row.record_kind,
                    row.record_version,
                    row.idempotency_key,
                    subject_refs_json,
                    row.canonicalization,
                    row.hash_algorithm,
                    row.payload,
                    row.payload_hash,
                    row.previous_entry_hash,
                    row.entry_hash,
                    row.committed_at,
                    row.writer_identity_ref,
                ],
            )
            .map_err(|e| {
                if is_unique_violation(&e) {
                    Error::Integrity(format!(
                        "concurrent duplicate append under idempotency_key {:?} ({e}); writer lock must be held",
                        entry.idempotency_key
                    ))
                } else {
                    e.into()
                }
            })?;
            if !outbox_projections.is_empty() {
                tx.execute(
                    "INSERT INTO outbox (ledger_id, append_ordinal, projections) VALUES (?1, ?2, json(?3))",
                    rusqlite::params![
                        self.ledger_id,
                        row.append_ordinal as i64,
                        serde_json::to_string(outbox_projections).expect("serializable"),
                    ],
                )?;
            }
            let position = LedgerPosition::new(self.ledger_id.clone(), row.append_ordinal);
            outcomes.push(AppendOutcome::Committed {
                row: Box::new(row),
                position,
            });
        }
        tx.commit()?;
        Ok(outcomes)
    }

    pub fn read(&self, ordinal: u64) -> Result<LedgerRow> {
        self.conn
            .query_row(
                "SELECT ledger_id, append_ordinal, entry_ulid, record_kind, record_version,
                        idempotency_key, subject_refs, canonicalization, hash_algorithm,
                        payload, payload_hash, previous_entry_hash, entry_hash,
                        committed_at, writer_identity_ref
                 FROM ledger WHERE append_ordinal = ?1",
                [ordinal as i64],
                row_from_sqlite,
            )
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("no entry at ordinal {ordinal}")))
    }

    pub fn read_all(&self) -> Result<Vec<LedgerRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT ledger_id, append_ordinal, entry_ulid, record_kind, record_version,
                    idempotency_key, subject_refs, canonicalization, hash_algorithm,
                    payload, payload_hash, previous_entry_hash, entry_hash,
                    committed_at, writer_identity_ref
             FROM ledger ORDER BY append_ordinal",
        )?;
        let rows = stmt
            .query_map([], row_from_sqlite)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Verify the last `n` ordinals against the chain (§11.3 startup check).
    pub fn verify_tail(&self, n: u64) -> Result<VerifyReport> {
        let all = self.read_all()?;
        let start = all.len().saturating_sub(n as usize);
        self.verify_range(&all[start..])
    }

    /// Verify the whole generation chain from ordinal 1 (AC-LEDGER-001).
    pub fn verify_full(&self) -> Result<VerifyReport> {
        let all = self.read_all()?;
        self.verify_range(&all)
    }

    fn verify_range(&self, rows: &[LedgerRow]) -> Result<VerifyReport> {
        let refs: Vec<&LedgerRow> = rows.iter().collect();
        let mut limitations = vec![
            "tamper-evident, not tamper-proof: anyone with write access can rewrite the whole chain (CEP-0008 §55)".to_string(),
        ];
        match chain::chain_verify(&refs) {
            Ok(()) => Ok(VerifyReport {
                ledger_id: self.ledger_id.clone(),
                verified_count: refs.len() as u64,
                first_divergence: None,
                complete: true,
                limitations,
            }),
            Err(msg) => {
                let position = divergence_position(&msg, rows);
                if position.is_none() {
                    limitations.push(msg.clone());
                }
                Ok(VerifyReport {
                    ledger_id: self.ledger_id.clone(),
                    verified_count: rows.len() as u64,
                    first_divergence: position,
                    complete: false,
                    limitations,
                })
            }
        }
    }
}

fn is_unique_violation(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(err, _)
            if err.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

/// Locate the first divergent `(ledger_id, append_ordinal)` by re-verifying
/// each row in order (AC-LEDGER-001 names both fields).
fn divergence_position(_msg: &str, rows: &[LedgerRow]) -> Option<LedgerPosition> {
    let refs: Vec<&LedgerRow> = rows.iter().collect();
    let mut prev_hash: Option<&str> = None;
    for (i, row) in refs.iter().enumerate() {
        let ok_hash = chain::verify_entry_hashes(row).is_ok();
        let ok_link = match (prev_hash, row.previous_entry_hash.as_deref()) {
            (Some(p), Some(q)) => p == q,
            (None, None) => true,
            _ => false,
        };
        if !ok_hash || !ok_link {
            let _ = i;
            return Some(LedgerPosition::new(
                row.ledger_id.clone(),
                row.append_ordinal,
            ));
        }
        prev_hash = Some(&row.entry_hash);
    }
    None
}

pub(crate) fn row_from_sqlite(row: &rusqlite::Row<'_>) -> rusqlite::Result<LedgerRow> {
    let subject_refs: String = row.get(6)?;
    let subject_refs: Vec<String> = serde_json::from_str(&subject_refs).unwrap_or_else(|_| vec![]);
    Ok(LedgerRow {
        ledger_id: row.get(0)?,
        append_ordinal: row.get::<_, i64>(1)? as u64,
        entry_ulid: row.get(2)?,
        record_kind: row.get(3)?,
        record_version: row.get(4)?,
        idempotency_key: row.get(5)?,
        subject_refs,
        canonicalization: row.get(7)?,
        hash_algorithm: row.get(8)?,
        payload: row.get(9)?,
        payload_hash: row.get(10)?,
        previous_entry_hash: row.get(11)?,
        entry_hash: row.get(12)?,
        committed_at: row.get(13)?,
        writer_identity_ref: row.get(14)?,
    })
}

/// Small test helper: a stable row for hash computations without SQLite.
#[allow(dead_code)]
pub(crate) fn row_with_hashes(mut row: LedgerRow) -> LedgerRow {
    row.payload_hash = chain::payload_hash(&row.payload);
    row.entry_hash = chain::entry_hash(&row);
    row
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn open_temp() -> (tempfile::TempDir, Ledger) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("gen").join("ledger.db");
        let ledger = Ledger::open(&path, "01JTESTLEDGER", LedgerConfig::default()).unwrap();
        (dir, ledger)
    }

    fn entry(kind: &str, key: Option<&str>) -> PendingEntry {
        PendingEntry {
            record_kind: kind.into(),
            idempotency_key: key.map(|k| k.into()),
            subject_refs: vec!["ledger:01JTESTLEDGER".into()],
            payload: format!(r#"{{"envelope_kind":"semantic_trace","record_kind":"{kind}"}}"#),
            entry_ulid: None,
            committed_at: None,
        }
    }

    #[test]
    fn schema_version_is_set_and_guard_refuses_higher() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ledger.db");
        let _ = Ledger::open(&path, "01JL1", LedgerConfig::default()).unwrap();
        {
            let conn = Connection::open(&path).unwrap();
            let v: i64 = conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap();
            assert_eq!(v, SCHEMA_VERSION);
            conn.pragma_update(None, "user_version", 99).unwrap();
        }
        let err = Ledger::open(&path, "01JL1", LedgerConfig::default()).unwrap_err();
        assert!(err.to_string().contains("forward incompatibility"), "{err}");
    }

    #[test]
    fn pragmas_are_applied() {
        let (_dir, ledger) = open_temp();
        let mode: String = ledger
            .conn()
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
        let sync: i64 = ledger
            .conn()
            .query_row("PRAGMA synchronous", [], |r| r.get(0))
            .unwrap();
        assert_eq!(sync, 2);
        let busy: i64 = ledger
            .conn()
            .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
            .unwrap();
        assert_eq!(busy, 5000);
        let fk: i64 = ledger
            .conn()
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
    }

    #[test]
    fn dense_ordinals_and_head() {
        let (_dir, mut ledger) = open_temp();
        assert_eq!(ledger.count().unwrap(), 0);
        assert!(ledger.head().unwrap().is_none());
        for i in 1..=3 {
            let out = ledger
                .append(&entry(&format!("kind.{i}"), Some(&format!("key{i}"))))
                .unwrap();
            let AppendOutcome::Committed { position, .. } = out else {
                panic!("fresh key must commit");
            };
            assert_eq!(position.append_ordinal, i);
        }
        let head = ledger.head().unwrap().unwrap();
        assert_eq!(head.position.append_ordinal, 3);
    }

    #[test]
    fn idempotent_append_returns_existing_position() {
        let (_dir, mut ledger) = open_temp();
        let out1 = ledger.append(&entry("kind.a", Some("dup-key"))).unwrap();
        let AppendOutcome::Committed { position: p1, .. } = out1 else {
            panic!("first append must commit");
        };
        let out2 = ledger.append(&entry("kind.a", Some("dup-key"))).unwrap();
        let AppendOutcome::AlreadyCommitted { position: p2 } = out2 else {
            panic!("duplicate must be a no-op returning the existing position");
        };
        assert_eq!(p1, p2);
        assert_eq!(ledger.count().unwrap(), 1);
    }

    #[test]
    fn append_chain_verifies_and_tamper_is_named() {
        let (_dir, mut ledger) = open_temp();
        for i in 1..=5 {
            ledger.append(&entry(&format!("kind.{i}"), None)).unwrap();
        }
        let report = ledger.verify_full().unwrap();
        assert!(report.complete);
        assert_eq!(report.verified_count, 5);

        // Flip a payload byte in the middle, keep the stored entry_hash.
        let conn = ledger.conn();
        conn.execute(
            "UPDATE ledger SET payload = json_set(payload, '$.tampered', true) WHERE append_ordinal = 3",
            [],
        )
        .unwrap();
        let report = ledger.verify_full().unwrap();
        assert!(!report.complete);
        let pos = report.first_divergence.expect("must name the divergence");
        assert_eq!(pos.ledger_id, "01JTESTLEDGER");
        assert_eq!(pos.append_ordinal, 3);
    }

    #[test]
    fn outbox_rows_commit_in_same_txn() {
        let (_dir, mut ledger) = open_temp();
        ledger
            .append_batch(
                &[entry("kind.a", None), entry("kind.b", None)],
                &["declarations".into(), "rollup".into()],
            )
            .unwrap();
        let n: i64 = ledger
            .conn()
            .query_row("SELECT COUNT(*) FROM outbox", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
        let projections: String = ledger
            .conn()
            .query_row("SELECT projections FROM outbox LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(projections, r#"["declarations","rollup"]"#);
    }

    #[test]
    fn wrong_generation_id_is_refused() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ledger.db");
        let mut ledger = Ledger::open(&path, "01JA", LedgerConfig::default()).unwrap();
        // Write at least one row so the generation identity is observable.
        ledger.append(&entry("kind.0", None)).unwrap();
        let err = Ledger::open(&path, "01JB", LedgerConfig::default()).unwrap_err();
        assert!(err.to_string().contains("holds generation"), "{err}");
    }
}

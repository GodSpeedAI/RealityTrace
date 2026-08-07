//! §11.2 transactional outbox: projectors drain `outbox` and advance their
//! own watermark. The watermark pair `(last_ledger_id, last_append_ordinal)`
//! is one `LedgerPosition`; a bare ordinal is forbidden.

use crate::error::{Error, Result};
use rusqlite::{Connection, OptionalExtension};
use sxr_core::types::LedgerPosition;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Watermark {
    pub name: String,
    pub position: LedgerPosition,
    pub projection_version: String,
    pub updated_at: String,
}

/// Drain everything at or before `position` for one projector and advance its
/// watermark in the same transaction. A row is deleted only when no other
/// projector still needs it (§11.2: each projector owns its watermark).
pub fn drain(
    conn: &mut Connection,
    projector: &str,
    position: &LedgerPosition,
) -> Result<Vec<(LedgerPosition, Vec<String>)>> {
    let tx = conn.transaction()?;
    let mut stmt = tx.prepare(
        "SELECT ledger_id, append_ordinal, projections
         FROM outbox
         WHERE ledger_id = ?1 AND append_ordinal <= ?2
         ORDER BY append_ordinal",
    )?;
    let mut rows = Vec::new();
    let mut iter = stmt.query_map(
        rusqlite::params![position.ledger_id, position.append_ordinal as i64],
        |r| {
            let projections: String = r.get(2)?;
            let projections: Vec<String> = serde_json::from_str(&projections).unwrap_or_default();
            Ok((
                LedgerPosition::new(r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64),
                projections,
            ))
        },
    )?;
    while let Some((pos, projections)) = iter.next().transpose()? {
        rows.push((pos, projections));
    }
    drop(iter);
    drop(stmt);
    for (pos, projections) in &rows {
        let remaining: Vec<String> = projections
            .iter()
            .filter(|p| *p != projector)
            .cloned()
            .collect();
        if remaining.is_empty() {
            tx.execute(
                "DELETE FROM outbox WHERE ledger_id = ?1 AND append_ordinal = ?2",
                rusqlite::params![pos.ledger_id, pos.append_ordinal as i64],
            )?;
        } else {
            tx.execute(
                "UPDATE outbox SET projections = json(?3) WHERE ledger_id = ?1 AND append_ordinal = ?2",
                rusqlite::params![
                    pos.ledger_id,
                    pos.append_ordinal as i64,
                    serde_json::to_string(&remaining).expect("serializable")
                ],
            )?;
        }
    }
    advance(&tx, projector, position)?;
    tx.commit()?;
    Ok(rows)
}

/// Set a projector watermark (upsert). Only meaningful inside a drain txn.
fn advance(
    tx: &rusqlite::Transaction<'_>,
    projector: &str,
    position: &LedgerPosition,
) -> Result<()> {
    let updated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    tx.execute(
        "INSERT INTO projection_watermark (name, last_ledger_id, last_append_ordinal, projection_version, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(name) DO UPDATE SET
           last_ledger_id = excluded.last_ledger_id,
           last_append_ordinal = excluded.last_append_ordinal,
           projection_version = excluded.projection_version,
           updated_at = excluded.updated_at",
        rusqlite::params![
            projector,
            position.ledger_id,
            position.append_ordinal as i64,
            "1",
            updated_at,
        ],
    )?;
    Ok(())
}

pub fn read_watermark(conn: &Connection, projector: &str) -> Result<Option<Watermark>> {
    conn.query_row(
        "SELECT name, last_ledger_id, last_append_ordinal, projection_version, updated_at
         FROM projection_watermark WHERE name = ?1",
        [projector],
        |r| {
            Ok(Watermark {
                name: r.get(0)?,
                position: LedgerPosition::new(r.get::<_, String>(1)?, r.get::<_, i64>(2)? as u64),
                projection_version: r.get(3)?,
                updated_at: r.get(4)?,
            })
        },
    )
    .optional()
    .map_err(Error::from)
}

/// Projection lag: how many unconsumed outbox rows name this projector.
/// Per-projector, never a global count (§11.2).
pub fn lag(conn: &Connection, projector: &str) -> Result<u64> {
    let wm = read_watermark(conn, projector)?;
    let sql = "SELECT COUNT(*) FROM outbox
               WHERE EXISTS (SELECT 1 FROM json_each(outbox.projections) WHERE json_each.value = ?1)
                 AND (?2 IS NULL OR append_ordinal > ?2)";
    Ok(conn.query_row(
        sql,
        rusqlite::params![projector, wm.map(|w| w.position.append_ordinal as i64)],
        |r| r.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::{Ledger, LedgerConfig, PendingEntry};
    use tempfile::tempdir;

    fn entry(kind: &str) -> PendingEntry {
        PendingEntry {
            record_kind: kind.into(),
            idempotency_key: None,
            subject_refs: vec!["ledger:01JTEST".into()],
            payload: format!(r#"{{"record_kind":"{kind}"}}"#),
            entry_ulid: None,
            committed_at: None,
        }
    }

    #[test]
    fn drain_advances_watermark_and_outbox_is_empty_after() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ledger.db");
        let mut ledger = Ledger::open(&path, "01JTEST", LedgerConfig::default()).unwrap();
        ledger
            .append_batch(
                &[entry("a"), entry("b"), entry("c")],
                &["declarations".into()],
            )
            .unwrap();
        let head = ledger.head().unwrap().unwrap();
        {
            let conn = ledger.conn_mut();
            assert_eq!(lag(conn, "declarations").unwrap(), 3);
            let drained = drain(conn, "declarations", &head.position).unwrap();
            assert_eq!(drained.len(), 3);
            assert_eq!(lag(conn, "declarations").unwrap(), 0);
            let wm = read_watermark(conn, "declarations").unwrap().unwrap();
            assert_eq!(wm.position, head.position);
            assert_eq!(wm.projection_version, "1");
        }
        // New entries after the watermark remain undrained.
        ledger
            .append_batch(&[entry("d")], &["declarations".into()])
            .unwrap();
        {
            let conn = ledger.conn_mut();
            assert_eq!(lag(conn, "declarations").unwrap(), 1);
        }
    }

    #[test]
    fn watermark_is_absent_before_first_drain() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ledger.db");
        let mut ledger = Ledger::open(&path, "01JTEST", LedgerConfig::default()).unwrap();
        ledger
            .append_batch(&[entry("a")], &["rollup".into()])
            .unwrap();
        let conn = ledger.conn();
        assert!(read_watermark(conn, "rollup").unwrap().is_none());
        assert_eq!(lag(conn, "never-used").unwrap(), 0);
    }
}

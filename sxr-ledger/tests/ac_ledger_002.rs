//! AC-LEDGER-002: corruption recovery (§11.3a) — the corruption report is
//! fsynced before the successor generation is created; the successor anchors
//! the exact old verified head; old entries keep their original hashes;
//! destructive recovery starts a `continuity: broken` root.

use sxr_ledger::generation::{Continuity, GenerationMeta};
use sxr_ledger::ledger::{AppendOutcome, Ledger, LedgerConfig, PendingEntry};
use sxr_ledger::quarantine::{CorruptionReport, quarantine};
use tempfile::tempdir;

fn entry(kind: &str, payload: &str) -> PendingEntry {
    PendingEntry {
        record_kind: kind.into(),
        idempotency_key: None,
        subject_refs: vec!["ledger:01JA".into()],
        payload: payload.to_string(),
        entry_ulid: None,
        committed_at: None,
    }
}

fn build_generation_a(root: &tempfile::TempDir) -> (String, Vec<String>) {
    let dir = root.path().join("generations").join("01JA");
    let mut ledger = Ledger::open(&dir.join("ledger.db"), "01JA", LedgerConfig::default()).unwrap();
    ledger
        .append(&entry(
            "ledger.generation_started",
            r#"{"record_kind":"ledger.generation_started","scope":{"ledger_id":"01JA"}}"#,
        ))
        .unwrap();
    for i in 1..=4 {
        ledger
            .append(&entry("semantic.diff", &format!(r#"{{"i":{i}}}"#)))
            .unwrap();
    }
    // Entries 1..=3 verified; entry 4 is corrupt (verified prefix k-1 = 3).
    let rows = ledger.read_all().unwrap();
    let head = ledger.head().unwrap().unwrap();
    let hashes: Vec<String> = rows.into_iter().map(|r| r.entry_hash).collect();
    assert_eq!(hashes.len(), 5);
    // Corrupt ordinal 4's payload without touching stored hashes.
    let conn = ledger.conn();
    conn.execute(
        "UPDATE ledger SET payload = '{\"i\":4,\"corrupt\":true}' WHERE append_ordinal = 4",
        [],
    )
    .unwrap();
    let _ = conn;
    drop(ledger);
    // Verify: divergence must be named at ordinal 4. Startup tail-check is
    // disabled here because the corruption is intentional and already known.
    let config = LedgerConfig {
        tail_verify_on_open: 0,
        ..LedgerConfig::default()
    };
    let reopen = Ledger::open(&dir.join("ledger.db"), "01JA", config).unwrap();
    let report = reopen.verify_full().unwrap();
    assert!(!report.complete);
    assert_eq!(report.first_divergence.unwrap().append_ordinal, 4);
    (head.entry_hash, hashes)
}

/// Full §11.3a flow: quarantine A byte-for-byte with the report, then create
/// anchored successor B with `continuity: continuous`, old hashes preserved.
#[test]
fn quarantine_then_anchored_successor_preserves_old_hashes() {
    let dir = tempdir().unwrap();
    let history_id = "01JHIST";
    let (head_a, hashes_a) = build_generation_a(&dir);

    let gen_dir = dir.path().join("generations").join("01JA");
    let quarantine_root = dir.path().join("quarantine");
    let report = CorruptionReport {
        detected_at: "2026-08-07T00:00:00.000000Z".into(),
        method: "chain-verify".into(),
        first_bad_ordinal: 4,
        last_verified_ordinal: 3,
        last_verified_hash: head_a.clone(),
        original_ledger_id: "01JA".into(),
        quarantined_file_sha256: "sha256:ledger-db".into(),
        verified_count: 3,
        unreadable_count: 1,
        filesystem_class: "safe".into(),
        recovery_action: "anchored successor generation".into(),
    };
    // Step 2–3: quarantine byte-for-byte + fsynced report BEFORE creating B.
    let (quarantined, report_hash) = quarantine(&gen_dir, &quarantine_root, &report).unwrap();
    assert!(quarantined.join("ledger.db").exists());
    assert!(quarantined.join("corruption-report.json").exists());
    assert!(!gen_dir.exists(), "generation A must be moved, not copied");
    let report_text = std::fs::read_to_string(quarantined.join("corruption-report.json")).unwrap();
    let written: CorruptionReport = serde_json::from_str(&report_text).unwrap();
    assert_eq!(written.first_bad_ordinal, 4);
    assert_eq!(written.last_verified_hash, head_a);
    assert_eq!(written.recovery_action, "anchored successor generation");
    assert!(report_hash.starts_with("sha256:"));

    // Step 4–5: successor B anchored on the exact verified head.
    let meta_b = GenerationMeta::successor(
        history_id,
        2,
        "01JA",
        &written.last_verified_hash,
        written.last_verified_ordinal,
        &report_hash,
    );
    assert_eq!(meta_b.previous_generation_ref.as_deref(), Some("01JA"));
    assert_eq!(meta_b.continuity, Continuity::Continuous);
    let b_dir = dir.path().join("generations").join(&meta_b.ledger_id);
    let mut b = Ledger::open(
        &b_dir.join("ledger.db"),
        &meta_b.ledger_id,
        LedgerConfig::default(),
    )
    .unwrap();
    b.append(&entry(
        "ledger.generation_started",
        &format!(
            r#"{{"record_kind":"ledger.generation_started","scope":{{"ledger_id":"{}","ledger_history_id":"{history_id}"}},"payload":{{"previous_generation_ref":"01JA","previous_generation_verified_head":"{}","transition_report_ref":"{report_hash}"}}}}"#,
            meta_b.ledger_id, written.last_verified_hash
        ),
    ))
    .unwrap();
    // Step 6: new events at ordinal 2+.
    b.append(&entry("semantic.diff", r#"{"i":5}"#)).unwrap();

    let report_b = b.verify_full().unwrap();
    assert!(report_b.complete, "{report_b:?}");
    let root = b.read(1).unwrap();
    assert_eq!(root.previous_entry_hash, None, "B is a fresh chain");
    assert_eq!(b.count().unwrap(), 2);

    // Old hashes are preserved byte-for-byte in quarantine (AC-LEDGER-002).
    // immutable=1: the quarantined bytes are fixed evidence; no locking/shm.
    let uri = format!(
        "file:{}?immutable=1",
        quarantined.join("ledger.db").display()
    );
    let q_conn = rusqlite::Connection::open_with_flags(
        &uri,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .unwrap();
    let stored: Vec<String> = q_conn
        .prepare("SELECT entry_hash FROM ledger ORDER BY append_ordinal")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(stored, hashes_a, "A's entry hashes must be untouched");
}

/// Destructive recovery: a fresh unanchored root with `continuity: broken`
/// and `ledger.destructive_recovery` at ordinal 1 (never automatic).
#[test]
fn destructive_recovery_marks_continuity_broken() {
    let dir = tempdir().unwrap();
    let history_id = "01JHIST";
    let meta = GenerationMeta::destructive_root(history_id, 2);
    assert_eq!(meta.continuity, Continuity::Broken);
    assert_eq!(meta.previous_generation_ref, None);
    assert_eq!(meta.previous_generation_verified_head, None);

    let d_dir = dir.path().join("generations").join(&meta.ledger_id);
    let mut d = Ledger::open(
        &d_dir.join("ledger.db"),
        &meta.ledger_id,
        LedgerConfig::default(),
    )
    .unwrap();
    d.append(&entry(
        "ledger.destructive_recovery",
        &format!(
            r#"{{"record_kind":"ledger.destructive_recovery","scope":{{"ledger_id":"{}","ledger_history_id":"{history_id}"}},"payload":{{"discarded_range":{{"generation_ref":"01JA","ordinals":"4.."}}}}}}"#,
            meta.ledger_id
        ),
    ))
    .unwrap();
    let out = d.append(&entry("semantic.diff", r#"{"i":1}"#)).unwrap();
    let AppendOutcome::Committed { row, .. } = out else {
        panic!("must commit")
    };
    assert_eq!(row.append_ordinal, 2);
    let report = d.verify_full().unwrap();
    assert!(report.complete, "{report:?}");
    // A broken component is never ordered against the old one; the new chain
    // starts at NULL (§10.3).
    let root = d.read(1).unwrap();
    assert_eq!(root.previous_entry_hash, None);
    assert_eq!(root.record_kind, "ledger.destructive_recovery");
}

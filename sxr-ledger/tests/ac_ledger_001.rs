//! AC-LEDGER-001: the root generation begins at ordinal 1 with
//! `ledger.generation_started` and a NULL predecessor; flipping a payload
//! byte makes `verify` name the exact `(ledger_id, append_ordinal)`; intact
//! generations and the generation graph still verify.

use rusqlite::Connection;
use sxr_ledger::generation::{Continuity, GenerationMeta};
use sxr_ledger::history::{HistoryCatalogEntry, LedgerHistoryCatalog, write_catalog};
use sxr_ledger::ledger::{Ledger, LedgerConfig, PendingEntry};
use tempfile::tempdir;

fn entry(kind: &str, payload: &str) -> PendingEntry {
    PendingEntry {
        record_kind: kind.into(),
        idempotency_key: None,
        subject_refs: vec!["ledger:01JTEST".into()],
        payload: payload.to_string(),
        entry_ulid: None,
        committed_at: None,
    }
}

fn generation_started_payload(ledger_id: &str, history_id: &str) -> String {
    format!(
        r#"{{"envelope_kind":"semantic_trace","record_kind":"ledger.generation_started","scope":{{"ledger_id":"{ledger_id}","ledger_history_id":"{history_id}"}}}}"#
    )
}

/// Root generation: ordinal 1 is the transition record with NULL predecessor;
/// runtime events begin at ordinal 2 (§10.3).
#[test]
fn root_ordinal_1_is_generation_started_with_null_predecessor() {
    let dir = tempdir().unwrap();
    let path = dir
        .path()
        .join("generations")
        .join("01JA")
        .join("ledger.db");
    let history_id = "01JHIST";
    let mut ledger = Ledger::open(&path, "01JA", LedgerConfig::default()).unwrap();

    let root = entry(
        "ledger.generation_started",
        &generation_started_payload("01JA", history_id),
    );
    let out = ledger.append(&root).unwrap();
    let sxr_ledger::ledger::AppendOutcome::Committed { row: root_row, .. } = out else {
        panic!("fresh root must commit");
    };
    assert_eq!(
        root_row.append_ordinal, 1,
        "ordinal 1 is reserved for the transition record"
    );
    assert_eq!(root_row.record_kind, "ledger.generation_started");
    assert_eq!(
        root_row.previous_entry_hash, None,
        "root has NULL predecessor"
    );

    // Runtime events begin at ordinal 2, chained to the root.
    let event = entry("semantic.diff", r#"{"record_kind":"semantic.diff"}"#);
    let out = ledger.append(&event).unwrap();
    let sxr_ledger::ledger::AppendOutcome::Committed { row, .. } = out else {
        panic!("must commit");
    };
    assert_eq!(row.append_ordinal, 2);
    assert_eq!(
        row.previous_entry_hash.as_deref(),
        Some(root_row.entry_hash.as_str())
    );
}

/// Flipping one payload byte while keeping the stored hashes must make
/// `verify` name the exact `ledger_id` and `append_ordinal` (AC-LEDGER-001).
#[test]
fn flipped_payload_byte_is_named_with_ledger_id_and_ordinal() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("ledger.db");
    let mut ledger = Ledger::open(&path, "01JA", LedgerConfig::default()).unwrap();
    for i in 1..=5 {
        ledger
            .append(&entry("kind", &format!(r#"{{"i":{i}}}"#)))
            .unwrap();
    }
    // Flip a byte in the middle entry without touching the stored hashes.
    let conn = Connection::open(&path).unwrap();
    conn.execute(
        "UPDATE ledger SET payload = '{\"i\":3,\"flipped\":true}' WHERE append_ordinal = 3",
        [],
    )
    .unwrap();

    let report = ledger.verify_full().unwrap();
    assert!(!report.complete);
    let divergence = report
        .first_divergence
        .expect("verify must name the divergence");
    assert_eq!(divergence.ledger_id, "01JA");
    assert_eq!(divergence.append_ordinal, 3);
    assert!(
        report
            .limitations
            .iter()
            .any(|l| l.contains("tamper-evident"))
    );
}

/// A fresh, untouched generation verifies completely, and two generations in
/// the same history verify independently without a cross-generation chain
/// (each chain starts at NULL).
#[test]
fn intact_generations_still_verify() {
    let dir = tempdir().unwrap();
    let history_id = "01JHIST";

    // Generation A: root + 2 events.
    let a_dir = dir.path().join("generations").join("01JA");
    let mut a = Ledger::open(&a_dir.join("ledger.db"), "01JA", LedgerConfig::default()).unwrap();
    a.append(&entry(
        "ledger.generation_started",
        &generation_started_payload("01JA", history_id),
    ))
    .unwrap();
    a.append(&entry("semantic.diff", r#"{"kind":"semantic.diff"}"#))
        .unwrap();
    a.append(&entry(
        "semantic.identity_drift",
        r#"{"kind":"semantic.identity_drift"}"#,
    ))
    .unwrap();
    let head_a = a.head().unwrap().unwrap();
    let report_a = a.verify_full().unwrap();
    assert!(report_a.complete, "{report_a:?}");
    assert_eq!(report_a.verified_count, 3);

    // Generation B: successor with an anchored predecessor, its own root.
    let meta_b = GenerationMeta::successor(
        history_id,
        2,
        "01JA",
        &head_a.entry_hash,
        head_a.position.append_ordinal,
        "sha256:report",
    );
    let b_dir = dir.path().join("generations").join(&meta_b.ledger_id);
    let mut b = Ledger::open(
        &b_dir.join("ledger.db"),
        &meta_b.ledger_id,
        LedgerConfig::default(),
    )
    .unwrap();
    b.append(&entry(
        "ledger.generation_started",
        &generation_started_payload(&meta_b.ledger_id, history_id),
    ))
    .unwrap();
    b.append(&entry("semantic.diff", r#"{"kind":"semantic.diff"}"#))
        .unwrap();
    let report_b = b.verify_full().unwrap();
    assert!(report_b.complete, "{report_b:?}");
    // B's root has NULL predecessor even though A exists: the chains never
    // join; the anchor lives in generation metadata (§10.3, §10.6).
    let b_root = b.read(1).unwrap();
    assert_eq!(b_root.previous_entry_hash, None);

    // Generation graph: B's metadata anchors the exact old head.
    assert_eq!(meta_b.previous_generation_ref.as_deref(), Some("01JA"));
    assert_eq!(
        meta_b.previous_generation_verified_head.as_deref(),
        Some(head_a.entry_hash.as_str())
    );
    assert_eq!(meta_b.previous_generation_last_verified_ordinal, Some(3));
    assert_eq!(meta_b.continuity, Continuity::Continuous);

    // The history catalog records both generations.
    let mut catalog = LedgerHistoryCatalog::new(history_id);
    catalog
        .push(HistoryCatalogEntry {
            ledger_id: "01JA".into(),
            generation_number: 1,
            continuity: "continuous".into(),
            previous_generation_ref: None,
            previous_generation_verified_head: None,
            ledger_db_rel: "generations/01JA/ledger.db".into(),
        })
        .unwrap();
    catalog
        .push(HistoryCatalogEntry {
            ledger_id: meta_b.ledger_id.clone(),
            generation_number: 2,
            continuity: "continuous".into(),
            previous_generation_ref: Some("01JA".into()),
            previous_generation_verified_head: Some(head_a.entry_hash.clone()),
            ledger_db_rel: format!("generations/{}/ledger.db", meta_b.ledger_id),
        })
        .unwrap();
    write_catalog(&dir.path().join("history"), &catalog).unwrap();
    let loaded =
        LedgerHistoryCatalog::load(&dir.path().join("history").join("ledger-history.json"))
            .unwrap();
    assert_eq!(loaded.generations.len(), 2);
    assert_eq!(loaded.generations[1].ledger_id, meta_b.ledger_id);
}

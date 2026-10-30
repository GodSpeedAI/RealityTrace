//! Startup tail-chain verification on a ledger longer than the tail window.
//! Regression: the window's first row was checked as if it were ordinal 1, so
//! every ledger with more than `tail_verify_on_open` entries refused to open.

use sxr_ledger::ledger::{Ledger, LedgerConfig, PendingEntry};
use tempfile::tempdir;

fn entry(i: u32) -> PendingEntry {
    PendingEntry {
        record_kind: "semantic.diff".into(),
        idempotency_key: None,
        subject_refs: vec!["ledger:01JT".into()],
        payload: format!(r#"{{"i":{i}}}"#),
        entry_ulid: None,
        committed_at: None,
    }
}

fn config(n: u64) -> LedgerConfig {
    LedgerConfig { tail_verify_on_open: n, ..LedgerConfig::default() }
}

#[test]
fn reopens_a_ledger_longer_than_the_tail_window() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("ledger.db");
    let mut ledger = Ledger::open(&path, "01JT", config(3)).unwrap();
    for i in 1..=7 {
        ledger.append(&entry(i)).unwrap();
    }
    drop(ledger);
    let reopened = Ledger::open(&path, "01JT", config(3)).expect("7 entries, window 3: must open");
    let tail = reopened.verify_tail(3).unwrap();
    assert!(tail.complete && tail.first_divergence.is_none());
    assert!(reopened.verify_full().unwrap().complete);
}

#[test]
fn tail_window_still_detects_corruption_inside_it() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("ledger.db");
    let mut ledger = Ledger::open(&path, "01JT", config(3)).unwrap();
    for i in 1..=7 {
        ledger.append(&entry(i)).unwrap();
    }
    ledger
        .conn()
        .execute("UPDATE ledger SET payload = '{\"i\":99}' WHERE append_ordinal = 6", [])
        .unwrap();
    drop(ledger);
    let err = Ledger::open(&path, "01JT", config(3)).unwrap_err().to_string();
    assert!(err.contains("01JT:6"), "{err}");
}

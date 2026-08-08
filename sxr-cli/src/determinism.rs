//! Determinism seam (§16.3 AC-DAEMON-001): `SXR_TEST_ID_STREAM` and
//! `SXR_TEST_FIXED_CLOCK` inject deterministic id minting and a fixed clock
//! for daemon-parity fixtures. This is test infrastructure, never identity
//! derivation — with the variables unset, minting is the ordinary fresh ULID/
//! wall clock.

use chrono::{DateTime, Utc};
use std::collections::VecDeque;
use std::sync::Mutex;

/// Cached per-env fixture state: reload whenever `SXR_TEST_ID_STREAM` changes
/// (parallel tests each set their own fixture).
fn id_stream() -> &'static Mutex<(Option<String>, VecDeque<String>)> {
    static STREAM: std::sync::OnceLock<Mutex<(Option<String>, VecDeque<String>)>> =
        std::sync::OnceLock::new();
    STREAM.get_or_init(|| Mutex::new((None, VecDeque::new())))
}

fn load_stream_if_set() {
    let mut guard = id_stream().lock().expect("id stream lock");
    let env = std::env::var("SXR_TEST_ID_STREAM")
        .ok()
        .filter(|v| !v.is_empty());
    if guard.0 == env {
        return;
    }
    let queue = match &env {
        Some(path) => {
            let raw = std::fs::read_to_string(path)
                .expect("SXR_TEST_ID_STREAM file readable (fixture setup)");
            raw.lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        }
        None => VecDeque::new(),
    };
    guard.0 = env.clone();
    guard.1 = queue;
}

/// Mint an id for `role`. Under `SXR_TEST_ID_STREAM` the ids come from the
/// fixture stream in consumption order (both parity runs consume the same
/// stream, so both mint the same ids); otherwise a fresh ULID. Consumption is
/// PERSISTENT: the stream file is rewritten with the remaining ids after each
/// pop, so every CLI invocation (a fresh process) continues where the
/// previous one stopped — without this, every process would re-mint the same
/// ids and distinct records would collide (e.g. run_id == repo_id).
pub fn mint_id(role: &str) -> String {
    load_stream_if_set();
    let mut guard = id_stream().lock().expect("id stream lock");
    let Some(next) = guard.1.pop_front() else {
        if std::env::var("SXR_TEST_ID_STREAM").is_ok_and(|v| !v.is_empty()) {
            panic!("SXR_TEST_ID_STREAM exhausted while minting {role} (fixture under-provisioned)");
        }
        return ulid::Ulid::new().to_string();
    };
    persist_remaining(&guard.0, &guard.1);
    next
}

/// Persist the unconsumed tail back to the stream file (test fixture only).
fn persist_remaining(env_value: &Option<String>, queue: &VecDeque<String>) {
    let Some(path) = env_value else {
        return;
    };
    let body = queue.iter().cloned().collect::<Vec<String>>().join("\n") + "\n";
    std::fs::write(path, body).expect("SXR_TEST_ID_STREAM rewritable (fixture setup)");
}

/// The clock for envelope timestamps: `SXR_TEST_FIXED_CLOCK` (RFC 3339)
/// yields a fixed clock, otherwise the system clock.
pub fn now_utc() -> DateTime<Utc> {
    match std::env::var("SXR_TEST_FIXED_CLOCK") {
        Ok(v) if !v.is_empty() => DateTime::parse_from_rfc3339(&v)
            .expect("SXR_TEST_FIXED_CLOCK RFC 3339 (fixture setup)")
            .with_timezone(&Utc),
        _ => chrono::Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // All three behaviors run in ONE test: the seam is process-global
    // (env + cached stream), so parallel tests would race on the env var.
    #[test]
    fn seam_behaviors_sequence() {
        // 1. stream consumed in order
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ids.txt");
        std::fs::write(&path, "id-one\nid-two\n").unwrap();
        // SAFETY: process-local fixture env; the seam reloads when the
        // variable changes, and this is the only test touching it.
        unsafe { std::env::set_var("SXR_TEST_ID_STREAM", &path) };
        assert_eq!(mint_id("run"), "id-one");
        assert_eq!(mint_id("evidence"), "id-two");

        // 2. fixed clock
        unsafe { std::env::set_var("SXR_TEST_FIXED_CLOCK", "2026-08-08T00:00:00Z") };
        assert_eq!(
            now_utc(),
            DateTime::parse_from_rfc3339("2026-08-08T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );
        unsafe { std::env::remove_var("SXR_TEST_FIXED_CLOCK") };

        // 3. fresh ULID when unset
        unsafe { std::env::remove_var("SXR_TEST_ID_STREAM") };
        let a = mint_id("run");
        let b = mint_id("run");
        assert_ne!(a, b);
        assert_eq!(a.len(), 26);
    }
}

//! The built-in `cargo-json` parser (§14.2): reduces a cargo
//! `--message-format json` stream to a `ParsedSummary`.
//!
//! On stable rustc, libtest has no JSON test-event mode (`--format json`
//! requires nightly `-Z unstable-options`), so cargo interleaves JSON
//! messages with the harness's plain-text summary lines. This parser reads
//! both: JSON `{"type":"test",...}` events when present (nightly), and the
//! `test result: <verdict>. N passed; M failed; K ignored` summary lines on
//! stable. Counts from both sources are summed. `None` when nothing parsed
//! and no summary line occurred (e.g. a compile error — the exit code and
//! stream refs still carry the observation).

use crate::port::ParsedSummary;
use serde_json::Value;

pub const SUMMARY_TYPE: &str = "cargo-json";

pub fn parse_cargo_json(stream: &[u8]) -> Option<ParsedSummary> {
    let text = std::str::from_utf8(stream).ok()?;
    let mut passed = 0u64;
    let mut failed = 0u64;
    let mut ignored = 0u64;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((p, f, i)) = parse_test_result_line(line) {
            passed += p;
            failed += f;
            ignored += i;
            continue;
        }
        // Nightly JSON test events, when present.
        let Ok(msg) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let is_test_msg = msg.get("type").and_then(Value::as_str) == Some("test")
            || msg.get("reason").and_then(Value::as_str) == Some("test");
        if !is_test_msg {
            continue;
        }
        match msg.get("event").and_then(Value::as_str) {
            Some("ok") => passed += 1,
            Some("failed") => failed += 1,
            Some("ignored") => ignored += 1,
            _ => {}
        }
    }
    let test_count = passed + failed + ignored;
    if test_count == 0 {
        return None;
    }
    Some(ParsedSummary {
        summary_type: SUMMARY_TYPE.into(),
        test_count,
        passed,
        failed,
        ignored,
    })
}

/// `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; ...`
fn parse_test_result_line(line: &str) -> Option<(u64, u64, u64)> {
    let rest = line.strip_prefix("test result: ")?;
    // Skip the verdict ("ok." / "FAILED." / "ignored.") before the counts.
    let counts = rest.split_once('.').map(|(_, r)| r).unwrap_or(rest);
    let mut passed = None;
    let mut failed = None;
    let mut ignored = None;
    for part in counts.split(';') {
        let part = part.trim();
        if let Some(n) = part.strip_suffix(" passed") {
            passed = n.trim().parse().ok();
        } else if let Some(n) = part.strip_suffix(" failed") {
            failed = n.trim().parse().ok();
        } else if let Some(n) = part.strip_suffix(" ignored") {
            ignored = n.trim().parse().ok();
        }
    }
    Some((passed?, failed?, ignored?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stable_summary_lines() {
        let stream = br#"{"reason":"compiler-artifact","package_id":"x"}
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 2 tests
test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
"#;
        let summary = parse_cargo_json(stream).unwrap();
        assert_eq!(summary.summary_type, "cargo-json");
        assert_eq!(summary.test_count, 3);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.ignored, 0);
    }

    #[test]
    fn parses_nightly_json_events() {
        let stream = br#"{"type":"test","event":"started","name":"a"}
{"type":"test","event":"ok","name":"a","exec_time":0.1}
{"type":"test","event":"failed","name":"b","exec_time":0.2}
{"type":"test","event":"ignored","name":"c"}
"#;
        let summary = parse_cargo_json(stream).unwrap();
        assert_eq!(summary.passed, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.ignored, 1);
    }

    #[test]
    fn legacy_reason_field_is_accepted() {
        let stream = br#"{"reason":"test","event":"ok","name":"x"}"#;
        let summary = parse_cargo_json(stream).unwrap();
        assert_eq!(summary.passed, 1);
    }

    #[test]
    fn no_terminal_events_is_none() {
        // A compile failure: build messages only, no summary line.
        let stream = br#"{"reason":"compiler-message","message":{"level":"error"}}
{"reason":"build-finished","success":false}
"#;
        assert!(parse_cargo_json(stream).is_none());
    }

    #[test]
    fn garbage_is_none() {
        assert!(parse_cargo_json(b"not json at all\n").is_none());
        assert!(parse_cargo_json(b"").is_none());
    }
}

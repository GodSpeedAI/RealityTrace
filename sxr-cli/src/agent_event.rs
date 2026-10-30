//! `agent-event-v1` ingest format and the ingest-format enum.
//!
//! Input is NDJSON, one `agent-event/v1` envelope per line (ADR-002). Every
//! line is validated before anything is appended: the batch is all-or-nothing,
//! mirroring spool drain, which rejects a stream it cannot parse rather than
//! recording it (a rejected batch is retried whole; per-event dedup makes the
//! retry idempotent). The envelope carries `payload_ref` only: unknown fields
//! are refused, so raw transcript text cannot ride along under a stray key.

use anyhow::{Result, anyhow, bail};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

/// `--stdin-format` value that selects the agent-event parser.
pub const AGENT_EVENT_FORMAT: &str = "agent-event-v1";
/// The `schema` every line must carry.
pub const AGENT_EVENT_SCHEMA: &str = "agent-event/v1";
/// Version of the `extensions.agent_event` object this build writes.
pub const AGENT_EVENT_VERSION: u64 = 1;

/// Spellings that have always selected the (only) cargo/libtest parser. The
/// string used to be ignored, so every spelling a caller ever passed worked;
/// the known in-repo spellings stay accepted and parse exactly as before.
const CARGO_JSON_SPELLINGS: &[&str] = &[
    "cargo-json",
    "cargo-test-json",
    "libtest",
    "car-go-json",
    "pytest-progress",
    "df-validate",
    "corpus",
];

/// The verifier-stream formats `ingest` understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IngestFormat {
    /// cargo/libtest text or JSON (the pre-existing, byte-identical path).
    CargoJson,
    /// NDJSON of `agent-event/v1` envelopes.
    AgentEventV1,
}

impl IngestFormat {
    /// Strict parse of a CLI `--stdin-format`: unknown values are an error.
    pub fn parse(s: &str) -> Result<Self> {
        if s == AGENT_EVENT_FORMAT {
            Ok(Self::AgentEventV1)
        } else if CARGO_JSON_SPELLINGS.contains(&s) {
            Ok(Self::CargoJson)
        } else {
            bail!(
                "unknown --stdin-format {s:?}; supported: {}, {AGENT_EVENT_FORMAT}",
                CARGO_JSON_SPELLINGS.join(", ")
            )
        }
    }

    /// Lenient read of a format string stored in a spool entry. Entries
    /// written before the enum existed carry arbitrary strings and were always
    /// parsed as cargo output; only the new format string selects the new path.
    pub fn from_stored(s: &str) -> Self {
        if s == AGENT_EVENT_FORMAT {
            Self::AgentEventV1
        } else {
            Self::CargoJson
        }
    }
}

/// Idempotency key for one agent event: includes `event_id`, so N distinct
/// events on one run are N observations and a re-delivered event is a no-op.
pub fn event_idempotency_key(run_id: &str, verifier: &str, event_id: &str) -> String {
    let mut h = Sha256::new();
    h.update(run_id.as_bytes());
    h.update(b"+");
    h.update(verifier.as_bytes());
    h.update(b"+agent-event-v1+");
    h.update(event_id.as_bytes());
    hex::encode(h.finalize())
}

/// Spool-entry key for an agent-event batch (file naming and drain check
/// only; the ledger dedups per event, never per batch).
pub fn batch_spool_key(run_id: &str, verifier: &str, stdin_sha256: &str) -> String {
    let mut h = Sha256::new();
    h.update(run_id.as_bytes());
    h.update(b"+");
    h.update(verifier.as_bytes());
    h.update(b"+agent-event-v1-batch+");
    h.update(stdin_sha256.as_bytes());
    hex::encode(h.finalize())
}

/// Deterministic observation id: no clock, no randomness, so replaying the
/// same NDJSON reproduces the same records.
pub fn observation_id_for(event_id: &str) -> String {
    format!("agent-event-{event_id}")
}

/// One validated line.
#[derive(Debug, Clone)]
pub struct AgentEvent {
    pub line_no: usize,
    pub event_id: String,
    pub runtime: String,
    pub role: String,
    pub exit_code: Option<i32>,
    /// The line's bytes (trailing CR trimmed), persisted as the observation's
    /// raw artifact.
    pub raw: Vec<u8>,
    /// The `extensions.agent_event` object (version, verifier included).
    pub record: Value,
}

impl AgentEvent {
    pub fn is_builder(&self) -> bool {
        self.role == "builder"
    }
}

const SOURCES: &[&str] = &["hook", "te_file", "continuity", "owner_output"];
const RUNTIMES: &[&str] = &["claude", "codex", "opencode", "prime", "pi", "cline", "agy"];
const KINDS: &[&str] = &[
    "session_start",
    "tool_post",
    "tool_fail",
    "journey_check",
    "stop",
    "session_end",
    "usage",
    "te_injection",
    "te_overhead",
    "backfill",
    "conversion",
];
const ROLES: &[&str] = &["user", "benchmark", "builder"];
const ARMS: &[&str] = &["B", "T", "G", "F", "none"];
const QUALITIES: &[&str] = &["EXACT", "DERIVED", "ESTIMATED", "UNAVAILABLE"];
const OWNERS: &[&str] = &["native", "te", "conversion", "backfill"];
const TOKEN_KEYS: &[&str] = &["input", "output", "cache_write", "cache_read"];
const MAX_STR: usize = 512;
const MAX_CASE_ID: usize = 200;

/// Every key an envelope may carry; anything else is refused.
const ALLOWED: &[&str] = &[
    "schema",
    "event_id",
    "source",
    "runtime",
    "native_session_id",
    "native_event_ref",
    "kind",
    "ts",
    "observed_at",
    "case_id",
    "unit_id",
    "te_session_id",
    "role",
    "arm",
    "repo",
    "worktree",
    "branch",
    "tokens",
    "token_quality",
    "owner",
    "attributed_input_tokens",
    "payload_ref",
    "exit_code",
];

fn req_str<'a>(o: &'a Map<String, Value>, k: &str) -> Result<&'a str> {
    match o.get(k) {
        None => bail!("missing required field {k:?}"),
        Some(Value::String(s)) if !s.is_empty() && s.len() <= MAX_STR => Ok(s),
        Some(Value::String(s)) if s.is_empty() => bail!("field {k:?} is empty"),
        Some(Value::String(_)) => bail!("field {k:?} exceeds {MAX_STR} bytes"),
        Some(_) => bail!("field {k:?} must be a string"),
    }
}

fn req_enum<'a>(o: &'a Map<String, Value>, k: &str, allowed: &[&str]) -> Result<&'a str> {
    let s = req_str(o, k)?;
    if allowed.contains(&s) {
        Ok(s)
    } else {
        bail!("field {k:?} = {s:?} is not one of {allowed:?}")
    }
}

fn opt_str_or_null(o: &Map<String, Value>, k: &str) -> Result<Value> {
    match o.get(k) {
        None | Some(Value::Null) => Ok(Value::Null),
        Some(Value::String(s)) if s.len() <= MAX_STR => Ok(Value::String(s.clone())),
        Some(Value::String(_)) => bail!("field {k:?} exceeds {MAX_STR} bytes"),
        Some(_) => bail!("field {k:?} must be a string or null"),
    }
}

fn rfc3339(o: &Map<String, Value>, k: &str, required: bool) -> Result<Option<String>> {
    match o.get(k) {
        None if !required => Ok(None),
        None => bail!("missing required field {k:?}"),
        Some(Value::String(s)) => {
            chrono::DateTime::parse_from_rfc3339(s)
                .map_err(|e| anyhow!("field {k:?} is not RFC 3339: {e}"))?;
            Ok(Some(s.clone()))
        }
        Some(_) => bail!("field {k:?} must be an RFC 3339 string"),
    }
}

fn validate_tokens(o: &Map<String, Value>) -> Result<Value> {
    match o.get("tokens") {
        None => bail!("missing required field \"tokens\" (use null when unavailable)"),
        Some(Value::Null) => Ok(Value::Null),
        Some(Value::Object(t)) => {
            if let Some(k) = t.keys().find(|k| !TOKEN_KEYS.contains(&k.as_str())) {
                bail!("tokens has unknown key {k:?}");
            }
            let mut out = Map::new();
            for k in TOKEN_KEYS {
                match t.get(*k).and_then(Value::as_u64) {
                    Some(n) => out.insert((*k).to_string(), json!(n)),
                    None => bail!("tokens.{k} must be a non-negative integer"),
                };
            }
            Ok(Value::Object(out))
        }
        Some(_) => bail!("field \"tokens\" must be null or an object of 4 integers"),
    }
}

fn validate_exit_code(o: &Map<String, Value>, kind: &str) -> Result<Option<i32>> {
    let code = match o.get("exit_code") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            v.as_i64()
                .and_then(|n| i32::try_from(n).ok())
                .ok_or_else(|| anyhow!("field \"exit_code\" must be a 32-bit integer or null"))?,
        ),
    };
    match (kind, code) {
        ("tool_post" | "tool_fail" | "journey_check", None) => {
            bail!("kind {kind:?} requires an integer exit_code (direction is never guessed)")
        }
        ("tool_fail", Some(0)) => bail!("kind \"tool_fail\" cannot carry exit_code 0"),
        // journey_check: 0 = declared artifact observed and satisfied; non-zero =
        // observed and contradicted. Absence of an observation is "undefined".
        ("tool_post" | "tool_fail" | "journey_check", Some(_)) => {}
        (_, Some(_)) => bail!("kind {kind:?} must have a null exit_code"),
        (_, None) => {}
    }
    Ok(code)
}

fn validate_line(line_no: usize, raw: &[u8], verifier: &str) -> Result<AgentEvent> {
    let v: Value = serde_json::from_slice(raw).map_err(|e| anyhow!("invalid JSON: {e}"))?;
    let o = v
        .as_object()
        .ok_or_else(|| anyhow!("line is not a JSON object"))?;
    if let Some(k) = o.keys().find(|k| !ALLOWED.contains(&k.as_str())) {
        bail!(
            "field {k:?} is not part of agent-event/v1 (raw transcript text is never stored; reference it with payload_ref)"
        );
    }
    if o.get("schema").and_then(Value::as_str) != Some(AGENT_EVENT_SCHEMA) {
        bail!("field \"schema\" must be {AGENT_EVENT_SCHEMA:?}");
    }
    let event_id = req_str(o, "event_id")?;
    if event_id.len() != 64
        || !event_id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        bail!("field \"event_id\" must be 64 lowercase hex characters (sha256)");
    }
    let source = req_enum(o, "source", SOURCES)?;
    let runtime = req_enum(o, "runtime", RUNTIMES)?;
    let native_session_id = req_str(o, "native_session_id")?;
    let native_event_ref = req_str(o, "native_event_ref")?;
    let kind = req_enum(o, "kind", KINDS)?;
    let ts = rfc3339(o, "ts", true)?.expect("required");
    let observed_at = rfc3339(o, "observed_at", false)?;
    // Opaque: Gauntlet ids, `case:{run_id}` from cognate-workbench, or the "unknown" sentinel.
    let case_id = req_str(o, "case_id")?;
    if case_id.chars().count() > MAX_CASE_ID {
        bail!("field \"case_id\" exceeds {MAX_CASE_ID} characters");
    }
    let unit_id = req_str(o, "unit_id")?;
    let te_session_id = req_str(o, "te_session_id")?;
    let role = req_enum(o, "role", ROLES)?;
    let token_quality = req_enum(o, "token_quality", QUALITIES)?;
    let owner = req_enum(o, "owner", OWNERS)?;
    let tokens = validate_tokens(o)?;
    let exit_code = validate_exit_code(o, kind)?;
    let arm = match o.get("arm") {
        None | Some(Value::Null) => Value::Null,
        Some(Value::String(s)) if ARMS.contains(&s.as_str()) => json!(s),
        Some(_) => bail!("field \"arm\" must be one of {ARMS:?}"),
    };
    let attributed = match o.get("attributed_input_tokens") {
        None | Some(Value::Null) => Value::Null,
        Some(v) => json!(v.as_u64().ok_or_else(|| anyhow!(
            "field \"attributed_input_tokens\" must be a non-negative integer"
        ))?),
    };
    let payload_ref = match o.get("payload_ref") {
        None | Some(Value::Null) => Value::Null,
        Some(Value::String(s)) if s.len() <= MAX_STR && !s.chars().any(char::is_control) => {
            json!(s)
        }
        Some(_) => bail!(
            "field \"payload_ref\" must be a short single-line string pointer (<= {MAX_STR} bytes), never transcript text"
        ),
    };
    let record = json!({
        "agent_event_version": AGENT_EVENT_VERSION,
        "schema": AGENT_EVENT_SCHEMA,
        "event_id": event_id,
        "source": source,
        "runtime": runtime,
        "verifier": verifier,
        "native_session_id": native_session_id,
        "native_event_ref": native_event_ref,
        "kind": kind,
        "ts": ts,
        "observed_at": observed_at,
        "case_id": case_id,
        "unit_id": unit_id,
        "te_session_id": te_session_id,
        "role": role,
        "arm": arm,
        "repo": opt_str_or_null(o, "repo")?,
        "worktree": opt_str_or_null(o, "worktree")?,
        "branch": opt_str_or_null(o, "branch")?,
        "tokens": tokens,
        "token_quality": token_quality,
        "owner": owner,
        "attributed_input_tokens": attributed,
        "payload_ref": payload_ref,
        "exit_code": exit_code,
    });
    Ok(AgentEvent {
        line_no,
        event_id: event_id.to_string(),
        runtime: runtime.to_string(),
        role: role.to_string(),
        exit_code,
        raw: raw.to_vec(),
        record,
    })
}

/// The verifier id an agent-tap batch must use for `runtime`.
pub fn verifier_for_runtime(runtime: &str) -> String {
    format!("agent-tap/{runtime}")
}

/// Parse and validate a whole NDJSON batch against the `--verifier` it is
/// ingested under. All-or-nothing: the first invalid line aborts with its
/// 1-based line number. Blank lines are skipped (they carry no evidence); a
/// batch with no events at all is an error. A batch that mixes builder and
/// non-builder roles is refused (HC-A2/INV-20).
pub fn parse_batch(bytes: &[u8], verifier: &str) -> Result<Vec<AgentEvent>> {
    let mut events: Vec<AgentEvent> = Vec::new();
    for (i, chunk) in bytes.split(|b| *b == b'\n').enumerate() {
        let line_no = i + 1;
        let line = chunk.strip_suffix(b"\r").unwrap_or(chunk);
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        if std::str::from_utf8(line).is_err() {
            bail!("agent-event-v1 line {line_no}: not valid UTF-8");
        }
        let ev = validate_line(line_no, line, verifier)
            .map_err(|e| anyhow!("agent-event-v1 line {line_no}: {e}"))?;
        let want = verifier_for_runtime(&ev.runtime);
        if verifier != want {
            bail!(
                "agent-event-v1 line {line_no}: --verifier {verifier:?} does not match runtime {:?} (expected {want:?})",
                ev.runtime
            );
        }
        if let Some(prev) = events.iter().find(|p| p.event_id == ev.event_id)
            && prev.raw != ev.raw
        {
            bail!(
                "agent-event-v1 line {line_no}: event_id {} already appears on line {} with different content",
                ev.event_id,
                prev.line_no
            );
        }
        events.push(ev);
    }
    if events.is_empty() {
        bail!("agent-event-v1: input contains no events");
    }
    if let Some(first) = events.first()
        && let Some(other) = events.iter().find(|e| e.is_builder() != first.is_builder())
    {
        bail!(
            "agent-event-v1: batch mixes builder and non-builder roles (line {} role={}, line {} role={}); builder events go to their own run",
            first.line_no,
            first.role,
            other.line_no,
            other.role
        );
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_enum_is_strict_but_keeps_legacy_spellings() {
        assert_eq!(
            IngestFormat::parse("cargo-json").unwrap(),
            IngestFormat::CargoJson
        );
        assert_eq!(
            IngestFormat::parse("libtest").unwrap(),
            IngestFormat::CargoJson
        );
        assert_eq!(
            IngestFormat::parse("agent-event-v1").unwrap(),
            IngestFormat::AgentEventV1
        );
        assert!(IngestFormat::parse("nope").is_err());
        assert_eq!(
            IngestFormat::from_stored("anything"),
            IngestFormat::CargoJson
        );
    }

    #[test]
    fn keys_differ_per_event_and_from_the_legacy_key() {
        let a = event_idempotency_key("r", "agent-tap/claude", &"a".repeat(64));
        let b = event_idempotency_key("r", "agent-tap/claude", &"b".repeat(64));
        assert_ne!(a, b);
        assert_ne!(
            a,
            crate::commands::ingest_idempotency_key("r", "agent-tap/claude")
        );
    }
}

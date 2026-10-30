//! Optional daemon (§16): a Unix-socket service that holds the writer lock,
//! accepts validated envelope requests, appends them to the durable commit
//! queue (fsynced segment + receipt), and commits them from the queue in
//! batches. The queue protocol and recovery live in the shared adapter
//! (`sxr_ledger::queue` + `sxr queue recover`) — nothing here is daemon-only
//! (§16.2). The daemon adds convenience, never semantics (§16.3 parity).

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(unix)]
use std::path::{Path, PathBuf};
use std::str::FromStr;
#[cfg(unix)]
use std::thread;

use crate::commands;
use crate::state::Ctx;

/// The request the client submits over the socket: one metadata line then the
/// envelope line. The daemon validates and enqueues exactly what the CLI
/// would have appended itself (§16.2).
pub fn record_request(repo_id: &str, built: &commands::IngestEnvelope) -> Value {
    serde_json::json!({
        "request_kind": "sxr.record/v1",
        "repo_id": repo_id,
        "record_kind": built.kind.as_str(),
        "idempotency_key": built.idempotency_key,
        "subject_refs": built.subject_refs,
        "envelope": built.envelope,
    })
}

/// Validate a queued record request enough to commit it: shape, registered
/// kind, and record-kind conformance (§16.2 — the daemon validates, the
/// recovery path commits without re-validating the envelope bytes).
pub fn validate_record_request(request: &Value) -> Result<commands::IngestEnvelope> {
    if request.get("request_kind").and_then(Value::as_str) != Some("sxr.record/v1") {
        bail!("queue entry is not an sxr.record/v1 request");
    }
    let kind = request
        .get("record_kind")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("queue entry missing record_kind"))?;
    let kind = sxr_core::record_kinds::RecordKind::from_str(kind)
        .map_err(|_| anyhow!("queue entry names unknown record kind {kind}"))?;
    let envelope = request
        .get("envelope")
        .cloned()
        .ok_or_else(|| anyhow!("queue entry missing envelope"))?;
    let subject_refs: Vec<String> = request
        .get("subject_refs")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .ok_or_else(|| anyhow!("queue entry missing subject_refs"))?;
    let idempotency_key = request
        .get("idempotency_key")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("queue entry missing idempotency_key"))?
        .to_string();
    sxr_core::envelope::validate_record_kind_conformance(&kind, &envelope, &subject_refs)
        .map_err(|e| anyhow!("queue entry fails kind conformance: {e}"))?;
    Ok(commands::IngestEnvelope {
        envelope,
        subject_refs,
        kind,
        idempotency_key,
        observation_id: String::new(),
        parsed: Value::Null,
    })
}

/// Drain every repo's queue into the ledger (daemon loop body and `sxr queue
/// recover` share this). Each repo's writer lock is held for the whole batch
/// (§11.1). Returns the number of segments committed.
pub fn drain_all_queues(ctx: &Ctx) -> Result<u64> {
    let mut total = 0u64;
    let enrollments = crate::state::list_enrollments(ctx)?;
    for e in &enrollments {
        total += drain_repo_queue(ctx, &e.repo_id)?;
    }
    Ok(total)
}

/// Drain one repo's queue (`sxr queue recover --repo <id>`).
pub fn drain_all_queues_limited(ctx: &Ctx, repo_id: &str) -> Result<u64> {
    crate::state::require_enrollment(ctx, repo_id)?;
    drain_repo_queue(ctx, repo_id)
}

fn drain_repo_queue(ctx: &Ctx, repo_id: &str) -> Result<u64> {
    let mut repo = crate::ledger_io::open_repo_ledger(ctx, repo_id)?;
    let report = sxr_ledger::queue::recover(&ctx.queue_root(), repo_id, |entry| {
        let request: Value = serde_json::from_str(&entry.request)
            .map_err(|e| anyhow!("queue segment {}: {e}", entry.path.display()))
            .map_err(|e| sxr_ledger::Error::Integrity(format!("{e:#}")))?;
        let built = validate_record_request(&request)
            .map_err(|e| sxr_ledger::Error::Integrity(format!("{e:#}")))?;
        crate::ledger_io::append_record(
            &mut repo,
            built.kind,
            Some(built.idempotency_key),
            built.subject_refs,
            &built.envelope,
        )
        .context("commit queued request")
        .map_err(|e| sxr_ledger::Error::Integrity(format!("{e:#}")))?;
        Ok(())
    })
    .with_context(|| format!("recover queue for {repo_id}"))?;
    drop(repo);
    if !report.corrupt.is_empty() {
        bail!(
            "queue for {repo_id} has corrupt segments (frozen, retained byte-for-byte): {}",
            report.corrupt.join(", ")
        );
    }
    Ok(report.recovered)
}

/// Minimal sd_notify(3) support: write `READY=1` to `$NOTIFY_SOCKET`
/// (systemd user unit, §18.4). A no-op when the variable is unset (tests and
/// proof scripts run without systemd).
#[cfg(unix)]
fn notify_ready() {
    use std::io::Write;
    let Ok(sock_path) = std::env::var("NOTIFY_SOCKET") else {
        return;
    };
    if let Ok(stream) = UnixStream::connect(&sock_path) {
        let mut stream = stream;
        let _ = stream.write_all(b"READY=1\n");
    }
}

/// Bind the listener, replacing a stale socket left by an unclean shutdown
/// (an old socket whose server is gone must not block startup).
#[cfg(unix)]
fn bind_listener(socket: &Path) -> Result<UnixListener> {
    if socket.exists() {
        match UnixStream::connect(socket) {
            Ok(_) => bail!("another sxr daemon is already serving {}", socket.display()),
            Err(_) => {
                std::fs::remove_file(socket)
                    .with_context(|| format!("remove stale socket {}", socket.display()))?;
            }
        }
    }
    if let Some(parent) = socket.parent() {
        std::fs::create_dir_all(parent).context("create socket parent dir")?;
    }
    UnixListener::bind(socket).with_context(|| format!("bind {}", socket.display()))
}

/// One accepted client: read the request line, validate, enqueue (fsync +
/// receipt), respond with the receipt line — or an `{"error": ...}` line when
/// the request is rejected, so the caller never mistakes rejection for
/// success (§11.1a: `accepted` is never a successful final response).
#[cfg(unix)]
fn handle_client(ctx: &Ctx, mut stream: UnixStream) -> Result<()> {
    use std::io::{BufRead, BufReader, Write};
    let mut reader = BufReader::new(stream.try_clone().context("clone socket")?);
    let mut line = String::new();
    let n = reader.read_line(&mut line).context("read request line")?;
    if n == 0 {
        return Ok(());
    }
    let outcome = (|| -> Result<Value> {
        let request: Value =
            serde_json::from_str(line.trim()).context("parse request metadata line")?;
        let built = validate_record_request(&request).context("validate request")?;
        let repo_id = request
            .get("repo_id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("request missing repo_id"))?;
        let canonical = serde_json::to_string(&request).context("serialize canonical request")?;
        let receipt = sxr_ledger::queue::enqueue(
            &ctx.queue_root(),
            repo_id,
            &built.idempotency_key,
            &canonical,
        )
        .context("enqueue")?;
        // Issue the receipt only now: file + parent dir are fsynced (§11.1a).
        serde_json::to_value(&receipt).context("serialize receipt")
    })();
    match outcome {
        Ok(receipt) => {
            let line = serde_json::to_string(&receipt).context("serialize receipt line")?;
            stream.write_all(line.as_bytes())?;
            stream.write_all(b"\n")?;
            stream.flush().ok();
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("{e:#}")});
            let line = serde_json::to_string(&err).context("serialize error line")?;
            stream.write_all(line.as_bytes())?;
            stream.write_all(b"\n")?;
            stream.flush().ok();
        }
    }
    Ok(())
}

/// `sxr daemon`: run until killed. `--drain-delay-ms` paces the commit loop
/// (a test hook: a long delay makes kill-after-receipt deterministic).
#[cfg(unix)]
pub fn daemon_run(ctx: &Ctx, socket: Option<&str>, drain_delay_ms: u64) -> Result<()> {
    let socket_path: PathBuf = socket
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.socket_path());
    let listener = bind_listener(&socket_path)?;
    notify_ready();
    let ctx2 = Ctx {
        data_root: ctx.data_root.clone(),
        state_root: ctx.state_root.clone(),
    };
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else {
                continue;
            };
            let handle = handle_client(&ctx2, stream);
            if let Err(e) = handle {
                eprintln!("sxr daemon: request failed: {e:#}");
            }
        }
    });
    loop {
        if let Err(e) = drain_all_queues(ctx) {
            eprintln!("sxr daemon: queue drain failed: {e:#}");
        }
        thread::sleep(std::time::Duration::from_millis(drain_delay_ms));
    }
}

/// Non-Unix builds carry no socket transport. The daemon is refused
/// explicitly — never silently degraded — while every queue/drain/recovery
/// entry point above remains available (they are transport-independent).
#[cfg(not(unix))]
pub fn daemon_run(_ctx: &Ctx, _socket: Option<&str>, _drain_delay_ms: u64) -> Result<()> {
    bail!(
        "the RealityTrace daemon is not supported on this platform: it communicates \
         over a Unix-domain socket, which this OS does not provide. Direct CLI \
         operation (including queue recovery) is fully supported."
    );
}

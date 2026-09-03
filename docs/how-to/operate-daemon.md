# How-To: Operate the Background Daemon & Recover Commit Queues

This guide describes how to run the SXR background daemon for high-throughput verifier event ingestion, monitor commit queues and spools, and recover pending records after an abnormal daemon termination.

---

## Goal
Run `sxr daemon` as a local service, submit asynchronous ingestion requests, and recover uncommitted queue segments following a crash.

---

## Prerequisites
- SXR binary compiled and available on `PATH`.
- Write access to `$XDG_RUNTIME_DIR` (or specify an explicit socket via `SXR_DAEMON_SOCKET`).

---

## Procedure

### 1. Start the Background Daemon
Run the daemon service in the background or in a separate terminal:

```bash
sxr daemon --socket /tmp/sxr.sock --drain-delay-ms 250
```

> [!TIP]
> In production systemd environments, deploy using the provided unit file:
> ```bash
> systemctl --user start sxr.service
> ```

Verify the daemon is alive:
```bash
sxr doctor --json
```
The output should report `"daemon_socket": "active"`.

### 2. Submit Asynchronous Ingestion Requests
When piping streaming verifier outputs into SXR, use `--async-mode` to submit through the daemon:

```bash
cargo test --message-format json | sxr ingest \
  --run <run_id> \
  --verifier cargo-test \
  --stdin-format cargo-json \
  --async-mode \
  --json
```

**Verification**: SXR exits immediately with code `0` once the segment is fsynced to disk, returning:
```json
{
  "status": "queued_for_commit",
  "receipt_id": "01J7...",
  "daemon": true
}
```

### 3. Await Commit Completion
To ensure all queued records for a run are committed before advancing to evidence derivation:

```bash
sxr await --run <run_id> --timeout-secs 30 --json
```

### 4. Recover After a Daemon Crash (`kill -9`)
If the daemon process terminates unexpectedly (e.g. power failure, host reboot, or `kill -9`):

Execute queue recovery:
```bash
sxr queue recover --json
```

**Expected Result**:
```json
{
  "recovered": 3
}
```
SXR scans `$XDG_STATE_HOME/sxr/daemon-queue/`, validates segment SHA-256 digests, acquires the ledger writer lock, and commits all pending segments. Replay is fully idempotent.

### 5. Inspect and Drain the Spool
If requests were submitted with `--spool` due to lock contention:

Inspect the spool status:
```bash
sxr spool status --repo <repo_id> --json
```

Drain the spool into the ledger:
```bash
sxr spool drain --repo <repo_id> --json
```

---

## Implementation Locations
- Daemon listener: [`sxr-cli/src/daemon.rs:daemon_run`](file:///home/sprime01/projects/sxr/sxr-cli/src/daemon.rs)
- Queue recovery: [`sxr-cli/src/daemon.rs:drain_all_queues`](file:///home/sprime01/projects/sxr/sxr-cli/src/daemon.rs)
- Queue serialization: [`sxr-ledger/src/queue.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/src/queue.rs)
- Durability scanner: [`sxr-cli/src/durability.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/durability.rs)

---

## Common Failure Symptoms & Troubleshooting

| Symptom | Cause | Resolution |
| :--- | :--- | :--- |
| `daemon: socket already in use` (Exit 2) | Another daemon instance is already bound to the socket. | Stop the existing daemon or configure a separate socket path. |
| `queue recover: corrupt segments found` (Exit 3) | A queue segment file was partially written or corrupted during a crash. | SXR refuses to delete corrupt segments. Inspect the corrupted file in `$XDG_STATE_HOME/sxr/daemon-queue/`. |
| `evidence create: spooled observations exist` (Exit 3) | Attempted evidence creation while uncommitted requests exist in the spool. | Run `sxr spool drain --repo <repo_id>` before proceeding. |

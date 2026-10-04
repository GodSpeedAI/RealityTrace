# Technical Reference: Environment Variables & Filesystem Layout

This document specifies all environment variables recognized by SXR and details the exact directory structure managed under the XDG Base Directory specification.

---

## 1. Environment Variables

| Variable | Default Value | Purpose |
| :--- | :--- | :--- |
| **`XDG_DATA_HOME`** | `~/.local/share` | Base path for permanent user data files. SXR stores repository ledgers, registries, and CAS blobs under `$XDG_DATA_HOME/sxr`. |
| **`XDG_STATE_HOME`** | `~/.local/state` | Base path for state and transient custody. SXR stores commit queues and spools under `$XDG_STATE_HOME/sxr`. |
| **`XDG_RUNTIME_DIR`** | `/tmp` (or system runtime) | Base path for user runtime files. The daemon binds its socket at `$XDG_RUNTIME_DIR/sxr.sock`. |
| **`SXR_DAEMON_SOCKET`** | `$XDG_RUNTIME_DIR/sxr.sock` | Explicit override for the UNIX domain socket path used by `sxr daemon` and client connections. |
| **`SXR_DOMAINFORGE_BIN`** | `domainforge` (on `PATH`) | Absolute path to the DomainForge compiler binary for semantic model compilation. `snapshot create` fails closed unless it reports the pinned release (`sxr_df::PINNED_DOMAINFORGE_VERSION`, currently 0.19.0). |
| **`SXR_BIN`** | Resolved via `command -v sxr` | Absolute path to the `sxr` executable (used in test and proof scripts). |
| **`SXR_TEST_ID_STREAM`** | Unset (random ULIDs minted) | Path to a text file containing pre-generated IDs for deterministic test replay (AC-DAEMON-001). |
| **`SXR_TEST_FIXED_CLOCK`** | Unset (system wall clock used) | Fixed RFC 3339 timestamp string for deterministic test replay. |

---

## 2. Sandboxed Filesystem Directory Tree

SXR strictly segregates persistent data from transient state:

```text
$XDG_DATA_HOME/sxr/
└── repos/
    └── <repo_id>/                        # 128-bit ULID directory for enrolled repository
        ├── registry.json                 # Enrollment metadata & Git identity evidence
        ├── semantic.json                 # Configured entry .sea path & namespace patterns
        ├── writer.lock                   # File descriptor target for advisory flock locking
        ├── ledger.db                     # SQLite database: ledger, outbox, and projection tables
        ├── ledger.db-wal                 # SQLite Write-Ahead Log (created during transactions)
        ├── ledger.db-shm                 # SQLite shared memory index for WAL
        └── cas/                          # Content-Addressable Storage root
            └── sha256/
                └── <prefix_2_chars>/
                    └── <remainder_62_chars>  # Raw representation bytes (immutable)

$XDG_STATE_HOME/sxr/
├── daemon-queue/
    └── <repo_id>/                        # Durable commit queue directory
        ├── <ulid>.segment                # Fsynced request payload awaiting batch commit
        └── <ulid>.receipt                # Fsynced receipt confirming custody to client
└── spool/
    └── <repo_id>/                        # Opt-in unvalidated request spool
        └── <timestamp>_<id>.ndjson       # Spooled request entries written under lock contention

$XDG_RUNTIME_DIR/
└── sxr.sock                              # Local UNIX domain socket listener for daemon IPC
```

---

## 3. Invariants Governing Filesystem Access

1. **Repository Non-Contamination (AC-SECURITY-001)**: Under no circumstances does SXR write, touch, or delete files inside the enrolled Git repository working tree.
2. **Crash Resilience**: Segment files in `daemon-queue/` are synced using `file.sync_all()` before receipts are issued.
3. **Atomic Writes**: `registry.json` and `semantic.json` are written via memory buffers before serialization.
4. **Lock Exclusivity**: `writer.lock` must be held exclusively whenever `ledger.db` is opened for writing.

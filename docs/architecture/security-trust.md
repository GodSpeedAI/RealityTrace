# Security & Trust Boundaries

SXR is designed for environments where autonomous agents, untrusted third-party code, or arbitrary test suites are executed. It enforces strict security boundaries to prevent repository pollution, identity spoofing, and unearned verification claims.

---

## 1. Repository Non-Mutation Invariant (AC-SECURITY-001)

SXR **never creates, modifies, or deletes any file inside the enrolled repository**:
- All databases (`ledger.db`), advisory locks (`writer.lock`), enrollment registries (`registry.json`), semantic configuration (`semantic.json`), and CAS blobs reside exclusively in user-sandboxed directories under `$XDG_DATA_HOME/sxr`.
- All transient queues, spools, and daemon logs reside exclusively under `$XDG_STATE_HOME/sxr`.
- This guarantee is verified in CI and proof scripts (`proof/gate1.sh`) via:
  ```bash
  git -C <target_repo> status --porcelain | wc -l | grep -qx 0
  ```

---

## 2. Identity Minting & Anti-Spoofing (AC-IDENTITY-001 & 002)

Traditional developer tools derive repository identity from Git remote URLs (`git remote get-url origin`) or initial commit hashes. This approach fails in modern workflows:
- Checkouts may lack a remote (local scratch repos).
- Remotes change when repositories are renamed or transferred.
- Two distinct checkouts (e.g. two branches or worktrees) have identical commit histories, leading to collision.

### SXR Identity Rules
1. **Fresh ULIDs at Enrollment**: When `sxr repo enroll <path>` is executed, SXR mints a random 128-bit ULID for `repo_id` and a separate ULID for `checkout_id`.
2. **Never Derived from Git**: Identity is permanently decoupled from commit hashes, branches, or author emails.
3. **Partitioned Graph Invariant (AC-IDENTITY-002)**: If two different enrolled repositories define identical `.sea` declarations (same concept names and IDs), SXR scopes their graphs by `(snapshot_id, decl_key)`. They remain completely isolated and never merge.

---

## 3. Verifier Execution & Process Sandboxing

When SXR runs test suites (`sxr verify run`), it invokes external binaries (like `cargo test`):
- **Working Directory**: The verifier is executed within the target repository checkout.
- **Environment Scrubbing**: SXR filters environment variables via `sxr_verify::port::filtered_env`, stripping internal telemetry and unwanted flags while injecting necessary execution context.
- **Read-Only Telemetry Capture**: SXR attaches pipes to stdout and stderr. The captured output is treated strictly as an unauthenticated external stream until its SHA-256 hash is computed and committed to the ledger.

---

## 4. Daemon Trust & Socket Security

The `sxr daemon` communicates over a local **UNIX domain socket**:
- **Socket Path**: `$XDG_RUNTIME_DIR/sxr.sock` (typically permissions `0700` owned by the user).
- **No Network Exposure**: SXR does not open TCP or UDP ports. Remote execution must be tunneled through standard SSH or container boundaries.
- **Singleton Enforcement**: The daemon binds with exclusive socket creation. A second concurrent daemon instance is immediately rejected.

---

## 5. Fail-Closed Validation

SXR strictly implements a **fail-closed security posture**:
- **Torn Queue Segments**: If a commit queue segment is corrupted or truncated during a power loss, recovery halts with exit code 3 (`corrupt segments`), preserving the corrupted file verbatim for forensic analysis.
- **Invalid Envelope Payloads**: If an envelope violates any rule of `cep-semantic-envelope.schema.json` or attempts an illegal collapse shape (e.g. adding `raw_data` to an evidence record), the parser rejects it with exit code 4 (`CEP validation`). It is never silently repaired or coerced.
- **Chain Breaks**: If a single byte in `ledger.db` is altered, `sxr ledger verify` detects the hash mismatch and pinpoints the exact `(ledger_id, append_ordinal)` where tampering occurred.

---

## 6. Source Trail

- [`sxr-cli/src/state.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/state.rs) — XDG directory layout and repository non-mutation enforcement.
- [`sxr-git/src/identity.rs`](file:///home/sprime01/projects/sxr/sxr-git/src/identity.rs) — ULID identity minting and identity evidence capture.
- [`sxr-verify/src/port.rs`](file:///home/sprime01/projects/sxr/sxr-verify/src/port.rs) — Subprocess execution environment filtering.
- [`sxr-cli/src/projections.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/projections.rs) — Multi-tenant graph partitioning (AC-IDENTITY-002).
- [`proof/gate1.sh`](file:///home/sprime01/projects/sxr/proof/gate1.sh) — Test verification of the Git clean invariant.

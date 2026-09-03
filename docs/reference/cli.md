# Technical Reference: `sxr` CLI Manual

This document provides the definitive reference for the `sxr` command-line executable.

---

## Global Options

| Flag | Type | Description |
| :--- | :--- | :--- |
| `--json` | Flag (global) | Emit machine-readable JSON on stdout. |
| `--help` / `-h` | Flag (global) | Print command usage instructions. |
| `--version` / `-V` | Flag (global) | Print version information. |

---

## Subcommand Catalog

### 1. `repo` (Repository Enrollment & Identity)

#### `sxr repo enroll <path> [--json]`
- **Purpose**: Enrolls a Git repository into SXR and mints fresh ULID identities.
- **Arguments**: `<path>` (Required) — Path to the target Git repository checkout.
- **Output JSON**: `{ repo_id: String, checkout_id: String, path: String, ledger_position: Object }`.
- **Exit Codes**: `0` on success, `2` if path is not a Git repo.

#### `sxr repo list [--json]`
- **Purpose**: Lists all enrolled repositories.
- **Output JSON**: `{ repos: [ { repo_id, checkout_id, filesystem_path, created_at } ] }`.

#### `sxr repo identity <repo_id> [--json]`
- **Purpose**: Displays the identity evidence and Git snapshot recorded at enrollment.

---

### 2. `semantic` (Semantic Context Configuration)

#### `sxr semantic configure --repo <repo_id> --entry <path> [--json]`
- **Purpose**: Configures the relative logical entry path to `.sea` model files and loads namespace rules.
- **Flags**:
  - `--repo <id>`: (Required) Enrolled repository ULID.
  - `--entry <path>`: (Required) Relative path to the entry `.sea` file.

#### `sxr semantic status --repo <repo_id> [--json]`
- **Purpose**: Prints current semantic namespace configuration for the repository.

---

### 3. `snapshot` (Semantic Snapshots)

#### `sxr snapshot create --repo <repo_id> [--json]`
- **Purpose**: Invokes DomainForge compiler to extract a declarative semantic snapshot ($G_{declared}$).
- **Output JSON**: `{ snapshot_id, repo_id, semantic_closure_hash, model_validation_status, checkpoint_hash }`.
- **Exit Codes**: `0` on success, `3` if repo not enrolled, `4` if compiler output violates CEP schema.

---

### 4. `question` & `threshold` (Context Declarations)

#### `sxr question declare --repo <repo_id> --file <path> [--json]`
- **Purpose**: Declares a formal CEP Question envelope.
- **Flags**: `--repo <id>`, `--file <path>` (JSON containing `question_form`, `question_type`, `target_entities`).
- **Output JSON**: `{ question_id: String }`.

#### `sxr threshold declare --repo <repo_id> --question <question_id> --file <path> [--json]`
- **Purpose**: Declares a settlement threshold spec bound to a question.
- **Flags**: `--repo <id>`, `--question <id>`, `--file <path>` (JSON containing `kind`: `AllEvidenceSupports`, `AnyEvidenceContradicts`, or `RequiresEvidence`).
- **Output JSON**: `{ threshold_id: String }`.

---

### 5. `run` & `plan` (Run Lifecycle)

#### `sxr run begin --repo <id> --snapshot <id> --question <id> --threshold <id> [--json]`
- **Purpose**: Initiates an active verification run binding a snapshot, question, and threshold.
- **Output JSON**: `{ run_id: String }`.

#### `sxr plan create --run <id> --verifier <id> --target <target> [--from-contract <c>] [--obligation <o>] [--json]`
- **Purpose**: Builds an executable verification plan.
- **Flags**:
  - `--run <id>`: Active run ID.
  - `--verifier <id>`: Verifier runner ID (e.g. `cargo-test`).
  - `--target <str>`: Target declaration URI (e.g. `concept:<uuid>`).
  - `--from-contract <str>`: (Optional) Import verification obligation from contract.
- **Output JSON**: `{ plan_id: String, transformation_ref: String }`.

#### `sxr run finish --run <id> [--json]`
- **Purpose**: Concludes an active run.
- **Output JSON**: `{ run_id: String, status: "finished" }`.

---

### 6. `verify` (Verification Execution)

#### `sxr verify run --run <id> [--json]`
- **Purpose**: Executes the plan steps using the designated verifier runner in a filtered environment.
- **Output JSON**: `{ run_id: String, status: "observed", observations: [ { observation_id, exit_code, parsed_summary } ] }`.
- **Exit Codes**: `0` if verifier completed, `6` if runner failed to spawn or timed out.

---

### 7. `evidence` & `claim` (Evidence Derivation)

#### `sxr evidence create --run <id> --observation <id> --derive [--evidence-type <type>] [--json]`
- **Purpose**: Derives question-bound evidence from an observation.
- **Flags**:
  - `--run <id>`: Active run ID.
  - `--observation <id>`: Committed observation ID.
  - `--derive`: (Required in Slice 1) Compute direction from exit code.
  - `--evidence-type <type>`: Defaults to `test_result`.
- **Output JSON**: `{ evidence_id: String, direction: "supports" | "contradicts" }`.

#### `sxr evidence for-claim --run <id> --question <id> --claim <id> --direction <dir> [--json]`
- **Purpose**: Creates evidence targeting a relation claim.

#### `sxr evidence for-hypothesis --run <id> --question <id> --hypothesis <id> --direction <dir> [--json]`
- **Purpose**: Creates evidence targeting a diagnostic hypothesis (`supports` = discriminating, `contradicts` = exclusion).

#### `sxr claim create --run <id> --question <id> --src <src> --dst <dst> [--kind <k>] [--json]`
- **Purpose**: Asserts a relation claim between two declarations.

---

### 8. `settlement` (Settlement Registration)

#### `sxr settlement register --run <id> --question <id> --threshold <id> --transformation <ref> [--debt <id>] [--json]`
- **Purpose**: Evaluates threshold over committed evidence and registers immutable settlement.
- **Output JSON**: `{ settlement_id, settlement_status, confidence }`.
- **Exit Codes**: `0` on success, `3` on AC-SETTLEMENT-002 violation (positive settlement on invalid model rejected).

---

### 9. `diagnostic` & `residual` (Diagnostic Attribution Loop)

#### `sxr residual compute --repo <id> --question <id> --snapshot <id> [--json]`
- **Purpose**: Computes disagreement residuals (`declared_contradicted`) between $G_{declared}$ and $G_{observed}$.

#### `sxr diagnostic enumerate --repo <id> --question <id> --snapshot <id> --residual <id> [--json]`
- **Purpose**: Creates diagnostic question, threshold, and 9 candidate failure locus hypotheses.

#### `sxr diagnostic settle --repo <id> --question <id> --hypothesis <id> [--json]`
- **Purpose**: Promotes a candidate locus to `diagnostic.settled` (enforces 4 §12.5a conditions).

#### `sxr diagnostic propose --repo <id> --question <id> --hypothesis <id> --locus <locus> --target <t> [--json]`
- **Purpose**: Raises an actionable repair proposal bound to diagnostic settlement (enforces AC-DIAG-001).

#### `sxr diagnostic status --repo <id> --residual <id> [--json]`
- **Purpose**: Reports the full §17.2 state machine of the diagnostic loop for a residual.

---

### 10. `ledger` & `projections` (Storage & Query)

#### `sxr ledger verify --repo <id> [--json]`
- **Purpose**: Cryptographically verifies the gapless SHA-256 hash chain from ordinal 1 to head.
- **Exit Codes**: `0` if verified, `5` if hash chain diverges.

#### `sxr projections rebuild --repo <id> --all [--json]`
- **Purpose**: Drops and reconstructs all projection tables ($G_{declared}$, $G_{observed}$, FTS5) from the ledger.

#### `sxr projections export --repo <id> --canonical-json`
- **Purpose**: Emits canonical JSON of all projection tables to stdout.

#### `sxr projections export-anti-collapse-corpus --repo <id> --question <id> --snapshot <id>`
- **Purpose**: Exports records as an external anti-collapse evaluation corpus.

---

### 11. `daemon`, `queue` & `spool` (Asynchronous Durability)

#### `sxr daemon [--socket <path>] [--drain-delay-ms <ms>]`
- **Purpose**: Runs the background UNIX domain socket service.

#### `sxr queue recover [--repo <id>] [--json]`
- **Purpose**: Drains uncommitted segments from the daemon queue into the ledger after a crash.

#### `sxr spool drain --repo <id> [--json]`
- **Purpose**: Drains unvalidated requests from the opt-in spool into the ledger.

#### `sxr await [--run <id>] [--repo <id>] [--all] [--timeout-secs <s>] [--json]`
- **Purpose**: Blocks until all pending queue and spool records are committed.

#### `sxr doctor [--json]`
- **Purpose**: Runs diagnostic self-checks on filesystem capabilities, socket status, and queue integrity.

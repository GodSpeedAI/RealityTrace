# Data Storage Architecture

SXR's persistence model combines an **append-only, hash-chained SQLite ledger**, a **transactional outbox**, an external **Content-Addressable Storage (CAS)** system for oversized representations, and **derived relational projection tables**.

---

## 1. Storage Layout Overview

For any enrolled repository with ID `<repo_id>`, its files are stored strictly under XDG paths:

```text
$XDG_DATA_HOME/sxr/repos/<repo_id>/
├── registry.json             # Enrollment identity (repo_id, checkout_id, created_at, identity_evidence)
├── semantic.json             # Semantic namespace configuration and entry path
├── writer.lock               # Advisory file lock (flock)
├── ledger.db                 # Primary SQLite database (STRICT mode, WAL enabled)
└── cas/                      # Content-Addressable Storage directory
    └── sha256/
        └── <hash[0..2]>/
            └── <hash[2..]>   # Raw payload bytes stored by content digest
```

---

## 2. The Frozen Ledger Table (`ledger`)

The `ledger` table is frozen at schema version 1. It is declared with SQLite `STRICT` mode:

```sql
CREATE TABLE ledger (
  ledger_id           TEXT NOT NULL,           -- Immutable generation identity
  append_ordinal      INTEGER PRIMARY KEY,     -- Dense, gapless, 1-based integer
  entry_ulid          TEXT NOT NULL UNIQUE,    -- ULID: time-ordered, unique
  record_kind         TEXT NOT NULL,           -- 1 of 26 registered kinds (e.g. 'run.started')
  record_version      TEXT NOT NULL,           -- 'sxr-record/v1'
  idempotency_key     TEXT UNIQUE,             -- Deduplication key; NULL allowed
  subject_refs        TEXT NOT NULL,           -- JSON array of subject URIs
  canonicalization    TEXT NOT NULL,           -- 'sxr-canonical-json/v1'
  hash_algorithm      TEXT NOT NULL,           -- 'sha256'
  payload             TEXT NOT NULL,           -- Canonical JSON (the CEP envelope)
  payload_hash        TEXT NOT NULL,           -- sha256:... over canonical payload text
  previous_entry_hash TEXT,                    -- NULL only at ordinal 1
  entry_hash          TEXT NOT NULL,           -- SHA-256 over canonical entry header
  committed_at        TEXT NOT NULL,           -- RFC3339 UTC timestamp (metadata only)
  writer_identity_ref TEXT NOT NULL            -- 'sxr@<host>/<pid>' or daemon identity
) STRICT;

CREATE UNIQUE INDEX ledger_generation_ordinal ON ledger(ledger_id, append_ordinal);
CREATE UNIQUE INDEX ledger_entry_hash ON ledger(entry_hash);
```

### Hash Chain Mathematics
Every row is cryptographically bound to its predecessor. The `entry_hash` is computed over the canonical JSON representation of the row header (excluding the payload, which is bound via `payload_hash`):

$$\text{entry\_hash}_n = \text{SHA256}(\text{canonical\_json}(\{ \text{ledger\_id}, \text{append\_ordinal}, \dots, \text{payload\_hash}, \text{previous\_entry\_hash} \}))$$

**Invariants (AC-LEDGER-001)**:
- `append_ordinal` must be gapless, strictly ascending, starting at 1.
- `previous_entry_hash` must be `NULL` at ordinal 1.
- For $n > 1$, `previous_entry_hash` must exactly equal $\text{entry\_hash}_{n-1}$.
- Running `sxr ledger verify --repo <id>` verifies the entire chain from ordinal 1 to the head.

---

## 3. Transactional Outbox & Projections

To support fast graph queries and text search without corrupting ledger append performance, SXR implements the **Transactional Outbox Pattern**:

```sql
CREATE TABLE outbox (
  ledger_id        TEXT NOT NULL,
  append_ordinal   INTEGER NOT NULL,
  projections      TEXT NOT NULL,   -- JSON array of pending projectors
  PRIMARY KEY (ledger_id, append_ordinal)
) STRICT;

CREATE TABLE projection_watermark (
  name                 TEXT PRIMARY KEY,
  last_ledger_id       TEXT NOT NULL,
  last_append_ordinal  INTEGER NOT NULL,
  projection_version   TEXT NOT NULL,
  updated_at           TEXT NOT NULL
) STRICT;
```

### Read Models (Projections)
Projections live in the same SQLite database (`ledger.db`) as separate tables:
- `sxr_snapshot`: Semantic snapshot metadata and validation status.
- `sxr_run`: Verification run contexts and execution lifecycles.
- `sxr_question`: Declared questions and scope definitions.
- `sxr_threshold`: Declared settlement threshold specifications.
- `sxr_evidence` & `sxr_evidence_target`: Project evidence records and declaration bindings.
- `sxr_settlement`: Settlement records, confidence, and recorded debt.
- `sxr_decl_node` & `sxr_decl_edge`: $G_{declared}$ graph of concepts, entities, and declared relations.
- `sxr_obs_node` & `sxr_obs_edge`: $G_{observed}$ graph of evidenced relationships.
- `sxr_fts_content` & `sxr_fts`: Contentless FTS5 virtual table for full-text search.
- `sxr_residual`, `sxr_hypothesis`, `sxr_diag_settlement`, `sxr_proposal`: State of the diagnostic attribution loop.

### Rebuild Determinism (AC-REPLAY-002)
Because all projection tables are derived entirely from the immutable ledger, dropping the projection tables and rebuilding them via:
```bash
sxr projections rebuild --repo <id> --all
```
produces a **byte-identical** canonical JSON export:
```bash
diff -q <(sxr projections export --repo <id> --canonical-json) <(sxr projections export --repo <id> --canonical-json)
```

---

## 4. Content-Addressable Storage (CAS)

When semantic models or verifier artifacts exceed inline carriage limits, SXR stores the raw representation bytes in the filesystem CAS store:
- **Location**: `<repo_dir>/cas/sha256/<prefix>/<remainder>`
- **URI Format**: `cas://sha256:<64_hex_digits>`
- **Carriage Invariant**: The ledger envelope carries the `content_ref` URI. When projections are materialized, the projection engine reads from the CAS store to extract nodes and edges. Downstream projection consumers do not need to know whether the representation was stored inline or in CAS.

---

## 5. Source Trail

- [`sxr-ledger/src/schema.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/src/schema.rs) — DDL definitions for `ledger`, `outbox`, and `projection_watermark`.
- [`sxr-core/src/chain.rs`](file:///home/sprime01/projects/sxr/sxr-core/src/chain.rs) — Pure hash chain computation and chain verification functions.
- [`sxr-ledger/src/ledger.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/src/ledger.rs) — Append operations, transaction boundaries, and SQLite connection configuration.
- [`sxr-ledger/src/cas.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/src/cas.rs) — File-backed CAS implementation.
- [`sxr-cli/src/projections.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/projections.rs) — DDL for all projection tables and projection row application logic.

# Technical Reference: Schemas & Database DDL

This document provides the exact SQL Data Definition Language (DDL) for all SQLite tables in `ledger.db` and references the JSON schemas governing SXR.

---

## 1. Primary Ledger & Outbox DDL (Frozen Schema v1)

Source: [`sxr-ledger/src/schema.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/src/schema.rs)

```sql
-- Frozen ledger table: STRICT, append-only
CREATE TABLE ledger (
  ledger_id           TEXT NOT NULL,
  append_ordinal      INTEGER PRIMARY KEY,
  entry_ulid          TEXT NOT NULL UNIQUE,
  record_kind         TEXT NOT NULL,
  record_version      TEXT NOT NULL,
  idempotency_key     TEXT UNIQUE,
  subject_refs        TEXT NOT NULL,
  canonicalization    TEXT NOT NULL,
  hash_algorithm      TEXT NOT NULL,
  payload             TEXT NOT NULL,
  payload_hash        TEXT NOT NULL,
  previous_entry_hash TEXT,
  entry_hash          TEXT NOT NULL,
  committed_at        TEXT NOT NULL,
  writer_identity_ref TEXT NOT NULL
) STRICT;

CREATE UNIQUE INDEX ledger_generation_ordinal ON ledger(ledger_id, append_ordinal);
CREATE UNIQUE INDEX ledger_entry_hash ON ledger(entry_hash);

-- Transactional Outbox table
CREATE TABLE outbox (
  ledger_id        TEXT NOT NULL,
  append_ordinal   INTEGER NOT NULL,
  projections      TEXT NOT NULL,
  PRIMARY KEY (ledger_id, append_ordinal)
) STRICT;

-- Projection watermark tracking
CREATE TABLE projection_watermark (
  name                 TEXT PRIMARY KEY,
  last_ledger_id       TEXT NOT NULL,
  last_append_ordinal  INTEGER NOT NULL,
  projection_version   TEXT NOT NULL,
  updated_at           TEXT NOT NULL
) STRICT;
```

---

## 2. Derived Relational & Graph Projections DDL

Source: [`sxr-cli/src/projections.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/projections.rs)

```sql
-- Semantic Snapshots
CREATE TABLE IF NOT EXISTS sxr_snapshot (
    snapshot_id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    semantic_closure_hash TEXT,
    content_hash TEXT,
    invalid_declared_checkpoint_hash TEXT,
    model_validation_status TEXT NOT NULL,
    representation_omission_json TEXT,
    source_set_hash TEXT NOT NULL,
    entry_logical_path TEXT NOT NULL,
    registry_content_hash TEXT,
    resolution_inputs_json TEXT NOT NULL,
    drift_class TEXT,
    first_seen_ledger_id TEXT NOT NULL,
    first_seen_append_ordinal INTEGER NOT NULL
) STRICT;

-- Verification Runs
CREATE TABLE IF NOT EXISTS sxr_run (
    run_id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    checkout_id TEXT NOT NULL,
    git_commit TEXT,
    dirty INTEGER NOT NULL,
    snapshot_id TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    status TEXT NOT NULL,
    plan_id TEXT
) STRICT;

-- Questions & Settlement Thresholds
CREATE TABLE IF NOT EXISTS sxr_question (
    question_id TEXT PRIMARY KEY,
    question_form TEXT NOT NULL,
    question_type TEXT NOT NULL,
    scope_json TEXT NOT NULL,
    ledger_id TEXT NOT NULL,
    append_ordinal INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS sxr_threshold (
    threshold_id TEXT PRIMARY KEY,
    question_id TEXT NOT NULL,
    ledger_id TEXT NOT NULL,
    append_ordinal INTEGER NOT NULL,
    spec_json TEXT NOT NULL
) STRICT;

-- Evidence & Settlements
CREATE TABLE IF NOT EXISTS sxr_evidence (
    evidence_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    question_id TEXT NOT NULL,
    evidence_type TEXT NOT NULL,
    target_kind TEXT NOT NULL,
    target_ref TEXT NOT NULL,
    relevance TEXT NOT NULL,
    reliability TEXT NOT NULL,
    direction TEXT NOT NULL,
    confidence_impact TEXT NOT NULL,
    representation_ref TEXT NOT NULL,
    ledger_id TEXT NOT NULL,
    append_ordinal INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS sxr_evidence_target (
    evidence_id TEXT NOT NULL,
    decl_key TEXT NOT NULL,
    PRIMARY KEY (evidence_id, decl_key)
) STRICT;

CREATE TABLE IF NOT EXISTS sxr_settlement (
    settlement_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    question_id TEXT NOT NULL,
    transformation_ref TEXT NOT NULL,
    threshold_id TEXT NOT NULL,
    settlement_status TEXT NOT NULL,
    confidence TEXT NOT NULL,
    debt_id TEXT,
    ledger_id TEXT NOT NULL,
    append_ordinal INTEGER NOT NULL
) STRICT;

-- Declared Graph G_declared (Slice 3)
CREATE TABLE IF NOT EXISTS sxr_decl_node (
    snapshot_id TEXT NOT NULL,
    decl_key TEXT NOT NULL,
    decl_kind TEXT NOT NULL,
    name TEXT NOT NULL,
    namespace TEXT NOT NULL,
    logical_module_id TEXT NOT NULL,
    PRIMARY KEY (snapshot_id, decl_key)
) STRICT;

CREATE TABLE IF NOT EXISTS sxr_decl_edge (
    snapshot_id TEXT NOT NULL,
    src_decl_key TEXT NOT NULL,
    dst_decl_key TEXT NOT NULL,
    edge_kind TEXT NOT NULL,
    field_path TEXT NOT NULL,
    PRIMARY KEY (snapshot_id, src_decl_key, dst_decl_key, edge_kind, field_path)
) STRICT;

-- Contentless FTS5 Full-Text Search (Slice 3)
CREATE TABLE IF NOT EXISTS sxr_fts_content (
    rowid INTEGER PRIMARY KEY,
    subject_key TEXT NOT NULL,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL
) STRICT;

CREATE VIRTUAL TABLE IF NOT EXISTS sxr_fts USING fts5(
    subject_key, kind, title, body,
    content = 'sxr_fts_content',
    content_rowid = 'rowid',
    tokenize = 'porter unicode61 remove_diacritics 2'
);

-- Observed Graph G_observed (Slice 4)
CREATE TABLE IF NOT EXISTS sxr_claim (
    claim_id TEXT PRIMARY KEY,
    question_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    src_decl_key TEXT NOT NULL,
    dst_decl_key TEXT NOT NULL,
    edge_kind TEXT NOT NULL,
    ledger_id TEXT NOT NULL,
    append_ordinal INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS sxr_obs_node (
    decl_key TEXT PRIMARY KEY,
    first_evidenced_ordinal INTEGER NOT NULL,
    last_evidenced_ordinal INTEGER NOT NULL,
    evidence_count INTEGER NOT NULL,
    settled_count INTEGER NOT NULL,
    unsettled_count INTEGER NOT NULL,
    debt_count INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS sxr_obs_edge (
    src_decl_key TEXT NOT NULL,
    dst_decl_key TEXT NOT NULL,
    edge_kind TEXT NOT NULL,
    claim_id TEXT NOT NULL,
    confidence TEXT NOT NULL,
    contradiction_count INTEGER NOT NULL,
    settlement_status TEXT NOT NULL,
    evidence_refs_json TEXT NOT NULL,
    settlement_refs_json TEXT NOT NULL,
    PRIMARY KEY (src_decl_key, dst_decl_key, edge_kind)
) STRICT;

-- Diagnostic Loop (Slice 5)
CREATE TABLE IF NOT EXISTS sxr_residual (
    residual_id TEXT PRIMARY KEY,
    question_id TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    decl_key TEXT NOT NULL,
    residual_kind TEXT NOT NULL,
    severity TEXT NOT NULL,
    attribution_status TEXT NOT NULL,
    supported_failure_locus TEXT,
    diagnostic_settlement_ref TEXT,
    ledger_id TEXT NOT NULL,
    append_ordinal INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS sxr_hypothesis (
    hypothesis_id TEXT PRIMARY KEY,
    question_id TEXT NOT NULL,
    residual_id TEXT NOT NULL,
    failure_locus_candidate TEXT NOT NULL,
    confidence TEXT NOT NULL,
    supporting_evidence_refs_json TEXT NOT NULL,
    contradicting_evidence_refs_json TEXT NOT NULL,
    alternatives_json TEXT NOT NULL,
    falsification_path TEXT NOT NULL,
    next_action_kind TEXT NOT NULL,
    next_action_ref TEXT NOT NULL,
    expected_discrimination TEXT NOT NULL,
    ledger_id TEXT NOT NULL,
    append_ordinal INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS sxr_diag_settlement (
    settlement_id TEXT PRIMARY KEY,
    question_id TEXT NOT NULL,
    hypothesis_id TEXT NOT NULL,
    residual_id TEXT NOT NULL,
    supported_locus TEXT NOT NULL,
    threshold_id TEXT NOT NULL,
    qualifying_evidence_refs_json TEXT NOT NULL,
    contradicting_evidence_refs_json TEXT NOT NULL,
    excluded_alternatives_json TEXT NOT NULL,
    ledger_id TEXT NOT NULL,
    append_ordinal INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS sxr_proposal (
    proposal_id TEXT PRIMARY KEY,
    question_id TEXT NOT NULL,
    hypothesis_id TEXT NOT NULL,
    locus TEXT NOT NULL,
    target_ref TEXT NOT NULL,
    transformation_ref TEXT NOT NULL,
    diagnostic_settlement_ref TEXT NOT NULL,
    ledger_id TEXT NOT NULL,
    append_ordinal INTEGER NOT NULL
) STRICT;
```

---

## 3. JSON Schemas Directory

| Schema File | Path | Description |
| :--- | :--- | :--- |
| **CEP Semantic Envelope** | [`schemas/cep-semantic-envelope.schema.json`](file:///home/sprime01/projects/sxr/schemas/cep-semantic-envelope.schema.json) | Complete JSON Schema enforcing all CEP-0008 required envelope fields, entity sections, and boundary records. |
| **CLI Output Format** | [`schemas/sxr-output-v1.json`](file:///home/sprime01/projects/sxr/schemas/sxr-output-v1.json) | Schema defining the machine-readable response format emitted on stdout. |
| **Plan Format** | [`schemas/sxr-plan-v1.json`](file:///home/sprime01/projects/sxr/schemas/sxr-plan-v1.json) | Schema defining `plan.created` configuration objects. |
| **Record Submission** | [`schemas/sxr-record-v1.json`](file:///home/sprime01/projects/sxr/schemas/sxr-record-v1.json) | Wire schema for daemon record requests over the Unix socket. |

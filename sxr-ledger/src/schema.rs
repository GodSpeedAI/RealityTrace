//! Frozen v1 ledger schema (§10.3, §10.5, §11.2) and the `user_version`
//! guard. The `ledger` table shape is frozen at v1; only derived tables may
//! evolve (`projection_version`).

/// The one and only schema version this build understands.
pub const SCHEMA_VERSION: i64 = 1;

/// §10.3 verbatim `ledger` DDL (frozen at v1). STRICT, append-only.
///
/// `mmr_leaf_index` and `authority_refs` from the sea-forge model are
/// omitted, not stubbed (§10.3).
pub const LEDGER_DDL: &str = r#"
CREATE TABLE ledger (
  ledger_id           TEXT NOT NULL,           -- immutable generation identity
  append_ordinal      INTEGER PRIMARY KEY,     -- dense, gapless, 1-based
  entry_ulid          TEXT NOT NULL UNIQUE,    -- ULID: time-ordered, unique
  record_kind         TEXT NOT NULL,           -- §10.4
  record_version      TEXT NOT NULL,           -- 'sxr-record/v1'
  idempotency_key     TEXT UNIQUE,             -- dedup; NULL allowed
  subject_refs        TEXT NOT NULL,           -- JSON array of subject URIs
  canonicalization    TEXT NOT NULL,           -- 'sxr-canonical-json/v1'
  hash_algorithm      TEXT NOT NULL,           -- 'sha256'
  payload             TEXT NOT NULL,           -- canonical JSON (the CEP envelope)
  payload_hash        TEXT NOT NULL,           -- sha256:… over canonical payload
  previous_entry_hash TEXT,                    -- NULL only at ordinal 1
  entry_hash          TEXT NOT NULL,           -- §10.6
  committed_at        TEXT NOT NULL,           -- RFC3339 UTC, metadata only
  writer_identity_ref TEXT NOT NULL            -- 'sxr@<host>/<pid>' or daemon id
) STRICT;
CREATE UNIQUE INDEX ledger_generation_ordinal ON ledger(ledger_id, append_ordinal);
CREATE UNIQUE INDEX ledger_entry_hash ON ledger(entry_hash);
"#;

/// §11.2 transactional outbox: projectors drain and advance their own
/// watermark. The pair (last_ledger_id, last_append_ordinal) is one
/// [`sxr_core::LedgerPosition`]; a bare ordinal is forbidden.
pub const OUTBOX_DDL: &str = r#"
CREATE TABLE outbox (
  ledger_id        TEXT NOT NULL,
  append_ordinal   INTEGER NOT NULL,
  projections      TEXT NOT NULL,   -- JSON array of projector names
  PRIMARY KEY (ledger_id, append_ordinal)
) STRICT;
CREATE TABLE projection_watermark (
  name                 TEXT PRIMARY KEY,
  last_ledger_id       TEXT NOT NULL,
  last_append_ordinal  INTEGER NOT NULL,
  projection_version   TEXT NOT NULL,
  updated_at           TEXT NOT NULL
) STRICT;
"#;

/// All DDL, applied exactly when `user_version` is 0.
pub fn full_ddl() -> String {
    format!("{LEDGER_DDL}\n{OUTBOX_DDL}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_version_is_1() {
        assert_eq!(SCHEMA_VERSION, 1);
    }

    #[test]
    fn ddl_contains_the_frozen_shape() {
        assert!(LEDGER_DDL.contains("append_ordinal"));
        assert!(LEDGER_DDL.contains("INTEGER PRIMARY KEY"));
        assert!(LEDGER_DDL.contains("idempotency_key     TEXT UNIQUE"));
        assert!(LEDGER_DDL.contains("previous_entry_hash TEXT"));
        assert!(LEDGER_DDL.contains(") STRICT"));
        assert!(!LEDGER_DDL.contains("mmr_leaf_index"));
        assert!(!LEDGER_DDL.contains("authority_refs"));
    }
}

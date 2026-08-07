use crate::canonical_json::{canonical_json, sha256_hex};
use crate::types::LedgerRow;
use serde_json::json;

/// `payload_hash = sha256:…` over the canonical payload bytes (§10.3).
/// The payload is the canonical JSON envelope text; the bytes are hashed
/// as-is, so streaming verification never re-parses the payload.
pub fn payload_hash(payload: &str) -> String {
    sha256_hex(payload.as_bytes())
}

/// §10.6 hash chain:
///
/// ```text
/// entry_hash(n) = sha256( canonical_json({
///     ledger_id, append_ordinal, entry_ulid, record_kind, record_version,
///     idempotency_key, subject_refs, canonicalization, hash_algorithm,
///     payload_hash, previous_entry_hash, committed_at, writer_identity_ref
/// }) )
/// ```
///
/// `payload` is excluded and bound via `payload_hash`; `previous_entry_hash`
/// is null only at ordinal 1 of each generation. The canonical_json ordering
/// (sorted keys, compact) makes the formula byte-exact across implementations.
pub fn entry_hash(row: &LedgerRow) -> String {
    let hashed = json!({
        "ledger_id": row.ledger_id,
        "append_ordinal": row.append_ordinal,
        "entry_ulid": row.entry_ulid,
        "record_kind": row.record_kind,
        "record_version": row.record_version,
        "idempotency_key": row.idempotency_key,
        "subject_refs": row.subject_refs,
        "canonicalization": row.canonicalization,
        "hash_algorithm": row.hash_algorithm,
        "payload_hash": row.payload_hash,
        "previous_entry_hash": row.previous_entry_hash,
        "committed_at": row.committed_at,
        "writer_identity_ref": row.writer_identity_ref,
    });
    sha256_hex(canonical_json(&hashed).as_bytes())
}

/// Verify one entry's own hashes (payload_hash over the payload, entry_hash
/// over the hashed struct). Does not check the chain; see `chain_verify`.
pub fn verify_entry_hashes(row: &LedgerRow) -> Result<(), String> {
    let want_payload = payload_hash(&row.payload);
    if want_payload != row.payload_hash {
        return Err(format!(
            "payload_hash mismatch at ({}, {}): stored {}, recomputed {}",
            row.ledger_id, row.append_ordinal, row.payload_hash, want_payload
        ));
    }
    let want_entry = entry_hash(row);
    if want_entry != row.entry_hash {
        return Err(format!(
            "entry_hash mismatch at ({}, {}): stored {}, recomputed {}",
            row.ledger_id, row.append_ordinal, row.entry_hash, want_entry
        ));
    }
    Ok(())
}

/// Verify a chain of rows in append order: hash bindings and the
/// `previous_entry_hash` linkage. Returns the first divergent ordinal on
/// failure; the error names ledger_id and append_ordinal (AC-LEDGER-001).
pub fn chain_verify(rows: &[&LedgerRow]) -> Result<(), String> {
    let mut prev: Option<&str> = None;
    for (i, row) in rows.iter().enumerate() {
        let expected_ordinal = (i as u64) + 1;
        if row.append_ordinal != expected_ordinal {
            return Err(format!(
                "append_ordinal not dense at ({}, {}): expected {}",
                row.ledger_id, row.append_ordinal, expected_ordinal
            ));
        }
        verify_entry_hashes(row)?;
        match (prev, row.previous_entry_hash.as_deref()) {
            (Some(_), None) => {
                return Err(format!(
                    "entry ({}, {}) has a predecessor but previous_entry_hash is NULL",
                    row.ledger_id, row.append_ordinal
                ));
            }
            (Some(p), Some(q)) if p != q => {
                return Err(format!(
                    "chain break at ({}, {}): previous_entry_hash {} != prior entry_hash {}",
                    row.ledger_id, row.append_ordinal, q, p
                ));
            }
            (None, Some(_)) => {
                return Err(format!(
                    "entry ({}, {}) at ordinal 1 has a previous_entry_hash; only NULL is allowed",
                    row.ledger_id, row.append_ordinal
                ));
            }
            _ => {}
        }
        prev = Some(&row.entry_hash);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{SXR_CANONICALIZATION, SXR_HASH_ALGORITHM, SXR_RECORD_VERSION};

    fn row(ordinal: u64, prev: Option<String>) -> LedgerRow {
        LedgerRow {
            ledger_id: "01JTEST".into(),
            append_ordinal: ordinal,
            entry_ulid: format!("01JTESTULID{ordinal:0>12}"),
            record_kind: "ledger.generation_started".into(),
            record_version: SXR_RECORD_VERSION.into(),
            idempotency_key: None,
            subject_refs: vec!["ledger:01JTEST".into()],
            canonicalization: SXR_CANONICALIZATION.into(),
            hash_algorithm: SXR_HASH_ALGORITHM.into(),
            payload: r#"{"k":"v"}"#.into(),
            payload_hash: payload_hash(r#"{"k":"v"}"#),
            previous_entry_hash: prev,
            entry_hash: String::new(),
            committed_at: "2026-08-07T00:00:00.000000Z".into(),
            writer_identity_ref: "sxr@test/1".into(),
        }
    }

    #[test]
    fn payload_hash_is_sha256_prefixed_hex() {
        let h = payload_hash("{}");
        assert!(h.starts_with("sha256:"));
        assert_eq!(h.len(), "sha256:".len() + 64);
        assert!(h[7..].chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(
            h,
            "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
        );
    }

    #[test]
    fn entry_hash_excludes_payload_and_is_stable() {
        let r1 = row(1, None);
        let mut r1b = r1.clone();
        r1b.payload = r#"{"k":"different"}"#.into();
        r1b.payload_hash = payload_hash(&r1b.payload);
        // entry_hash is a pure function of the hashed struct: changing the
        // payload (and its binding) must change entry_hash.
        assert_ne!(entry_hash(&r1), entry_hash(&r1b));
        // But two rows identical except payload_hash being stale (not
        // recomputed) hash identically — the binding discipline is enforced
        // by verify_entry_hashes, not by entry_hash.
        let mut r1c = r1.clone();
        r1c.payload = r#"{"k":"different"}"#.into();
        assert_eq!(entry_hash(&r1), entry_hash(&r1c));
        assert!(verify_entry_hashes(&r1c).is_err());
    }

    #[test]
    fn verify_accepts_a_clean_chain() {
        let mut a = row(1, None);
        a.entry_hash = entry_hash(&a);
        let mut b = row(2, Some(a.entry_hash.clone()));
        b.entry_hash = entry_hash(&b);
        let rows = [&a, &b];
        assert!(chain_verify(&rows).is_ok());
    }

    #[test]
    fn verify_names_ledger_id_and_ordinal_on_divergence() {
        let mut a = row(1, None);
        a.entry_hash = entry_hash(&a);
        // Corrupt a payload byte and keep the stored entry_hash (the
        // tamper-evident scenario of AC-LEDGER-001).
        a.payload = r#"{"k":"v"}"#.replace('v', "w");
        let err = chain_verify(&[&a]).unwrap_err();
        assert!(err.contains("01JTEST"), "{err}");
        assert!(err.contains("1"), "{err}");
    }

    #[test]
    fn verify_rejects_broken_linkage() {
        let mut a = row(1, None);
        a.entry_hash = entry_hash(&a);
        let mut b = row(2, Some("sha256:beef".into()));
        b.entry_hash = entry_hash(&b);
        let err = chain_verify(&[&a, &b]).unwrap_err();
        assert!(err.contains("chain break"), "{err}");
        assert!(err.contains("2"), "{err}");
    }
}

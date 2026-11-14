//! Tail verification (WP-17, closes the WP-16 gap).
//!
//! A hash chain proves that no row was changed or removed from the middle. It
//! cannot prove that rows were not removed from the END: a chain that ends one
//! row earlier is itself a valid chain (CEP-0008 section 55). The only
//! defence is a head recorded elsewhere. `ExpectedHead` carries one (the entry
//! hash and/or the record count of the ledger when it was anchored). Without
//! it the result says `unverified`; it is never reported as verified.

use crate::ObservedError;
use serde_json::{Value, json};
use sxr_core::LedgerRow;

/// A head recorded outside the ledger.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExpectedHead {
    /// `entry_hash` of the last row at anchor time.
    pub entry_hash: Option<String>,
    /// Number of rows at anchor time.
    pub record_count: Option<u64>,
}

impl ExpectedHead {
    pub fn is_empty(&self) -> bool {
        self.entry_hash.is_none() && self.record_count.is_none()
    }
}

/// Check the whole ledger (not the derive window) against `expected`.
/// Later appends are legitimate: the anchored row must still be at its
/// position; the ledger may extend past it. Returns the `tail_verification`
/// document, or `TAIL_TRUNCATED`.
pub fn check(rows: &[LedgerRow], expected: Option<&ExpectedHead>) -> Result<Value, ObservedError> {
    let Some(exp) = expected.filter(|e| !e.is_empty()) else {
        return Ok(json!({
            "status": "unverified",
            "reason": "no expected head supplied: deletion of the last ledger records is not detectable by the hash chain alone",
        }));
    };
    let truncated = |m: String| Err(ObservedError::new("TAIL_TRUNCATED", m));
    if let Some(n) = exp.record_count {
        if n == 0 || (rows.len() as u64) < n {
            return truncated(format!(
                "expected at least {n} ledger records, found {}",
                rows.len()
            ));
        }
        if let Some(h) = &exp.entry_hash {
            let at = &rows[(n - 1) as usize].entry_hash;
            if at != h {
                return truncated(format!(
                    "record {n} has entry hash {at}, expected {h}: the ledger differs from the anchored one"
                ));
            }
        }
    } else if let Some(h) = &exp.entry_hash
        && !rows.iter().any(|r| &r.entry_hash == h)
    {
        return truncated(format!("expected head entry {h} is not in the ledger"));
    }
    Ok(json!({
        "status": "verified_against_expected_head",
        "expected_entry_hash": exp.entry_hash,
        "expected_record_count": exp.record_count,
    }))
}

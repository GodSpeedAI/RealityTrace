//! RFC 8785 canonical JSON over the I-JSON domain (CEP-0008 section 16.5).
//!
//! The repository already has one canonicaliser, `sxr_core::canonical_json`
//! (sorted keys, compact, serde_json number writer). It equals RFC 8785 on
//! the domain checked here: integers within +/-(2^53 - 1), no floats, and
//! object keys in the Basic Multilingual Plane (so UTF-8 byte order equals
//! UTF-16 code-unit order). Anything outside that domain is refused, never
//! repaired. No second JCS implementation exists here.

use crate::ObservedError;
use serde_json::Value;
use sxr_core::canonical_json::{canonical_json, sha256_hex};

const MAX_SAFE: i64 = (1_i64 << 53) - 1;

fn check(value: &Value, path: &str) -> Result<(), String> {
    match value {
        Value::Number(n) => {
            let ok = n
                .as_i64()
                .is_some_and(|i| (-MAX_SAFE..=MAX_SAFE).contains(&i))
                || n.as_u64().is_some_and(|u| u <= MAX_SAFE as u64);
            if ok {
                Ok(())
            } else {
                Err(format!(
                    "{path}: number {n} is outside the I-JSON integer domain"
                ))
            }
        }
        Value::Array(items) => items
            .iter()
            .enumerate()
            .try_for_each(|(i, v)| check(v, &format!("{path}/{i}"))),
        Value::Object(map) => map.iter().try_for_each(|(k, v)| {
            if k.chars().any(|c| c as u32 > 0xFFFF) {
                return Err(format!("{path}: key {k:?} has a non-BMP character"));
            }
            check(v, &format!("{path}/{k}"))
        }),
        _ => Ok(()),
    }
}

/// Canonical JSON text, or `CANONICALIZATION_REFUSED` outside the domain.
pub fn canonical(value: &Value) -> Result<String, ObservedError> {
    check(value, "").map_err(|m| ObservedError::new("CANONICALIZATION_REFUSED", m))?;
    Ok(canonical_json(value))
}

/// `sha256:<hex>` over the canonical bytes.
pub fn digest(value: &Value) -> Result<String, ObservedError> {
    Ok(sha256_hex(canonical(value)?.as_bytes()))
}

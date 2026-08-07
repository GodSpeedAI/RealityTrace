use serde_json::{Map, Value};

/// Canonical JSON serialization, the `sxr-canonical-json/v1` scheme.
///
/// Rules: objects serialize with keys in byte-ascending order, recursively;
/// no insignificant whitespace; no key duplication (serde_json maps forbid it);
/// arrays preserve order. String/number/bool/null serialization follows
/// serde_json's stable writer. Two documents with the same semantic content
/// produce identical bytes, which makes `sha256(canonical_json(v))` a valid
/// content fingerprint.
pub fn canonical_json(value: &Value) -> String {
    let sorted = sort_value(value);
    serde_json::to_string(&sorted).expect("canonical JSON serialization cannot fail")
}

/// Canonical JSON of a JSON-serializable Rust value.
pub fn canonical_json_of<T: serde::Serialize>(value: &T) -> Result<String, serde_json::Error> {
    Ok(canonical_json(&serde_json::to_value(value)?))
}

fn sort_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::with_capacity(map.len());
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys {
                out.insert(key.clone(), sort_value(&map[key]));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(sort_value).collect()),
        other => other.clone(),
    }
}

/// `sha256:<lowercase hex>` over canonical JSON of `value`.
pub fn sha256_canonical(value: &Value) -> String {
    sha256_hex(canonical_json(value).as_bytes())
}

/// `sha256:<lowercase hex>` over raw bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(bytes);
    format!("sha256:{}", hex::encode(digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn objects_sort_keys_recursively() {
        let a = json!({"b": 1, "a": {"d": 4, "c": 3}, "z": [1, {"y": 2, "x": 1}]});
        let b = json!({"z": [1, {"x": 1, "y": 2}], "a": {"c": 3, "d": 4}, "b": 1});
        assert_eq!(canonical_json(&a), canonical_json(&b));
        assert_eq!(
            canonical_json(&a),
            r#"{"a":{"c":3,"d":4},"b":1,"z":[1,{"x":1,"y":2}]}"#
        );
    }

    #[test]
    fn array_order_is_preserved() {
        let a = json!(["x", "y"]);
        let b = json!(["y", "x"]);
        assert_ne!(canonical_json(&a), canonical_json(&b));
    }

    #[test]
    fn no_whitespace() {
        assert_eq!(canonical_json(&json!({"a": 1})), r#"{"a":1}"#);
        assert_eq!(canonical_json(&json!(null)), "null");
        assert_eq!(canonical_json(&json!("s")), r#""s""#);
    }

    #[test]
    fn sha256_is_stable_and_prefixed() {
        let h1 = sha256_canonical(&json!({"a": 1}));
        let h2 = sha256_canonical(&json!({"a": 1}));
        assert_eq!(h1, h2);
        assert!(h1.starts_with("sha256:"));
        assert_eq!(h1.len(), 7 + 64);
        let reordered = sha256_canonical(&json!({"a": 1}));
        assert_eq!(h1, reordered);
    }

    #[test]
    fn sha256_hex_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}

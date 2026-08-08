//! JSON output envelope (§15): every `--json` command emits a stable,
//! versioned document on stdout with `sxr_output_version` first, mandatory
//! `unknowns`/`omissions`, and `durability` on final outputs.

use serde_json::{Map, Value, json};

/// Build the top-level output object. `sxr_output_version: "1"` is always the
/// first key; `unknowns` and `omissions` are always present (§17.2).
pub fn output(pairs: Vec<(&str, Value)>) -> Value {
    let mut m = Map::new();
    m.insert("sxr_output_version".into(), json!("1"));
    for (k, v) in pairs {
        m.insert(k.into(), v);
    }
    m.entry("unknowns".to_string()).or_insert_with(|| json!([]));
    m.entry("omissions".to_string())
        .or_insert_with(|| json!([]));
    Value::Object(m)
}

/// Convenience for a single-key output.
pub fn output1(key: &str, value: Value) -> Value {
    output(vec![(key, value)])
}

/// Emit a JSON output document to stdout, followed by a newline.
pub fn print_json(value: &Value) {
    println!(
        "{}",
        serde_json::to_string(value).expect("serialize output")
    );
}

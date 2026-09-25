use serde_json::Value;

/// The CEP canonical full Semantic Envelope schema, pinned from the CEP
/// repository at commit `008034e` (branch `slice-0a-cep-semantic-envelope`),
/// file `schemas/cep-semantic-envelope.schema.json`.
///
/// Provenance: copied by sxr Slice 1 (M1); source sha256
/// `4b42bfddf2da553e4ee5860ffa6f7b1374eb0bb13b8ede0d722bc41e9d8b5b17`.
/// The CEP project owns this schema; sxr consumes it, never edits it. The
/// authoritative gate is the real `cep validate semantic-envelope --stdin`
/// run in the Gate 1 proof; the embedded copy enforces I-LED in-process.
pub const CEP_SCHEMA_PINNED_COMMIT: &str = "008034e";
pub const CEP_SCHEMA_SOURCE_SHA256: &str =
    "sha256:4b42bfddf2da553e4ee5860ffa6f7b1374eb0bb13b8ede0d722bc41e9d8b5b17";
const CEP_SCHEMA_JSON: &str = include_str!("../schemas/cep-semantic-envelope.schema.json");

pub fn schema_json() -> Value {
    serde_json::from_str(CEP_SCHEMA_JSON).expect("embedded CEP schema must be valid JSON")
}

/// A compiled validator over the embedded schema.
pub struct EnvelopeSchemaValidator {
    validator: jsonschema::JSONSchema,
}

impl EnvelopeSchemaValidator {
    pub fn new() -> Self {
        let validator =
            jsonschema::JSONSchema::compile(&schema_json()).expect("schema must compile");
        Self { validator }
    }

    /// Validate an envelope instance; returns all schema errors.
    pub fn validate(&self, envelope: &Value) -> Result<(), Vec<String>> {
        match self.validator.validate(envelope) {
            Ok(()) => Ok(()),
            Err(errors) => Err(errors.map(|e| e.to_string()).collect()),
        }
    }
}

impl Default for EnvelopeSchemaValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_schema_digest_matches_source() {
        use sha2::Digest;
        let digest = sha2::Sha256::digest(CEP_SCHEMA_JSON.as_bytes());
        assert_eq!(
            format!("sha256:{}", hex::encode(digest)),
            CEP_SCHEMA_SOURCE_SHA256,
            "embedded schema bytes must match the recorded cep commit digest"
        );
    }
}

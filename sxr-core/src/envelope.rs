use crate::clock::{ClockPort, rfc3339_micros};
use crate::embedded_schema::EnvelopeSchemaValidator;
use crate::record_kinds::RecordKind;
use crate::types::{CEP_VERSION, ENVELOPE_VERSION};
use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};
use uuid::Uuid;

/// Error building or validating an sxr-produced CEP envelope.
#[derive(Debug, thiserror::Error)]
pub enum EnvelopeError {
    #[error("scope must be a JSON object, got {0:?}")]
    ScopeNotObject(Value),
    #[error("boundary_record must be a JSON object, got {0:?}")]
    BoundaryNotObject(Value),
    #[error("entity_sections must be a JSON object, got {0:?}")]
    SectionsNotObject(Value),
    #[error("no entity sections supplied; CEP-0008 §52 requires at least one")]
    NoEntitySections,
    #[error("entity section {0:?} is not in the CEP-0008 §9 vocabulary")]
    UnknownEntitySection(String),
    #[error("entity section {0:?} must be an array")]
    SectionNotArray(String),
    #[error("boundary_record key {0:?} is not in the closed boundary vocabulary")]
    UnknownBoundaryKey(String),
    #[error("envelope failed CEP schema validation: {0:?}")]
    SchemaValidation(Vec<String>),
    #[error(
        "envelope_kind mismatch: record kind {kind} requires envelope_kind {expected}, envelope carries {actual}"
    )]
    KindEnvelopeMismatch {
        kind: String,
        expected: String,
        actual: String,
    },
    #[error(
        "required subject ref missing: kind {kind} requires a ref with prefix {prefix:?}; got {got:?}"
    )]
    MissingSubjectRef {
        kind: String,
        prefix: String,
        got: Vec<String>,
    },
}

/// Entity section keys of the CEP-0008 §9 vocabulary (array-valued envelope
/// sections). Enforced so the builder cannot drift from the published schema.
pub const ENTITY_SECTIONS: &[&str] = &[
    "references",
    "relations",
    "distinctions",
    "representations",
    "states",
    "transformations",
    "observations",
    "questions",
    "claims",
    "evidence",
    "settlements",
    "authority",
    "affordances",
    "capabilities",
    "metrics",
    "benchmarks",
    "omissions",
    "uncertainty",
    "contradictions",
    "debt",
    "conformance",
    "extensions",
];

/// Closed boundary_record vocabulary (schema `additionalProperties: false`).
pub const BOUNDARY_KEYS: &[&str] = &[
    "scope",
    "included_sections",
    "excluded_sections",
    "known_omissions",
    "unknowns",
    "redactions",
    "compression_notes",
    "out_of_scope_entities",
    "classification",
    "limitations",
];

/// Everything needed to build one sxr runtime envelope (§5.4).
#[derive(Debug, Clone)]
pub struct EnvelopeParts {
    /// Outer identity: a UUIDv7. Varies freely per emission (I-DET).
    pub envelope_id: String,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub record_kind: RecordKind,
    /// Producer-preserved subset applicable to the event: keys from
    /// {repo_id, checkout_id, filesystem_path, git_commit, semantic_root,
    /// model_ref, run_id}; paths remain observed metadata.
    pub scope: Value,
    pub boundary_record: Value,
    pub completeness_status: &'static str,
    /// Ledger refs of the envelopes this derives from (§5.4).
    pub provenance_refs: Vec<String>,
    /// Prior envelope ids for the same subject (§5.4).
    pub lineage_refs: Vec<String>,
    /// Entity sections; keys MUST be a subset of [`ENTITY_SECTIONS`].
    pub entity_sections: Value,
    pub content_hash: Option<String>,
    pub semantic_hash: Option<String>,
}

/// Assemble a complete CEP Semantic Envelope for an sxr runtime record and
/// validate it against the embedded canonical schema (I-LED, in-process).
///
/// `validation_status` is always `"validated"` — subject/model validation is a
/// distinct entity/result field, never the envelope's own status (§5.4).
pub fn build_envelope(parts: &EnvelopeParts) -> Result<Value, EnvelopeError> {
    let scope = parts.scope.clone();
    if !scope.is_object() {
        return Err(EnvelopeError::ScopeNotObject(scope));
    }
    let boundary = parts.boundary_record.clone();
    if !boundary.is_object() {
        return Err(EnvelopeError::BoundaryNotObject(boundary));
    }
    for key in boundary.as_object().unwrap().keys() {
        if !BOUNDARY_KEYS.contains(&key.as_str()) {
            return Err(EnvelopeError::UnknownBoundaryKey(key.clone()));
        }
    }
    let sections = parts.entity_sections.clone();
    if !sections.is_object() {
        return Err(EnvelopeError::SectionsNotObject(sections));
    }
    let sections_obj = sections.as_object().unwrap();
    if sections_obj.is_empty() {
        return Err(EnvelopeError::NoEntitySections);
    }
    for key in sections_obj.keys() {
        if !ENTITY_SECTIONS.contains(&key.as_str()) {
            return Err(EnvelopeError::UnknownEntitySection(key.clone()));
        }
        if !sections_obj[key].is_array() {
            return Err(EnvelopeError::SectionNotArray(key.clone()));
        }
    }

    let mut envelope = Map::new();
    envelope.insert("envelope_id".into(), json!(parts.envelope_id));
    envelope.insert("cep_version".into(), json!(CEP_VERSION));
    envelope.insert("envelope_version".into(), json!(ENVELOPE_VERSION));
    envelope.insert(
        "envelope_kind".into(),
        json!(parts.record_kind.def().envelope_kind),
    );
    envelope.insert("created_at".into(), json!(rfc3339_micros(parts.created_at)));
    envelope.insert("created_by".into(), json!(parts.created_by));
    envelope.insert("scope".into(), scope);
    envelope.insert("boundary_record".into(), boundary);
    envelope.insert(
        "completeness_status".into(),
        json!(parts.completeness_status),
    );
    envelope.insert("provenance_refs".into(), json!(parts.provenance_refs));
    envelope.insert("omission_status".into(), json!("no_known_omissions"));
    envelope.insert("lineage_refs".into(), json!(parts.lineage_refs));
    envelope.insert("validation_status".into(), json!("validated"));
    for (key, value) in sections_obj {
        envelope.insert(key.clone(), value.clone());
    }
    if parts.content_hash.is_some() || parts.semantic_hash.is_some() {
        let mut integrity = Map::new();
        if let Some(ch) = &parts.content_hash {
            integrity.insert("content_hash".into(), json!(ch));
        }
        if let Some(sh) = &parts.semantic_hash {
            integrity.insert("semantic_hash".into(), json!(sh));
        }
        integrity.insert("verification_method".into(), json!("sha256-canonical-json"));
        integrity.insert("verification_status".into(), json!("verified"));
        integrity.insert("tamper_status".into(), json!("intact"));
        envelope.insert("integrity".into(), Value::Object(integrity));
    }

    let envelope = Value::Object(envelope);
    let validator = EnvelopeSchemaValidator::new();
    if let Err(errors) = validator.validate(&envelope) {
        return Err(EnvelopeError::SchemaValidation(errors));
    }
    Ok(envelope)
}

/// §5.3b conformance of a row's record kind: the envelope's `envelope_kind`
/// must match the mapping, and the row's `subject_refs` must carry every
/// required subject prefix. Unregistered kinds are rejected outright.
pub fn validate_record_kind_conformance(
    kind: &RecordKind,
    envelope: &Value,
    subject_refs: &[String],
) -> Result<(), EnvelopeError> {
    let def = kind.def();
    let actual = envelope
        .get("envelope_kind")
        .and_then(Value::as_str)
        .unwrap_or("<missing>");
    if actual != def.envelope_kind {
        return Err(EnvelopeError::KindEnvelopeMismatch {
            kind: kind.as_str().into(),
            expected: def.envelope_kind.into(),
            actual: actual.into(),
        });
    }
    for prefix in def.subject_prefixes.split(';').filter(|p| !p.is_empty()) {
        if !subject_refs.iter().any(|r| r.starts_with(prefix)) {
            return Err(EnvelopeError::MissingSubjectRef {
                kind: kind.as_str().into(),
                prefix: prefix.into(),
                got: subject_refs.to_vec(),
            });
        }
    }
    Ok(())
}

/// Standard §5.4 scope builder for runtime events.
pub fn runtime_scope(
    repo_id: &str,
    checkout_id: &str,
    filesystem_path: Option<&str>,
    git_commit: Option<&str>,
    model_ref: Option<&str>,
    run_id: Option<&str>,
) -> Value {
    let mut scope = Map::new();
    scope.insert("repo_id".into(), json!(repo_id));
    scope.insert("checkout_id".into(), json!(checkout_id));
    if let Some(p) = filesystem_path {
        scope.insert("filesystem_path".into(), json!(p));
    }
    if let Some(c) = git_commit {
        scope.insert("git_commit".into(), json!(c));
    }
    if let Some(m) = model_ref {
        scope.insert("model_ref".into(), json!(m));
    }
    if let Some(r) = run_id {
        scope.insert("run_id".into(), json!(r));
    }
    Value::Object(scope)
}

/// Mint a UUIDv7 envelope id.
pub fn new_envelope_id() -> String {
    Uuid::now_v7().to_string()
}

/// `created_by` for sxr runtime records (§5.4).
pub fn sxr_created_by(version: &str) -> String {
    format!("sxr {version}")
}

/// Deterministic idempotency key per §11.4:
/// `sha256(run_id + verifier_id + step_id + attempt)`.
pub fn idempotency_key(parts: &[&str]) -> String {
    crate::canonical_json::sha256_hex(parts.join("+").as_bytes())
}

/// Compute the checkpoint identity for a snapshot row (AC-SXR-002):
/// resolved — pure function of inner identity; validation-failed — pure
/// function of the exact failed-inputs checkpoint hash.
pub fn snapshot_id(
    semantic_closure_hash: Option<&str>,
    content_hash: Option<&str>,
    invalid_checkpoint: Option<&str>,
    entry: &str,
) -> String {
    let value = match (semantic_closure_hash, content_hash, invalid_checkpoint) {
        (Some(sh), Some(ch), None) => {
            json!({"semantic_closure_hash": sh, "content_hash": ch, "entry_logical_path": entry})
        }
        (None, None, Some(ic)) => {
            json!({"invalid_declared_checkpoint_hash": ic, "entry_logical_path": entry})
        }
        _ => panic!(
            "snapshot_id: closed sum violated (resolved rows carry both hashes; failed rows carry only the checkpoint)"
        ),
    };
    crate::canonical_json::sha256_canonical(&value)
}

/// §12.1 closed-sum guard for sxr_snapshot rows: exactly one shape.
pub fn validate_snapshot_row_shape(
    semantic_closure_hash: Option<&str>,
    content_hash: Option<&str>,
    invalid_declared_checkpoint_hash: Option<&str>,
    model_validation_status: &str,
) -> Result<(), String> {
    let resolved = semantic_closure_hash.is_some() || content_hash.is_some();
    let failed = invalid_declared_checkpoint_hash.is_some();
    match (resolved, failed) {
        (true, true) => Err("mixed sxr_snapshot shape: resolved hashes and invalid checkpoint hash are mutually exclusive".into()),
        (true, false) => {
            if semantic_closure_hash.is_none() || content_hash.is_none() {
                Err("resolved sxr_snapshot row must carry both semantic and content hashes".into())
            } else if model_validation_status != "valid" && model_validation_status != "invalid" {
                Err("resolved sxr_snapshot row must have model_validation_status 'valid' or 'invalid'".into())
            } else {
                Ok(())
            }
        }
        (false, true) => {
            if model_validation_status != "invalid" {
                Err("validation-failed sxr_snapshot row must have model_validation_status 'invalid'".into())
            } else {
                Ok(())
            }
        }
        (false, false) => Err("empty sxr_snapshot row: neither resolved nor validation-failed shape".into()),
    }
}

/// An injectable clock passed to builders.
pub fn created_at(clock: &dyn ClockPort) -> DateTime<Utc> {
    clock.now_utc()
}

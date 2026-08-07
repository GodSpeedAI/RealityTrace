//! Unit tests: fixture-driven parsing of the three Slice 0B output shapes,
//! D hash verification (AC-REPLAY-001), §10.4a rejection of malformed input.

use serde_json::json;
use std::path::PathBuf;
use sxr_core::types::{DeclKey, SemanticContext};
use sxr_df::adapter::CliDomainForgeAdapter;

const FIXTURES: &str = env!("CARGO_MANIFEST_DIR");

fn fixture(name: &str) -> String {
    std::fs::read_to_string(PathBuf::from(FIXTURES).join("tests/fixtures").join(name))
        .expect("fixture exists")
}

#[test]
fn resolved_shape_maps_to_resolved_context() {
    let adapter = CliDomainForgeAdapter::new("/nonexistent"); // no process in unit tests
    let envelope: serde_json::Value =
        serde_json::from_str(&fixture("env-valid-basic.json")).unwrap();
    let ctx = adapter.parse_envelope(&envelope).unwrap();
    let SemanticContext::Resolved {
        content_hash,
        semantic_closure_hash,
        representation_json,
        resolution_inputs,
        declarations,
        diagnostics,
        ..
    } = ctx
    else {
        panic!("valid-basic must resolve");
    };
    assert!(content_hash.starts_with("sha256:"));
    assert!(semantic_closure_hash.starts_with("sha256:"));
    assert_ne!(content_hash, semantic_closure_hash);
    // D must be present verbatim as JSON.
    assert!(serde_json::from_str::<serde_json::Value>(&representation_json).is_ok());
    // Resolution inputs from the provenance record.
    assert_eq!(
        resolution_inputs.source_set_hash,
        "sha256:199407284bab273208ccea52deb56197c582816a668ee726d7ab1ab3797d42b5"
    );
    assert_eq!(
        resolution_inputs.language_schema_version,
        "domainforge-ast/v3"
    );
    assert_eq!(resolution_inputs.producer, "domainforge-core 0.16.0");
    assert_eq!(
        resolution_inputs.resolved_namespaces,
        vec![("valid-basic.sea".to_string(), "auth".to_string())]
    );
    assert!(resolution_inputs.registry_content_hash.is_some());
    // Declarations from preserved_entities.
    assert_eq!(declarations.len(), 5);
    assert!(declarations.contains(&DeclKey::Concept(
        "5ad6a959-5924-5cf9-af04-7b502ea74784".into()
    )));
    assert!(declarations.contains(&DeclKey::Occurrence {
        kind: "flow".into(),
        content_hash:
            "sha256:0054e494b25408165361fa7d0fb1e090a7aee22d7dce2c9f188bc87e24451543".into(),
        duplicate_index: 0,
    }));
    assert!(diagnostics.is_empty(), "valid model carries no diagnostics");
}

#[test]
fn invalid_model_without_d_maps_to_validation_failed() {
    let adapter = CliDomainForgeAdapter::new("/nonexistent");
    let envelope: serde_json::Value =
        serde_json::from_str(&fixture("env-invalid-no-d.json")).unwrap();
    let ctx = adapter.parse_envelope(&envelope).unwrap();
    let SemanticContext::ValidationFailed {
        invalid_declared_checkpoint_hash,
        resolution_inputs,
        diagnostics,
        representation_omission,
        ..
    } = ctx
    else {
        panic!("invalid-syntax must map to ValidationFailed");
    };
    assert_eq!(
        invalid_declared_checkpoint_hash,
        "sha256:602c3e2e32f848d8d5f47b5c2c3df0b88c877a8d2819fc6e46efa49e52c950fa"
    );
    assert_eq!(
        representation_omission.omission_type,
        "representation_unavailable"
    );
    assert_eq!(
        representation_omission.affected_entities,
        vec!["domainforge-core/tests/fixtures/envelope/invalid-syntax.sea"]
    );
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].rule, "artifact_schema_violation");
    assert!(diagnostics[0].message.contains("failed to parse"));
    // Failure-boundary inputs: only what the envelope/request actually carry.
    assert_eq!(
        resolution_inputs.source_set_hash,
        "sha256:1da2f040c24d5498b63de891e834d8f882facc4676e68f7ea90c61bb46de0b9e"
    );
    // entry_logical_path comes from the envelope's own scope, not the request.
    assert_eq!(
        resolution_inputs.entry_logical_path,
        "domainforge-core/tests/fixtures/envelope/invalid-syntax.sea"
    );
    assert_eq!(resolution_inputs.semantic_pack_set_hash, "");
    assert_eq!(resolution_inputs.language_schema_version, "");
}

#[test]
fn invalid_model_with_d_maps_to_resolved_with_diagnostics() {
    // The dogfood entry shape: exit 1 but D constructed — resolved, never
    // misclassified into ValidationFailed (§14.4, §7.1).
    let adapter = CliDomainForgeAdapter::new("/nonexistent");
    let envelope: serde_json::Value =
        serde_json::from_str(&fixture("env-invalid-with-d.json")).unwrap();
    let ctx = adapter.parse_envelope(&envelope).unwrap();
    let SemanticContext::Resolved {
        content_hash,
        diagnostics,
        declarations,
        ..
    } = &ctx
    else {
        panic!("D-constructed invalid model must resolve");
    };
    assert!(content_hash.starts_with("sha256:"));
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].rule, "logistics_flow");
    assert_eq!(declarations.len(), 5);
    assert_eq!(ctx.model_validation_status(), "valid");
}

#[test]
fn tampered_inline_content_fails_hash_verification() {
    let adapter = CliDomainForgeAdapter::new("/nonexistent");
    let mut envelope: serde_json::Value =
        serde_json::from_str(&fixture("env-valid-basic.json")).unwrap();
    let inline = envelope["representations"][0]["inline_content"]
        .as_str()
        .unwrap()
        .to_string();
    // Alter a value inside D (keeping it valid JSON) without touching the
    // declared hashes.
    let tampered = inline.replacen(
        "domainforge-semantic-envelope/v1",
        "domainforge-semantic-envelope/v2",
        1,
    );
    assert_ne!(tampered, inline);
    envelope["representations"][0]["inline_content"] = json!(tampered);
    let err = adapter.parse_envelope(&envelope).unwrap_err();
    assert!(err.to_string().contains("hash mismatch"), "got {err}");
}

#[test]
fn tampered_declared_hash_fails_verification() {
    let adapter = CliDomainForgeAdapter::new("/nonexistent");
    let mut envelope: serde_json::Value =
        serde_json::from_str(&fixture("env-valid-basic.json")).unwrap();
    envelope["representations"][0]["semantic_hash"] =
        json!("sha256:".to_string() + &"0".repeat(64));
    let err = adapter.parse_envelope(&envelope).unwrap_err();
    assert!(err.to_string().contains("hash mismatch"), "got {err}");
}

#[test]
fn schema_violation_is_rejected_not_misclassified() {
    let adapter = CliDomainForgeAdapter::new("/nonexistent");
    let mut envelope: serde_json::Value =
        serde_json::from_str(&fixture("env-valid-basic.json")).unwrap();
    // Drop a required envelope field.
    envelope
        .as_object_mut()
        .unwrap()
        .remove("completeness_status");
    let err = adapter.parse_envelope(&envelope).unwrap_err();
    assert!(err.to_string().contains("schema"), "got {err}");
}

#[test]
fn malformed_json_is_rejected() {
    let adapter = CliDomainForgeAdapter::new("/nonexistent");
    let err = adapter.parse_json(b"not json at all").unwrap_err();
    assert!(err.to_string().contains("not JSON"), "got {err}");
    let err = adapter.parse_json(b"[]").unwrap_err();
    assert!(err.to_string().contains("not an object"), "got {err}");
}

#[test]
fn neither_shape_is_rejected() {
    let adapter = CliDomainForgeAdapter::new("/nonexistent");
    // Schema-valid (validation_status present) but lacking representations:
    // neither the resolved nor the failure shape.
    let mut envelope: serde_json::Value =
        serde_json::from_str(&fixture("env-valid-basic.json")).unwrap();
    envelope.as_object_mut().unwrap().remove("representations");
    let err = adapter.parse_envelope(&envelope).unwrap_err();
    assert!(err.to_string().contains("matches neither"), "got {err}");
}

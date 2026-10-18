//! `evidence.recorded` (E8 persistence): a real emitted E8 maps to a
//! schema-valid, kind-conformant CEP envelope; direction follows the verdict;
//! compared values never enter the envelope; malformed E8 is refused.

use serde_json::{Value, json};
use sxr_core::embedded_schema::EnvelopeSchemaValidator;
use sxr_core::envelope::{
    build_envelope, runtime_scope, sxr_created_by, validate_record_kind_conformance,
};
use sxr_core::evidence_record::{
    RecordContext, RecordRefusal, evidence_recorded_parts, frozen_expectation_question,
};
use sxr_core::record_kinds::RecordKind;

fn emitted() -> Value {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/t08_evidence_recorded.json")).unwrap();
    fixture["evidence_recorded"].clone()
}

fn ctx() -> RecordContext {
    RecordContext {
        envelope_id: "0190b9a2-0000-7000-8000-000000000001".into(),
        created_at: chrono::DateTime::parse_from_rfc3339("2026-10-18T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
        created_by: sxr_created_by("0.1.0-test"),
        scope: runtime_scope("repo-test", "checkout-test", None, None, None, None),
        provenance_refs: vec!["ledger:gen-test:7".into()],
        lineage_refs: vec![],
    }
}

fn envelope(e8: &Value) -> Value {
    let (parts, subjects) = evidence_recorded_parts(e8, ctx()).unwrap();
    let envelope = build_envelope(&parts).unwrap();
    EnvelopeSchemaValidator::new()
        .validate(&envelope)
        .unwrap_or_else(|errs| panic!("schema: {errs:?}"));
    validate_record_kind_conformance(&RecordKind::EvidenceRecorded, &envelope, &subjects).unwrap();
    envelope
}

#[test]
fn emitted_e8_persists_as_a_conformant_evidence_packet() {
    let e8 = emitted();
    let envelope = envelope(&e8);
    assert_eq!(envelope["envelope_kind"], "evidence_packet");
    let evidence = &envelope["evidence"][0];
    let proof = e8["payload"]["proof_result_ref"].as_str().unwrap();
    assert_eq!(evidence["question_ref"], frozen_expectation_question(proof));
    assert_eq!(
        envelope["questions"][0]["question_id"],
        evidence["question_ref"]
    );
    assert_eq!(evidence["direction"], "contradicts");
    assert_eq!(evidence["provenance_refs"], e8["provenance"]["chain"]);
    for rep in envelope["representations"].as_array().unwrap() {
        assert_eq!(rep["content_availability"], "unavailable");
    }
}

#[test]
fn direction_follows_the_verdict_mechanically() {
    let mut e8 = emitted();
    let d = &mut e8["payload"]["difference"];
    d["observed_sha256"] = d["expected_sha256"].clone();
    d["verdict"] = json!("equivalent");
    assert_eq!(envelope(&e8)["evidence"][0]["direction"], "supports");
}

#[test]
fn compared_values_never_enter_the_envelope() {
    let e8 = emitted();
    let text = serde_json::to_string(&envelope(&e8)).unwrap();
    let mut values = Vec::new();
    fn strings(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::String(s) if s.len() > 3 => out.push(s.clone()),
            Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
            Value::Object(o) => o.values().for_each(|x| strings(x, out)),
            _ => {}
        }
    }
    strings(&e8["payload"]["expected_outcome"], &mut values);
    strings(&e8["payload"]["observed_outcome"], &mut values);
    assert!(!values.is_empty());
    for value in values {
        assert!(
            !text.contains(&value),
            "outcome value copied into envelope: {value}"
        );
    }
}

#[test]
fn malformed_e8_is_refused_before_any_envelope() {
    let base = emitted();
    let refuse = |f: &dyn Fn(&mut Value)| {
        let mut v = base.clone();
        f(&mut v);
        evidence_recorded_parts(&v, ctx()).unwrap_err()
    };
    assert!(matches!(
        refuse(&|v| v["source_agent"] = json!("sea-forge")),
        RecordRefusal::NotEvidenceRecorded { .. }
    ));
    assert!(matches!(
        refuse(&|v| v["event_type"] = json!("ProofCompleted")),
        RecordRefusal::NotEvidenceRecorded { .. }
    ));
    assert_eq!(
        refuse(&|v| v["payload"]["difference"]["verdict"] = json!("equivalent")),
        RecordRefusal::InconsistentVerdict
    );
    assert_eq!(
        refuse(&|v| v["payload"]["difference"]["verdict"] = json!("partial")),
        RecordRefusal::Malformed {
            field: "difference.verdict"
        }
    );
    assert_eq!(
        refuse(&|v| v["payload"]["proof_result_ref"] = json!("abc")),
        RecordRefusal::Malformed {
            field: "proof_result_ref"
        }
    );
    assert_eq!(
        refuse(&|v| v["payload"]["operational_settlement_ref"] = Value::Null),
        RecordRefusal::Malformed {
            field: "operational_settlement_ref"
        }
    );
    assert_eq!(
        refuse(&|v| v["provenance"]["chain"] = json!([])),
        RecordRefusal::Malformed {
            field: "provenance.chain"
        }
    );
    assert_eq!(
        refuse(&|v| v["payload"]["work_request_id"] = json!("  ")),
        RecordRefusal::Malformed {
            field: "work_request_id"
        }
    );
}

#[test]
fn mapping_is_deterministic_for_a_fixed_context() {
    let e8 = emitted();
    let a = evidence_recorded_parts(&e8, ctx()).unwrap();
    let b = evidence_recorded_parts(&e8, ctx()).unwrap();
    assert_eq!(a.0.entity_sections, b.0.entity_sections);
    assert_eq!(a.1, b.1);
}

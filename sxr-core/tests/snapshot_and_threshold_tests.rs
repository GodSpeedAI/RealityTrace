use serde_json::json;
use sxr_core::envelope::{
    snapshot_id, validate_record_kind_conformance, validate_snapshot_row_shape,
};
use sxr_core::record_kinds::RecordKind;
use sxr_core::threshold::{
    EvidenceDirection, ThresholdKind, ThresholdSpec, authorize_settlement, evaluate,
};
use sxr_core::types::SettlementStatus;

#[test]
fn snapshot_id_resolved_is_deterministic() {
    let a = snapshot_id(Some("sha256:aa"), Some("sha256:bb"), None, "core.sea");
    let b = snapshot_id(Some("sha256:aa"), Some("sha256:bb"), None, "core.sea");
    assert_eq!(a, b);
    assert!(a.starts_with("sha256:"));
}

#[test]
fn snapshot_id_failed_is_deterministic_and_distinct() {
    let a = snapshot_id(None, None, Some("sha256:cc"), "core.sea");
    let b = snapshot_id(None, None, Some("sha256:cc"), "core.sea");
    assert_eq!(a, b);
    let resolved = snapshot_id(Some("sha256:aa"), Some("sha256:bb"), None, "core.sea");
    assert_ne!(a, resolved);
}

#[test]
fn snapshot_id_never_mixes_shapes() {
    let result = std::panic::catch_unwind(|| {
        snapshot_id(Some("sha256:aa"), None, Some("sha256:cc"), "core.sea")
    });
    assert!(result.is_err(), "mixed shape must panic — closed sum");
}

#[test]
fn snapshot_row_closed_sum_accepts_exactly_two_shapes() {
    assert!(validate_snapshot_row_shape(Some("a"), Some("b"), None, "valid").is_ok());
    assert!(validate_snapshot_row_shape(None, None, Some("c"), "invalid").is_ok());
    // mixed
    assert!(validate_snapshot_row_shape(Some("a"), None, Some("c"), "invalid").is_err());
    // empty
    assert!(validate_snapshot_row_shape(None, None, None, "unknown").is_err());
    // partial resolved
    assert!(validate_snapshot_row_shape(Some("a"), None, None, "valid").is_err());
    // status mismatch
    assert!(validate_snapshot_row_shape(Some("a"), Some("b"), None, "invalid").is_err());
    assert!(validate_snapshot_row_shape(None, None, Some("c"), "valid").is_err());
}

#[test]
fn threshold_evaluation_is_deterministic() {
    let spec = ThresholdSpec {
        threshold_id: "t-1".into(),
        question_id: "q-1".into(),
        kind: ThresholdKind::AllEvidenceSupports,
        spec: json!({"k": "v"}),
    };
    assert_eq!(
        evaluate(
            &spec,
            &[EvidenceDirection::Supports, EvidenceDirection::Supports]
        ),
        SettlementStatus::Settled
    );
    assert_eq!(
        evaluate(
            &spec,
            &[EvidenceDirection::Supports, EvidenceDirection::Unknown]
        ),
        SettlementStatus::PartiallySettled
    );
    assert_eq!(evaluate(&spec, &[]), SettlementStatus::Unsettled);

    let contradict = ThresholdSpec {
        threshold_id: "t-2".into(),
        question_id: "q-1".into(),
        kind: ThresholdKind::AnyEvidenceContradicts,
        spec: json!({}),
    };
    assert_eq!(
        evaluate(&contradict, &[EvidenceDirection::Contradicts]),
        SettlementStatus::Failed
    );
    assert_eq!(
        evaluate(&contradict, &[EvidenceDirection::Supports]),
        SettlementStatus::Settled
    );
}

#[test]
fn invalid_model_cannot_settle_positively() {
    let kinds = [
        ThresholdKind::AllEvidenceSupports,
        ThresholdKind::AnyEvidenceContradicts,
        ThresholdKind::RequiresEvidence,
    ];
    for kind in kinds {
        assert!(
            authorize_settlement("invalid", &kind, SettlementStatus::Settled).is_err(),
            "{kind:?}"
        );
        assert!(
            authorize_settlement("invalid", &kind, SettlementStatus::PartiallySettled).is_err()
        );
        assert!(authorize_settlement("invalid", &kind, SettlementStatus::Failed).is_ok());
        assert!(authorize_settlement("invalid", &kind, SettlementStatus::Blocked).is_ok());
        assert!(authorize_settlement("valid", &kind, SettlementStatus::Settled).is_ok());
    }
}

#[test]
fn every_kind_conformance_error_names_the_kind() {
    let err = validate_record_kind_conformance(
        &RecordKind::RepoEnrolled,
        &json!({"envelope_kind": "semantic_snapshot"}),
        &[],
    )
    .unwrap_err();
    assert!(format!("{err}").contains("repo.enrolled"));
}

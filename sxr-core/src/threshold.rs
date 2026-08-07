use crate::types::SettlementStatus;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Evidence direction vocabulary (§5.5.1 derivation rule for direction).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceDirection {
    Supports,
    Contradicts,
    Unknown,
}

impl EvidenceDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            EvidenceDirection::Supports => "supports",
            EvidenceDirection::Contradicts => "contradicts",
            EvidenceDirection::Unknown => "unknown",
        }
    }
}

/// Threshold evaluation kinds — a small closed set with defined semantics.
/// The full evaluation profile (CEP-0007 §13) is carried in `spec`; Slice 1
/// implements the deterministic core used by `settlement register`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThresholdKind {
    /// Settled only when every qualifying evidence supports and none is
    /// unknown or contradicts; otherwise partially_settled.
    AllEvidenceSupports,
    /// Failed when any evidence contradicts; settled when all support.
    AnyEvidenceContradicts,
    /// No qualifying evidence at all: unsettled.
    RequiresEvidence,
}

impl ThresholdKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ThresholdKind::AllEvidenceSupports => "all_evidence_supports",
            ThresholdKind::AnyEvidenceContradicts => "any_evidence_contradicts",
            ThresholdKind::RequiresEvidence => "requires_evidence",
        }
    }
}

/// A declared Threshold (§5.5.1, CEP-0007 §13.2): declared BEFORE any
/// observation and evaluated only against strictly earlier evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThresholdSpec {
    pub threshold_id: String,
    pub question_id: String,
    pub kind: ThresholdKind,
    pub spec: Value,
}

/// Deterministic threshold evaluation over evidence directions.
///
/// Returns `settled`, `partially_settled`, `failed`, or `unsettled`; never
/// invents a status beyond what the directions support.
pub fn evaluate(threshold: &ThresholdSpec, directions: &[EvidenceDirection]) -> SettlementStatus {
    match threshold.kind {
        ThresholdKind::AllEvidenceSupports => {
            if directions.is_empty() {
                return SettlementStatus::Unsettled;
            }
            if directions.iter().all(|d| *d == EvidenceDirection::Supports) {
                SettlementStatus::Settled
            } else {
                SettlementStatus::PartiallySettled
            }
        }
        ThresholdKind::AnyEvidenceContradicts => {
            if directions.is_empty() {
                return SettlementStatus::Unsettled;
            }
            if directions.contains(&EvidenceDirection::Contradicts) {
                SettlementStatus::Failed
            } else if directions.iter().all(|d| *d == EvidenceDirection::Supports) {
                SettlementStatus::Settled
            } else {
                SettlementStatus::PartiallySettled
            }
        }
        ThresholdKind::RequiresEvidence => {
            if directions.is_empty() {
                SettlementStatus::Unsettled
            } else {
                SettlementStatus::Settled
            }
        }
    }
}

/// AC-SETTLEMENT-002 core rule: a validation-failed model settles the validity
/// question as failed or blocked; positive acceptance is rejected. This is the
/// question-agnostic guard used by `settlement register` when the snapshot's
/// model_validation_status is `invalid`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettlementAuthorization {
    Allowed,
    RejectedPositiveAcceptance,
}

/// Guard: given the snapshot's model status and the threshold kind, may a
/// positive (settled) outcome be produced? Negative outcomes (failed/blocked)
/// are always lawful for the invalidity question (§14.4).
pub fn authorize_settlement(
    model_validation_status: &str,
    _threshold: &ThresholdKind,
    requested: SettlementStatus,
) -> Result<SettlementAuthorization, String> {
    if model_validation_status == "invalid" {
        match requested {
            SettlementStatus::Settled | SettlementStatus::PartiallySettled => Err(
                "positive acceptance requires valid declared semantics (AC-SETTLEMENT-002)".into(),
            ),
            _ => Ok(SettlementAuthorization::Allowed),
        }
    } else if model_validation_status == "valid" {
        Ok(SettlementAuthorization::Allowed)
    } else {
        Err(format!(
            "unknown model_validation_status {model_validation_status:?}"
        ))
    }
}

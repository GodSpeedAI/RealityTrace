//! `evidence.recorded`: the CEP envelope for an EvidenceRecorded (E8), so an
//! `sxr-ledger` can persist the expected-versus-observed comparison that
//! [`crate::evidence_emit`] returns.
//!
//! The mapping is fixed and derived, never claimed (owner decisions recorded
//! 2026-10-18):
//!
//! - **Question:** the frozen expectation of the admitted ProofCompleted,
//!   identified by that envelope's digest. It was bound before execution, so
//!   it cannot be chosen after the fact.
//! - **Direction:** mechanical from `difference.verdict` (`equivalent` →
//!   `supports`, `divergent` → `contradicts`). It states how the observation
//!   relates to the Question, not an audit verdict; the auditor owns that.
//! - **Content:** digests only. Expected and observed outcomes become
//!   representations with `content_availability: "unavailable"`; their values
//!   stay in the producing ledger and are never copied here.
//!
//! Only a well-formed RealityTrace E8 maps; anything else is refused before
//! an envelope exists.

use crate::envelope::EnvelopeParts;
use crate::record_kinds::RecordKind;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

/// Why an E8 could not be mapped. Nothing partial is ever returned.
#[derive(Debug, Clone, PartialEq)]
pub enum RecordRefusal {
    /// Not an EvidenceRecorded produced by RealityTrace.
    NotEvidenceRecorded {
        event_type: String,
        source_agent: String,
    },
    /// A required field is missing or malformed.
    Malformed { field: &'static str },
    /// The verdict disagrees with the compared digests.
    InconsistentVerdict,
}

impl std::fmt::Display for RecordRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotEvidenceRecorded {
                event_type,
                source_agent,
            } => write!(
                f,
                "not a RealityTrace EvidenceRecorded: event_type={event_type} source_agent={source_agent}"
            ),
            Self::Malformed { field } => write!(f, "malformed EvidenceRecorded field {field}"),
            Self::InconsistentVerdict => write!(f, "verdict disagrees with the compared digests"),
        }
    }
}

impl std::error::Error for RecordRefusal {}

fn hex64(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn text<'a>(v: &'a Value, field: &'static str) -> Result<&'a str, RecordRefusal> {
    v.as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or(RecordRefusal::Malformed { field })
}

fn digest<'a>(v: &'a Value, field: &'static str) -> Result<&'a str, RecordRefusal> {
    text(v, field).and_then(|s| {
        if hex64(s) {
            Ok(s)
        } else {
            Err(RecordRefusal::Malformed { field })
        }
    })
}

fn sha_ref<'a>(v: &'a Value, field: &'static str) -> Result<&'a str, RecordRefusal> {
    let s = text(v, field)?;
    match s.strip_prefix("sha256:") {
        Some(d) if hex64(d) => Ok(s),
        _ => Err(RecordRefusal::Malformed { field }),
    }
}

/// The ledger write context the caller supplies; everything else is derived.
#[derive(Debug, Clone)]
pub struct RecordContext {
    pub envelope_id: String,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub scope: Value,
    /// Ledger refs of the envelopes this E8 derives from.
    pub provenance_refs: Vec<String>,
    /// Prior envelope ids for the same subject.
    pub lineage_refs: Vec<String>,
}

/// The Question id an E8 answers: its proof's frozen expectation.
pub fn frozen_expectation_question(proof_result_ref: &str) -> String {
    format!("question:frozen-expectation:{proof_result_ref}")
}

/// Map an E8 to the parts of its `evidence.recorded` envelope and the
/// subject refs `validate_record_kind_conformance` checks.
pub fn evidence_recorded_parts(
    e8: &Value,
    ctx: RecordContext,
) -> Result<(EnvelopeParts, Vec<String>), RecordRefusal> {
    let event_type = e8["event_type"].as_str().unwrap_or_default();
    let source_agent = e8["source_agent"].as_str().unwrap_or_default();
    if event_type != crate::evidence_emit::EVENT_TYPE
        || source_agent != crate::evidence_emit::PRODUCER
    {
        return Err(RecordRefusal::NotEvidenceRecorded {
            event_type: event_type.into(),
            source_agent: source_agent.into(),
        });
    }
    let event_id = text(&e8["event_id"], "event_id")?;
    let p = &e8["payload"];
    let work_request = text(&p["work_request_id"], "work_request_id")?;
    let affordance = text(&p["affordance_id"], "affordance_id")?;
    let proof = sha_ref(&p["proof_result_ref"], "proof_result_ref")?;
    let settlement = sha_ref(
        &p["operational_settlement_ref"],
        "operational_settlement_ref",
    )?;
    let d = &p["difference"];
    let expected = digest(&d["expected_sha256"], "difference.expected_sha256")?;
    let observed = digest(&d["observed_sha256"], "difference.observed_sha256")?;
    let direction = match d["verdict"].as_str() {
        Some("equivalent") if expected == observed => "supports",
        Some("divergent") if expected != observed => "contradicts",
        Some("equivalent" | "divergent") => return Err(RecordRefusal::InconsistentVerdict),
        _ => {
            return Err(RecordRefusal::Malformed {
                field: "difference.verdict",
            });
        }
    };
    let chain: Vec<Value> = e8["provenance"]["chain"]
        .as_array()
        .filter(|c| !c.is_empty() && c.iter().all(Value::is_string))
        .ok_or(RecordRefusal::Malformed {
            field: "provenance.chain",
        })?
        .clone();

    let question = frozen_expectation_question(proof);
    let expected_rep = format!("representation:expected:sha256:{expected}");
    let observed_rep = format!("representation:observed:sha256:{observed}");
    let mut limitations = vec![json!(
        "digest-level comparison; compared values are cited by digest and held by the producing ledger"
    )];
    if d["fields_truncated"].as_bool() == Some(true) {
        limitations.push(json!("field-level differences truncated"));
    }
    let entity_sections = json!({
        "questions": [{"question_id": question, "question_form": "is_true"}],
        "representations": [
            {"representation_id": expected_rep, "representation_kind": "atomic",
             "representation_type": "canonical_json_expected_outcome", "content_availability": "unavailable"},
            {"representation_id": observed_rep, "representation_kind": "atomic",
             "representation_type": "canonical_json_observed_outcome", "content_availability": "unavailable"}
        ],
        "evidence": [{
            "evidence_id": format!("evidence:recorded:{event_id}"),
            "evidence_type": "expectation_comparison",
            "question_ref": question,
            "target_entity_ref": format!("work_request:{work_request}"),
            "representation_ref": observed_rep,
            "source": crate::evidence_emit::PRODUCER,
            "provenance_refs": chain,
            "relevance": "direct",
            "reliability": "deterministic",
            "direction": direction,
            "limitations": limitations,
        }],
    });
    let subject_refs = vec![
        format!("work_request:{work_request}"),
        format!("affordance:{affordance}"),
        question.clone(),
        format!("proof:{proof}"),
        format!("settlement:{settlement}"),
    ];
    let parts = EnvelopeParts {
        envelope_id: ctx.envelope_id,
        created_at: ctx.created_at,
        created_by: ctx.created_by,
        record_kind: RecordKind::EvidenceRecorded,
        scope: ctx.scope,
        boundary_record: json!({
            "included_sections": ["questions", "representations", "evidence"],
            "excluded_sections": [],
            // Compared values are declared as content-unavailable
            // representations, not omitted, so no omission is hidden.
            "known_omissions": [],
            "unknowns": [],
            "limitations": [
                "compared outcome values are cited by digest only",
                "RealityTrace states the comparison; the audit verdict is the auditor's"
            ]
        }),
        completeness_status: "complete_for_declared_state",
        provenance_refs: ctx.provenance_refs,
        lineage_refs: ctx.lineage_refs,
        entity_sections,
        content_hash: None,
        semantic_hash: None,
    };
    Ok((parts, subject_refs))
}

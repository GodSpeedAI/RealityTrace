//! Canonical `EvidenceRecorded` emission (convergence plan task T08; frozen
//! edge E8, realitytrace -> godspeed_agent). RealityTrace (`sxr`) is the
//! EXCLUSIVE authoritative producer of EvidenceRecorded; GodSpeed-Agent is its
//! provisional-only consumer.
//!
//! This is the EMITTER twin of [`crate::proof_ingest`]: the canonical v1
//! envelope is emitted ONLY from the [`ComparisonInputs`] facts that the T07
//! ingestion gate preserved BY REFERENCE — expected versus observed are never
//! invented here, only projected verbatim alongside a derived, digest-bound
//! `difference`.
//!
//! Fail-closed emission chain, every link checked BEFORE any envelope exists:
//! identity gates on the locally-resolved model digest (structural, no
//! placeholder pseudo-hash — the standalone fallback constant `dc144cb…` and
//! the all-zero digest are refused); placeholder refusal for correlation ids;
//! well-formed event ids for the envelope and every causal parent; a complete
//! expected-vs-observed binding (a missing observed consequence — i.e.
//! comparison inputs whose referenced settlement never resolved — refuses
//! emission outright: no EvidenceRecorded without inspectable comparison
//! evidence); and reference validation for everything cited.
//!
//! Reference pinning where the substrate permits: `proof_result_ref` is the
//! CONTENT DIGEST of the exact ProofCompleted envelope that was ingested, and
//! `operational_settlement_ref` travels VERBATIM from upstream (already
//! content-addressed by construction). Causality cites the settlement and
//! ProofCompleted event ids as `caused_by:` parents, preserving the E7 chain
//! (ENV-I4/I2). The `difference` object carries the canonical-JSON digests of
//! BOTH compared values plus an equivalence verdict, so a consumer can verify
//! the comparison cryptographically instead of trusting editable copies
//! (ENV-I5 basis; downstream mutation probes fail closed).
//!
//! This module is PURE (AC-CORE-001): no clock, no filesystem — event id and
//! timestamp arrive as explicit parameters so output is deterministic under
//! fixed inputs.

use serde_json::{Map, Value, json};

use crate::canonical_json::{canonical_json, sha256_hex};
use crate::proof_ingest::ComparisonInputs;
use crate::world_ref::is_world_ref;

/// The event type owned exclusively by `realitytrace` at edge E8.
pub const EVENT_TYPE: &str = "EvidenceRecorded";

/// Loop namespace of the v1 wire family (mirrored frozen constant).
pub const NAMESPACE: &str = "agentic_capability_loop";

/// v1 schema version label (mirrored frozen constant).
pub const SCHEMA_VERSION: &str = "v1";

/// The exclusive authoritative producer of EvidenceRecorded (edge E8).
pub const PRODUCER: &str = "realitytrace";

/// The standalone fallback pseudo-hash: `sha256("agentic_capability_loop")`.
/// A namespace-derived constant must never masquerade as model identity
/// (ENV-I2).
pub const FALLBACK_PSEUDO_HASH: &str =
    "dc144cbd71a483431301ca1bf32e95c014af4edba8dbcc525f505310e30a107e";

/// Explicit placeholder identities that must never enter an emission.
const PLACEHOLDER_IDS: [&str; 5] = ["placeholder", "unknown", "none", "<missing>", "tbd"];

/// Why an EvidenceRecorded could not be emitted. Every refusal happens BEFORE
/// any canonical envelope exists; nothing partial ever leaves this surface.
#[derive(Debug, Clone, PartialEq)]
pub enum EmitRefusal {
    /// The expected-versus-observed binding inputs are missing: either the
    /// declared expectation or the RESOLVED observed consequence is absent
    /// (falsifier class: evidence_without_proof / incomplete comparison).
    /// No EvidenceRecorded exists without inspectable comparison evidence.
    MissingBindingInput { field: &'static str },
    /// Structurally invalid or placeholder domain identity (ENV-I1/I2),
    /// including the standalone fallback constant and the all-zero digest.
    MalformedIdentity { got: String },
    /// A required correlation field is blank or a literal placeholder.
    PlaceholderField { field: &'static str },
    /// An envelope-level field (event id, timestamp) is unusable.
    MalformedEventField { field: &'static str, got: String },
    /// A cited reference (or pinned digest) is malformed (empty, whitespace,
    /// scheme-less, or a non-SHA-256 digest).
    InvalidRef { got: String },
    /// The pinned world is not a well-formed `world_ref`, or the comparison was
    /// made in a different world than the one being stamped. Evidence is never
    /// recorded against a world other than the one it was observed in.
    WorldRef { pinned: String, comparison: String },
    /// A causal-parent id is not a well-formed event id (ENV-I4).
    MalformedParentId { got: String },
    /// The comparison inputs claim the referenced settlement RESOLVED to
    /// content that does not match the cited operational_settlement_ref.
    SettlementRefMismatch { cited: String, resolved: String },
}

impl std::fmt::Display for EmitRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingBindingInput { field } => write!(
                f,
                "no EvidenceRecorded without inspectable comparison evidence: {field} binding input missing"
            ),
            Self::MalformedIdentity { got } => write!(
                f,
                "malformed or placeholder domain identity (want 64 lowercase hex, never the fallback pseudo-hash or all-zero): {got}"
            ),
            Self::PlaceholderField { field } => {
                write!(f, "{field} is blank or a placeholder identity")
            }
            Self::MalformedEventField { field, got } => {
                write!(f, "event field {field} is unusable: {got:?}")
            }
            Self::InvalidRef { got } => write!(f, "malformed reference: {got:?}"),
            Self::WorldRef { pinned, comparison } => write!(
                f,
                "world_ref refused: pinned {pinned:?}, comparison made in {comparison:?} (both must be one well-formed pinned world)"
            ),
            Self::MalformedParentId { got } => {
                write!(f, "causal parent id is not a well-formed event id: {got:?}")
            }
            Self::SettlementRefMismatch { cited, resolved } => write!(
                f,
                "comparison inputs are internally inconsistent: cited {cited}, resolved {resolved}"
            ),
        }
    }
}

impl std::error::Error for EmitRefusal {}

/// Everything the emission needs beyond the ingested comparison facts, as
/// typed inputs so an incomplete emission is unrepresentable at the call
/// site. Event id and timestamp are explicit because this crate is pure.
#[derive(Debug, Clone)]
pub struct EvidenceEmission<'a> {
    /// THIS deployment's resolved canonical model digest — the drift anchor
    /// stamped into both `domain_model_hash` and `domain_model_ref`.
    pub local_model_sha256: &'a str,
    /// THIS deployment's pinned semantic world, stamped into the payload.
    /// Must equal the world the ingested comparison was made in.
    pub local_world_ref: &'a str,
    /// The work request of the tracked cycle (must be the SAME id the T07
    /// ingestion bound; composition tests drive both surfaces from one chain).
    pub work_request_id: &'a str,
    /// The affordance GodSpeed-Agent navigated to (E1 lineage).
    pub affordance_id: &'a str,
    /// The ProofCompleted event id RealityTrace actually ingested — cited as
    /// causal parent (ENV-I4).
    pub proof_event_id: &'a str,
    /// Fresh event id for THIS EvidenceRecorded envelope.
    pub event_id: &'a str,
    /// Caller-supplied RFC3339 timestamp (purity: no clock in this crate).
    pub occurred_at: &'a str,
    /// The exact context bundle identity from the proof lineage
    /// (E7 `context_bundle_ref`), carried into E8 so the evidence lineage can
    /// recover the observer representation used for the act. Optional: the
    /// bundle is context, never proof that retrieved claims were true.
    pub context_bundle_ref: Option<&'a Value>,
}

fn is_canonical_digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn well_formed_event_id(id: &str) -> bool {
    if uuid::Uuid::parse_str(id).is_ok() {
        return true;
    }
    // ULID shape: 26 Crockford base32 characters.
    id.len() == 26
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'A'..=b'H' | b'J' | b'K' | b'M' | b'N' | b'P'..=b'T' | b'V' | b'W' | b'X' | b'Z'))
}

fn check_identity_digest(declared: &str) -> Result<(), EmitRefusal> {
    if !is_canonical_digest(declared)
        || declared == FALLBACK_PSEUDO_HASH
        || declared == "0".repeat(64)
    {
        return Err(EmitRefusal::MalformedIdentity {
            got: declared.to_string(),
        });
    }
    Ok(())
}

fn check_identity_string(field: &'static str, value: &str) -> Result<(), EmitRefusal> {
    let trimmed = value.trim();
    if trimmed.is_empty() || PLACEHOLDER_IDS.contains(&trimmed.to_ascii_lowercase().as_str()) {
        return Err(EmitRefusal::PlaceholderField { field });
    }
    Ok(())
}

fn validate_ref(r: &str) -> Result<(), EmitRefusal> {
    let trimmed = r.trim();
    if trimmed.is_empty() || trimmed.contains(char::is_whitespace) {
        return Err(EmitRefusal::InvalidRef { got: r.to_string() });
    }
    match trimmed.strip_prefix("sha256:") {
        Some(digest) if is_canonical_digest(digest) => Ok(()),
        _ => Err(EmitRefusal::InvalidRef { got: r.to_string() }),
    }
}

/// Bare lowercase-hex SHA-256 over canonical JSON — the fingerprint recorded
/// inside `difference` (deliberately NOT a `sha256:` reference: these digest
/// INLINE values, they do not resolve to external artifacts).
fn bare_digest(value: &Value) -> String {
    let prefixed = sha256_hex(canonical_json(value).as_bytes());
    prefixed.trim_start_matches("sha256:").to_string()
}

/// Bare lowercase-hex SHA-256 over raw bytes.
fn bare_sha256_hex(bytes: &[u8]) -> String {
    sha256_hex(bytes).trim_start_matches("sha256:").to_string()
}

/// Derive the frozen `difference` fact from the two compared values. No
/// invention: the verdict is exactly canonical-JSON equivalence, and both
/// digests let any consumer recompute the comparison from the cited,
/// content-addressed upstream artifacts.
fn derive_difference(expected: &Value, observed: &Value) -> Value {
    let expected_digest = bare_digest(expected);
    let observed_digest = bare_digest(observed);
    json!({
        "verdict": if expected_digest == observed_digest { "equivalent" } else { "divergent" },
        "expected_sha256": expected_digest,
        "observed_sha256": observed_digest,
    })
}

/// Emit the canonical v1 `EvidenceRecorded` envelope stamped `realitytrace`,
/// carrying ALL NINE frozen required fields, projected ONLY from the T07
/// comparison facts:
///
/// - `expected_outcome`: VERBATIM as ingested from ProofCompleted;
/// - `observed_outcome`: VERBATIM from the RESOLVED OperationalSettlement's
///   observed effects — absent means the comparison never resolved, which
///   REFUSES emission (no evidence without inspectable comparison);
/// - `difference`: derived (verdict + both canonical digests), never claimed;
/// - `proof_result_ref`: content digest of the exact ingested ProofCompleted
///   envelope bytes (out-of-band digest pinning where the substrate permits);
/// - `operational_settlement_ref`: verbatim upstream (already content-
///   addressed);
/// - `domain_model_ref`: OUR resolved model identity (namespace + hash).
pub fn emit_evidence_recorded(
    comparison: &ComparisonInputs,
    emission: &EvidenceEmission<'_>,
) -> Result<Value, EmitRefusal> {
    // 1. Identity gates on what we would stamp.
    check_identity_digest(emission.local_model_sha256)?;
    check_identity_string("work_request_id", emission.work_request_id)?;
    check_identity_string("affordance_id", emission.affordance_id)?;

    if !is_world_ref(emission.local_world_ref) || emission.local_world_ref != comparison.world_ref {
        return Err(EmitRefusal::WorldRef {
            pinned: emission.local_world_ref.to_string(),
            comparison: comparison.world_ref.clone(),
        });
    }

    // 2. Envelope-level fields and causal parents must be usable.
    if !well_formed_event_id(emission.event_id.trim()) {
        return Err(EmitRefusal::MalformedEventField {
            field: "event_id",
            got: emission.event_id.to_string(),
        });
    }
    if !well_formed_event_id(emission.proof_event_id.trim()) {
        return Err(EmitRefusal::MalformedParentId {
            got: emission.proof_event_id.to_string(),
        });
    }
    if !well_formed_event_id(comparison.settlement_event_id.trim()) {
        return Err(EmitRefusal::MalformedParentId {
            got: comparison.settlement_event_id.clone(),
        });
    }
    if emission.occurred_at.trim().is_empty() {
        return Err(EmitRefusal::MalformedEventField {
            field: "occurred_at",
            got: emission.occurred_at.to_string(),
        });
    }

    // 3. Complete expected-vs-observed binding (TOOTH 1, emission half):
    //    both sides must exist and be non-null.
    if matches!(comparison.expected_outcome, Value::Null) {
        return Err(EmitRefusal::MissingBindingInput {
            field: "expected_outcome",
        });
    }
    let observed =
        comparison
            .observed_effects
            .as_ref()
            .ok_or(EmitRefusal::MissingBindingInput {
                field: "observed_outcome",
            })?;
    if matches!(observed, Value::Null) {
        return Err(EmitRefusal::MissingBindingInput {
            field: "observed_outcome",
        });
    }

    // 4. Every cited reference validates, and the comparison inputs must be
    //    internally consistent about what the settlement resolved to.
    validate_ref(&comparison.proof_envelope_sha256)?;
    validate_ref(&comparison.operational_settlement_ref)?;
    let resolved = comparison.referenced_settlement_sha256.as_deref().ok_or(
        EmitRefusal::MissingBindingInput {
            field: "referenced_settlement_resolution",
        },
    )?;
    validate_ref(resolved)?;
    if resolved != comparison.operational_settlement_ref {
        return Err(EmitRefusal::SettlementRefMismatch {
            cited: comparison.operational_settlement_ref.clone(),
            resolved: resolved.to_string(),
        });
    }

    // 5. Frozen E8 payload — verbatim projections + derived difference.
    let difference = derive_difference(&comparison.expected_outcome, observed);
    let mut payload = Map::new();
    payload.insert(
        "domain_model_hash".into(),
        Value::String(emission.local_model_sha256.trim().to_string()),
    );
    payload.insert(
        "world_ref".into(),
        Value::String(emission.local_world_ref.to_string()),
    );
    payload.insert("namespace".into(), json!(NAMESPACE));
    payload.insert(
        "work_request_id".into(),
        Value::String(emission.work_request_id.trim().to_string()),
    );
    payload.insert(
        "affordance_id".into(),
        Value::String(emission.affordance_id.trim().to_string()),
    );
    payload.insert(
        "expected_outcome".into(),
        comparison.expected_outcome.clone(),
    );
    payload.insert("observed_outcome".into(), observed.clone());
    payload.insert("difference".into(), difference);
    payload.insert(
        "proof_result_ref".into(),
        Value::String(comparison.proof_envelope_sha256.clone()),
    );
    payload.insert(
        "operational_settlement_ref".into(),
        Value::String(comparison.operational_settlement_ref.clone()),
    );
    payload.insert(
        "domain_model_ref".into(),
        json!({
            "namespace": NAMESPACE,
            "model_hash": emission.local_model_sha256.trim(),
        }),
    );
    if let Some(v) = emission.context_bundle_ref {
        payload.insert("context_bundle_ref".into(), v.clone());
    }

    // 6. Causality: the settlement and the ProofCompleted event ids — the
    //    ids RealityTrace itself bound during E7 ingestion — as caused_by
    //    parents, oldest first (ENV-I2/I4).
    let chain = vec![
        format!("domain_model_hash:{}", emission.local_model_sha256.trim()),
        format!("caused_by:{}", comparison.settlement_event_id.trim()),
        format!("caused_by:{}", emission.proof_event_id.trim()),
    ];

    // 7. Content-derived idempotency key, same producer formula shape the
    //    loop family uses (bare 64-hex over a typed prefix + payload).
    let idempotency_key = bare_sha256_hex(
        format!(
            "|EvidenceRecorded|{}",
            canonical_json(&Value::Object(payload.clone()))
        )
        .as_bytes(),
    );

    Ok(json!({
        "schema_version": SCHEMA_VERSION,
        "event_id": emission.event_id.trim(),
        "source_agent": PRODUCER,
        "event_type": EVENT_TYPE,
        "occurred_at": emission.occurred_at.trim(),
        "idempotency_key": idempotency_key,
        "payload": Value::Object(payload),
        "provenance": {
            "origin": PRODUCER,
            "chain": chain,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const MODEL: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const WORLD: &str =
        "world:demo@sha256:2222222222222222222222222222222222222222222222222222222222222222";

    fn comparison() -> ComparisonInputs {
        ComparisonInputs {
            world_ref: WORLD.into(),
            proof_envelope_sha256: format!("sha256:{}", "a".repeat(64)),
            operational_settlement_ref: format!("sha256:{}", "b".repeat(64)),
            settlement_event_id: "d77960c1-0658-4d40-b121-cb94c65c2cb6".into(),
            expected_outcome: json!({"declared": "green"}),
            observed_effects: Some(json!([{"effect": "green"}])),
            referenced_settlement_sha256: Some(format!("sha256:{}", "b".repeat(64))),
        }
    }

    fn emission<'a>() -> EvidenceEmission<'a> {
        EvidenceEmission {
            local_model_sha256: MODEL,
            local_world_ref: WORLD,
            work_request_id: "wr-t08",
            affordance_id: "aff-t08",
            proof_event_id: "0000000a-0000-4000-8000-000000000001",
            event_id: "0000000b-0000-4000-8000-000000000002",
            occurred_at: "2026-08-25T12:00:00+00:00",
            context_bundle_ref: None,
        }
    }

    #[test]
    fn unit_difference_is_derived_not_claimed() {
        let d = derive_difference(&json!({"x": 1}), &json!({"x": 1}));
        assert_eq!(d["verdict"], "equivalent");
        assert_eq!(d["expected_sha256"], d["observed_sha256"]);
        let d = derive_difference(&json!({"x": 1}), &json!({"x": 2}));
        assert_eq!(d["verdict"], "divergent");
        assert_ne!(d["expected_sha256"], d["observed_sha256"]);
    }

    #[test]
    fn unit_unresolved_observation_refuses_emission() {
        let mut c = comparison();
        c.observed_effects = None;
        c.referenced_settlement_sha256 = None;
        assert_eq!(
            emit_evidence_recorded(&c, &emission()),
            Err(EmitRefusal::MissingBindingInput {
                field: "observed_outcome"
            })
        );
    }

    #[test]
    fn unit_placeholder_identity_and_parents_refuse() {
        for pseudo in [FALLBACK_PSEUDO_HASH, "0".repeat(64).as_str(), "deadbeef"] {
            let e = EvidenceEmission {
                local_model_sha256: pseudo,
                ..emission()
            };
            assert!(
                matches!(
                    emit_evidence_recorded(&comparison(), &e),
                    Err(EmitRefusal::MalformedIdentity { .. })
                ),
                "{pseudo}"
            );
        }
        let e = EvidenceEmission {
            proof_event_id: "not-an-id",
            ..emission()
        };
        assert!(matches!(
            emit_evidence_recorded(&comparison(), &e),
            Err(EmitRefusal::MalformedParentId { .. })
        ));
    }

    #[test]
    fn unit_world_ref_is_stamped_and_must_match_the_comparison() {
        let env = emit_evidence_recorded(&comparison(), &emission()).unwrap();
        assert_eq!(env["payload"]["world_ref"], json!(WORLD));
        let other =
            "world:other@sha256:3333333333333333333333333333333333333333333333333333333333333333";
        for bad in [other, "world:demo", ""] {
            let e = EvidenceEmission {
                local_world_ref: bad,
                ..emission()
            };
            assert!(matches!(
                emit_evidence_recorded(&comparison(), &e),
                Err(EmitRefusal::WorldRef { .. })
            ));
        }
        let mut c = comparison();
        c.world_ref = String::new();
        assert!(matches!(
            emit_evidence_recorded(&c, &emission()),
            Err(EmitRefusal::WorldRef { .. })
        ));
    }

    #[test]
    fn unit_emitted_envelope_is_deterministic_and_complete() {
        let a = emit_evidence_recorded(&comparison(), &emission()).unwrap();
        let b = emit_evidence_recorded(&comparison(), &emission()).unwrap();
        assert_eq!(canonical_json(&a), canonical_json(&b));
        let obj = a.as_object().unwrap();
        assert_eq!(obj["source_agent"], json!(PRODUCER));
        assert_eq!(obj["event_type"], json!(EVENT_TYPE));
        let payload = obj["payload"].as_object().unwrap();
        for field in [
            "work_request_id",
            "affordance_id",
            "expected_outcome",
            "observed_outcome",
            "difference",
            "proof_result_ref",
            "operational_settlement_ref",
            "domain_model_ref",
        ] {
            assert!(payload.contains_key(field), "missing {field}");
        }
        let chain = obj["provenance"]["chain"].as_array().unwrap();
        assert!(chain.contains(&json!("caused_by:d77960c1-0658-4d40-b121-cb94c65c2cb6")));
        assert!(chain.contains(&json!("caused_by:0000000a-0000-4000-8000-000000000001")));
    }
}

//! Convergence plan T07 — ProofCompleted ingestion at the frozen E7 boundary
//! on the RealityTrace side (swe_seed -> realitytrace).
//!
//! The cross-repo golden fixture written by the REAL SWE_SEED emitter (see
//! `SWE_SEED/crates/swe-seed-core/tests/convergence_t07_proof_completed.rs`)
//! is ingested through this REAL gate when present. Everything else mutates
//! that wire family to prove the negative space.
//!
//! Plan teeth:
//!   TOOTH 1: a passing proof tied to a nonexistent/forged
//!            OperationalSettlement cannot be accepted;
//!   TOOTH 2: altering observed/declared data while the referenced
//!            settlement says otherwise is detected; the upstream fact is
//!            never silently replaced.

use std::sync::OnceLock;

use serde_json::{Value, json};

use sxr_core::canonical_json::{canonical_json, sha256_hex};
use sxr_core::proof_ingest::{
    Admission, ComparisonInputs, FALLBACK_PSEUDO_HASH, IngestParams, IngestState, REQUIRED_FIELDS,
    Refusal, ingest,
};

const ZERO_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const WORK_REQUEST_ID: &str = "wr-gsf-001";
/// MUST equal the world the SWE_SEED golden fixture is pinned to.
const WORLD: &str =
    "world:t07@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const OTHER_WORLD: &str =
    "world:t07@sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

/// MUST equal the producing-side digest the SWE_SEED golden fixture records.
fn model_digest() -> &'static str {
    static D: OnceLock<String> = OnceLock::new();
    D.get_or_init(|| bare_sha256(b"canonical-model-t07-proof-completion"))
}

/// Bare lowercase-hex SHA-256 (wire identity form; canonical_json's variant
/// carries the `sha256:` scheme prefix).
fn bare_sha256(input: &[u8]) -> String {
    sha256_hex(input).trim_start_matches("sha256:").to_string()
}

fn settlement_event_id() -> &'static str {
    "d77960c1-0658-4d40-b121-cb94c65c2cb6"
}

fn e4_parent_id() -> &'static str {
    "0342bcdb-8887-4cce-9ef1-692732a47513"
}

fn params<'a>() -> IngestParams<'a> {
    IngestParams {
        local_model_sha256: model_digest(),
        local_world_ref: WORLD,
        expected_work_request_id: WORK_REQUEST_ID,
        expected_settlement_event_id: settlement_event_id(),
    }
}

/// Canonical payload JSON the way the SWE_SEED producer computes its
/// content-derived idempotency key (sorted keys, compact separators).
fn producer_idempotency(payload: &Value) -> String {
    let canonical = canonical_json(payload);
    sha256_hex(format!("|ProofCompleted|{canonical}").as_bytes())
}

/// A canonical ProofCompleted in the exact v1 wire family the real emitter
/// projects (same shape as the golden fixture's `proof_completed`).
fn proof_envelope(
    event_id: &str,
    work_request_id: &str,
    proof_status: &str,
    expected_outcome: Value,
    settlement_ref: &str,
    parents: &[&str],
    model_hash: &str,
) -> Value {
    let payload = json!({
        "domain_model_hash": model_hash,
        "world_ref": WORLD,
        "namespace": "agentic_capability_loop",
        "work_request_id": work_request_id,
        "proof_result_id": "pr-t07-0001",
        "proof_contract": {
            "criterion": "convergence_t07 suites exit zero in both repos",
            "command": "just e2e-gate T07",
            "proof_type": "live",
        },
        "proof_status": proof_status,
        "expected_outcome": expected_outcome,
        "operational_settlement_ref": settlement_ref,
        "domain_model_ref": {
            "namespace": "agentic_capability_loop",
            "model_hash": model_hash,
        },
    });
    let chain: Vec<Value> =
        std::iter::once(Value::String(format!("domain_model_hash:{model_hash}")))
            .chain(
                parents
                    .iter()
                    .map(|p| Value::String(format!("caused_by:{p}"))),
            )
            .collect();
    json!({
        "schema_version": "v1",
        "event_id": event_id,
        "source_agent": "swe-seed",
        "event_type": "ProofCompleted",
        "occurred_at": "2026-08-25T12:30:00+00:00",
        "idempotency_key": producer_idempotency(&payload),
        "payload": payload,
        "provenance": {
            "origin": "swe-seed",
            "chain": chain,
        }
    })
}

fn valid_delivery(tag: u8) -> (Value, String) {
    // Returns (delivery, its cited settlement reference).
    let settlement_ref = cited_settlement_ref();
    let delivery = proof_envelope(
        &format!("0000000a-0000-4000-8000-{tag:012x}"),
        WORK_REQUEST_ID,
        "passed",
        json!({"affordance": "convergence-e2e-gate", "declared_result": "all frozen contract tests exit zero"}),
        &settlement_ref,
        &[settlement_event_id(), e4_parent_id()],
        model_digest(),
    );
    (delivery, settlement_ref)
}

fn settlement_envelope(status: &str, effects: Value) -> Value {
    let payload = json!({
        "domain_model_hash": model_digest(),
        "namespace": "agentic_capability_loop",
        "work_request_id": WORK_REQUEST_ID,
        "authority_decision_id": "dec_01T07AUTHORITY000000",
        "invocation_id": "inv_0123abcdef456789",
        "execution_status": "completed",
        "observed_effects": effects,
        "operational_settlement_status": status,
        "evidence_refs": [
            format!("sha256:{}", bare_sha256(b"effects-digest-t07")),
            "authorized_invocation:aaaa7777-2222-4333-8444-555555555555".to_string(),
            "execution_observation:bbbb7777-2222-4333-8444-555555555555".to_string(),
        ],
        "domain_model_ref": {
            "namespace": "agentic_capability_loop",
            "model_hash": model_digest(),
        },
    });
    json!({
        "schema_version": "v1",
        "event_id": settlement_event_id(),
        "source_agent": "sea-forge",
        "event_type": "OperationalSettlement",
        "occurred_at": "2026-08-25T12:00:00+00:00",
        "idempotency_key": sha256_hex(
            format!("|OperationalSettlement|{}", canonical_json(&payload)).as_bytes()
        ),
        "payload": payload,
        "provenance": {
            "origin": "sea-forge",
            "chain": [
                format!("domain_model_hash:{}", model_digest()),
                "caused_by:aaaa7777-2222-4333-8444-555555555555",
                "caused_by:bbbb7777-2222-4333-8444-555555555555",
            ]
        }
    })
}

/// The content-addressed reference the cited settlement resolves to: the
/// canonical-JSON digest of the attesting settlement envelope.
fn cited_settlement_ref() -> String {
    sxr_core::proof_ingest::content_digest(&settlement_envelope(
        "accepted",
        json!([
            {"effect": "health check green", "probe": "lb/ready"},
            {"effect": "zero-downtime observed", "window_ms": 0}
        ]),
    ))
}

// --- E7 positive path ---------------------------------------------------------

#[test]
fn t07_golden_fixture_from_the_real_swe_seed_emitter_is_ingested() {
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/t07_proof_completed.json");
    if !fixture_path.exists() {
        eprintln!(
            "SKIP: SWE_SEED producer golden fixture absent at {} (run the SWE_SEED T07 suite to regenerate)",
            fixture_path.display()
        );
        return;
    }
    let fixture: Value =
        serde_json::from_str(&std::fs::read_to_string(&fixture_path).unwrap()).unwrap();
    let proof = &fixture["proof_completed"];
    let settlement = &fixture["referenced_settlement"];
    let fx_params = IngestParams {
        local_model_sha256: fixture["domain_model_sha256"].as_str().unwrap(),
        local_world_ref: WORLD,
        expected_work_request_id: fixture["work_request_id"].as_str().unwrap(),
        expected_settlement_event_id: fixture["expected_settlement_event_id"].as_str().unwrap(),
    };
    let mut state = IngestState::new();

    let outcome = ingest(proof, Some(settlement), &fx_params, &mut state)
        .expect("REAL emitter output must be ingested");
    let Admission::First(comparison) = outcome else {
        panic!("first delivery must admit as First");
    };

    // Comparison inputs are preserved BY REFERENCE with verifiable digests.
    assert_eq!(
        comparison.proof_envelope_sha256,
        sxr_core::proof_ingest::content_digest(proof),
        "the ingested envelope's own content digest is recorded"
    );
    assert_eq!(
        comparison.operational_settlement_ref,
        proof["payload"]["operational_settlement_ref"]
            .as_str()
            .unwrap()
    );
    assert_eq!(
        comparison.settlement_event_id,
        fixture["expected_settlement_event_id"].as_str().unwrap()
    );
    assert_eq!(
        comparison.expected_outcome,
        proof["payload"]["expected_outcome"]
    );
    let referenced = comparison
        .referenced_settlement_sha256
        .expect("settlement was delivered");
    assert_eq!(
        referenced,
        sxr_core::proof_ingest::content_digest(settlement),
        "the referenced settlement's content digest resolves"
    );
    assert!(
        comparison.observed_effects.is_some(),
        "observed execution facts ride along from the REFERENCED settlement, not editable copies"
    );

    // Redelivery of the same golden delivery is consequence-free.
    let replay = ingest(proof, Some(settlement), &fx_params, &mut state).unwrap();
    assert_eq!(replay, Admission::DuplicateDelivery);
}

#[test]
fn t07_expected_outcome_is_preserved_verbatim_for_the_later_comparison() {
    let (delivery, _) = valid_delivery(0x21);
    let declared = delivery["payload"]["expected_outcome"].clone();
    let mut state = IngestState::new();
    let Admission::First(comparison) = ingest(&delivery, None, &params(), &mut state).unwrap()
    else {
        unreachable!()
    };
    assert_eq!(
        comparison.expected_outcome, declared,
        "no normalization may touch the declared expectation"
    );
    assert!(comparison.observed_effects.is_none());
    assert!(comparison.referenced_settlement_sha256.is_none());
}

// --- TOOTH 1: forged / missing / unresolvable settlement references -------------

#[test]
fn t07_tooth1_a_passing_proof_cannot_ride_a_forged_or_missing_settlement_reference() {
    // (a) Missing the required reference outright.
    let (mut delivery, _) = valid_delivery(0x31);
    delivery["payload"]
        .as_object_mut()
        .unwrap()
        .remove("operational_settlement_ref")
        .unwrap();
    let mut state = IngestState::new();
    assert!(matches!(
        ingest(&delivery, None, &params(), &mut state),
        Err(Refusal::OpaquePayload { .. })
    ));

    // (b) A passing proof bound to a NONEXISTENT settlement: the causal
    //     parent does not name the locally-known OperationalSettlement.
    let stranger = "99999999-9999-4999-8999-999999999999".to_string();
    let forged_ref = cited_settlement_ref();
    let delivery = proof_envelope(
        "0000000a-0000-4000-8000-000000000032",
        WORK_REQUEST_ID,
        "passed",
        json!("anything"),
        &forged_ref,
        &[&stranger],
        model_digest(),
    );
    let mut state = IngestState::new();
    let err = ingest(&delivery, None, &params(), &mut state).unwrap_err();
    assert!(matches!(err, Refusal::CausalityMissing { .. }), "{err}");

    // (c) The parent names the right settlement but the cited CONTENT does
    //     not resolve: the "referenced" envelope's bytes hash elsewhere.
    let tampered_settlement = {
        let mut s = settlement_envelope(
            "accepted",
            json!([{"effect": "health check green"}, {"effect": "zero-downtime observed"}]),
        );
        s["payload"]["operational_settlement_status"] = json!("rejected");
        s
    };
    let (delivery, _) = valid_delivery(0x33);
    let mut state = IngestState::new();
    let err = ingest(&delivery, Some(&tampered_settlement), &params(), &mut state).unwrap_err();
    assert!(
        matches!(err, Refusal::SettlementBindingMismatch(_)),
        "{err}"
    );
    assert!(state.is_empty(), "refusals record nothing");

    // (d) The referenced settlement is stamped by a non-authoritative
    //     component (SWE_SEED forging its executor's settlement).
    let mut forged = settlement_envelope("accepted", json!([{"effect": "health check green"}]));
    forged["source_agent"] = json!("swe-seed");
    let (delivery, _) = valid_delivery(0x34);
    let mut state = IngestState::new();
    assert!(matches!(
        ingest(&delivery, Some(&forged), &params(), &mut state),
        Err(Refusal::SettlementBindingMismatch(_))
    ));
}

#[test]
fn t07_tooth1_settlement_correlation_must_match_the_proof_cycle() {
    // A real settlement FOR ANOTHER CYCLE delivered as the reference.
    let foreign = {
        let mut s = settlement_envelope("accepted", json!([{"effect": "health check green"}]));
        s["payload"]["work_request_id"] = json!("wr-not-this-cycle");
        s["event_id"] = json!(settlement_event_id());
        s
    };
    let (delivery, _) = valid_delivery(0x41);
    let mut state = IngestState::new();
    let err = ingest(&delivery, Some(&foreign), &params(), &mut state).unwrap_err();
    assert!(
        matches!(err, Refusal::SettlementBindingMismatch(_)),
        "cross-cycle settlement binding refused: {err}"
    );

    // The proof itself correlating to another cycle is refused even earlier.
    let crossed = proof_envelope(
        "0000000a-0000-4000-8000-000000000042",
        "wr-not-this-cycle",
        "passed",
        json!("anything"),
        &cited_settlement_ref(),
        &[settlement_event_id()],
        model_digest(),
    );
    let mut state = IngestState::new();
    assert_eq!(
        ingest(&crossed, None, &params(), &mut state),
        Err(Refusal::CrossWiredWorkRequest {
            expected: WORK_REQUEST_ID.to_string(),
            got: "wr-not-this-cycle".to_string(),
        })
    );

    // And when the proof cites the FOREIGN settlement's own content digest,
    // content resolution succeeds — but the cross-cycle CORRELATION check
    // then refuses the binding on its own merits.
    let cited_foreign = sxr_core::proof_ingest::content_digest(&foreign);
    let rebound = proof_envelope(
        "0000000a-0000-4000-8000-000000000043",
        WORK_REQUEST_ID,
        "passed",
        json!("anything"),
        &cited_foreign,
        &[settlement_event_id()],
        model_digest(),
    );
    let mut state = IngestState::new();
    let err = ingest(&rebound, Some(&foreign), &params(), &mut state).unwrap_err();
    assert!(
        matches!(err, Refusal::SettlementBindingMismatch(ref r) if r.contains("correlates")),
        "cross-cycle correlation refused even under exact content resolution: {err}"
    );
}

// --- TOOTH 2: upstream facts are never silently replaced -------------------------

#[test]
fn t07_tooth2_altered_claims_are_detected_and_the_first_fact_stands() {
    let (first, _) = valid_delivery(0x51);
    let mut state = IngestState::new();
    let Admission::First(original) =
        ingest(&first, None, &params(), &mut state).expect("first claim ingests")
    else {
        unreachable!()
    };
    let recorded_len = state.len();

    // A FRESH envelope (new ids, so not a duplicate) re-claims the SAME
    // (cycle, proof result) with an altered expectation.
    let rewritten_expected = json!({"affordance": "convergence-e2e-gate", "declared_result": "REWRITTEN AFTER THE FACT"});
    let resubmitted = proof_envelope(
        "0000000a-0000-4000-8000-000000000052",
        WORK_REQUEST_ID,
        "passed",
        rewritten_expected.clone(),
        &cited_settlement_ref(),
        &[settlement_event_id()],
        model_digest(),
    );
    let err = ingest(&resubmitted, None, &params(), &mut state).unwrap_err();
    assert_eq!(
        err,
        Refusal::ClaimMismatch {
            diverged: vec!["expected_outcome"],
        },
        "the rewrite must be detected and NAMED"
    );

    // Altered proof status under the same cycle claim.
    let flipped = proof_envelope(
        "0000000a-0000-4000-8000-000000000053",
        WORK_REQUEST_ID,
        "failed",
        json!({"affordance": "convergence-e2e-gate", "declared_result": "all frozen contract tests exit zero"}),
        &cited_settlement_ref(),
        &[settlement_event_id()],
        model_digest(),
    );
    let err = ingest(&flipped, None, &params(), &mut state).unwrap_err();
    assert_eq!(
        err,
        Refusal::ClaimMismatch {
            diverged: vec!["proof_status"],
        }
    );

    // Altered settlement reference (claiming a DIFFERENT adjudicated fact).
    let rebound = proof_envelope(
        "0000000a-0000-4000-8000-000000000054",
        WORK_REQUEST_ID,
        "passed",
        json!({"affordance": "convergence-e2e-gate", "declared_result": "all frozen contract tests exit zero"}),
        "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        &[settlement_event_id()],
        model_digest(),
    );
    let err = ingest(&rebound, None, &params(), &mut state).unwrap_err();
    assert_eq!(
        err,
        Refusal::ClaimMismatch {
            diverged: vec!["operational_settlement_ref"],
        }
    );

    // Nothing moved: the recorded identities and the ORIGINAL comparison
    // inputs stand exactly as first admitted.
    assert_eq!(state.len(), recorded_len, "refusals record nothing");
    assert_eq!(
        original.expected_outcome,
        first["payload"]["expected_outcome"]
    );
    assert_eq!(original.operational_settlement_ref, cited_settlement_ref());
}

#[test]
fn t07_identical_redelivery_of_a_settled_cycle_is_consequence_free() {
    let (first, _) = valid_delivery(0x61);
    let mut state = IngestState::new();
    assert!(matches!(
        ingest(&first, None, &params(), &mut state).unwrap(),
        Admission::First(_)
    ));
    let recorded_len = state.len();

    // Exact replay under stable identity…
    let replay = ingest(&first, None, &params(), &mut state).unwrap();
    assert_eq!(replay, Admission::DuplicateDelivery);

    // …and a re-WRAPPED envelope (fresh event id) with identical claims: the
    // settled cycle already stands, so nothing is appended twice.
    let rewrapped = proof_envelope(
        "0000000a-0000-4000-8000-000000000062",
        WORK_REQUEST_ID,
        "passed",
        json!({"affordance": "convergence-e2e-gate", "declared_result": "all frozen contract tests exit zero"}),
        &cited_settlement_ref(),
        &[settlement_event_id()],
        model_digest(),
    );
    let outcome = ingest(&rewrapped, None, &params(), &mut state).unwrap();
    assert_eq!(outcome, Admission::DuplicateDelivery);
    assert_eq!(state.len(), recorded_len);
}

#[test]
fn t07_duplicate_recognition_survives_a_restart_of_the_gate_state() {
    let (first, _) = valid_delivery(0x71);
    let mut state = IngestState::new();
    assert!(matches!(
        ingest(&first, None, &params(), &mut state).unwrap(),
        Admission::First(_)
    ));

    // Simulate a restart: round-trip the state through its serialized form.
    let serialized = serde_json::to_string(&state).unwrap();
    let mut reopened: IngestState = serde_json::from_str(&serialized).unwrap();
    assert!(!reopened.is_empty(), "prior identities reload");
    let replay = ingest(&first, None, &params(), &mut reopened).unwrap();
    assert_eq!(replay, Admission::DuplicateDelivery);
}

// --- Producer authority both directions -------------------------------------------

#[test]
fn t07_wrong_producers_are_refused_in_both_directions() {
    let (base, _) = valid_delivery(0x81);

    // Any component other than swe_seed forging ProofCompleted.
    for impostor in [
        "sea-forge",
        "godspeed-agent",
        "realitytrace",
        "execution-environment",
        "mystery-component",
    ] {
        let mut forged = base.clone();
        forged["source_agent"] = json!(impostor);
        let mut state = IngestState::new();
        let err = ingest(&forged, None, &params(), &mut state).unwrap_err();
        assert!(
            matches!(err, Refusal::ProducerForge { .. }),
            "{impostor}: {err}"
        );
        assert!(state.is_empty());
    }

    // This gate ingests ONLY ProofCompleted: any other event type — including
    // RealityTrace's own downstream EvidenceRecorded — is refused outright.
    let mut other_event = base.clone();
    other_event["event_type"] = json!("EvidenceRecorded");
    let mut state = IngestState::new();
    assert_eq!(
        ingest(&other_event, None, &params(), &mut state),
        Err(Refusal::WrongType {
            expected: "ProofCompleted".to_string(),
            got: "EvidenceRecorded".to_string(),
        })
    );
}

// --- Identity gates ----------------------------------------------------------------

#[test]
fn t07_placeholder_and_drifted_model_identities_fail_closed() {
    let (base, _) = valid_delivery(0x91);

    for pseudo in [FALLBACK_PSEUDO_HASH.to_string(), ZERO_HASH.to_string()] {
        let mut tampered = base.clone();
        tampered["payload"]["domain_model_hash"] = json!(pseudo);
        tampered["payload"]["domain_model_ref"]["model_hash"] = json!(pseudo.clone());
        let mut state = IngestState::new();
        let err = ingest(&tampered, None, &params(), &mut state).unwrap_err();
        assert!(
            matches!(err, Refusal::PlaceholderIdentity { ref got } if *got == pseudo),
            "{pseudo}: {err}"
        );
    }

    // Structurally malformed identity.
    let mut malformed = base.clone();
    malformed["payload"]["domain_model_hash"] = json!("deadbeef");
    let mut state = IngestState::new();
    assert!(matches!(
        ingest(&malformed, None, &params(), &mut state),
        Err(Refusal::MalformedIdentity { .. })
    ));

    // Drift against OUR locally-resolved model.
    let mut drifted = base.clone();
    drifted["payload"]["domain_model_hash"] =
        json!(bare_sha256(b"a-model-realitytrace-does-not-resolve"));
    let mut state = IngestState::new();
    assert!(matches!(
        ingest(&drifted, None, &params(), &mut state),
        Err(Refusal::HashDrift { .. })
    ));

    // domain_model_ref rewriting the declared identity (projection ≠ rewrite).
    let mut rewritten = base.clone();
    rewritten["payload"]["domain_model_ref"]["model_hash"] = json!(bare_sha256(b"another-model"));
    let mut state = IngestState::new();
    assert!(matches!(
        ingest(&rewritten, None, &params(), &mut state),
        Err(Refusal::RefIdentityMismatch { .. })
    ));
}

// --- Field battery / namespace-schema tampering -------------------------------------

#[test]
fn t07_every_required_field_is_validated_with_the_complete_battery() {
    for omitted in REQUIRED_FIELDS {
        let (mut delivery, _) = valid_delivery(0xA0);
        delivery["payload"]
            .as_object_mut()
            .unwrap()
            .remove(omitted)
            .unwrap_or_else(|| panic!("{omitted} existed"));
        let mut state = IngestState::new();
        assert_eq!(
            ingest(&delivery, None, &params(), &mut state),
            Err(Refusal::OpaquePayload {
                missing: vec![omitted],
            }),
            "omitting {omitted} must be refused"
        );
        assert!(state.is_empty(), "refusals record nothing");
    }

    // Typed-field violations.
    for (field, value) in [
        ("proof_status", json!("success")),
        ("proof_contract", json!("attested verbally")),
        ("proof_contract", json!({"criterion": ""})),
        ("artifact_refs", json!({"a": 1})),
        ("artifact_refs", json!(["not-a-ref"])),
        ("proof_evidence_refs", json!([42])),
        ("trace_root", json!("has whitespace")),
        ("output_ref", json!("")),
    ] {
        let (mut delivery, _) = valid_delivery(0xA1);
        if field == "proof_contract" && value.as_object().is_some() {
            delivery["payload"]["proof_contract"] = value;
        } else {
            delivery["payload"][field] = value;
        }
        let mut state = IngestState::new();
        let err = ingest(&delivery, None, &params(), &mut state).unwrap_err();
        assert!(
            matches!(
                err,
                Refusal::InvalidProofStatus { .. }
                    | Refusal::OpaquePayload { .. }
                    | Refusal::InvalidOptionalField { .. }
                    | Refusal::InvalidRef { .. }
                    | Refusal::PlaceholderField { .. }
            ),
            "{field}: {err}"
        );
    }

    // Blank correlation identities: empty is "missing", placeholder literals
    // are refused as placeholders (T06 battery convention).
    let (mut delivery, _) = valid_delivery(0xA2);
    delivery["payload"]["proof_result_id"] = json!("");
    let mut state = IngestState::new();
    assert_eq!(
        ingest(&delivery, None, &params(), &mut state),
        Err(Refusal::OpaquePayload {
            missing: vec!["proof_result_id"],
        })
    );
    for placeholder in ["placeholder", "<missing>", "tbd"] {
        let (mut delivery, _) = valid_delivery(0xA2);
        delivery["payload"]["proof_result_id"] = json!(placeholder);
        let mut state = IngestState::new();
        assert!(
            matches!(
                ingest(&delivery, None, &params(), &mut state),
                Err(Refusal::PlaceholderField { .. })
            ),
            "placeholder proof_result_id {placeholder:?} refused"
        );
    }
}

#[test]
fn t07_namespace_and_schema_tampering_is_refused() {
    let (base, _) = valid_delivery(0xB0);

    // Payload-level namespace swap.
    let mut tampered = base.clone();
    tampered["payload"]["namespace"] = json!("some_other_namespace");
    let mut state = IngestState::new();
    assert!(matches!(
        ingest(&tampered, None, &params(), &mut state),
        Err(Refusal::WrongNamespace { .. })
    ));

    // Namespace swap inside domain_model_ref.
    let mut tampered = base.clone();
    tampered["payload"]["domain_model_ref"]["namespace"] = json!("other");
    let mut state = IngestState::new();
    assert!(matches!(
        ingest(&tampered, None, &params(), &mut state),
        Err(Refusal::WrongNamespace { .. })
    ));

    // Schema-version tampering.
    let mut tampered = base.clone();
    tampered["schema_version"] = json!("v2");
    let mut state = IngestState::new();
    assert!(matches!(
        ingest(&tampered, None, &params(), &mut state),
        Err(Refusal::MalformedEnvelope(_))
    ));

    // Malformed causal-parent id smuggled into the provenance chain.
    let mut tampered = base.clone();
    tampered["provenance"]["chain"] = json!([
        "caused_by:not-an-id",
        format!("caused_by:{}", settlement_event_id())
    ]);
    let mut state = IngestState::new();
    assert!(matches!(
        ingest(&tampered, None, &params(), &mut state),
        Err(Refusal::CausalityMissing { .. })
    ));

    // Non-object deliveries fail closed.
    let mut state = IngestState::new();
    assert!(matches!(
        ingest(&json!([1, 2, 3]), None, &params(), &mut state),
        Err(Refusal::MalformedEnvelope(_))
    ));
}

// --- Honest characterization of the reference-resolution boundary -------------------

#[test]
fn t07_without_the_delivered_settlement_the_gate_binds_to_locally_known_identity_only() {
    // When the transport delivers ONLY the ProofCompleted (no settlement
    // bundle), causality + correlation + every other rule still bind; what
    // the gate CANNOT do is verify the cited content digest against actual
    // settlement bytes. That limitation is surfaced on the comparison inputs
    // (referenced_settlement_sha256 stays None) instead of being papered over.
    let (delivery, _) = valid_delivery(0xC0);
    let mut state = IngestState::new();
    let Admission::First(comparison) =
        ingest(&delivery, None, &params(), &mut state).expect("admits without the bundle")
    else {
        unreachable!()
    };
    let ComparisonInputs {
        observed_effects,
        referenced_settlement_sha256,
        ..
    } = *comparison;
    assert!(observed_effects.is_none());
    assert!(referenced_settlement_sha256.is_none());

    // And when the settlement IS delivered but cites different bytes than the
    // proof claims, resolution fails closed (already proven by tooth 1c).
}

// --- CEP-0008 world_ref ---------------------------------------------------------

/// Re-stamp the world on an otherwise valid delivery (idempotency key is
/// recomputed so only the world differs).
fn with_world(mut delivery: Value, world: Option<&str>) -> Value {
    let payload = delivery["payload"].as_object_mut().unwrap();
    match world {
        Some(w) => payload.insert("world_ref".into(), json!(w)),
        None => payload.remove("world_ref"),
    };
    let key = producer_idempotency(&delivery["payload"]);
    delivery["idempotency_key"] = json!(key);
    delivery
}

#[test]
fn world_ref_is_carried_into_the_comparison_inputs() {
    let (delivery, _) = valid_delivery(1);
    let mut state = IngestState::new();
    let Admission::First(c) = ingest(&delivery, None, &params(), &mut state).unwrap() else {
        panic!("first")
    };
    assert_eq!(c.world_ref, WORLD);
}

#[test]
fn a_delivery_without_a_pinned_world_is_refused() {
    let (delivery, _) = valid_delivery(2);
    for bad in [
        None,
        Some("world:t07"),
        Some("t07"),
        Some("world:t07@sha256:abc"),
        Some(""),
    ] {
        let mut state = IngestState::new();
        let r = ingest(
            &with_world(delivery.clone(), bad),
            None,
            &params(),
            &mut state,
        );
        assert!(
            matches!(
                r,
                Err(sxr_core::proof_ingest::Refusal::MalformedWorldRef { .. })
            ),
            "{bad:?} -> {r:?}"
        );
        assert!(state.is_empty(), "a refused delivery settles nothing");
    }
}

#[test]
fn a_delivery_from_another_world_is_refused_as_drift() {
    let (delivery, _) = valid_delivery(3);
    let mut state = IngestState::new();
    let r = ingest(
        &with_world(delivery, Some(OTHER_WORLD)),
        None,
        &params(),
        &mut state,
    );
    assert!(
        matches!(
            r,
            Err(sxr_core::proof_ingest::Refusal::WorldRefDrift { .. })
        ),
        "{r:?}"
    );
    assert!(state.is_empty());
}

// --- Bound producer: frozen security acceptance ---------------------------------

/// A cycle bound to the frozen security-acceptance producer admits only that
/// stamp. The SWE_SEED harness and, above all, the actor under test
/// (`sea-forge`) are refused, and the admitted comparison names its producer.
/// Default `ingest` keeps the exclusive `swe_seed` authority.
#[test]
fn t07_bound_security_acceptance_producer_is_exclusive_for_its_cycles() {
    use sxr_core::proof_ingest::{Admission, ProofProducer, ingest_from};
    let (base, _) = valid_delivery(0x91);
    let mut acceptance = base.clone();
    acceptance["source_agent"] = json!("sea-forge-security-acceptance");

    let mut state = IngestState::new();
    match ingest_from(
        ProofProducer::SecurityAcceptance,
        &acceptance,
        None,
        &params(),
        &mut state,
    ) {
        Ok(Admission::First(comparison)) => {
            assert_eq!(comparison.proof_producer, "security_acceptance")
        }
        other => panic!("acceptance proof not admitted: {other:?}"),
    }

    for impostor in ["swe-seed", "sea-forge", "realitytrace", "mystery-component"] {
        let mut forged = base.clone();
        forged["source_agent"] = json!(impostor);
        let mut state = IngestState::new();
        let err = ingest_from(
            ProofProducer::SecurityAcceptance,
            &forged,
            None,
            &params(),
            &mut state,
        )
        .unwrap_err();
        assert!(
            matches!(err, Refusal::ProducerForge { .. }),
            "{impostor}: {err}"
        );
        assert!(state.is_empty());
    }

    // The default gate still refuses the acceptance stamp.
    let mut state = IngestState::new();
    assert!(matches!(
        ingest(&acceptance, None, &params(), &mut state),
        Err(Refusal::ProducerForge { .. })
    ));
}

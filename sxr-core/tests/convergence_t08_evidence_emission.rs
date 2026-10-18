//! Convergence plan T08 — canonical EvidenceRecorded emission at the frozen
//! E8 boundary on the RealityTrace side (realitytrace -> godspeed_agent).
//!
//! Everything exercises the REAL composed chain: a ProofCompleted delivery is
//! ingested through the REAL T07 gate (`proof_ingest::ingest`), and ONLY the
//! returned [`ComparisonInputs`] may drive the REAL E8 emitter
//! (`evidence_emit::emit_evidence_recorded`). Expected, observed, difference,
//! and every cited reference are projected from those ingested facts — never
//! invented. When this suite runs it REGENERATES the cross-repo golden
//! fixture consumed by godspeed_agent's Python gate test (T03/T06/T07
//! pattern, direction reversed).
//!
//! Plan teeth exercised here (emission basis):
//!   TOOTH 1 basis half: no EvidenceRecorded exists when the expected or the
//!            RESOLVED observed consequence is missing;
//!   TOOTH 2 basis half: every cited ref is content-addressed/pinned, so the
//!            emitter cannot fabricate lineage — parents are the exact ids
//!            the E7 gate bound.

use serde_json::{Value, json};

use sxr_core::canonical_json::{canonical_json, sha256_hex};
use sxr_core::evidence_emit::{
    EmitRefusal, EvidenceEmission, FALLBACK_PSEUDO_HASH, emit_evidence_recorded,
};
use sxr_core::proof_ingest::{Admission, ComparisonInputs, IngestParams, IngestState, ingest};

const ZERO_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const WORLD: &str =
    "world:t07@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const WORK_REQUEST_ID: &str = "wr-gsf-001";
const AFFORDANCE_ID: &str = "aff-blue-green-001";
const PROOF_EVENT_ID: &str = "0000000a-0000-4000-8000-00000000c001";
const EVIDENCE_EVENT_ID: &str = "0000000b-0000-4000-8000-00000000c002";
const SETTLEMENT_EVENT_ID: &str = "d77960c1-0658-4d40-b121-cb94c65c2cb6";
const OCCURRED_AT: &str = "2026-08-25T13:05:00+00:00";

/// MUST equal the producing-side digest the SWE_SEED golden fixture records.
fn model_digest() -> String {
    bare_sha256(b"canonical-model-t07-proof-completion")
}

/// Bare lowercase-hex SHA-256 (wire identity form).
fn bare_sha256(input: &[u8]) -> String {
    sha256_hex(input).trim_start_matches("sha256:").to_string()
}

fn params<'a>() -> IngestParams<'a> {
    static MODEL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    IngestParams {
        local_model_sha256: MODEL.get_or_init(model_digest),
        local_world_ref: WORLD,
        expected_work_request_id: WORK_REQUEST_ID,
        expected_settlement_event_id: SETTLEMENT_EVENT_ID,
    }
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
        ],
        "domain_model_ref": {
            "namespace": "agentic_capability_loop",
            "model_hash": model_digest(),
        },
    });
    json!({
        "schema_version": "v1",
        "event_id": SETTLEMENT_EVENT_ID,
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

/// The canonical ProofCompleted in the same v1 wire family the real SWE_SEED
/// emitter projects (mirrors the T07 ingestion test helpers).
fn proof_envelope(expected_outcome: Value) -> Value {
    let settlement_ref = cited_settlement_ref();
    let payload = json!({
        "domain_model_hash": model_digest(),
        "world_ref": WORLD,
        "namespace": "agentic_capability_loop",
        "work_request_id": WORK_REQUEST_ID,
        "proof_result_id": "pr-t07-0001",
        "proof_contract": {
            "criterion": "convergence_t07 suites exit zero in both repos",
            "command": "just e2e-gate T07",
            "proof_type": "live",
        },
        "proof_status": "passed",
        "expected_outcome": expected_outcome,
        "operational_settlement_ref": settlement_ref,
        "domain_model_ref": {
            "namespace": "agentic_capability_loop",
            "model_hash": model_digest(),
        },
    });
    let chain: Vec<Value> = vec![
        json!(format!("domain_model_hash:{}", model_digest())),
        json!(format!("caused_by:{SETTLEMENT_EVENT_ID}")),
    ];
    json!({
        "schema_version": "v1",
        "event_id": PROOF_EVENT_ID,
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

fn producer_idempotency(payload: &Value) -> String {
    sha256_hex(format!("|ProofCompleted|{}", canonical_json(payload)).as_bytes())
}

fn cited_settlement_ref() -> String {
    sxr_core::proof_ingest::content_digest(&settlement_envelope(
        "accepted",
        json!([
            {"effect": "health check green", "probe": "lb/ready"},
            {"effect": "zero-downtime observed", "window_ms": 0}
        ]),
    ))
}

fn declared_expectation() -> Value {
    json!({"affordance": "convergence-e2e-gate", "declared_result": "all frozen contract tests exit zero"})
}

fn emission_for<'a>(model: &'a str) -> EvidenceEmission<'a> {
    EvidenceEmission {
        local_model_sha256: model,
        local_world_ref: WORLD,
        work_request_id: WORK_REQUEST_ID,
        affordance_id: AFFORDANCE_ID,
        proof_event_id: PROOF_EVENT_ID,
        event_id: EVIDENCE_EVENT_ID,
        occurred_at: OCCURRED_AT,
        context_bundle_ref: None,
    }
}

/// Ingest the proof through the REAL T07 gate WITH its referenced settlement
/// resolved, then hand ONLY the returned comparison facts to the REAL E8
/// emitter. Returns (envelope, comparison, cited settlement ref).
fn emit_from_real_chain() -> (Value, ComparisonInputs, String) {
    let model = model_digest();
    let proof = proof_envelope(declared_expectation());
    let settlement = settlement_envelope(
        "accepted",
        json!([
            {"effect": "health check green", "probe": "lb/ready"},
            {"effect": "zero-downtime observed", "window_ms": 0}
        ]),
    );
    let mut state = IngestState::new();
    let Admission::First(comparison) = ingest(&proof, Some(&settlement), &params(), &mut state)
        .expect("REAL T07 ingestion must admit the composed delivery")
    else {
        panic!("first delivery must admit as First");
    };
    let emission = EvidenceEmission {
        local_model_sha256: &model,
        local_world_ref: WORLD,
        work_request_id: WORK_REQUEST_ID,
        affordance_id: AFFORDANCE_ID,
        proof_event_id: PROOF_EVENT_ID,
        event_id: EVIDENCE_EVENT_ID,
        occurred_at: OCCURRED_AT,
        context_bundle_ref: None,
    };
    let envelope = emit_evidence_recorded(&comparison, &emission)
        .expect("REAL E8 emission must succeed from resolved comparison inputs");
    (envelope, *comparison, cited_settlement_ref())
}

// --- Positive path: one real composed chain drives everything --------------------

#[test]
fn t08_emission_from_real_ingested_comparison_carries_all_nine_frozen_fields() {
    let (envelope, comparison, settlement_ref) = emit_from_real_chain();

    assert_eq!(envelope["schema_version"], json!("v1"));
    assert_eq!(envelope["event_type"], json!("EvidenceRecorded"));
    // Exclusive producer authority: realitytrace, nobody else.
    assert_eq!(envelope["source_agent"], json!("realitytrace"));
    assert_eq!(envelope["provenance"]["origin"], json!("realitytrace"));

    let payload = envelope["payload"].as_object().unwrap();
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
        assert!(payload.contains_key(field), "missing frozen field {field}");
    }
    assert!(
        envelope["event_id"].as_str().is_some_and(|s| !s.is_empty()),
        "event_id (ninth frozen required field) present at envelope level"
    );

    // Verbatim projection, no invention: expected comes from the proof,
    // observed from the RESOLVED settlement's observed effects.
    assert_eq!(payload["expected_outcome"], json!(declared_expectation()));
    assert_eq!(
        payload["observed_outcome"],
        json!([
            {"effect": "health check green", "probe": "lb/ready"},
            {"effect": "zero-downtime observed", "window_ms": 0}
        ])
    );
    assert_eq!(comparison.expected_outcome, payload["expected_outcome"]);

    // References are pinned where the substrate permits: proof_result_ref is
    // the content digest of the EXACT ingested ProofCompleted bytes;
    // operational_settlement_ref travels verbatim upstream.
    assert_eq!(
        payload["proof_result_ref"],
        json!(comparison.proof_envelope_sha256)
    );
    assert_eq!(payload["operational_settlement_ref"], json!(settlement_ref));

    // Causality cites the ids RealityTrace itself bound during E7 ingestion.
    let chain = envelope["provenance"]["chain"].as_array().unwrap();
    assert!(chain.contains(&json!(format!("caused_by:{SETTLEMENT_EVENT_ID}"))));
    assert!(chain.contains(&json!(format!("caused_by:{PROOF_EVENT_ID}"))));

    // The derived difference is cryptographically inspectable: both digests
    // recompute from the projected values, and the verdict follows.
    let difference = &payload["difference"];
    let expected_digest = sha256_hex(canonical_json(&payload["expected_outcome"]).as_bytes())
        .trim_start_matches("sha256:")
        .to_string();
    let observed_digest = sha256_hex(canonical_json(&payload["observed_outcome"]).as_bytes())
        .trim_start_matches("sha256:")
        .to_string();
    assert_eq!(difference["expected_sha256"], json!(expected_digest));
    assert_eq!(difference["observed_sha256"], json!(observed_digest));
    assert_eq!(difference["verdict"], json!("divergent"));

    // Identity: OUR resolved model, namespace intact, ref agrees with hash.
    assert_eq!(payload["domain_model_hash"], json!(model_digest()));
    assert_eq!(payload["namespace"], json!("agentic_capability_loop"));
    assert_eq!(
        payload["domain_model_ref"]["model_hash"],
        json!(model_digest())
    );
    assert_eq!(
        payload["domain_model_ref"]["namespace"],
        json!("agentic_capability_loop")
    );

    // Content-derived idempotency key rides along for downstream dedup.
    assert_eq!(envelope["idempotency_key"].as_str().map(str::len), Some(64));
}

#[test]
fn t08_emission_is_deterministic_under_fixed_inputs() {
    let (a, _, _) = emit_from_real_chain();
    let (b, _, _) = emit_from_real_chain();
    assert_eq!(
        canonical_json(&a),
        canonical_json(&b),
        "pure emitter: identical comparison inputs and emission parameters must produce byte-identical envelopes"
    );
}

// --- TOOTH 1 (emission half): no evidence without inspectable comparison ---------

#[test]
fn t08_tooth1_no_emission_exists_when_the_observed_consequence_never_resolved() {
    // Ingest WITHOUT delivering the referenced settlement: T07 admits the
    // proof but observed_effects stay unresolved — exactly the state in which
    // an EvidenceRecorded would have NO inspectable observed consequence.
    let proof = proof_envelope(declared_expectation());
    let mut state = IngestState::new();
    let Admission::First(comparison) =
        ingest(&proof, None, &params(), &mut state).expect("admits without the bundle")
    else {
        unreachable!()
    };
    assert!(comparison.observed_effects.is_none());
    let model = model_digest();
    let emission = emission_for(&model);
    let err = emit_evidence_recorded(&comparison, &emission).unwrap_err();
    assert_eq!(
        err,
        EmitRefusal::MissingBindingInput {
            field: "observed_outcome"
        },
        "no EvidenceRecorded without an inspectable observed consequence: {err}"
    );
}

#[test]
fn t08_tooth1_missing_or_null_expected_binding_refuses_emission() {
    let (_, mut comparison, _) = emit_from_real_chain();
    comparison.expected_outcome = json!(null);
    let model = model_digest();
    let emission = emission_for(&model);
    assert_eq!(
        emit_evidence_recorded(&comparison, &emission),
        Err(EmitRefusal::MissingBindingInput {
            field: "expected_outcome"
        })
    );
}

// --- Identity gates / placeholder / malformed references -------------------------

#[test]
fn t08_identity_placeholders_and_malformed_refs_refuse_before_any_envelope_exists() {
    let (_, comparison, _) = emit_from_real_chain();

    // Placeholder pseudo-hashes can never be stamped as domain identity.
    for pseudo in [FALLBACK_PSEUDO_HASH, ZERO_HASH] {
        let refused = emit_evidence_recorded(&comparison, &emission_for(pseudo)).unwrap_err();
        assert!(
            matches!(refused, EmitRefusal::MalformedIdentity { ref got } if got == pseudo),
            "{pseudo}: {refused}"
        );
    }

    // Drift guard: a structurally invalid identity is refused.
    assert!(matches!(
        emit_evidence_recorded(&comparison, &emission_for("deadbeef")),
        Err(EmitRefusal::MalformedIdentity { .. })
    ));

    // Blank / placeholder correlation fields refuse.
    let model = model_digest();
    for (field, value) in [("work_request_id", ""), ("affordance_id", "tbd")] {
        let emission = EvidenceEmission {
            work_request_id: if field == "work_request_id" {
                value
            } else {
                WORK_REQUEST_ID
            },
            affordance_id: if field == "affordance_id" {
                value
            } else {
                AFFORDANCE_ID
            },
            ..emission_for(&model)
        };
        assert!(
            matches!(
                emit_evidence_recorded(&comparison, &emission),
                Err(EmitRefusal::PlaceholderField { .. })
            ),
            "{field}={value:?} must refuse"
        );
    }

    // Forged/malformed upstream references refuse before anything exists.
    let mut forged_proof_ref = comparison.clone();
    forged_proof_ref.proof_envelope_sha256 = "sha256:not-a-digest".into();
    assert!(matches!(
        emit_evidence_recorded(&forged_proof_ref, &emission_for(&model)),
        Err(EmitRefusal::InvalidRef { .. })
    ));
    let mut forged_settlement_ref = comparison;
    forged_settlement_ref.operational_settlement_ref = "path/to/whatever".into();
    assert!(matches!(
        emit_evidence_recorded(&forged_settlement_ref, &emission_for(&model)),
        Err(EmitRefusal::InvalidRef { .. })
    ));
}

#[test]
fn t08_inconsistent_resolution_inputs_refuse_even_when_individually_well_formed() {
    let (_, mut comparison, _) = emit_from_real_chain();
    comparison.referenced_settlement_sha256 = Some(sxr_core::proof_ingest::content_digest(
        &json!({"other": "bytes"}),
    ));
    let model = model_digest();
    let emission = emission_for(&model);
    let err = emit_evidence_recorded(&comparison, &emission).unwrap_err();
    assert!(
        matches!(err, EmitRefusal::SettlementRefMismatch { .. }),
        "cited-vs-resolved divergence must refuse: {err}"
    );
}

// --- Cross-repo golden fixture ----------------------------------------------------

#[test]
fn t08_regenerates_the_cross_repo_golden_fixture_for_the_python_gate() {
    let (envelope, _, _) = emit_from_real_chain();

    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/t08_evidence_recorded.json");
    let body = json!({
        "domain_model_sha256": model_digest(),
        "world_ref": WORLD,
        "work_request_id": WORK_REQUEST_ID,
        "affordance_id": AFFORDANCE_ID,
        "proof_event_id": PROOF_EVENT_ID,
        "expected_settlement_event_id": SETTLEMENT_EVENT_ID,
        "known_parent_ids": [SETTLEMENT_EVENT_ID, PROOF_EVENT_ID],
        "evidence_recorded": envelope,
    });
    if let Some(parent) = fixture_path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(
        &fixture_path,
        format!("{}\n", serde_json::to_string_pretty(&body).unwrap()),
    )
    .expect("write golden fixture");
    println!("regenerated {}", fixture_path.display());

    // Round-trip: what the Python gate receives decodes identically.
    let decoded: Value =
        serde_json::from_str(&std::fs::read_to_string(&fixture_path).unwrap()).unwrap();
    assert_eq!(
        decoded["evidence_recorded"]["event_type"],
        "EvidenceRecorded"
    );
    assert_eq!(decoded["evidence_recorded"]["source_agent"], "realitytrace");
    assert_eq!(
        decoded["evidence_recorded"]["payload"]["work_request_id"],
        WORK_REQUEST_ID
    );
}

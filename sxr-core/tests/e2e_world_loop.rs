//! End-to-end world loop, RealityTrace's hop (driven by sea-rs
//! `scripts/e2e-world-loop.sh`). Ingests the real E7 (SWE_SEED) with its real E6
//! (SEA-Forge) and emits the E8 EvidenceRecorded, all in the one pinned
//! `E2E_WORLD_REF`. Skipped unless the driver sets `E2E_DIR`.

use std::path::PathBuf;

use serde_json::Value;
use sxr_core::evidence_emit::{EvidenceEmission, emit_evidence_recorded};
use sxr_core::proof_ingest::{Admission, IngestParams, IngestState, ingest};

#[test]
fn hop_5_evidence_from_the_real_e7_and_e6() {
    let Some(dir) = std::env::var_os("E2E_DIR").map(PathBuf::from) else {
        eprintln!("SKIP: E2E_DIR not set");
        return;
    };
    let world = std::env::var("E2E_WORLD_REF").expect("E2E_WORLD_REF");
    let hash = std::env::var("E2E_DOMAIN_HASH").expect("E2E_DOMAIN_HASH");
    let wr_id = std::env::var("E2E_WORK_REQUEST_ID").expect("E2E_WORK_REQUEST_ID");
    let read = |name: &str| -> Value {
        serde_json::from_str(&std::fs::read_to_string(dir.join(name)).unwrap()).unwrap()
    };
    let e7 = read("e7_proof_completed.json");
    let e6 = read("e6_operational_settlement.json");
    let e1 = read("e1_work_requested.json");
    let settlement_id = e6["event_id"].as_str().unwrap();

    let mut state = IngestState::new();
    let Admission::First(comparison) = ingest(
        &e7,
        Some(&e6),
        &IngestParams {
            local_model_sha256: &hash,
            local_world_ref: &world,
            expected_work_request_id: &wr_id,
            expected_settlement_event_id: settlement_id,
        },
        &mut state,
    )
    .expect("the real E7 is ingested against the real E6") else {
        panic!("first delivery admits as First");
    };
    assert_eq!(comparison.world_ref, world);

    // A well-formed UUID derived from the proof event, so reruns are deterministic.
    let h = sxr_core::canonical_json::sha256_hex(e7["event_id"].as_str().unwrap().as_bytes());
    let h = h.trim_start_matches("sha256:");
    let evidence_id = format!(
        "{}-{}-4{}-8{}-{}",
        &h[0..8],
        &h[8..12],
        &h[13..16],
        &h[17..20],
        &h[20..32]
    );
    let e8 = emit_evidence_recorded(
        &comparison,
        &EvidenceEmission {
            local_model_sha256: &hash,
            local_world_ref: &world,
            work_request_id: &wr_id,
            affordance_id: e1["payload"]["affordance_id"].as_str().unwrap(),
            proof_event_id: e7["event_id"].as_str().unwrap(),
            event_id: &evidence_id,
            occurred_at: "2026-10-05T12:30:00+00:00",
        },
    )
    .expect("the evidence is emitted");
    assert_eq!(e8["payload"]["world_ref"], world.as_str());

    // A world other than the pinned one is refused at ingestion, settling nothing.
    let other = format!("world:other@sha256:{}", "d".repeat(64));
    let mut fresh = IngestState::new();
    let refused = ingest(
        &e7,
        Some(&e6),
        &IngestParams {
            local_model_sha256: &hash,
            local_world_ref: &other,
            expected_work_request_id: &wr_id,
            expected_settlement_event_id: settlement_id,
        },
        &mut fresh,
    );
    assert!(refused.is_err() && fresh.is_empty());

    std::fs::write(
        dir.join("e8_evidence_recorded.json"),
        serde_json::to_string_pretty(&e8).unwrap() + "\n",
    )
    .unwrap();
}

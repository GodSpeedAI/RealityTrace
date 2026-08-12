"""Deterministic successor freeze and predecessor-pilot compatibility tests."""
from __future__ import annotations

import dataclasses
import json

import pytest

from eval.v2 import evidence_revision_002 as science
from eval.v2.evidence_revision_002_freeze import (
    FULL_MATRIX_NAME, build_artifacts, runner_implementation_sha256)
from eval.v2.evidence_revision_002_run import (
    ExecutionGate, GateRequest, load_frozen_full_matrix, run_authorized_full)
from eval.v2.evidence_revision_run import ExecutionRefused


def test_successor_identity_and_lineage_are_new_and_exact():
    doc = science.freeze()
    assert doc["experiment_id"] == "evidence-revision-002"
    assert doc["design_fingerprint"] != science.PREDECESSOR["predecessor_design_fingerprint"]
    assert science.PREDECESSOR["predecessor_pilot_manifest_sha256"] == (
        "ae2827c1ea32c2860e53f19b763955908860865266791932f1454252ffeb2a13")
    assert doc["lineage"]["pilot_disposition"] == (
        "VALID_PREDECESSOR_INSTRUMENT_VALIDATION")
    assert doc["pilot"]["confirmatory_sample_overlap_permitted"] is False
    assert doc["pilot_outcomes_consulted_for_successor_parameters"] is False


def test_only_requested_endpoint_semantics_move():
    doc = science.freeze()
    revision = doc["secondary_endpoints"]["revision_rate"]
    assert revision == science.REVISION_RATE
    assert "matched_evidence_use" not in doc["secondary_endpoints"]
    unavailable = doc["unavailable_endpoints"]["matched_evidence_use"]
    assert unavailable["status"] == "NOT_EVALUATED_UNDER_THIS_EXPERIMENT"
    assert unavailable["value"] is None
    assert unavailable["derived_proxy_permitted"] is False
    assert unavailable["utilization_claim_permitted"] is False
    assert unavailable["absence_is_zero_utilization"] is False
    assert doc["secondary_endpoints"]["response_mode"] == (
        "modal share and entropy — labelled POST-HOC, not a test")


def test_revision_rate_typed_rule_is_exact():
    rule = science.REVISION_RATE
    assert rule["reporting_unit"] == "case_repetition"
    assert rule["null_round1"] == "EXCLUDE_NO_SETTLEMENT_NO_CLAIM"
    assert rule["unknown_round1"].startswith("INCLUDE_AS_WRONG")
    assert rule["unresolved_distinction"] == "STATE_METADATA_NOT_A_CLAIM"
    assert rule["final_claim"] == "gate_blind_last_model_claim"
    assert rule["zero_denominator"] == "NOT_ESTIMABLE"


def test_primary_science_and_comparison_families_are_preserved():
    assert science.PRIMARY_POPULATION["n_cases"] == 43
    assert science.STATISTICAL_PLAN["primary_unit"] == "case"
    assert "ties resolved as incorrect" in science.STATISTICAL_PLAN["repetition_handling"]
    expected = {
        "C1_evidence_effect": (("R0", "R1"), "primary"),
        "C2_legibility_governed": (("R1", "R2"), "primary"),
        "C3_legibility_upper": (("R3", "R4"), "primary"),
        "C4_coverage_headroom": (("R1", "R3"), "secondary"),
    }
    for name, (arms, family) in expected.items():
        assert science.COMPARISONS[name]["arms"] == arms
        assert science.COMPARISONS[name]["family"] == family
    assert "TWO" in science.COMPARISONS["C4_coverage_headroom"]["declared_differences"]
    assert "EXEMPT_UPPER_BOUND" in science.COMPARISONS["C4_coverage_headroom"]["constraint"]


def test_owner_state_is_frozen_and_unauthorized():
    owner = science.OWNER_STATE
    assert owner["status"] == "FROZEN_NOT_AUTHORIZED"
    assert owner["execution_authorized"] is False
    assert owner["additional_provider_calls_authorized"] is False
    assert owner["full_execution_authorized"] is False
    assert owner["release_token_issued"] is False
    assert owner["release_marker_issued"] is False


def test_full_matrix_is_complete_unique_and_not_authorized():
    docs = build_artifacts()
    matrix = docs[FULL_MATRIX_NAME]
    assert matrix["n_cases"] == 43
    assert matrix["n_arms"] == 5
    assert matrix["repetitions"] == 2
    assert matrix["total_units"] == 430
    assert len(matrix["unit_ids"]) == len(set(matrix["unit_ids"])) == 430
    assert matrix["execution_authorized"] is False
    assert matrix["predecessor_pilot_units_included"] is False
    validation = docs["evidence-revision-002-full-matrix-validation.json"]
    assert set(validation["checks"].values()) == {"PASS"}


def test_fresh_executable_runner_and_matrix_identities():
    docs = build_artifacts()
    exe = docs["evidence-revision-002-executable-contract.json"]
    lineage = docs["evidence-revision-002-lineage.json"]
    assert exe["executable_contract_fingerprint"] != (
        science.PREDECESSOR["predecessor_executable_fingerprint"])
    assert runner_implementation_sha256() != (
        science.PREDECESSOR["predecessor_runner_implementation_sha256"])
    assert lineage["full_matrix_sha256"] == docs[FULL_MATRIX_NAME]["matrix_sha256"]
    assert lineage["case_inputs_sha256"] == (
        science.PREDECESSOR["predecessor_case_inputs_sha256"])


def test_predecessor_pilot_compatibility_is_behavioral_not_asserted():
    compat = build_artifacts()[
        "evidence-revision-002-predecessor-pilot-compatibility.json"]
    assert compat["status"] == "PREDECESSOR_PILOT_ACCEPTED"
    assert compat["outcome_artifacts_read"] is False
    assert compat["behavioral_deltas"] == []
    assert compat["execution_vectors_compared"] == 60
    assert compat["execution_vectors_equal"] is True
    assert compat["counterfactual_new_experiment_seed_vectors_changed"] > 0
    assert compat["predecessor_revision_002_scientific_artifact_verified"] is True
    assert all(row["byte_equal"] for row in compat["runtime_source_equality"])
    assert compat["pilot_rerun_required"] is False
    assert compat["evidence_selection_seed_identity"] == "evidence-revision-001"


def test_gate_refuses_before_any_client_or_release_artifact(tmp_path):
    docs = build_artifacts()
    matrix_doc = docs[FULL_MATRIX_NAME]
    matrix_path = tmp_path / "matrix.json"
    matrix_path.write_text(json.dumps(matrix_doc, indent=2, sort_keys=True) + "\n")
    matrix = load_frozen_full_matrix(matrix_path)
    request = GateRequest(matrix=matrix, run_id="forbidden", run_root=tmp_path)
    with pytest.raises(ExecutionRefused, match="FULL release token missing"):
        ExecutionGate.validate_static(request)




def test_static_gate_rejects_manually_constructed_noncanonical_matrix(tmp_path):
    docs = build_artifacts()
    matrix_path = tmp_path / "matrix.json"
    matrix_path.write_text(json.dumps(docs[FULL_MATRIX_NAME], sort_keys=True))
    matrix = load_frozen_full_matrix(matrix_path)
    forged = dataclasses.replace(matrix, repetitions=3)
    with pytest.raises(ExecutionRefused, match="not the frozen successor FULL matrix"):
        ExecutionGate.validate_static(
            GateRequest(matrix=forged, run_id="forged", run_root=tmp_path))


def test_live_client_factory_is_unreachable_without_static_authority(tmp_path):
    docs = build_artifacts()
    matrix_path = tmp_path / "matrix.json"
    matrix_path.write_text(json.dumps(docs[FULL_MATRIX_NAME], sort_keys=True))
    matrix = load_frozen_full_matrix(matrix_path)
    called = False

    def factory():
        nonlocal called
        called = True
        raise AssertionError("client construction must be unreachable")

    with pytest.raises(ExecutionRefused):
        run_authorized_full(
            GateRequest(matrix=matrix, run_id="forbidden", run_root=tmp_path),
            factory)
    assert called is False


def test_design_fingerprint_excludes_governance_only():
    before = science.contract_fingerprint()
    old = science.OWNER_STATE["status"]
    try:
        science.OWNER_STATE["status"] = "SYNTHETIC_GOVERNANCE_CHANGE"
        assert science.contract_fingerprint() == before
    finally:
        science.OWNER_STATE["status"] = old

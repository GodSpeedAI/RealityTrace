"""Contract-driven successor analysis regression tests."""
import json

import pytest

from eval.v2.evidence_revision_002_analysis import (
    EndpointNotEvaluated, aggregate_case_primary, comparison_specs,
    matched_evidence_use, paired_case_table, reconstruct_committed_full,
    revision_rate, secondary_endpoint_status)


def test_case_primary_majority_and_tie_incorrect():
    rows = [
        {"case_id": "a", "arm": "R0", "repetition": 1, "final_gate_blind_correct": True},
        {"case_id": "a", "arm": "R0", "repetition": 2, "final_gate_blind_correct": False},
        {"case_id": "b", "arm": "R0", "repetition": 1, "final_gate_blind_correct": True},
        {"case_id": "b", "arm": "R0", "repetition": 2, "final_gate_blind_correct": True},
    ]
    agg = aggregate_case_primary(rows, allow_partial=True)
    assert agg[("a", "R0")]["R_correct"] is False
    assert agg[("a", "R0")]["tie_resolved_as_incorrect"] is True
    assert agg[("b", "R0")]["R_correct"] is True


def test_revision_rate_null_unknown_and_wrong_typed_semantics():
    rows = [
        # null: excluded
        {"arm": "R0", "round1_proposed_settlement": None,
         "evaluator_true_locus": "implementation",
         "final_gate_blind_claim": "implementation"},
        # explicit unknown against determinate truth: denominator + revision
        {"arm": "R0", "round1_proposed_settlement": "unknown",
         "evaluator_true_locus": "implementation",
         "final_gate_blind_claim": "implementation"},
        # wrong concrete, unchanged: denominator only
        {"arm": "R0", "round1_proposed_settlement": "projection",
         "evaluator_true_locus": "implementation",
         "final_gate_blind_claim": "projection"},
        # initially correct: excluded even if later changed
        {"arm": "R0", "round1_proposed_settlement": "implementation",
         "evaluator_true_locus": "implementation",
         "final_gate_blind_claim": "projection"},
    ]
    result = revision_rate(rows, allow_partial=True)["R0"]
    assert result == {"numerator": 1, "denominator": 2,
                      "rate": 0.5, "status": "ESTIMATED"}
    assert revision_rate([], allow_partial=True)["R1"]["status"] == "NOT_ESTIMABLE"
    assert revision_rate([], allow_partial=True)["R1"]["rate"] is None


def test_unresolved_metadata_does_not_create_a_claim():
    rows = [{"arm": "R1", "round1_proposed_settlement": None,
             "round1_unresolved_distinction": "which path",
             "evaluator_true_locus": "authority",
             "final_gate_blind_claim": "authority"}]
    assert revision_rate(rows, allow_partial=True)["R1"]["denominator"] == 0


def test_matched_use_is_not_evaluated_and_never_proxied():
    assert secondary_endpoint_status("matched_evidence_use") == (
        "NOT_EVALUATED_UNDER_THIS_EXPERIMENT")
    with pytest.raises(EndpointNotEvaluated, match="NOT_EVALUATED"):
        matched_evidence_use([{"held_evidence": True, "correct": True}])


def test_comparisons_are_contract_derived_and_paired_at_case_level():
    assert comparison_specs()["C3_legibility_upper"]["arms"] == ("R3", "R4")
    rows = []
    for arm, values in {"R0": [False, False], "R1": [True, True]}.items():
        for rep, value in enumerate(values, 1):
            rows.append({"case_id": "x", "arm": arm, "repetition": rep,
                         "final_gate_blind_correct": value})
    table = paired_case_table(rows, "C1_evidence_effect", allow_partial=True)
    assert table["family"] == "primary"
    assert table["R1_only"] == 1


def test_canonical_analysis_rejects_partial_or_duplicate_full_rows():
    partial = [{"case_id": "ag-04", "arm": "R0", "repetition": 1,
                "final_gate_blind_correct": False,
                "round1_proposed_settlement": None,
                "evaluator_true_locus": "authority",
                "final_gate_blind_claim": "authority"}]
    with pytest.raises(ValueError, match="matrix mismatch"):
        aggregate_case_primary(partial)
    with pytest.raises(ValueError, match="matrix mismatch"):
        revision_rate(partial)
    with pytest.raises(ValueError, match="duplicate"):
        aggregate_case_primary(partial + partial)



def test_primary_rejects_truthy_non_boolean_fixture():
    rows = [{"case_id": "x", "arm": "R0", "repetition": 1,
             "final_gate_blind_correct": "false"}]
    with pytest.raises(ValueError, match="must be boolean"):
        aggregate_case_primary(rows, allow_partial=True)


def test_committed_result_reconstruction_verifies_exact_matrix_and_ledger(tmp_path):
    from eval.v2.evidence_revision_002_freeze import FULL_MATRIX_NAME, build_artifacts
    from eval.v2.ledger import CommitLedger, CommitViolation

    matrix = build_artifacts()[FULL_MATRIX_NAME]
    ledger = CommitLedger(str(tmp_path / "result-commits.jsonl"))
    coordinates = [(case, arm, rep)
                   for case in matrix["cases"] for arm in matrix["arms"]
                   for rep in range(1, matrix["repetitions"] + 1)]
    payloads = {}
    for uid, (case, arm, rep) in zip(matrix["unit_ids"], coordinates):
        doc = {
            "unit_id": uid, "case_id": case, "condition": arm, "rep": rep,
            "last_model_claim": "unknown", "executions": [],
            "rounds": [{"decision": {
                "proposed_settlement": None,
                "unresolved_distinction": "synthetic offline fixture"}}],
            "runner_contract": {
                "experiment_id": "evidence-revision-002",
                "design_fingerprint": matrix["design_fingerprint"],
                "executable_contract_fingerprint": matrix[
                    "executable_contract_fingerprint"],
                "full_matrix_sha256": matrix["matrix_sha256"]},
        }
        if uid == matrix["unit_ids"][0]:
            doc["rounds"] = []
            doc["terminal_state"] = "MODEL_OUTPUT_CONTRACT_FAILURE"
            doc["last_model_claim"] = None
        payload = json.dumps(doc, sort_keys=True)
        payloads[uid] = payload
        ledger.commit_raw(uid, str(tmp_path / "results" / f"{uid}.json"), payload)
    rows = reconstruct_committed_full(tmp_path, matrix)
    assert len(rows) == 430
    assert rows[0]["arm"] == matrix["arms"][0]
    assert rows[0]["round1_proposed_settlement"] is None
    assert rows[0]["output_class"] == "MODEL_OUTPUT_CONTRACT_FAILURE"
    orphan_call_ledger = CommitLedger(str(tmp_path / "call-commits.jsonl"))
    orphan_call_ledger.commit_raw(
        "orphan:call:001", str(tmp_path / "raw" / "orphan.json"), "{}")
    with pytest.raises(ValueError, match="call ledger.*orphan"):
        reconstruct_committed_full(tmp_path, matrix)
    (tmp_path / "call-commits.jsonl").unlink()
    first = matrix["unit_ids"][0]
    first_path = tmp_path / "results" / f"{first}.json"
    first_path.write_text("{}")
    with pytest.raises(CommitViolation, match="hash mismatch"):
        reconstruct_committed_full(tmp_path, matrix)
    ledger.commit_raw(first, str(first_path), payloads[first])
    first_path.unlink()
    with pytest.raises(ValueError, match="result set mismatch"):
        reconstruct_committed_full(tmp_path, matrix)

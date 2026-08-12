"""Contract-driven successor analysis regression tests."""
import pytest

from eval.v2.evidence_revision_002_analysis import (
    EndpointNotEvaluated, aggregate_case_primary, comparison_specs,
    matched_evidence_use, paired_case_table, revision_rate,
    secondary_endpoint_status)


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

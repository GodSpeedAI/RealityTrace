"""R-12/R-14/R-17: the rescorer must be read-only and must reproduce the
historical numbers where the definitions coincide."""

import json
import os

import pytest

ANALYSIS = os.path.join(os.path.dirname(os.path.dirname(
    os.path.dirname(os.path.abspath(__file__)))), "analysis", "post-audit-v2")


def _load(name):
    p = os.path.join(ANALYSIS, name)
    if not os.path.exists(p):
        pytest.skip("rescore output not generated in this environment")
    with open(p) as f:
        return json.load(f)


def test_rescore_declares_zero_model_calls_and_no_mutation():
    idx = _load("index.json")
    assert idx["model_calls_performed"] == 0
    assert idx["historical_artifacts_modified"] == []
    assert idx["source_artifacts"], "source artifacts must be hash-bound"


def test_s231_reproduces_the_historical_per_arm_counts():
    d = _load("s231-retrieval-ablation.json")
    hist = {"A": 30, "B": 26, "C": 21, "D": 26}
    for arm, n in hist.items():
        assert d["arms"][arm]["accuracy_as_historically_reported"]["numerator"] == n
    assert d["all_conditional_cis_overlap"] is True
    assert d["unparseable_spread_ratio"] > 6


def test_governed_reproduces_the_historical_gated_accuracy():
    d = _load("governed.json")
    for cond, num in (("V", 28), ("G", 17), ("O", 60)):
        m = d["conditions"][cond]["gated_accuracy_feasible_determinate"]
        assert (m["numerator"], m["denominator"]) == (num, 129)


def test_governed_adds_the_gate_blind_measurement_the_audit_required():
    d = _load("governed.json")
    g = d["conditions"]["G"]
    assert g["gate_blind_model_claim_accuracy_lower_bound"]["numerator"] >= 17
    assert g["gate_suppressed_correct_claims_lower_bound"]["numerator"] >= 1


def test_matched_evidence_use_shows_no_oracle_advantage():
    d = _load("governed.json")
    m = d["matched_evidence_use_O_vs_G"]
    assert m["n_matched_pairs"] == 19
    assert m["G"]["rate"] == m["O"]["rate"]


def test_p_values_are_never_reported_as_zero():
    d = _load("governed.json")
    for cmp_ in d["paired"].values():
        for level in cmp_.values():
            assert level["p"] is None or level["p"] > 0


def test_acquisition_decomposition_separates_denial_from_choice():
    d = _load("acquisition.json")
    dec = d["D2_acquisition_decomposition"]
    assert dec["historically_reported_no_acquisition"]["numerator"] == 146
    assert dec["denied_after_parse_failure"]["numerator"] >= 100
    assert dec["clean_immediate_settlement"]["numerator"] == 40
    assert (dec["acquisition_rate_conditional_on_valid_action_contract"]["rate"]
            > dec["acquired"]["rate"])
    assert d["UNRECOVERABLE_FROM_HISTORICAL_EVIDENCE"]["units"] >= 100


def test_response_mode_analysis_is_labelled_post_hoc():
    d = _load("acquisition.json")
    for cond in ("D0", "D1", "D2"):
        rm = d["conditions"][cond]["response_mode_all"]
        assert rm["label"] == "POST-HOC RESPONSE-MODE ANALYSIS"
    assert d["conditions"]["D1"]["response_mode_determinate"]["modal_answer"] == "unknown"
    assert d["conditions"]["D1"]["response_mode_determinate"]["modal_share"] > 0.9


def test_hash_provenance_records_unverifiable_without_inventing_hashes():
    d = _load("hash-provenance.json")
    assert d["hash_verified"] == 13
    assert d["hash_unverifiable"] == 527
    assert d["hash_verified_and_recomputed_ok"] == 13
    assert len(d["unverifiable_unit_ids"]) == 527


def test_corpus_adjudication_quarantines_nothing_and_rewrites_no_truth():
    d = _load("corpus-adjudication.json")
    assert d["n_flagged"] == 3
    assert d["n_quarantined"] == 0
    assert d["ground_truth_modified"] is False
    assert {e["case_id"] for e in d["flagged_cases"]} == {"ag-10", "sr-07", "df-06"}
    assert all(e["verdict"] == "VALID_DESPITE_MISSING_SIGNATURE"
               for e in d["flagged_cases"])


def test_construct_divergence_is_non_empty_in_both_directions():
    d = _load("construct-divergence.json")
    assert ["rerun_verifier_isolated", "verifier"] in d["evaluator_only"]
    assert ["inspect_verifier_config", "environment"] in d["gate_only"]
    assert d["actions_never_evaluator_discriminating"] == ["inspect_verifier_config"]


def test_exclusion_topology_records_the_corrected_f9_diagnosis():
    d = _load("exclusion-topology.json")
    assert d["legacy_contract"]["reproduces_frozen_topology"] is True
    assert d["proposed_contract_at_frozen_budget"]["reachable"] is False
    assert d["proposed_contract_at_frozen_budget"]["premise_cost"] == 13
    assert d["frontier_summary"]["any_with_zero_false_positives"] is False
    assert "NOT ADOPTED" in d["adjudication"]["status"]

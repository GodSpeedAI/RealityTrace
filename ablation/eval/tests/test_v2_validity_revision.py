"""Offline validity closure proofs for evidence-revision-001 revision 002."""
from __future__ import annotations

import inspect
import json
from pathlib import Path

import pytest

from ..v2.case_inputs_v2 import (build_case_inputs_v2, covariates_for,
                                 initial_state_for, load_case_inputs)
from ..v2.channel import ActionCatalogue, ObservationChannel, UnsafeDisclosure, unit_seed
from ..v2.evidence_revision import ARMS, OWNER_STATE
from ..v2.evidence_revision_run import (
    EXPECTED_DESIGN_FINGERPRINT, EXPECTED_EXECUTABLE_FINGERPRINT,
    REVISED_PILOT_MATRIX, DeterministicFakeClient, ExecutionRefused,
    inspect_matrix, load_frozen_matrix, run_offline_unit)
from ..v2.legibility import render_legible, render_raw
from ..v2.ids import unit_id
from ..v2.marker_neutralization import (MARKER_RE, REPLACEMENT,
                                        assert_marker_free, neutralize_payload,
                                        neutralize_text)
from ..v2.replication import COSTS, PROBES
from ..v2.validity_revision import (
    DIRECT_CANDIDATE_UNIVERSE, EVIDENCE_POLICY, FULL_NAMESPACE, K,
    LEAKAGE_POLICY_VERSION, MAX_PAYMENT, PILOT_NAMESPACE, PILOT_REPETITIONS,
    PRIMARY_CASES, build_artifacts, evidence_seed, feasibility_report,
    leakage_report, neutralized_observations, pair_equality_report, pilot_cases,
    selected_delivery, validate_revision_artifacts)


def test_predecessor_artifacts_remain_byte_bound():
    root = Path(__file__).parents[2] / "analysis" / "post-audit-v2"
    old = json.loads((root / "case-inputs-v1.json").read_text())
    old_matrix = json.loads((root / "evidence-revision-001-pilot-matrix.json").read_text())
    assert old["case_inputs_sha256"] == "521dd42ff152b564be2a1d3c4daca9a80538b5ec734cb0d973b9f110b42a9dd9"
    assert old_matrix["matrix_sha256"] == "7a16e55f45c11110452baaf8c448c946f9b7fdb6c269677f58f69828fa17d32b"


def test_marker_neutralizer_is_deterministic_idempotent_and_marker_free():
    source = "FAILED test_corpus_scenario_test_missing_fixture: behavior stayed"
    a, hits_a = neutralize_text(source)
    b, hits_b = neutralize_text(source)
    assert (a, hits_a) == (b, hits_b)
    assert neutralize_text(a)[0] == a
    assert REPLACEMENT in a and "behavior stayed" in a
    assert not MARKER_RE.search(a)


def test_marker_neutralizer_api_and_imports_are_key_locus_model_blind():
    import eval.v2.marker_neutralization as module
    source = inspect.getsource(module)
    assert tuple(inspect.signature(neutralize_text).parameters) == ("text",)
    assert tuple(inspect.signature(neutralize_payload).parameters) == ("payload",)
    for forbidden in ("governed_keys", "hidden_meta", "true_locus",
                      "model_output", "discriminating_probes"):
        assert forbidden not in source


def test_all_primary_model_visible_inputs_and_probe_payloads_are_marker_free():
    inputs = load_case_inputs(); observations, _ = neutralized_observations()
    for case in PRIMARY_CASES:
        state = initial_state_for(case, inputs)
        assert_marker_free(state["question"])
        assert_marker_free(state["initial_evidence"])
        assert_marker_free(observations[case])
        for payload in observations[case].values():
            assert_marker_free(render_raw(payload, 10**9))
            assert_marker_free(render_legible(payload, 10**9))


def test_marker_covariates_are_non_model_visible_and_arm_invariant():
    inputs = load_case_inputs()
    for case in PRIMARY_CASES:
        visible = initial_state_for(case, inputs)
        covariates = covariates_for(case, inputs)
        assert set(visible) == {"question", "initial_evidence", "binding_sha256"}
        assert not set(covariates) & set(visible)
        for _arm in ARMS:
            assert initial_state_for(case, inputs) == visible


def test_known_marker_mutation_is_caught():
    with pytest.raises(ValueError, match="survived"):
        assert_marker_free({"summary": "test_corpus_scenario_config_env"})


def test_complete_feasibility_enumeration_proves_every_case():
    report = feasibility_report()
    assert report["all_cases_feasible"] is True
    assert report["structurally_unaffordable_cases"] == []
    assert len(report["case_spaces"]) == 43
    assert all(x["complete_space_size"] == 21 for x in report["case_spaces"])
    assert all(len(x["complete_candidate_sets"]) == 21
               for x in report["case_spaces"])


def test_every_masked_delivery_is_exact_three_distinct_and_affordable():
    observations, _ = neutralized_observations()
    hidden = json.loads((Path(__file__).parents[2] / "corpus" /
                         "acquisition-hidden-meta.json").read_text())
    for case in PRIMARY_CASES:
        for rep in range(1, PILOT_REPETITIONS + 1):
            _, delivery = selected_delivery(case, rep, "R1", observations, hidden)
            assert len(delivery.delivered_probes) == len(set(delivery.delivered_probes)) == K
            assert delivery.payment_charged <= MAX_PAYMENT
            assert delivery.observations


def test_masked_leakage_is_actual_nonzero_bounded_and_has_no_missingness():
    report = leakage_report()
    assert report["declared_k"] == 3
    assert report["realized_k_distribution"] == {3: 86}
    assert report["realized_membership_leakage_bits"] == {
        "mean": 1.16448, "maximum": 1.222392, "minimum": 0.807355}
    assert report["singleton_equivalent_count"] == 0
    assert report["unaffordable_count"] == report["missingness_count"] == 0
    assert report["cost_range"] == [3, 5]
    assert report["channels"]["FULL_membership"]["bits"] == 0
    assert report["channels"]["NONE_membership"]["bits"] == 0


def test_pair_hash_proofs_are_exact_and_rendering_only():
    report = pair_equality_report()
    for pair in ("R1_vs_R2", "R3_vs_R4"):
        proof = report[pair]
        assert proof["pairs"] == 86
        assert proof["underlying_evidence_equal"] is True
        assert proof["rendered_hash_different"] is True
        assert proof["mismatch_count"] == 0
        for row in proof["rows"]:
            assert row["evidence_set_hash_a"] == row["evidence_set_hash_b"]
            assert row["semantic_evidence_hash_a"] == row["semantic_evidence_hash_b"]
            assert row["raw_rendered_hash"] != row["legible_rendered_hash"]


def test_arm_name_no_longer_affects_masked_membership_or_handles():
    case, rep = "cep-14", 2
    observations, _ = neutralized_observations()
    hidden = json.loads((Path(__file__).parents[2] / "corpus" /
                         "acquisition-hidden-meta.json").read_text())
    ca, da = selected_delivery(case, rep, "R1", observations, hidden)
    cb, db = selected_delivery(case, rep, "R2", observations, hidden)
    assert da.delivered_probes == db.delivered_probes
    assert [o.handle for o in da.observations] == [o.handle for o in db.observations]
    # The predecessor arm-keyed behavior is deliberately different: mutation
    # regression proves reintroducing it would break the pair.
    old_a = unit_seed("evidence-revision-001", PILOT_NAMESPACE, case, "R1", rep)
    old_b = unit_seed("evidence-revision-001", PILOT_NAMESPACE, case, "R2", rep)
    assert old_a != old_b


def test_seed_and_handle_assignment_accept_no_locus_or_key():
    params = tuple(inspect.signature(evidence_seed).parameters)
    assert params == ("case_id", "rep", "policy")
    assert "true_locus" not in inspect.getsource(evidence_seed)
    a = ActionCatalogue(PROBES, evidence_seed("cep-07", 1, "MASKED_K"))
    b = ActionCatalogue(PROBES, evidence_seed("cep-07", 1, "MASKED_K"))
    assert [a.handle_of(p) for p in PROBES] == [b.handle_of(p) for p in PROBES]


def test_pilot_selection_is_deterministic_public_only(monkeypatch):
    import eval.v2.validity_revision as revision
    def forbidden(*_a, **_k):
        raise AssertionError("sealed data reached pilot selection")
    monkeypatch.setattr(revision, "_sealed_keys", forbidden)
    monkeypatch.setattr(revision, "_sealed_hidden_meta", forbidden)
    expected = ("cep-07", "df-11", "ag-04", "sr-10", "df-06", "cep-05")
    assert revision.pilot_cases() == revision.pilot_cases() == expected


def test_pilot_matrix_is_deterministic_complete_and_stage_disjoint():
    a = inspect_matrix("pilot"); b = inspect_matrix("pilot")
    assert a == b and a["total_units"] == 60 and len(set(a["uids"])) == 60
    full = {unit_id("evidence-revision-001", FULL_NAMESPACE, case, arm, rep,
                    EXPECTED_EXECUTABLE_FINGERPRINT)
            for case in a["cases"] for arm in a["arms"]
            for rep in range(1, a["repetitions"] + 1)}
    assert set(a["uids"]).isdisjoint(full)


def test_matrix_hash_comes_from_executable_contract_not_owner_state():
    matrix = load_frozen_matrix(REVISED_PILOT_MATRIX, "pilot",
                                {"pilot_matrix_hash": "malicious-owner-value"})
    assert matrix.matrix_hash == inspect_matrix("pilot")["matrix_sha256"]
    assert matrix.repetitions == 2 and matrix.namespace == PILOT_NAMESPACE


def test_runner_uses_frozen_case_input_and_persists_non_prompt_covariates(tmp_path):
    class CapturingFake(DeterministicFakeClient):
        def __init__(self):
            super().__init__(); self.prompts = []
        def complete(self, system, user, requested_max_tokens, temperature):
            self.prompts.append(user)
            return super().complete(system, user, requested_max_tokens, temperature)
    fake = CapturingFake()
    result, _ = run_offline_unit(tmp_path, fake,
                                 case_id="cep-14", arm="R1", rep=1)
    committed = json.loads((tmp_path / "results" / f"{result.unit_id}.json").read_text())
    meta = committed["runner_contract"]
    assert meta["case_input_binding_sha256"] == load_case_inputs()["case_inputs_sha256"]
    assert meta["source_marker_present"] is True
    assert meta["marker_neutralization_applied"] is True
    assert meta["marker_type"] == "INJECTED_TEST_IDENTIFIER"
    assert meta["masked_k_realized_k"] == 3
    assert meta["masked_k_implied_cost"] <= 7
    assert meta["leakage_policy_version"] == LEAKAGE_POLICY_VERSION
    raw_prompts = "\n".join(fake.prompts)
    assert "source_marker_present" not in raw_prompts
    assert "test_corpus_scenario_" not in raw_prompts


def test_runner_validates_new_executable_fingerprint_and_remains_forbidden():
    report = inspect_matrix("pilot")
    assert report["design_fingerprint"] == EXPECTED_DESIGN_FINGERPRINT
    assert report["executable_contract_fingerprint"] == EXPECTED_EXECUTABLE_FINGERPRINT
    assert OWNER_STATE["execution_authorized"] is True
    assert OWNER_STATE["authorized_stage"] == "pilot"
    assert OWNER_STATE["full_execution_authorized"] is False


def test_revision_modules_have_no_network_or_provider_transport_import():
    root = Path(__file__).parents[1] / "v2"
    for name in ("validity_revision.py", "case_inputs_v2.py",
                 "marker_neutralization.py"):
        text = (root / name).read_text()
        assert "urllib" not in text and "requests" not in text
        assert "ZaiCodingPlanClient" not in text


@pytest.mark.parametrize("mutation, message", [
    ("singleton", "exactly K=3"),
    ("overbudget", "over-budget"),
    ("missing", "missingness"),
    ("pair", "paired evidence mismatch"),
])
def test_contract_mutations_fail_closed(mutation, message):
    artifacts = build_artifacts()
    if mutation == "singleton":
        artifacts["evidence-revision-001-masked-k-feasibility-v2.json"]["rows"][0]["selected_k"] = 1
    elif mutation == "overbudget":
        artifacts["evidence-revision-001-masked-k-feasibility-v2.json"]["rows"][0]["selected_cost"] = 9
    elif mutation == "missing":
        artifacts["evidence-revision-001-masked-k-leakage-v2.json"]["missingness_count"] = 1
    else:
        artifacts["evidence-revision-001-pair-equality-v2.json"]["R1_vs_R2"]["underlying_evidence_equal"] = False
    with pytest.raises(AssertionError, match=message):
        validate_revision_artifacts(artifacts)


def test_arm_specific_seed_mutation_breaks_required_pair():
    case, rep = "cep-14", 1
    sa = unit_seed("evidence-revision-001", PILOT_NAMESPACE, case, "R1", rep)
    sb = unit_seed("evidence-revision-001", PILOT_NAMESPACE, case, "R2", rep)
    assert sa != sb
    assert evidence_seed(case, rep, EVIDENCE_POLICY["R1"]) == evidence_seed(
        case, rep, EVIDENCE_POLICY["R2"])

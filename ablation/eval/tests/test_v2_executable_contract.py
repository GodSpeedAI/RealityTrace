"""Guards on the EXECUTABLE completion of `evidence-revision-001`.

Everything here is offline. No test in this file may perform a model call or a
provider call, and two of them prove that no import path to one exists.

The tests fall into three groups:

* the four previously-unspecified execution inputs are now determined, so a
  runner has nothing left to invent;
* the frozen SCIENCE did not move while that happened;
* the four blockers found during completion are declared rather than quietly
  repaired — a test that would pass only if a blocker had been fixed is a test
  that would let the fix happen unnoticed.
"""

import inspect
import json
from pathlib import Path

import pytest

from ..v2 import case_inputs as ci
from ..v2 import evidence_revision as er
from ..v2 import executable_contract as xc
from ..v2.channel import DisclosurePolicy
from ..v2.controller import arm_difference_report
from ..v2.dstate import DiagnosticState, missing_state_sections
from ..v2.ids import unit_id


@pytest.fixture(scope="module")
def inputs():
    return ci.load_case_inputs()


@pytest.fixture(scope="module")
def report():
    return xc.answer_channel_report()


# ── nothing here can reach a provider ────────────────────────────────────────

def test_neither_module_imports_a_network_transport():
    forbidden = ("provider", "zai_client", "requests", "httpx", "urllib",
                 "http.client", "socket")
    for module in (xc, ci):
        imports = [line.strip() for line in
                   inspect.getsource(module).splitlines()
                   if line.lstrip().startswith(("import ", "from "))]
        for line in imports:
            assert not any(name in line for name in forbidden), line


def test_emitted_documents_record_zero_calls():
    doc = xc.executable_contract()
    assert doc["model_calls_performed"] == 0
    assert doc["provider_calls_performed"] == 0
    assert ci.load_case_inputs()["model_calls_performed"] == 0


# ── 1 & 2: the pilot matrix is exactly determined ────────────────────────────

def test_the_pilot_matrix_is_fully_determined():
    matrix = xc.pilot_matrix()
    assert matrix["n_cases"] == 8
    assert matrix["arms"] == list(er.ARMS)
    assert matrix["repetitions"] == 2
    assert matrix["total_units"] == 8 * 5 * 2 == 80
    assert matrix["namespace"] == "pilot:er001-pilot-v1"


def test_pilot_repetitions_are_frozen_at_the_smallest_useful_count():
    """Two is the smallest count that can expose a disagreement at all, and
    owner §12 already recorded that repetitions are not a power remedy."""
    assert xc.PILOT_REPETITIONS == 2
    rationale = xc.PILOT_REPETITION_RATIONALE
    assert rationale["value"] == 2
    assert "nothing to disagree with" in rationale["why_not_one"]
    assert "do NOT help" in rationale["not_a_power_remedy"]


def test_pilot_selection_is_deterministic_and_saturates_its_axes():
    first = xc.pilot_cases()
    assert first == xc.pilot_cases()          # pure function of frozen inputs
    assert len(set(first)) == len(first)
    population = xc.primary_population()
    assert set(first).issubset(population)

    inputs, observations, hidden = (ci.load_case_inputs(),
                                    xc._probe_observations(),
                                    xc._sealed_hidden_meta())
    seen = {axis: set() for axis in xc._axes(
        first[0], {}, inputs, observations, hidden)}
    for case_id in first:
        for axis, value in xc._axes(case_id, {}, inputs, observations,
                                    hidden).items():
            seen[axis].add(value)
    required = {axis: set() for axis in seen}
    for case_id in population:
        for axis, value in xc._axes(case_id, {}, inputs, observations,
                                    hidden).items():
            required[axis].add(value)
    assert seen == required, "the pilot must exercise every declared axis value"


def test_the_pilot_exercises_both_masked_k_delivery_paths():
    """BLOCKER-03's failure path is a runner path, so the pilot must hit it."""
    inputs, observations, hidden = (ci.load_case_inputs(),
                                    xc._probe_observations(),
                                    xc._sealed_hidden_meta())
    affordable = {xc._axes(c, {}, inputs, observations,
                           hidden)["masked_k_delivery_affordable"]
                  for c in xc.pilot_cases()}
    assert affordable == {True, False}


def test_pilot_selection_never_reads_a_sealed_label():
    """The selection axes are public properties. Nothing keyed on the answer."""
    source = inspect.getsource(xc._axes) + inspect.getsource(xc.pilot_cases)
    for sealed in ("true_locus", "governed-keys", "_sealed_keys",
                   "discriminating_probes"):
        assert sealed not in source


# ── deterministic unit ids ───────────────────────────────────────────────────

def test_all_pilot_uids_are_deterministic_unique_and_executably_bound():
    uids = xc.pilot_uids()
    assert len(uids) == 80 and len(set(uids)) == 80
    assert uids == xc.pilot_uids()
    matrix = xc.pilot_matrix()
    expected = unit_id(er.EXPERIMENT_ID, matrix["namespace"], matrix["cases"][0],
                       matrix["arms"][0], 1,
                       xc.executable_contract_fingerprint())
    assert uids[0] == expected


def test_unit_ids_move_when_the_executable_design_moves():
    """A unit id must not survive a change to the matrix or the case inputs."""
    assert xc.pilot_uids("a" * 64) != xc.pilot_uids()


def test_the_pilot_matrix_document_is_self_hashing():
    doc = xc.pilot_matrix_document()
    body = {k: v for k, v in doc.items() if k != "matrix_sha256"}
    assert doc["matrix_sha256"] == xc._sha256(xc._canonical(body))
    assert doc["execution_authorized"] is False
    assert doc["contract_fingerprint"] == xc.DESIGN_FINGERPRINT


# ── 3: the per-case input binding ────────────────────────────────────────────

def test_every_population_case_has_a_canonical_binding(inputs):
    for case_id in xc.primary_population():
        record = inputs["cases"][case_id]
        assert record["question"].startswith("Which failure locus")
        assert case_id in record["question"]
        assert record["failure_evidence"]["content_sha256"]
        assert record["failure_evidence"]["ledger_content_sha256"]
        assert record["initial_context"]["rendered_sha256"]
        assert record["initial_evidence"].startswith("[sql]")
        assert (ci._sha256(record["initial_evidence"])
                == record["initial_evidence_sha256"])


def test_the_binding_is_hash_bound_as_a_whole(inputs):
    assert ci.case_inputs_hash(inputs) == inputs["case_inputs_sha256"]


def test_no_case_input_carries_hidden_answer_metadata(inputs):
    ci.assert_no_answer_metadata(inputs)
    blob = json.dumps({k: v for k, v in inputs.items()
                       if k != "answer_metadata_fields_checked"}).lower()
    for field in ("true_locus", "discriminating_probes", "gate_infeasible"):
        assert field not in blob


def test_the_initial_state_is_identical_in_every_arm(inputs):
    for case_id in xc.pilot_cases():
        proof = ci.assert_arm_invariant_initial_state(case_id, er.ARMS, inputs)
        assert proof["arm_invariant"] is True
        assert proof["arms"] == sorted(er.ARMS)


def test_later_rounds_still_carry_the_bound_initial_state(inputs):
    """R-02's invariant, checked against the frozen binding rather than a fixture."""
    case_id = xc.pilot_cases()[0]
    base = ci.initial_state_for(case_id, inputs)
    state = DiagnosticState(
        question=base["question"], case_id=case_id,
        initial_evidence=base["initial_evidence"], action_schema="  - act-01: x",
        locus_vocabulary=("implementation",), max_rounds=3, max_payment=7)
    for round_num in (1, 2, 3):
        rendered = state.with_round(round_num).render()
        assert missing_state_sections(rendered) == []
        assert base["question"] in rendered
        assert base["initial_evidence"] in rendered


def test_the_question_template_cannot_vary_by_arm_or_locus():
    signature = inspect.signature(ci.initial_state_for)
    assert "arm" not in signature.parameters
    assert "{case_id}" in ci.QUESTION_TEMPLATE
    assert ci.QUESTION_TEMPLATE.count("{") == 1


# ── 4: MASKED_K vs SINGLE ────────────────────────────────────────────────────

def test_masked_k_is_frozen_and_explicit():
    assert xc.DISCLOSURE_PARAMETERS["MASKED_K"]["k"] == 3
    assert xc.DISCLOSURE_PARAMETERS["MASKED_K"]["constant_across_cases"] is True
    assert xc.MASKED_K_MECHANISM["hidden_evaluator_metadata_participates"] is True
    assert xc.MASKED_K_MECHANISM["membership_encodes_answer_information"] is True


def test_masked_k_is_adjudicated_as_different_from_single_not_renamed():
    adjudication = xc.MASKED_K_VS_SINGLE
    assert adjudication["resolution"].startswith("B")
    assert adjudication["not_a_rename"] is True
    assert adjudication["canonical_machine_term"] == "MASKED_K"
    assert adjudication["single"]["adversary_consistency_set"] == 1
    assert adjudication["masked_k"]["declared_consistency_set"] == 3
    assert "NOT an arm" in adjudication["single"]["status_in_this_experiment"]


def test_single_still_appears_in_no_arm():
    assert all(spec.disclosure is not DisclosurePolicy.SINGLE
               for spec in er.ARMS.values())
    assert "SINGLE_LEGACY_LEAK" in er.REGRESSION_FIXTURES


def test_every_evaluator_selected_arm_is_marked_not_deployable():
    """Owner §4: an evaluator-selected evidence condition is mechanistic."""
    for arm, record in xc.EVIDENCE_SELECTION.items():
        assert record["deployable_navigation"] is False
    assert "EVALUATOR" in xc.EVIDENCE_SELECTION["R1"]["selector"]
    assert "EVALUATOR" in xc.EVIDENCE_SELECTION["R2"]["selector"]
    assert xc.EVIDENCE_SELECTION["R3"]["class"] == "EXEMPT_UPPER_BOUND"


# ── 5: answer-channel capacity ───────────────────────────────────────────────

def test_full_disclosure_membership_remains_a_measured_zero(report):
    for arm in ("R3", "R4"):
        channel = report["per_arm"][arm]
        assert channel["policy"] == "FULL"
        assert channel["delivered_set_constant_across_units"] is True
        assert channel["realized_membership_bits"]["max"] == 0.0
        assert channel["bound_holds"] is True
        assert channel["delivery_failures"] == 0


def test_the_control_discloses_nothing(report):
    channel = report["per_arm"]["R0"]
    assert channel["policy"] == "NONE"
    assert channel["realized_membership_bits"]["max"] == 0.0


def test_masked_k_leakage_is_reported_as_exceeding_its_declared_bound(report):
    """BLOCKER-02. This asserts the DEFECT, so silently fixing it fails here."""
    for arm in ("R1", "R2"):
        channel = report["per_arm"][arm]
        assert channel["bound_holds"] is False
        assert channel["realized_membership_bits"]["max"] > channel[
            "declared_locus_bits"]
        # the tail reaches complete determination of the locus
        assert channel["realized_candidate_locus_set"]["min"] == 1


def test_missing_observations_are_reported_as_a_locus_correlated_channel(report):
    """BLOCKER-03. Delivery failure is not noise; it varies with the answer."""
    rates = report["per_arm"]["R1"]["delivery_failure_rate_by_locus"]
    assert report["per_arm"]["R1"]["delivery_failures"] > 0
    assert max(rates.values()) > min(rates.values())


def test_the_construction_marker_channel_is_declared_and_differential(report):
    """BLOCKER-01. The markers are not cleaned, so they must be visible here."""
    channel = report["construction_marker_channel"]
    assert channel["differential_across_arms"] is True
    assert channel["initial_context"]["n_cases"] == 7
    assert channel["delivered_evidence"]["n_cases"] == 12
    assert channel["delivered_evidence"]["carrier"] == "rerun_verifier_isolated"


def test_ordering_handle_and_rendering_channels_are_argued_to_zero(report):
    for key in ("ordering_channel", "handle_identity_channel",
                "rendering_channel"):
        assert "0 bits" in report[key]


# ── legibility pairs ─────────────────────────────────────────────────────────

def test_raw_and_legible_arms_differ_in_rendering_only():
    diffs = arm_difference_report(er.ARMS.values())
    for pair in ("R1_vs_R2", "R3_vs_R4"):
        assert diffs[pair]["n_declared_differences"] == 1
        assert "observation_rendering" in diffs[pair]["differences"]
        assert diffs[pair]["isolates_single_variable"] is True


def test_r1_and_r2_do_not_actually_receive_the_same_evidence(report):
    """BLOCKER-04: the declared difference is one field, the delivered
    evidence differs in 93% of unit pairs. Asserting the defect keeps a silent
    repair from passing unnoticed."""
    pairs = report["legibility_pair_evidence_identity"]
    assert pairs["R1_vs_R2::as_implemented"]["identical_evidence"] is False
    assert pairs["R1_vs_R2::as_implemented"]["rate"] > 0.9
    assert pairs["R1_vs_R2::required"]["identical_evidence"] is True
    assert pairs["R3_vs_R4::as_implemented"]["identical_evidence"] is True
    assert xc.DELIVERY_SEED_SCOPE["implemented"] is False


# ── the science did not move ─────────────────────────────────────────────────

def test_the_design_fingerprint_is_unchanged_by_this_completion():
    assert er.contract_fingerprint() == xc.DESIGN_FINGERPRINT
    assert er.freeze_time_fingerprint() == er.FINGERPRINT_PROVENANCE[
        "published_at_freeze"]


def test_the_executable_fingerprint_is_distinct_stable_and_supersedes():
    executable = xc.executable_contract_fingerprint()
    assert executable != xc.DESIGN_FINGERPRINT
    assert executable == xc.executable_contract_fingerprint()
    relationship = xc.FINGERPRINT_RELATIONSHIP
    assert relationship["prior"]["value"] == xc.DESIGN_FINGERPRINT
    assert relationship["previously_frozen_scientific_meaning_changed"] is False
    assert relationship["new"]["supersedes_for"] == "anything that executes"


def test_the_executable_fingerprint_moves_when_execution_semantics_move(
        monkeypatch):
    before = xc.executable_contract_fingerprint()
    monkeypatch.setattr(xc, "PILOT_REPETITIONS", 3)
    assert xc.executable_contract_fingerprint() != before


def test_the_executable_fingerprint_ignores_derived_analysis_and_authorization():
    """A leakage measurement is a reading of the design, not part of it."""
    declarations = xc.executable_declarations()
    assert "answer_channel_report" not in declarations
    assert "owner_state" not in declarations
    assert "pilot_unit_ids" not in declarations


def test_frozen_population_and_threshold_are_untouched():
    assert er.PRIMARY_POPULATION["n_cases"] == 43
    assert er.STATISTICAL_PLAN["prespecified_effect_threshold_pp"] == 10
    assert er.STATISTICAL_PLAN["primary_unit"] == "case"
    assert er.ELIGIBLE_CASE_COUNT["n_eligible_for_improvement"] == 34
    assert xc.primary_population() == tuple(sorted(xc.primary_population()))
    assert len(xc.primary_population()) == 43


# ── blockers and owner state ─────────────────────────────────────────────────

def test_all_four_blockers_are_declared_and_none_is_repaired():
    assert sorted(xc.BLOCKERS) == ["BLOCKER-01", "BLOCKER-02", "BLOCKER-03",
                                   "BLOCKER-04"]
    for name, blocker in xc.BLOCKERS.items():
        assert blocker["owner_decision_required"] is True
        assert blocker["measured"] if "measured" in blocker else True
        assert blocker["title"] and blocker["class"]


def test_blocker_one_is_recorded_per_case_rather_than_cleaned(inputs):
    flagged = [c for c in xc.primary_population()
               if inputs["cases"][c]["answer_bearing_marker_present"]]
    assert len(flagged) == 7
    for case_id in flagged:
        assert inputs["cases"][case_id]["construction_markers"]
    assert "MANDATORY STRATIFIER" in xc.BLOCKERS["BLOCKER-01"][
        "mandatory_handling"]


def test_predecessor_executable_artifact_records_its_freeze_time_state():
    artifact = json.loads(Path(xc.EXECUTABLE_ARTIFACT).read_text())
    assert artifact["status"].startswith("FROZEN_NOT_STARTED")
    assert artifact["owner_state"]["execution_authorized"] is False
    assert artifact["owner_state"]["additional_provider_calls_authorized"] is False
    assert artifact["owner_state"]["power_issue"] == "OWNER_ACKNOWLEDGED_UNRESOLVED"
    assert artifact["owner_state"]["original_10pp_threshold_preserved"] is True
    assert xc.pilot_matrix_document()["execution_authorized"] is False


def test_later_owner_state_authorizes_pilot_but_not_full():
    assert er.OWNER_STATE["adjudication_is_not_authorization"]
    assert er.OWNER_STATE["execution_authorized"] is True
    assert er.OWNER_STATE["authorized_stage"] == "pilot"
    assert er.OWNER_STATE["full_execution_authorized"] is False

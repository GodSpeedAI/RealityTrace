"""Guards on the FROZEN contract for `evidence-revision-001`.

These tests do not check that the design is good. They check that the frozen
document says what the owner decided, that its arithmetic is reproducible, and
that the properties the audit was built to protect cannot silently lapse.
"""

import math

import pytest

from ..v2 import evidence_revision as er
from ..v2.channel import DisclosurePolicy
from ..v2.controller import arm_difference_report


# ── the contract is frozen and self-describing ───────────────────────────────

def test_status_is_frozen_and_not_started():
    assert er.STATUS.startswith("FROZEN_NOT_STARTED")
    assert "NOT AUTHORIZED" in er.STATUS
    assert er.freeze()["model_calls_performed"] == 0


def test_owner_state_records_pilot_only_authorization_exactly():
    """The later governance commit authorizes only revision-002 PILOT."""
    s = er.OWNER_STATE
    assert s["status"] == "PILOT_AUTHORIZED"
    assert s["execution_authorized"] is True
    assert s["additional_provider_calls_authorized"] is True
    assert s["authorized_stage"] == "pilot"
    assert s["full_execution_authorized"] is False
    assert s["power_issue"] == "OWNER_ACKNOWLEDGED_UNRESOLVED"
    assert s["original_10pp_threshold_preserved"] is True
    assert s["provider_calls_to_date"]["scored"] == 0
    assert s["provider_calls_to_date"]["non_scored_preflight"] == 2
    assert er.contract()["owner_state"] == s


def test_the_declined_remedies_are_recorded_as_declined():
    declined = er.OWNER_STATE["remedies_declined_by_owner"]
    assert any("threshold" in d for d in declined)
    assert any("corpus" in d for d in declined)
    assert any("estimation exercise" in d for d in declined)
    # and the threshold the owner preserved is still the one in the plan
    assert er.STATISTICAL_PLAN["prespecified_effect_threshold_pp"] == 10


def test_module_imports_no_network_transport():
    """A frozen contract must not be able to run itself."""
    import inspect
    src = inspect.getsource(er)
    for forbidden in ("urllib", "requests", "ZaiCodingPlanClient", "http.client"):
        assert forbidden not in src, f"{forbidden} reachable from the contract"


def test_fingerprint_moves_when_a_frozen_field_changes(monkeypatch):
    before = er.contract_fingerprint()
    monkeypatch.setitem(er.STATISTICAL_PLAN, "prespecified_effect_threshold_pp", 25)
    assert er.contract_fingerprint() != before


def test_fingerprint_does_not_depend_on_derived_analysis(monkeypatch):
    """Power is computed FROM the contract; it is not an input to it."""
    before = er.contract_fingerprint()
    monkeypatch.setattr(er, "power_analysis", lambda alpha=0.05: {"x": 1})
    assert er.contract_fingerprint() == before


def test_the_design_has_not_moved_since_the_freeze():
    """The fingerprint's DEFINITION changed when the authorization decision was
    recorded. The design did not. Recomputing under the original definition
    must still yield the value published at freeze time."""
    assert (er.freeze_time_fingerprint()
            == er.FINGERPRINT_PROVENANCE["published_at_freeze"])
    assert (er.contract_fingerprint()
            == er.FINGERPRINT_PROVENANCE["design_fingerprint"])


def test_fingerprint_does_not_move_when_authorization_state_changes(monkeypatch):
    """The fingerprint identifies the DESIGN. Granting or withholding execution
    is a governance act and must not look like a change to the science."""
    before = er.contract_fingerprint()
    monkeypatch.setitem(er.OWNER_STATE, "execution_authorized", True)
    assert er.contract_fingerprint() == before


# ── arms (owner §2, §6, §7) ──────────────────────────────────────────────────

def test_all_five_arms_are_declared():
    assert sorted(er.ARMS) == ["R0", "R1", "R2", "R3", "R4"]
    assert sorted(er.ARM_ROLE) == sorted(er.ARMS)
    assert sorted(er.GOVERNANCE_CLASS) == sorted(er.ARMS)


def test_the_leaking_policy_is_in_no_arm():
    """Owner §6: SINGLE may exist only as a regression fixture."""
    assert all(spec.disclosure is not DisclosurePolicy.SINGLE
               for spec in er.ARMS.values())
    assert "SINGLE_LEGACY_LEAK" in er.REGRESSION_FIXTURES
    assert "NOT RUN" in er.REGRESSION_FIXTURES["SINGLE_LEGACY_LEAK"]


def test_the_upper_bound_is_a_governance_class_not_a_zero_payment():
    """Owner §2: the exemption must be represented explicitly."""
    for arm in ("R3", "R4"):
        assert er.GOVERNANCE_CLASS[arm] == "EXEMPT_UPPER_BOUND"
        assert er.ARMS[arm].budget_semantics == "EXEMPT_UPPER_BOUND"
        assert er.ARMS[arm].max_payment == 7      # not zeroed out
    for arm in ("R1", "R2"):
        assert er.ARMS[arm].budget_semantics == "ENFORCED"


def test_legibility_pairs_differ_only_in_rendering():
    """R2/R4 must deliver the SAME evidence as R1/R3 (owner §7)."""
    for governed, legible in (("R1", "R2"), ("R3", "R4")):
        d = er.ARMS[governed].diff(er.ARMS[legible])
        assert list(d) == ["observation_rendering"]


def test_no_arm_removes_original_context():
    """Owner §5. R0 withholds NEW evidence; the case context is untouched."""
    assert er.ARMS["R0"].disclosure is DisclosurePolicy.NONE
    assert all(spec.forced_final_offered for spec in er.ARMS.values())


def test_action_selection_is_not_a_manipulation_in_any_arm():
    assert {s.action_source for s in er.ARMS.values()} == {"harness"}
    assert "action selection" in er.NOT_TESTED


# ── comparisons and isolation claims (audit A-06) ────────────────────────────

def test_primary_comparisons_isolate_a_single_variable():
    report = arm_difference_report(er.ARMS.values())
    for key in ("C1_evidence_effect", "C2_legibility_governed",
                "C3_legibility_upper"):
        a, b = er.COMPARISONS[key]["arms"]
        assert report[f"{a}_vs_{b}"]["isolates_single_variable"], key


def test_the_headroom_comparison_admits_it_does_not_isolate():
    report = arm_difference_report(er.ARMS.values())
    a, b = er.COMPARISONS["C4_coverage_headroom"]["arms"]
    assert report[f"{a}_vs_{b}"]["isolates_single_variable"] is False
    assert "does NOT isolate" in er.COMPARISONS["C4_coverage_headroom"][
        "declared_differences"]
    assert er.COMPARISONS["C4_coverage_headroom"]["family"] == "secondary"


# ── history may not be reused as a statistic (owner §4, §12) ─────────────────

def test_the_historical_zero_of_63_is_motivation_not_a_baseline():
    m = er.MOTIVATION_NOT_BASELINE
    assert m["status"] == "MOTIVATION"
    assert m["may_be_used_as_baseline"] is False
    assert m["may_be_used_as_formal_null"] is False


def test_the_unresolved_unit_is_excluded_without_an_invented_story():
    e = er.EXCLUSIONS["c096c6b111e05554"]
    assert e["status"] == "PROVENANCE_UNRESOLVED"
    assert e["disposition"] == "EXCLUDED_FROM_CORRECTED_CAUSAL_ANALYSIS"
    assert "no historical explanation" in e["note"].lower()


# ── statistics ───────────────────────────────────────────────────────────────

def test_exact_mcnemar_matches_the_binomial_sign_test():
    assert er.exact_mcnemar_p(0, 0) == 1.0
    assert er.exact_mcnemar_p(5, 0) == pytest.approx(2 * 0.5 ** 5)
    assert er.exact_mcnemar_p(6, 0) == pytest.approx(2 * 0.5 ** 6)
    assert er.exact_mcnemar_p(3, 3) == 1.0
    assert er.exact_mcnemar_p(4, 1) == pytest.approx(
        2 * (math.comb(5, 0) + math.comb(5, 1)) * 0.5 ** 5)


def test_five_discordant_pairs_can_never_reach_significance():
    """The hard floor the whole power argument rests on."""
    assert er._min_significant_discordance(0.05) == 6
    assert er.exact_mcnemar_p(5, 0) > 0.05


def test_power_is_a_probability_and_rises_with_effect_size():
    p10 = er.power_exact_mcnemar(34, 0.10)
    p30 = er.power_exact_mcnemar(34, 0.30)
    assert 0.0 <= p10 <= 1.0 and p10 < p30 <= 1.0


def test_power_falls_when_the_treated_arm_can_also_regress():
    assert er.power_exact_mcnemar(43, 0.10, 0.02) < er.power_exact_mcnemar(43, 0.10)


def test_the_prespecified_threshold_is_underpowered_and_triggers_the_stop():
    pa = er.power_analysis()
    assert pa["min_discordant_pairs_for_significance"] == 6
    eligible = pa["results"]["n=34 (eligible for improvement)"]
    whole = pa["results"]["n=43 (whole primary population)"]
    assert eligible["power_monotone"] < 0.20
    assert whole["power_monotone"] < 0.40
    assert eligible["mde_at_80pct_power_pp"] > 10
    assert er.STOP_CONDITION["triggered"] is True


def test_the_stop_does_not_quietly_move_the_threshold():
    """Owner §12 forbids altering thresholds; the STOP returns an argument."""
    assert er.STATISTICAL_PLAN["prespecified_effect_threshold_pp"] == 10
    assert "No threshold is altered here" in er.STOP_CONDITION[
        "what_is_NOT_proposed"]


def test_the_inferential_unit_is_the_case_not_the_repetition():
    assert er.STATISTICAL_PLAN["primary_unit"] == "case"
    assert "never alone" in er.STATISTICAL_PLAN["repetition_handling"]


def test_truncation_is_a_mandatory_covariate():
    """An arm that truncates more looks worse for reasons unrelated to the
    hypothesis — audit NEW-02."""
    assert any("truncation" in c for c in er.MANDATORY_COVARIATES)


# ── population and preflight ─────────────────────────────────────────────────

def test_the_primary_population_is_the_43_gate_feasible_cases():
    p = er.PRIMARY_POPULATION
    assert p["n_cases"] == 43
    assert p["corpus_adjudication"]["effect_on_n"] == "none — 43 stands"
    assert p["gate_infeasible_excluded"]["disposition"] == "GATE_INFEASIBLE"


def test_the_preflight_gate_records_the_ceiling_finding():
    c = er.PREFLIGHT["completion_ceiling"]
    assert er.PREFLIGHT["required_before_any_pilot_unit"] is True
    assert c["route_ceiling_below_request"] is False
    assert c["requested_max_tokens"] == 8192
    assert c["max_completion_tokens_observed"] > 4096
    # the limit of what two calls establish is stated, not glossed
    assert "not demonstrated reachable" in c["scope_limit"]

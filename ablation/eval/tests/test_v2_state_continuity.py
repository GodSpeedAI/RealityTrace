"""R-02 regression: round-N context amnesia (audit A-11).

State continuity is a first-class experimental invariant. These tests assert it
on the RENDERED text, including a negative test that must fail when the initial
evidence is removed.
"""

import dataclasses

import pytest

from ..v2.decision import ControllerDecision
from ..v2.dstate import (REQUIRED_ROUND_STATE, AcquiredObservation,
                         DiagnosticState, missing_state_sections)

EVIDENCE = ("verifier exit 101; error[E0599]: no function or associated item "
            "named `new_with_namespace` found for struct `Entity`")
VOCAB = ("verifier", "implementation", "environment", "unknown")


def make_state(**kw):
    base = dict(
        question="Which failure locus does the recorded failure belong to?",
        case_id="df-04", initial_evidence=EVIDENCE,
        action_schema="  - act-01: Inspect X (cost: 1)\n  - act-02: Inspect Y (cost: 1)",
        locus_vocabulary=VOCAB, max_rounds=3, max_payment=7,
        available_actions=("act-01", "act-02"))
    base.update(kw)
    return DiagnosticState(**base)


# ── the defect, reproduced ───────────────────────────────────────────────────

LEGACY_ROUNDN = (
    "Round {round_num}/{max_rounds}. Payment used so far: {payment_used}/"
    "{max_payment}.\n\nPrevious round:\n  Candidates: {prev_candidates}\n"
    "  Unresolved: {prev_unresolved}\n  Action taken: {prev_action}\n\n"
    "--- observation from {prev_action} ---\n{observation}\n"
    "--- end observation ---\n")


def test_A11_legacy_round2_prompt_loses_the_failure_evidence():
    """Reproduces the audited defect: the legacy round-N scaffold carries no
    case description and no retrieval context, and calls were stateless."""
    rendered = LEGACY_ROUNDN.format(
        round_num=2, max_rounds=3, payment_used=1, max_payment=7,
        prev_candidates="implementation, verifier", prev_unresolved="which layer",
        prev_action="inspect_source_symbols", observation='{"missing": []}')
    assert EVIDENCE not in rendered
    missing = missing_state_sections(rendered)
    assert set(missing) >= {"question", "initial_evidence", "available_actions"}


# ── the repair ───────────────────────────────────────────────────────────────

def test_round1_render_carries_every_required_section():
    assert missing_state_sections(make_state().render()) == []


def test_round_n_retains_all_required_state_after_a_full_cycle():
    """round 1 context + acquired observation + hypotheses -> round 2 input."""
    s = make_state()
    s = s.with_decision(ControllerDecision(
        candidate_loci=("implementation", "verifier"),
        unresolved_distinction="whether the symbol was ever defined",
        selected_action="act-01"))

    class _Obs:
        handle, payload = "act-01", {"missing": ["Entity::new_with_namespace"]}

    s = s.with_observations([_Obs()], "model", payment=1,
                            available=("act-02",)).with_round(2)

    rendered = s.render()
    assert missing_state_sections(rendered) == []
    assert EVIDENCE in rendered                       # original failure retained
    assert "implementation, verifier" in rendered     # hypotheses retained
    assert "whether the symbol was ever defined" in rendered
    assert "act-01" in rendered and "new_with_namespace" in rendered
    assert "used 1/7" in rendered and "remaining 6" in rendered
    assert "[available_actions] act-02" in rendered


def test_negative_control_removing_initial_evidence_must_fail():
    """The invariant test itself must be capable of failing."""
    s = make_state()
    broken = dataclasses.replace(s, initial_evidence="")
    rendered = broken.render().replace("[initial_evidence]", "[redacted]")
    assert "initial_evidence" in missing_state_sections(rendered)
    with pytest.raises(AssertionError):
        assert missing_state_sections(rendered) == []


def test_rejection_reasons_are_categories_and_carry_no_hint():
    s = make_state().with_rejection("concrete", "implementation",
                                    "EVIDENCE_INSUFFICIENT")
    rendered = s.render()
    assert "rejected (EVIDENCE_INSUFFICIENT)" in rendered
    assert s.governance_state == "CONCRETE_REJECTED"
    for hint in ("true", "correct", "should have", "the answer"):
        assert hint not in rendered.lower().split("[initial_evidence]")[0]


def test_rejected_attempts_accumulate_across_rounds():
    s = make_state()
    s = s.with_rejection("concrete", "implementation", "EVIDENCE_INSUFFICIENT")
    s = s.with_round(2).with_rejection("unknown", "unknown", "AFFORDABLE_ACTION_REMAINS")
    rendered = s.render()
    assert rendered.count("rejected (") == 2
    assert "round 1: concrete 'implementation'" in rendered
    assert "round 2: unknown 'unknown'" in rendered


def test_observation_payloads_are_truncated_visibly_not_silently():
    big = {"blob": "x" * 5000}

    class _Obs:
        handle, payload = "act-01", big

    s = make_state().with_observations([_Obs()], "harness", 1, ())
    rendered = s.render()
    assert "[truncated at 2000 chars]" in rendered


def test_state_is_bounded_not_a_chat_transcript():
    """Repair 2 requires the smallest deterministic representation, not an
    appended history: two rounds must not duplicate the evidence block."""
    s = make_state().with_round(2)
    assert s.render().count(EVIDENCE) == 1


def test_digest_is_deterministic_and_change_sensitive():
    a, b = make_state(), make_state()
    assert a.digest() == b.digest()
    assert a.with_round(2).digest() != a.digest()


def test_upper_bound_arm_declares_budget_exemption_rather_than_hiding_it():
    s = make_state(budget_semantics="EXEMPT_UPPER_BOUND", payment_used=14)
    assert s.payment_remaining == 7
    assert "EXEMPT_UPPER_BOUND" in s.render()


def test_required_state_list_is_the_single_source_of_truth():
    assert set(REQUIRED_ROUND_STATE) == set(
        missing_state_sections("")), "renderer markers must cover the invariant"

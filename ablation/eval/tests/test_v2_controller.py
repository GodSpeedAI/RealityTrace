"""R-03/R-16 regression: contract repair, honest termination, arm-difference
disclosure, and end-to-end replication readiness against a deterministic stub.

No network I/O. Zero model calls.
"""

import pytest

from ..v2.channel import DisclosurePolicy
from ..v2.controller import (HARNESS_DEFAULTED, ConditionSpec,
                             DiagnosticController, Terminal,
                             arm_difference_report)
from ..v2.sufficiency import SufficiencyContract

PROBES = ("inspect_target_binding", "inspect_source_symbols",
          "inspect_environment", "inspect_permissions",
          "inspect_fixture_integrity", "validate_semantic_model",
          "rerun_verifier_isolated", "inspect_verifier_config")
DESCR = {p: f"description of {p.replace('_', ' ')}" for p in PROBES}
COSTS = {p: 1 for p in PROBES}
COSTS["validate_semantic_model"] = 3
COSTS["rerun_verifier_isolated"] = 5
VOCAB = ("verifier", "verification_plan", "projection", "implementation",
         "environment", "authority", "evidence_collection", "declared_model",
         "unknown")
OBS = {p: {"probe_field": "clean"} for p in PROBES}
OBS["inspect_permissions"] = {"all_accessible": False}

CONTRACT = SufficiencyContract("test-v1", {
    "authority": {"probe": "inspect_permissions", "field": "all_accessible",
                  "op": "is_false"},
})


class StubClient:
    """Deterministic scripted client. Records every prompt it was given."""

    def __init__(self, script, provider=None):
        self.script = list(script)
        self.prompts = []
        self.provider = provider or {"resolved_model": "stub/glm-5.2",
                                     "generation_id": "gen_stub",
                                     "finish_reason": "stop",
                                     "completion_tokens": 120,
                                     "prompt_tokens": 900, "retries": 0}

    def complete(self, system, user, requested_max_tokens, temperature):
        self.prompts.append(user)
        text = self.script.pop(0) if self.script else "still not valid json"
        return {"text": text, "provider": dict(self.provider)}


def spec(name="G", **kw):
    base = dict(action_source="model", schema_visible=True, gated=True,
                disclosure=DisclosurePolicy.FULL, max_rounds=3, max_payment=7,
                max_model_calls=8, max_repair_attempts=2)
    base.update(kw)
    return ConditionSpec(name=name, **base)


def controller(client, s=None):
    return DiagnosticController(s or spec(), CONTRACT, PROBES, DESCR, COSTS,
                                VOCAB, client)


def run(ctrl, discriminating=("inspect_permissions",)):
    return ctrl.run_unit("u1", "exp", "ns", "cep-12", 1,
                         "Which locus?", "verifier exit 101; permission denied",
                         OBS, discriminating)


def qualifying_handle():
    """The handle that maps to `inspect_permissions` for this unit."""
    from ..v2.channel import ActionCatalogue, unit_seed
    return ActionCatalogue(PROBES, unit_seed("exp", "ns", "cep-12", "G", 1)
                           ).handle_of("inspect_permissions")


def act(handle):
    return ('{"candidate_loci": ["authority", "environment"], '
            f'"selected_action": "{handle}", "proposed_settlement": null}}')


def settle(locus):
    return ('{"candidate_loci": ["authority"], "selected_action": null, '
            f'"proposed_settlement": "{locus}"}}')


# ── A-02: false budget exhaustion is now impossible ──────────────────────────

def test_A02_parse_failure_does_not_fake_budget_exhaustion():
    """The audited path told the model 'Budget exhausted (0/7 payment)' after a
    round-1 format failure. Here a malformed reply must produce a bounded
    re-ask and, if unresolved, an explicit contract failure."""
    c = StubClient(["not json at all", "still not json", "nope"])
    res = run(controller(c))
    assert res.terminal_state == Terminal.MODEL_OUTPUT_CONTRACT_FAILURE.value
    assert res.payment_used == 0
    assert res.acquired_probes == []
    assert res.final_locus is None          # NOT silently "unknown"
    joined = "\n".join(c.prompts)
    assert "Budget exhausted" not in joined
    assert "No further evidence acquisition is possible" not in joined


def test_A02_bounded_repair_recovers_a_valid_decision():
    """Round 1 is malformed, repaired into a valid action; round 2 settles on
    evidence the gate accepts."""
    c = StubClient(["garbage", act(qualifying_handle()), settle("authority")])
    res = run(controller(c))
    assert res.terminal_state == Terminal.CONCRETE_SETTLEMENT.value
    assert res.final_locus == "authority"
    assert res.rounds[0].repair_attempts == 1
    assert "did not satisfy the response contract" in c.prompts[1]


def test_repair_is_bounded_not_an_infinite_loop():
    c = StubClient(["bad"] * 50)
    res = run(controller(c, spec(max_repair_attempts=2)))
    assert len(res.executions) == 3          # 1 attempt + 2 repairs
    assert res.terminal_state == Terminal.MODEL_OUTPUT_CONTRACT_FAILURE.value


def test_every_raw_response_is_persisted_before_parsing():
    c = StubClient(["garbage", act(qualifying_handle()), settle("authority")])
    res = run(controller(c))
    assert len(res.executions) == 3
    assert all(e["raw_hash"] and e["raw_len"] >= 0 for e in res.executions)
    assert res.executions[0]["returned"]["resolved_model"] == "stub/glm-5.2"


def test_ambiguous_decision_routes_to_repair_not_to_precedence():
    both = ('{"candidate_loci": ["authority"], "selected_action": "act-01", '
            '"proposed_settlement": "authority"}')
    c = StubClient([both, act(qualifying_handle()), settle("authority")])
    res = run(controller(c))
    assert res.rounds[0].repair_attempts == 1
    assert res.terminal_state == Terminal.CONCRETE_SETTLEMENT.value


# ── A-10: honest termination, offered to every arm ───────────────────────────

def test_A10_round_limit_is_explicit_and_still_asks_for_a_final_answer():
    """The audited O branch fell out of the loop with no terminal state and was
    never asked to settle: 84/180 units. Here the round limit is a declared
    outcome AND a forced-final request is issued."""
    c = StubClient([act("act-01"), act("act-02"), act("act-03"),
                    settle("authority")])
    res = run(controller(c))
    assert res.terminal_state == Terminal.BUDGET_EXHAUSTED_SETTLED.value
    assert "No further evidence acquisition is possible" in c.prompts[-1]
    assert "round limit reached" in c.prompts[-1]


def test_forced_final_is_offered_identically_to_every_arm():
    script = [act("act-01"), act("act-02"), act("act-03"), settle("authority")]
    seen = {}
    for name, s in (("G", spec("G", action_source="model")),
                    ("O", spec("O", action_source="harness"))):
        c = StubClient(list(script))
        run(controller(c, s))
        seen[name] = sum("No further evidence acquisition" in p
                         for p in c.prompts)
    assert seen["G"] == seen["O"] == 1


def test_harness_defaulted_units_are_flagged_and_have_no_final_locus():
    c = StubClient(["bad", "bad", "bad"])
    res = run(controller(c))
    assert res.harness_defaulted is True
    assert res.final_locus is None
    assert Terminal(res.terminal_state) in HARNESS_DEFAULTED


def test_every_path_sets_a_terminal_state():
    for script in ([settle("unknown")], [settle("authority")],
                   ["bad", "bad", "bad"],
                   [act("act-01"), act("act-02"), act("act-03"), settle("unknown")]):
        res = run(controller(StubClient(list(script))))
        assert res.terminal_state in {t.value for t in Terminal}


# ── A-03 / corrective action 4: gate-blind claim is always recorded ──────────

def test_gate_rejection_records_the_model_claim_for_gate_blind_accuracy():
    """Governance is preserved (a claim the evidence does not support is still
    refused) but the model's claim is recorded, so gated and gate-blind
    accuracy are both reportable."""
    c = StubClient([settle("environment")] * 6)
    res = run(controller(c))
    assert res.rejected_concrete >= 1
    assert res.last_model_claim == "environment"     # gate-blind claim kept
    assert res.final_locus in (None, "unknown")      # gate held
    assert res.terminal_state == Terminal.BUDGET_EXHAUSTED_GATE_REJECTED.value


def test_gate_accepts_a_qualifying_claim_after_acquisition():
    c = StubClient([act(qualifying_handle()), settle("authority")])
    res = run(controller(c))
    assert res.acquired_probes == ["inspect_permissions"]
    assert res.terminal_state == Terminal.CONCRETE_SETTLEMENT.value
    assert res.final_locus == "authority"


def test_unknown_is_rejected_while_actions_remain_then_accepted_when_none_do():
    c = StubClient([settle("unknown"), settle("unknown"), settle("unknown"),
                    settle("unknown")])
    res = run(controller(c))
    assert res.rejected_unknown >= 1


# ── A-06: arm differences are disclosed, not asserted away ───────────────────

def test_arm_difference_report_enumerates_every_declared_difference():
    g = spec("G", action_source="model", disclosure=DisclosurePolicy.FULL)
    o = spec("O", action_source="harness", schema_visible=False,
             max_model_calls=6, budget_semantics="EXEMPT_UPPER_BOUND")
    rep = arm_difference_report([g, o])["G_vs_O"]
    assert rep["isolates_single_variable"] is False
    assert set(rep["differences"]) == {"action_source", "schema_visible",
                                       "max_model_calls", "budget_semantics"}


def test_a_genuinely_isolated_contrast_is_reported_as_such():
    a = spec("A", action_source="model")
    b = spec("B", action_source="harness")
    assert arm_difference_report([a, b])["A_vs_B"]["isolates_single_variable"]


# ── R-01/R-02 hold end to end ────────────────────────────────────────────────

def test_no_canonical_probe_id_reaches_the_model_in_any_prompt():
    c = StubClient([act("act-01"), settle("unknown")])
    run(controller(c, spec("O", action_source="harness")))
    for prompt in c.prompts:
        for pid in PROBES:
            assert pid not in prompt


def test_every_prompt_carries_the_full_diagnostic_state():
    from ..v2.dstate import missing_state_sections
    c = StubClient([act("act-01"), act("act-02"), settle("unknown")])
    run(controller(c))
    for prompt in c.prompts:
        assert missing_state_sections(prompt) == []
        assert "verifier exit 101; permission denied" in prompt


def test_upper_bound_arm_delivers_a_locus_independent_observation_set():
    c = StubClient([settle("unknown"), settle("unknown"), settle("unknown"),
                    settle("unknown")])
    s = spec("O", action_source="harness", disclosure=DisclosurePolicy.FULL,
             budget_semantics="EXEMPT_UPPER_BOUND")
    res = run(controller(c, s))
    deliveries = [r.delivery for r in res.rounds if r.delivery]
    assert deliveries and all(d["answer_isolated"] for d in deliveries)
    assert all(d["bits_disclosed"] == 0.0 for d in deliveries)


def test_ceiling_detector_is_wired_through_the_controller():
    c = StubClient([settle("unknown")],
                   provider={"completion_tokens": 4096, "finish_reason": "length",
                             "resolved_model": "stub/glm-5.2"})
    res = run(controller(c))
    assert res.ceiling["route_ceiling_below_request"] is True
    assert res.ceiling["effective_ceiling"] == 4096


def test_controller_makes_no_network_calls(monkeypatch):
    import urllib.request

    def boom(*a, **k):
        raise AssertionError("network access attempted")

    monkeypatch.setattr(urllib.request, "urlopen", boom)
    res = run(controller(StubClient([settle("unknown")])))
    assert res.terminal_state


# ── R-16 replication readiness ───────────────────────────────────────────────

def test_replication_validation_passes_end_to_end_with_zero_model_calls(tmp_path):
    from ..v2 import replication
    r = replication.validate(str(tmp_path))
    assert r["model_calls_performed"] == 0
    assert r["integrity_suite"]["all_passed"]
    assert r["state_continuity"]["prompts_missing_required_state"] == 0
    assert (r["state_continuity"]["prompts_retaining_initial_evidence"]
            == r["state_continuity"]["prompts_checked"])
    assert r["no_fall_through_terminal"]
    assert r["contract_repair"]["payment_consumed"] == 0
    assert r["contract_repair"]["final_locus"] is None
    assert r["provenance"]["all_have_provider_resolved_model"]
    assert r["status"] == "PREPARED — NOT STARTED"


def test_replication_declares_r0_leakage_rather_than_hiding_it(tmp_path):
    from ..v2 import replication
    r = replication.validate(str(tmp_path))
    bounds = r["per_arm_disclosure_bound"]
    assert bounds["R0"]["answer_isolated"] is False
    assert bounds["R1"]["answer_isolated"] is True
    assert bounds["R3"]["answer_isolated"] is True
    assert "R2" in r["subsumed_arms"]


def test_observation_rendering_is_a_real_manipulation():
    from ..v2.dstate import DiagnosticState

    class _O:
        handle, payload = "act-01", {"all_accessible": False, "mode": "600"}

    base = dict(question="q", case_id="c", initial_evidence="e",
                action_schema="s", locus_vocabulary=("unknown",), max_rounds=3,
                max_payment=7)
    raw = DiagnosticState(**base, observation_rendering="raw_json"
                          ).with_observations([_O()], "harness", 1, ())
    lab = DiagnosticState(**base, observation_rendering="labelled_structural"
                          ).with_observations([_O()], "harness", 1, ())
    assert '{"all_accessible": false' in raw.render()
    assert "all_accessible = false" in lab.render()
    assert raw.digest() != lab.digest()

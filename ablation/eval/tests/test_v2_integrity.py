"""R-06: every integrity check must have a mutation that makes it FAIL.

The audited gate reported 9/9 PASS with a negative control that could not fail,
an empty model-uniformity loop, a substring-matching namespace check and a
source-grep oracle check. A check with no failing mutation is not evidence.
"""

import pytest

from ..v2.channel import DisclosurePolicy
from ..v2.controller import (ConditionSpec, DiagnosticController, Terminal)
from ..v2.integrity import (check_gate_negative_control, check_model_uniformity,
                            check_oracle_isolation, check_parse_failure_behaviour,
                            check_pilot_full_separation, check_raw_before_score,
                            run_suite)
from ..v2.ledger import CommitLedger
from ..v2.sufficiency import SufficiencyContract

PROBES = ("inspect_permissions", "inspect_environment", "inspect_source_symbols")

CONTRACT = SufficiencyContract("t", {
    "authority": {"probe": "inspect_permissions", "field": "all_accessible",
                  "op": "is_false"},
    "environment": {"probe": "inspect_environment", "field": "toolchain",
                    "op": "ne", "value": "available"},
    "implementation": {"probe": "inspect_source_symbols", "field": "missing",
                       "op": "nonempty"},
})

PROBES_BY_CASE = {
    # authority case; the permissions probe fires
    "c1": {"inspect_permissions": {"all_accessible": False},
           "inspect_environment": {"toolchain": "available"},
           "inspect_source_symbols": {"missing": []}},
    # environment case; BOTH environment and implementation fire -> the gate
    # can license a locus that is not the true one
    "c2": {"inspect_permissions": {"all_accessible": True},
           "inspect_environment": {"toolchain": "missing"},
           "inspect_source_symbols": {"missing": ["sym"]}},
    # unknown case; nothing fires
    "c3": {"inspect_permissions": {"all_accessible": True},
           "inspect_environment": {"toolchain": "available"},
           "inspect_source_symbols": {"missing": []}},
}
TRUTH = {"c1": "authority", "c2": "environment", "c3": "unknown"}


# ── 1. gate negative control ─────────────────────────────────────────────────

def test_gate_negative_control_passes_on_an_honest_gate():
    r = check_gate_negative_control(CONTRACT, PROBES_BY_CASE, TRUTH)
    assert r.passed
    assert r.evidence["n_licensed_but_false"] >= 1


def test_MUTATION_gate_reading_a_truth_side_channel_is_caught():
    """Assertion (a): a gate that consults a smuggled truth key must fail.

    The key name is one the check injects, so the leak is detected by the
    truth-injection invariant rather than incidentally."""

    class LeakingContract(SufficiencyContract):
        def qualifies(self, locus, observations):
            if "true_locus" in observations:
                return observations["true_locus"] == locus
            return super().qualifies(locus, observations)

    leaking = LeakingContract("leaky", dict(CONTRACT.rules))
    r = check_gate_negative_control(leaking, PROBES_BY_CASE, TRUTH)
    assert not r.passed
    assert "truth side channel" in r.detail


def test_MUTATION_gate_not_a_function_of_the_evidence_is_caught():
    """Assertion (b): identical observations, different verdicts."""

    class CaseSensitiveContract(SufficiencyContract):
        def __init__(self, *a, **k):
            super().__init__(*a, **k)
            object.__setattr__(self, "_seen", {})

        def qualifies(self, locus, observations):
            import json
            key = json.dumps(observations, sort_keys=True, default=str)
            self._seen[key] = self._seen.get(key, 0) + 1
            if locus == "authority" and self._seen[key] > len(self.rules):
                return True            # same evidence, different answer
            return super().qualifies(locus, observations)

    twins = {"t1": {"inspect_permissions": {"all_accessible": True}},
             "t2": {"inspect_permissions": {"all_accessible": True}}}
    c = CaseSensitiveContract("twin", dict(CONTRACT.rules))
    r = check_gate_negative_control(c, twins, {"t1": "authority",
                                               "t2": "environment"})
    assert not r.passed


def test_MUTATION_gate_that_only_ever_licenses_the_truth_is_caught():
    only_true = {"c1": {"inspect_permissions": {"all_accessible": False}},
                 "c3": {"inspect_permissions": {"all_accessible": True}}}
    r = check_gate_negative_control(CONTRACT, only_true,
                                    {"c1": "authority", "c3": "unknown"})
    assert not r.passed
    assert "leaking gate" in r.detail


# ── 2. model uniformity ──────────────────────────────────────────────────────

def rec(model="zai/glm-5.2", max_tokens=8192, temp=0.0):
    return {"requested": {"endpoint": "https://x/v4", "temperature": temp,
                          "max_tokens": max_tokens},
            "returned": {"resolved_model": model}}


def test_model_uniformity_passes_on_provider_returned_records():
    r = check_model_uniformity([rec(), rec(), rec()])
    assert r.passed and r.evidence["resolved_model"] == "zai/glm-5.2"


def test_MUTATION_mixed_models_are_caught():
    r = check_model_uniformity([rec(), rec("openai/gpt-x")])
    assert not r.passed and "non-uniform resolved_model" in r.detail


def test_MUTATION_missing_provider_model_is_caught_not_defaulted():
    """The historical sidecars hardcoded the model, so the check was vacuous."""
    r = check_model_uniformity([rec(), {"requested": {}, "returned": {}}])
    assert not r.passed and "no provider-returned resolved_model" in r.detail


def test_MUTATION_mixed_max_tokens_is_caught():
    assert not check_model_uniformity([rec(max_tokens=4096), rec()]).passed


# ── 3. pilot / full separation ───────────────────────────────────────────────

def sep(ns, ids):
    return {"namespace": ns, "run_id": f"run-{ns}",
            "checkpoint_store": f"/runs/{ns}/manifest.jsonl",
            "artifact_dir": f"/runs/{ns}/raw", "unit_ids": ids}


def test_pilot_full_separation_passes_when_genuinely_disjoint():
    assert check_pilot_full_separation(sep("gov-pilot", ["p1", "p2"]),
                                       sep("gov-full", ["f1", "f2"])).passed


def test_MUTATION_colliding_unit_ids_are_caught():
    """The real historical state: 45/45 gov-pilot ids equalled gov-full ids."""
    shared = [f"u{i}" for i in range(45)]
    r = check_pilot_full_separation(sep("gov-pilot", shared),
                                    sep("gov-full", shared))
    assert not r.passed and "45 colliding unit ids" in r.detail


def test_MUTATION_shared_checkpoint_store_is_caught():
    a, b = sep("gov-pilot", ["p"]), sep("gov-full", ["f"])
    b["checkpoint_store"] = a["checkpoint_store"]
    r = check_pilot_full_separation(a, b)
    assert not r.passed and "shared checkpoint_store" in r.detail


# ── 4. oracle isolation ──────────────────────────────────────────────────────

CLEAN_PROMPT = ("--- observation from act-03 ---\n{\"all_accessible\": false}\n"
                "candidate loci: authority, environment")


def test_oracle_isolation_passes_on_clean_prompts_with_full_disclosure():
    r = check_oracle_isolation([CLEAN_PROMPT] * 5, PROBES,
                               DisclosurePolicy.FULL, len(PROBES))
    assert r.passed and r.evidence["bits_disclosed"] == 0.0


def test_MUTATION_injected_probe_identity_is_caught():
    """The exact legacy renderer string."""
    leaky = "--- observation from automated probe: inspect_permissions ---"
    r = check_oracle_isolation([CLEAN_PROMPT, leaky], PROBES,
                               DisclosurePolicy.FULL, len(PROBES))
    assert not r.passed and "inspect_permissions" in r.detail


def test_MUTATION_injected_rationale_is_caught():
    r = check_oracle_isolation(
        [CLEAN_PROMPT + "\nselection rationale: cheapest discriminating"],
        PROBES, DisclosurePolicy.FULL, len(PROBES))
    assert not r.passed


def test_MUTATION_single_disclosure_policy_is_caught_even_with_clean_prompts():
    """Prompt hygiene is not sufficient: the structural bound must also hold."""
    r = check_oracle_isolation([CLEAN_PROMPT], PROBES,
                               DisclosurePolicy.SINGLE, len(PROBES))
    assert not r.passed and "discloses" in r.detail


def test_MUTATION_masked_k_is_reported_as_not_isolated():
    r = check_oracle_isolation([CLEAN_PROMPT], PROBES,
                               DisclosurePolicy.MASKED_K, 8)
    assert not r.passed


# ── 5. raw-before-score ──────────────────────────────────────────────────────

def test_raw_before_score_passes_on_a_verified_chain(tmp_path):
    led = CommitLedger(str(tmp_path / "c.jsonl"))
    led.commit_raw("u1", str(tmp_path / "u1.txt"), "payload")
    assert check_raw_before_score(led, ["u1"]).passed


def test_MUTATION_deleted_hash_blocks_scoring(tmp_path):
    """The historical state: 527/540 units marked committed with no hash."""
    led = CommitLedger(str(tmp_path / "c.jsonl"))
    p = tmp_path / "u1.txt"
    p.write_text("payload")
    led.reconstruct("u1", str(p), "concurrency loss")
    r = check_raw_before_score(led, ["u1"])
    assert not r.passed and "UNVERIFIABLE" in r.detail


def test_MUTATION_tampered_artifact_blocks_scoring(tmp_path):
    led = CommitLedger(str(tmp_path / "c.jsonl"))
    p = tmp_path / "u1.txt"
    led.commit_raw("u1", str(p), "payload")
    p.write_text("tampered")
    assert not check_raw_before_score(led, ["u1"]).passed


# ── 6. parse-failure behaviour ───────────────────────────────────────────────

class _Client:
    def __init__(self, script):
        self.script, self.prompts = list(script), []

    def complete(self, system, user, requested_max_tokens, temperature):
        self.prompts.append(user)
        return {"text": self.script.pop(0) if self.script else "bad",
                "provider": {"resolved_model": "stub", "finish_reason": "stop",
                             "completion_tokens": 10}}


def _run(script):
    spec = ConditionSpec(name="G", action_source="model",
                         disclosure=DisclosurePolicy.FULL, max_rounds=3,
                         max_payment=7, max_model_calls=8, max_repair_attempts=2)
    c = _Client(script)
    ctrl = DiagnosticController(spec, CONTRACT, PROBES,
                                {p: f"desc {i}" for i, p in enumerate(PROBES)},
                                {p: 1 for p in PROBES},
                                ("authority", "environment", "implementation",
                                 "unknown"), c)
    res = ctrl.run_unit("u1", "e", "ns", "c1", 1, "Q?", "evidence text",
                        PROBES_BY_CASE["c1"], ("inspect_permissions",))
    return res, c.prompts


def test_parse_failure_behaviour_passes_on_the_repaired_controller():
    res, prompts = _run(["garbage"] * 6)
    assert check_parse_failure_behaviour(res, prompts).passed


def test_MUTATION_fake_budget_exhaustion_is_caught():
    """Simulates the audited path: the model is told the budget is exhausted
    after a format failure."""
    res, prompts = _run(["garbage"] * 6)
    prompts.append("Budget exhausted (0/7 payment, 1/3 rounds). "
                   "No further acquisition possible.")
    r = check_parse_failure_behaviour(res, prompts)
    assert not r.passed and "budget exhausted" in r.detail


def test_MUTATION_silent_unknown_is_caught():
    res, prompts = _run(["garbage"] * 6)
    object.__setattr__(res, "final_locus", "unknown")
    r = check_parse_failure_behaviour(res, prompts)
    assert not r.passed and "final_locus was set" in r.detail


def test_MUTATION_payment_consumed_by_a_parse_failure_is_caught():
    res, prompts = _run(["garbage"] * 6)
    object.__setattr__(res, "payment_used", 3)
    assert not check_parse_failure_behaviour(res, prompts).passed


# ── suite ────────────────────────────────────────────────────────────────────

def test_suite_reports_failures_rather_than_a_flat_pass_count(tmp_path):
    led = CommitLedger(str(tmp_path / "c.jsonl"))
    led.commit_raw("u1", str(tmp_path / "u1.txt"), "x")
    good = run_suite([
        check_gate_negative_control(CONTRACT, PROBES_BY_CASE, TRUTH),
        check_model_uniformity([rec(), rec()]),
        check_pilot_full_separation(sep("p", ["a"]), sep("f", ["b"])),
        check_oracle_isolation([CLEAN_PROMPT], PROBES, DisclosurePolicy.FULL, 3),
        check_raw_before_score(led, ["u1"]),
    ])
    assert good["all_passed"] and good["n_pass"] == 5

    bad = run_suite([check_model_uniformity([rec(), rec("other")])])
    assert bad["n_fail"] == 1 and not bad["all_passed"]

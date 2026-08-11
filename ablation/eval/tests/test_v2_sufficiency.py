"""R-10 regression: composable sufficiency predicates and topology re-derivation.

The corpus files are 0600 evaluator-side artifacts; tests that need them skip
cleanly when they are unreadable.
"""

import json
import os

import pytest

from ..v2.sufficiency import (FALSE, LEGACY_CONTRACT, PROPOSED_CONTRACT, TRUE,
                              UNKNOWN, RuleError, SufficiencyContract,
                              derive_topology, evaluate, exclusion_frontier,
                              referenced_probes,
                              topology_summary)

CORPUS = os.path.join(os.path.dirname(os.path.dirname(
    os.path.dirname(os.path.abspath(__file__)))), "corpus")

COSTS = {"inspect_target_binding": 1, "inspect_source_symbols": 1,
         "inspect_environment": 1, "inspect_permissions": 1,
         "inspect_fixture_integrity": 1, "validate_semantic_model": 3,
         "rerun_verifier_isolated": 5, "inspect_verifier_config": 1}


def _load(name):
    path = os.path.join(CORPUS, name)
    try:
        if name.endswith(".jsonl"):
            rows = {}
            with open(path) as f:
                for line in f:
                    d = json.loads(line)
                    if "case_id" in d:
                        rows[d["case_id"]] = d
            return rows
        with open(path) as f:
            return json.load(f)
    except (OSError, PermissionError):
        pytest.skip(f"{name} not readable in this environment")


# ── three-valued logic ───────────────────────────────────────────────────────

def test_unacquired_probe_is_unknown_not_false():
    r = {"probe": "p1", "field": "x", "op": "is_true"}
    assert evaluate(r, {}) is UNKNOWN
    assert evaluate(r, {"p1": {"x": True}}) is TRUE
    assert evaluate(r, {"p1": {"x": False}}) is FALSE


def test_absence_of_evidence_can_never_qualify_a_claim():
    """The reason three-valued logic is required: with boolean logic,
    `not(<unacquired probe>)` would evaluate True and an unacquired probe would
    satisfy an exclusion premise."""
    rule = {"not": {"probe": "p1", "field": "bad", "op": "is_true"}}
    assert evaluate(rule, {}) is UNKNOWN
    c = SufficiencyContract("t", {"L": rule})
    assert c.settlement_eligible("L", {}) is False
    assert c.settlement_eligible("L", {"p1": {"bad": False}}) is True


def test_all_of_short_circuits_to_false_over_unknown():
    rule = {"all_of": [{"probe": "p1", "field": "x", "op": "is_true"},
                       {"probe": "p2", "field": "y", "op": "is_true"}]}
    assert evaluate(rule, {"p1": {"x": False}}) is FALSE      # definitely false
    assert evaluate(rule, {"p1": {"x": True}}) is UNKNOWN     # p2 unacquired


def test_any_of_short_circuits_to_true_over_unknown():
    rule = {"any_of": [{"probe": "p1", "field": "x", "op": "is_true"},
                       {"probe": "p2", "field": "y", "op": "is_true"}]}
    assert evaluate(rule, {"p1": {"x": True}}) is TRUE
    assert evaluate(rule, {"p1": {"x": False}}) is UNKNOWN


def test_locus_with_no_rule_is_never_eligible():
    assert LEGACY_CONTRACT.qualifies("verifier", {}) is FALSE


def test_unrecognised_node_raises():
    with pytest.raises(RuleError):
        evaluate({"bogus": 1}, {})


def test_referenced_probes_walks_the_tree():
    assert referenced_probes(PROPOSED_CONTRACT.rules["verifier"]) == {
        "inspect_target_binding", "inspect_source_symbols",
        "inspect_environment", "inspect_permissions",
        "inspect_fixture_integrity", "validate_semantic_model",
        "rerun_verifier_isolated"}


# ── backward compatibility: the language is a strict extension ───────────────

def test_legacy_contract_reproduces_the_frozen_signal_rules():
    frozen = _load("governed-sufficiency-contract.json")
    frozen_pairs = {(v["probe"], v["locus"])
                    for v in frozen["signal_rules"].values()}
    new_pairs = {(p, l) for l in LEGACY_CONTRACT.loci()
                 for p in LEGACY_CONTRACT.probes_for(l)}
    assert new_pairs == frozen_pairs
    assert "verifier" not in LEGACY_CONTRACT.loci()


def test_legacy_contract_reproduces_the_historical_topology():
    """43 gate-feasible determinate / 10 gate-infeasible / 7 true-UNKNOWN."""
    probes = {cid: d["probes"] for cid, d in _load("acquisition-probes.jsonl").items()}
    keys = _load("governed-keys.json")
    truth = {c: v["true_locus"] for c, v in keys.items()}
    topo = derive_topology(probes, LEGACY_CONTRACT, COSTS, 7)
    s = topology_summary(topo, truth)
    assert s["counts"]["gate_feasible_determinate"] == 43
    assert s["counts"]["gate_infeasible_determinate"] == 10
    assert s["counts"]["true_unknown"] == 7
    assert s["counts"]["unknown_false_positives"] == 0
    assert set(s["gate_infeasible_determinate"]) == {
        c for c, v in keys.items() if v.get("gate_infeasible")}


# ── the proposed extension, measured but NOT adopted ─────────────────────────

def test_proposed_verifier_rule_is_10_of_10_under_full_observation():
    """The audit's F9 measurement, reproduced: the evidence IS present."""
    probes = {cid: d["probes"] for cid, d in _load("acquisition-probes.jsonl").items()}
    keys = _load("governed-keys.json")
    truth = {c: v["true_locus"] for c, v in keys.items()}
    topo = derive_topology(probes, PROPOSED_CONTRACT)      # no budget filter
    s = topology_summary(topo, truth)
    verifier_cases = [c for c, t in truth.items() if t == "verifier"]
    assert all("verifier" in topo[c] for c in verifier_cases)   # 10/10
    assert s["counts"]["gate_infeasible_determinate"] == 0
    assert s["counts"]["unknown_false_positives"] == 1
    assert s["unknown_false_positives"][0][0] == "df-14"        # the cost


def test_CORRECTION_proposed_rule_is_unaffordable_at_the_frozen_budget():
    """Correction to the audit's F9 conclusion.

    The audit said "the limitation is the rule language, not the evidence
    representation". Implementing the extension shows that is incomplete: the
    exclusion premise references seven probes costing 13 against a frozen
    budget of 7, so no acquisition order can reach it."""
    probes = {cid: d["probes"] for cid, d in _load("acquisition-probes.jsonl").items()}
    keys = _load("governed-keys.json")
    truth = {c: v["true_locus"] for c, v in keys.items()}
    need = PROPOSED_CONTRACT.probes_for("verifier")
    assert sum(COSTS[p] for p in need) == 13 > 7
    topo = derive_topology(probes, PROPOSED_CONTRACT, COSTS, 7)
    s = topology_summary(topo, truth)
    assert s["counts"]["gate_infeasible_determinate"] == 10   # still infeasible
    assert s["counts"]["unknown_false_positives"] == 0


def test_no_affordable_exclusion_rule_is_safe():
    """Full frontier: every affordable rule that reaches 10/10 buys it with
    false positives on OTHER determinate loci, which is worse than leaving the
    verifier cases infeasible."""
    probes = {cid: d["probes"] for cid, d in _load("acquisition-probes.jsonl").items()}
    keys = _load("governed-keys.json")
    truth = {c: v["true_locus"] for c, v in keys.items()}
    rows = exclusion_frontier(
        probes, truth, COSTS, max_payment=7, target_locus="verifier",
        positive_signal={"probe": "rerun_verifier_isolated",
                         "field": "isolated_result", "op": "eq",
                         "value": "failed"})
    assert len(rows) == 22
    clean = [r for r in rows if r["unknown_false_positives"] == 0
             and r["other_locus_false_positives"] == 0]
    assert clean == [], "no affordable rule achieves zero false positives"
    best = rows[0]
    assert best["sensitivity"] == 10
    assert best["other_locus_false_positives"] >= 6
    assert min(r["other_locus_false_positives"] for r in rows) >= 6


def test_proposed_contract_is_explicitly_not_adopted():
    assert "NOT-ADOPTED" in PROPOSED_CONTRACT.version
    assert LEGACY_CONTRACT.fingerprint() != PROPOSED_CONTRACT.fingerprint()


def test_contract_fingerprint_is_stable_and_change_sensitive():
    a = SufficiencyContract("v", {"L": {"probe": "p", "field": "f", "op": "is_true"}})
    b = SufficiencyContract("v", {"L": {"probe": "p", "field": "f", "op": "is_true"}})
    c = SufficiencyContract("v", {"L": {"probe": "p", "field": "f", "op": "is_false"}})
    assert a.fingerprint() == b.fingerprint() != c.fingerprint()


def test_budget_gates_feasibility():
    """A locus whose rule needs unaffordable probes is not feasible."""
    obs = {"rerun_verifier_isolated": {"isolated_result": "compile_error"}}
    contract = SufficiencyContract("t", {"implementation": {
        "probe": "rerun_verifier_isolated", "field": "isolated_result",
        "op": "eq", "value": "compile_error"}})
    assert derive_topology({"c": obs}, contract, COSTS, 7)["c"] == ("implementation",)
    assert derive_topology({"c": obs}, contract, COSTS, 3)["c"] == ()

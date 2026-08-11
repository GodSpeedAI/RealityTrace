"""R-10: composable sufficiency predicates.

Audit defect closed
-------------------
F9.  The report claimed 10 verifier cases (30 units) are "structurally
unresolvable under the current evidence representation". Pass B showed the
information IS present: the conjunctive rule

    every other structural probe is clean AND isolated rerun == "failed"

separates all 10 verifier cases with a single false positive among the 7
true-UNKNOWN cases. The limit is the rule LANGUAGE — `SIGNAL_RULES` is a
disjunction of single-probe predicates and cannot express diagnosis by
exclusion, which is the operator the SXR diagnostic loop is built around.

Repair
------
The smallest general extension: `all_of` / `any_of` / `not` over probe-field
predicates. Deliberately not an unrestricted rule engine — no arithmetic, no
cross-case state, no ground-truth access, no free-text matching on error
content (which would approach diagnosis rather than structural sufficiency).

Three-valued evaluation is essential. The gate evaluates over ACQUIRED
observations only, so a predicate on an unacquired probe is UNKNOWN, not False.
Without this, `not(<unacquired probe>)` would evaluate True and absence of
evidence would qualify a claim. Only TRUE qualifies a settlement.

The legacy nine signal rules are expressible in this language and are provided
as `LEGACY_CONTRACT`, which reproduces the historical topology exactly (asserted
by test). The conjunctive verifier rule is provided separately as a PROPOSAL
and is NOT adopted: it trades the corpus's zero-false-positive property for ten
newly settleable cases, which is an owner governance decision.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass

TRUE, FALSE, UNKNOWN = True, False, None

OPS = ("eq", "ne", "in", "nonempty", "empty", "is_true", "is_false",
       "present", "absent")


class RuleError(Exception):
    pass


def _eval_predicate(pred: dict, observations: dict):
    """Evaluate one probe-field predicate under three-valued logic."""
    pid = pred["probe"]
    if pid not in observations:
        return UNKNOWN                      # not acquired: no evidence either way
    obs = observations[pid] or {}
    field = pred.get("field")
    op = pred["op"]
    if op == "present":
        return field in obs
    if op == "absent":
        return field not in obs
    value = obs.get(field)
    if op == "eq":
        return value == pred["value"]
    if op == "ne":
        return value != pred["value"]
    if op == "in":
        return value in pred["value"]
    if op == "nonempty":
        return bool(value)
    if op == "empty":
        return not bool(value)
    if op == "is_true":
        return value is True
    if op == "is_false":
        return value is False
    raise RuleError(f"unsupported op {op!r}")


def evaluate(node: dict, observations: dict):
    """Three-valued evaluation of a rule node."""
    if not isinstance(node, dict):
        raise RuleError(f"rule node must be an object, got {type(node).__name__}")
    if "all_of" in node:
        vals = [evaluate(c, observations) for c in node["all_of"]]
        if any(v is FALSE for v in vals):
            return FALSE
        return UNKNOWN if any(v is UNKNOWN for v in vals) else TRUE
    if "any_of" in node:
        vals = [evaluate(c, observations) for c in node["any_of"]]
        if any(v is TRUE for v in vals):
            return TRUE
        return UNKNOWN if any(v is UNKNOWN for v in vals) else FALSE
    if "not" in node:
        v = evaluate(node["not"], observations)
        return UNKNOWN if v is UNKNOWN else (not v)
    if "probe" in node:
        return _eval_predicate(node, observations)
    raise RuleError(f"unrecognised rule node: {sorted(node)}")


def referenced_probes(node: dict) -> set:
    if "probe" in node:
        return {node["probe"]}
    for key in ("all_of", "any_of"):
        if key in node:
            out = set()
            for c in node[key]:
                out |= referenced_probes(c)
            return out
    if "not" in node:
        return referenced_probes(node["not"])
    return set()


@dataclass(frozen=True)
class SufficiencyContract:
    """A named, hashable set of per-locus sufficiency rules."""

    version: str
    rules: dict          # locus -> rule node
    description: str = ""

    def loci(self) -> tuple[str, ...]:
        return tuple(sorted(self.rules))

    def probes_for(self, locus: str) -> tuple[str, ...]:
        return tuple(sorted(referenced_probes(self.rules[locus])))

    def qualifies(self, locus: str, observations: dict):
        """Three-valued. Only TRUE qualifies a settlement."""
        rule = self.rules.get(locus)
        if rule is None:
            return FALSE          # no rule: this locus is not gate-settleable
        return evaluate(rule, observations)

    def settlement_eligible(self, locus: str, observations: dict) -> bool:
        return self.qualifies(locus, observations) is TRUE

    def fingerprint(self) -> str:
        blob = json.dumps({"version": self.version, "rules": self.rules},
                          sort_keys=True, separators=(",", ":"))
        return hashlib.sha256(blob.encode()).hexdigest()

    def to_dict(self) -> dict:
        return {"version": self.version, "description": self.description,
                "rules": self.rules, "fingerprint": self.fingerprint()}


def _p(probe, field, op, value=None):
    d = {"probe": probe, "field": field, "op": op}
    if value is not None:
        d["value"] = value
    return d


#: The nine historical single-probe signal rules, re-expressed. Reproduces the
#: frozen topology exactly (asserted by test) — this is the compatibility proof
#: that the language is a strict extension, not a redefinition.
LEGACY_CONTRACT = SufficiencyContract(
    version="governed-sufficiency-v1-as-composable",
    description="Exact re-expression of the frozen v1 disjunctive signal rules.",
    rules={
        "verification_plan": _p("inspect_target_binding", "all_exist", "is_false"),
        "implementation": {"any_of": [
            _p("inspect_source_symbols", "missing", "nonempty"),
            _p("rerun_verifier_isolated", "isolated_result", "eq", "compile_error"),
        ]},
        "environment": {"any_of": [
            _p("inspect_environment", "toolchain", "ne", "available"),
            _p("inspect_environment", "tmpdir", "eq", "not_writable"),
            _p("inspect_environment", "path", "eq", "restricted"),
            _p("inspect_verifier_config", "env_assignments", "ne", "(none)"),
        ]},
        "authority": _p("inspect_permissions", "all_accessible", "is_false"),
        "evidence_collection": {"any_of": [
            _p("inspect_fixture_integrity", "missing", "nonempty"),
            _p("inspect_fixture_integrity", "stale", "nonempty"),
        ]},
        "declared_model": _p("validate_semantic_model", "valid", "is_false"),
        "projection": _p("validate_semantic_model", "valid", "is_false"),
    },
)

#: All structural probes clean — the exclusion premise.
_ALL_CLEAN = {"all_of": [
    _p("inspect_target_binding", "all_exist", "is_true"),
    _p("inspect_source_symbols", "missing", "empty"),
    _p("inspect_environment", "toolchain", "eq", "available"),
    _p("inspect_environment", "tmpdir", "ne", "not_writable"),
    _p("inspect_environment", "path", "ne", "restricted"),
    _p("inspect_permissions", "all_accessible", "is_true"),
    _p("inspect_fixture_integrity", "missing", "empty"),
    _p("inspect_fixture_integrity", "stale", "empty"),
    {"not": _p("validate_semantic_model", "valid", "is_false")},
]}

#: PROPOSAL ONLY — not adopted, not used in any scored result.
#:
#: CORRECTION TO THE AUDIT (F9). The audit concluded "the limitation is the
#: rule language, not the evidence representation". Implementing the extension
#: showed that conclusion is incomplete. Under FULL observation this rule is
#: 10/10 on verifier with one true-UNKNOWN false positive (`df-14`) and zero
#: false positives on other determinate loci — so the information is indeed
#: present. But its premise references seven probes costing **13 payment units
#: against the frozen budget of 7**, so it is unreachable in the experiment as
#: designed. See `exclusion_frontier()`: every affordable truncation of the
#: premise buys its 10/10 sensitivity at 6-16 false positives on other
#: determinate loci, and no affordable rule achieves zero false positives.
#:
#: The corrected diagnosis is therefore three-way: the evidence exists, v1's
#: language could not express it, AND diagnosis-by-exclusion is intrinsically
#: expensive because it must rule everything else out.
PROPOSED_EXCLUSION_RULES = {
    "verifier": {"all_of": [
        _ALL_CLEAN,
        _p("rerun_verifier_isolated", "isolated_result", "eq", "failed"),
    ]},
}

PROPOSED_CONTRACT = SufficiencyContract(
    version="governed-sufficiency-v2-PROPOSED-NOT-ADOPTED",
    description=("v1 rules plus a conjunctive exclusion rule for `verifier`. "
                 "NOT ADOPTED: unaffordable at max_payment=7 (premise costs 13) "
                 "and every affordable truncation is unsafe. Owner governance "
                 "decision pending."),
    rules={**LEGACY_CONTRACT.rules, **PROPOSED_EXCLUSION_RULES},
)

#: Per-probe "this probe shows nothing anomalous" premises, used to enumerate
#: the exclusion frontier. Structural only; no error-content interpretation.
CLEAN_PREMISES = {
    "inspect_target_binding": [_p("inspect_target_binding", "all_exist", "is_true")],
    "inspect_source_symbols": [_p("inspect_source_symbols", "missing", "empty")],
    "inspect_environment": [
        _p("inspect_environment", "toolchain", "eq", "available"),
        _p("inspect_environment", "tmpdir", "ne", "not_writable"),
        _p("inspect_environment", "path", "ne", "restricted")],
    "inspect_permissions": [_p("inspect_permissions", "all_accessible", "is_true")],
    "inspect_fixture_integrity": [
        _p("inspect_fixture_integrity", "missing", "empty"),
        _p("inspect_fixture_integrity", "stale", "empty")],
    "validate_semantic_model": [
        {"not": _p("validate_semantic_model", "valid", "is_false")}],
    "inspect_verifier_config": [
        _p("inspect_verifier_config", "env_assignments", "eq", "(none)")],
}


def exclusion_frontier(probes_by_case: dict, true_locus: dict, costs: dict,
                       max_payment: int, target_locus: str,
                       positive_signal: dict) -> list[dict]:
    """Enumerate EVERY affordable conjunctive exclusion rule for a locus.

    This is a complete characterisation of the reachable design space, not a
    search for the rule that scores best: the whole frontier is returned and
    reported, so a favourable point cannot be selected after the fact.

    Returns one row per affordable premise subset with its cost, sensitivity on
    the target locus, and false positives split into true-UNKNOWN cases and
    other determinate loci.
    """
    import itertools

    pos_probe = positive_signal["probe"]
    targets = [c for c, t in true_locus.items() if t == target_locus]
    unknowns = [c for c, t in true_locus.items() if t == "unknown"]
    others = [c for c, t in true_locus.items()
              if t not in (target_locus, "unknown")]

    rows = []
    pool = sorted(CLEAN_PREMISES)
    for r in range(len(pool) + 1):
        for sub in itertools.combinations(pool, r):
            cost = costs[pos_probe] + sum(costs[p] for p in sub)
            if cost > max_payment:
                continue
            premise = [positive_signal]
            for p in sub:
                premise += CLEAN_PREMISES[p]
            contract = SufficiencyContract(
                "frontier-probe", {target_locus: {"all_of": premise}})

            def fires(cid):
                return contract.qualifies(target_locus,
                                          probes_by_case[cid]) is TRUE

            rows.append({
                "premise_probes": list(sub),
                "cost": cost,
                "sensitivity": sum(1 for c in targets if fires(c)),
                "n_target": len(targets),
                "unknown_false_positives": sum(1 for c in unknowns if fires(c)),
                "other_locus_false_positives": sum(1 for c in others if fires(c)),
            })
    rows.sort(key=lambda r: (-r["sensitivity"],
                             r["unknown_false_positives"]
                             + r["other_locus_false_positives"], r["cost"]))
    return rows


def derive_topology(probes_by_case: dict, contract: SufficiencyContract,
                    costs: dict | None = None, max_payment: int | None = None
                    ) -> dict:
    """Deterministically re-derive which loci each case can settle.

    Evaluated under FULL acquisition — the most permissive reachable evidence
    state — so a locus absent here is unreachable under any acquisition order.
    If costs and a budget are supplied, a locus is feasible only when the probes
    its rule references are jointly affordable.
    """
    out = {}
    for cid, obs in probes_by_case.items():
        settleable = []
        for locus in contract.loci():
            if contract.qualifies(locus, obs) is not TRUE:
                continue
            if costs is not None and max_payment is not None:
                need = contract.probes_for(locus)
                if sum(costs[p] for p in need if p in obs) > max_payment:
                    continue
            settleable.append(locus)
        out[cid] = tuple(settleable)
    return out


def topology_summary(topology: dict, true_locus: dict) -> dict:
    """Partition cases into gate-feasible / gate-infeasible / true-UNKNOWN."""
    feasible, infeasible, unknown_cases, false_positive = [], [], [], []
    for cid, settleable in sorted(topology.items()):
        tl = true_locus[cid]
        if tl == "unknown":
            unknown_cases.append(cid)
            if settleable:
                false_positive.append((cid, tuple(settleable)))
        elif tl in settleable:
            feasible.append(cid)
        else:
            infeasible.append(cid)
    return {
        "gate_feasible_determinate": feasible,
        "gate_infeasible_determinate": infeasible,
        "true_unknown": unknown_cases,
        "unknown_false_positives": false_positive,
        "counts": {
            "gate_feasible_determinate": len(feasible),
            "gate_infeasible_determinate": len(infeasible),
            "true_unknown": len(unknown_cases),
            "unknown_false_positives": len(false_positive),
        },
    }

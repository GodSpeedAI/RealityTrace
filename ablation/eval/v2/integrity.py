"""R-06: integrity checks that can actually fail.

Audit defect closed
-------------------
A-04.  The governed gate reported "9/9 PASS" while:
  * `check_gate_negative_control` had `pass` in both comparison branches, never
    incremented its counter, and unconditionally returned True — it could not
    fail under any input;
  * `check_model_uniformity` was an empty loop returning True, and was not even
    registered in `run_gate`;
  * `check_pilot_full_separation` only asserted that no filename in the pilot
    directory contained the substring "full";
  * `check_oracle_isolation` grepped the SOURCE TEXT of `oracle_select` and
    `format_observation` for "true_locus"/"discriminating", which cannot detect
    a leak that runs through the probe's identity;
  * `check_raw_committed` tested only that a `.txt` file exists beside each
    scored file, not that its content matches what was committed.

Repair
------
Every check below has a real assertion and a paired mutation fixture in
`eval/tests/test_v2_integrity.py` that makes it fail. A check with no failing
mutation is not evidence.
"""

from __future__ import annotations

from dataclasses import dataclass

from .channel import AnswerLeak, DisclosurePolicy, PromptLeakGuard, leakage_bound
from .controller import HARNESS_DEFAULTED, Terminal
from .ledger import CommitViolation
from .sufficiency import TRUE


@dataclass(frozen=True)
class CheckResult:
    name: str
    passed: bool
    detail: str = ""
    evidence: dict | None = None

    def to_dict(self) -> dict:
        return {"name": self.name, "passed": self.passed,
                "detail": self.detail, "evidence": self.evidence or {}}


# ── 1. gate negative control ─────────────────────────────────────────────────

#: Keys a leaking gate might smuggle truth through. Injected by the negative
#: control to prove the gate ignores them.
_TRUTH_DECOYS = ("__true_locus__", "true_locus", "answer", "locus")


def check_gate_negative_control(contract, probes_by_case, true_locus) -> CheckResult:
    """The gate must respond to EVIDENCE, not to which locus is true.

    Three executable assertions. The audited version's only comparison had
    `pass` in both branches; an assertion that cannot fire is the defect this
    check exists to prevent, so each of these has a paired mutation test.

      (a) TRUTH-INJECTION INVARIANCE. Re-evaluate every verdict with the true
          locus smuggled into the observation dict under several plausible key
          names. A gate that consults any side channel changes its answer.
      (b) OBSERVATION DETERMINISM. Any two cases with byte-identical
          observations must receive identical verdicts regardless of their
          labels. (Skipped, and reported as skipped, when the corpus contains
          no such pair.)
      (c) LICENSED-BUT-FALSE EXISTS. The gate must somewhere license a locus
          that is not the true one. A gate that only ever licenses the truth is
          an oracle, not a sufficiency test.
    """
    def verdicts(obs_by_case):
        return {(cid, locus): contract.qualifies(locus, obs_by_case[cid]) is TRUE
                for cid in obs_by_case for locus in contract.loci()}

    baseline = verdicts(probes_by_case)

    # (a) truth-injection invariance
    for decoy in _TRUTH_DECOYS:
        injected = {cid: {**obs, decoy: true_locus[cid]}
                    for cid, obs in probes_by_case.items()}
        if verdicts(injected) != baseline:
            return CheckResult(
                "gate_negative_control", False,
                f"gate verdicts changed when the true locus was injected under "
                f"key {decoy!r}: the gate is reading a truth side channel")

    # (b) observation determinism across differently-labelled identical evidence
    import json as _json
    by_obs: dict[str, list[str]] = {}
    for cid, obs in probes_by_case.items():
        by_obs.setdefault(_json.dumps(obs, sort_keys=True, default=str),
                          []).append(cid)
    twins = [cids for cids in by_obs.values()
             if len(cids) > 1 and len({true_locus[c] for c in cids}) > 1]
    for cids in twins:
        first = {locus: baseline[(cids[0], locus)] for locus in contract.loci()}
        for other in cids[1:]:
            if {locus: baseline[(other, locus)] for locus in contract.loci()} != first:
                return CheckResult(
                    "gate_negative_control", False,
                    f"cases {cids[0]} and {other} have identical observations "
                    "but different verdicts: the gate is not a function of "
                    "the evidence")

    # (c) the gate must be able to license a false claim
    wrong_licensed = [(cid, locus) for (cid, locus), ok in baseline.items()
                      if ok and locus != true_locus[cid]]
    if not wrong_licensed:
        return CheckResult("gate_negative_control", False,
                           "the gate licensed only true loci across the whole "
                           "corpus, which is the signature of a leaking gate")

    return CheckResult(
        "gate_negative_control", True,
        f"invariant under truth injection ({len(_TRUTH_DECOYS)} decoy keys); "
        f"{len(twins)} identical-observation group(s) consistent; "
        f"{len(wrong_licensed)} evidentially-licensed-but-false pairings exist",
        {"n_licensed_but_false": len(wrong_licensed),
         "examples": sorted(wrong_licensed)[:5],
         "identical_observation_groups": len(twins),
         "decoy_keys_tested": list(_TRUTH_DECOYS)})


# ── 2. model uniformity, from provider-returned records ──────────────────────

def check_model_uniformity(execution_records) -> CheckResult:
    """Verify inference identity from what the PROVIDER returned.

    The audited check verified nothing; the audited usage sidecars recorded
    harness constants, so "540/540 resolved_model = glm-5.2" was a statement
    about a constant.
    """
    records = list(execution_records)
    if not records:
        return CheckResult("model_uniformity", False, "no execution records")

    def field(rec, section, key):
        return rec.get(section, {}).get(key)

    resolved = {field(r, "returned", "resolved_model") for r in records}
    endpoints = {field(r, "requested", "endpoint") for r in records}
    temps = {field(r, "requested", "temperature") for r in records}
    maxtok = {field(r, "requested", "max_tokens") for r in records}

    if any(v is None for v in resolved):
        n = sum(1 for r in records
                if field(r, "returned", "resolved_model") is None)
        return CheckResult("model_uniformity", False,
                           f"{n}/{len(records)} records have no "
                           "provider-returned resolved_model")
    for name, vals in (("resolved_model", resolved), ("endpoint", endpoints),
                       ("temperature", temps), ("max_tokens", maxtok)):
        if len(vals) > 1:
            return CheckResult("model_uniformity", False,
                               f"non-uniform {name}: {sorted(map(str, vals))}")
    return CheckResult("model_uniformity", True,
                       f"{len(records)} calls uniform",
                       {"resolved_model": sorted(resolved)[0],
                        "endpoint": sorted(endpoints)[0],
                        "temperature": sorted(map(str, temps))[0],
                        "requested_max_tokens": sorted(map(str, maxtok))[0]})


# ── 3. pilot / full separation ───────────────────────────────────────────────

def check_pilot_full_separation(pilot: dict, full: dict) -> CheckResult:
    """Namespace, run id, checkpoint store, artifact dir AND unit ids."""
    problems = []
    for key in ("namespace", "run_id", "checkpoint_store", "artifact_dir"):
        if pilot.get(key) == full.get(key):
            problems.append(f"shared {key}: {pilot.get(key)!r}")
    overlap = set(pilot.get("unit_ids", ())) & set(full.get("unit_ids", ()))
    if overlap:
        problems.append(f"{len(overlap)} colliding unit ids "
                        f"(e.g. {sorted(overlap)[:3]})")
    if problems:
        return CheckResult("pilot_full_separation", False, "; ".join(problems))
    return CheckResult("pilot_full_separation", True,
                       f"{len(pilot.get('unit_ids', ()))} pilot / "
                       f"{len(full.get('unit_ids', ()))} full ids disjoint")


# ── 4. oracle isolation, from rendered prompts ───────────────────────────────

def check_oracle_isolation(rendered_prompts, canonical_probe_ids, policy,
                           n_actions) -> CheckResult:
    """Inspect the bytes actually sent to the model, plus the structural bound."""
    guard = PromptLeakGuard(canonical_probe_ids)
    hits = []
    for i, p in enumerate(rendered_prompts):
        found = guard.findings(p)
        if found:
            hits.append((i, sorted(set(found))))
    if hits:
        return CheckResult("oracle_isolation", False,
                           f"{len(hits)} rendered prompt(s) contain "
                           f"answer-bearing material: {hits[:3]}")
    bound = leakage_bound(policy, n_actions)
    if not bound["answer_isolated"]:
        return CheckResult(
            "oracle_isolation", False,
            f"disclosure policy {bound['policy']} discloses "
            f"{bound['bits_disclosed']} bits about the locus "
            f"(consistency set {bound['loci_consistent_with_delivery']}/"
            f"{n_actions})", bound)
    return CheckResult("oracle_isolation", True,
                       f"{len(rendered_prompts)} prompts clean; delivery "
                       "carries 0 bits", bound)


# ── 5. raw-before-score content binding ──────────────────────────────────────

def check_raw_before_score(ledger, unit_ids) -> CheckResult:
    """Content identity, not file existence."""
    failures = []
    for uid in unit_ids:
        try:
            ledger.assert_scorable(uid)
        except CommitViolation as e:
            failures.append(str(e))
    if failures:
        return CheckResult("raw_before_score", False,
                           f"{len(failures)} unit(s) not scorable: "
                           f"{failures[:2]}")
    summary = ledger.provenance_summary()
    return CheckResult("raw_before_score", True,
                       f"{summary['verified']} verified content chains", summary)


# ── 6. parse-failure behaviour ───────────────────────────────────────────────

FORBIDDEN_ON_PARSE_FAILURE = (
    "budget exhausted", "no further acquisition possible",
    "no further evidence acquisition is possible")


def check_parse_failure_behaviour(result, prompts) -> CheckResult:
    """A malformed decision must not become budget exhaustion, UNKNOWN, or a
    settlement (audit A-02)."""
    problems = []
    if Terminal(result.terminal_state) not in HARNESS_DEFAULTED:
        problems.append(f"terminal {result.terminal_state} is not an explicit "
                        "harness-default outcome")
    if result.final_locus is not None:
        problems.append(f"final_locus was set to {result.final_locus!r} despite "
                        "no valid decision")
    if result.payment_used:
        problems.append(f"payment {result.payment_used} consumed by a parse failure")
    if result.acquired_probes:
        problems.append("an action was executed without a valid decision")
    joined = "\n".join(prompts).lower()
    for phrase in FORBIDDEN_ON_PARSE_FAILURE:
        if phrase in joined:
            problems.append(f"model was told {phrase!r} after a format failure")
    if problems:
        return CheckResult("parse_failure_behaviour", False, "; ".join(problems))
    return CheckResult("parse_failure_behaviour", True,
                       "explicit contract failure; no payment, no action, "
                       "no fabricated terminal")


# ── suite ────────────────────────────────────────────────────────────────────

def run_suite(checks) -> dict:
    results = [c.to_dict() for c in checks]
    n_pass = sum(1 for r in results if r["passed"])
    return {"checks": results, "n_checks": len(results), "n_pass": n_pass,
            "n_fail": len(results) - n_pass, "all_passed": n_pass == len(results)}

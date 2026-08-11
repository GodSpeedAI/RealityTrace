#!/usr/bin/env python3
"""FROZEN SCIENTIFIC CONTRACT — `evidence-revision-001`.

STATUS: FROZEN_NOT_STARTED. EXECUTION IS NOT AUTHORIZED, and no further
provider call of any kind is authorized — see `OWNER_STATE`.

This module declares. It does not execute. There is no provider client here and
no `__main__` path that contacts one; `freeze()` emits the contract document and
`power_analysis()` computes the design's sensitivity from first principles.

The power question is OWNER_ACKNOWLEDGED_UNRESOLVED: the owner accepted that
10pp is underpowered and declined every remedy, so the 10pp threshold stands
unchanged and the design and its power analysis are held for later
adjudication.

Everything below is fixed by owner decision at freeze time. Nothing here may be
changed after the pilot — in particular no threshold, no population, no
endpoint, and no comparison. A change to any frozen field requires a new
experiment id.

Provenance of the design
------------------------
The audit (`opus-audit/FINAL-ASSURANCE-REPORT.md`) returned PARTIALLY SETTLED —
REPAIR BEFORE CONTINUING. The repair (`opus-repair/`) rebuilt the instrument as
`eval/v2/` and rescored history under `post-audit-v2` without touching a single
historical byte. The owner then accepted the repair and fixed the decisions
recorded here.

Deliberately NOT adopted (owner §1)
-----------------------------------
The proposed conjunctive/exclusion sufficiency extension is NOT adopted, the
payment budget is NOT raised, and the verifier cases stay explicitly
GATE_INFEASIBLE. `sufficiency.PROPOSED_CONTRACT` and its exhaustive
affordable-rule analysis are retained as developmental evidence.
"""

from __future__ import annotations

import hashlib
import json
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(os.path.dirname(HERE)))

from eval.v2 import VERSION                                       # noqa: E402
from eval.v2.channel import DisclosurePolicy                      # noqa: E402
from eval.v2.controller import ConditionSpec, arm_difference_report  # noqa: E402
from eval.v2.legibility import contract_fingerprint as legibility_fp  # noqa: E402
from eval.v2.semantics import METRIC_CONSTRUCT                    # noqa: E402
from eval.v2.sufficiency import LEGACY_CONTRACT                   # noqa: E402

EXPERIMENT_ID = "evidence-revision-001"
STATUS = "FROZEN_NOT_STARTED — EXECUTION NOT AUTHORIZED"
HYPOTHESIS_ID = "H-EVIDENCE-REVISION"

HYPOTHESIS = (
    "When a controller holds evidence that contradicts its current diagnostic "
    "claim, it revises the claim. The historical chain observed 0/63 revisions "
    "and never tested this directly, because the acquisition channel, the "
    "action identity, the state continuity and the termination path all "
    "differed between the arms that were compared.")

#: Owner §4. The historical 0/63 is MOTIVATION for asking the question. It is
#: not a baseline effect size, not a formal null, and may not be reused as one
#: (owner §12) — it was produced by an instrument with a false
#: budget-exhaustion path, an oracle identity leak and no state continuity.
MOTIVATION_NOT_BASELINE = {
    "observation": "0/63 revisions in the governed acquisition experiment",
    "status": "MOTIVATION",
    "may_be_used_as_baseline": False,
    "may_be_used_as_formal_null": False,
    "reason": ("produced by the pre-repair instrument; audit NEW-01/NEW-02 and "
               "A-02/A-10 each independently affect the denominator"),
}

#: Owner §4. This experiment does NOT test action selection. The action is
#: harness-chosen in every arm precisely so that selection cannot confound the
#: revision measurement.
NOT_TESTED = (
    "action selection",
    "affordable navigation under a payment budget",
    "resource efficiency",
    "production-controller performance",
    "payment optimality",
)


# ── arms (owner §7) ──────────────────────────────────────────────────────────
#
# Five arms in two families. Within a family the delivered evidence is
# identical; only the manipulation named in the family differs.
#
#   control           R0             no new evidence is delivered
#   family GOVERNED   R1 · R2        bounded-leakage delivery, paid for out of
#                                    the frozen 7-unit budget under the legacy gate
#   family UPPER      R3 · R4        every observation delivered, without payment
#
# R2 and R4 deliver exactly the evidence R1 and R3 deliver, through the same
# opaque handles, with the same information content, rendered through the frozen
# legibility contract (owner §7). Legibility is therefore a pure representation
# manipulation, and `assert_semantic_equivalence` hash-binds the two renderings
# of every payload so it cannot quietly become anything else.
#
# Owner §2: the FULL-disclosure arms are an EXEMPT UPPER BOUND. That is
# represented explicitly as a governance class, never hidden as `payment = 0`,
# and results from R3/R4 may not be used to claim any item in NOT_TESTED.
#
# Owner §5: no arm intentionally removes original context. R0 withholds NEW
# evidence; it does not remove the case's original context. State continuity is
# a harness invariant of every arm (`dstate.REQUIRED_ROUND_STATE`).
#
# `DisclosurePolicy.SINGLE` appears in no arm. Its delivered set narrows the
# adversary's consistency set to 1, which is the historical leak; owner §6
# confines it to a regression fixture.

ARMS = {
    "R0": ConditionSpec(
        name="R0", action_source="harness", schema_visible=True, gated=True,
        disclosure=DisclosurePolicy.NONE,
        budget_semantics="ENFORCED", observation_rendering="raw_json"),
    "R1": ConditionSpec(
        name="R1", action_source="harness", schema_visible=True, gated=True,
        disclosure=DisclosurePolicy.MASKED_K,
        budget_semantics="ENFORCED", observation_rendering="raw_json"),
    "R2": ConditionSpec(
        name="R2", action_source="harness", schema_visible=True, gated=True,
        disclosure=DisclosurePolicy.MASKED_K,
        budget_semantics="ENFORCED",
        observation_rendering="labelled_structural"),
    "R3": ConditionSpec(
        name="R3", action_source="harness", schema_visible=True, gated=True,
        disclosure=DisclosurePolicy.FULL,
        budget_semantics="EXEMPT_UPPER_BOUND",
        observation_rendering="raw_json"),
    "R4": ConditionSpec(
        name="R4", action_source="harness", schema_visible=True, gated=True,
        disclosure=DisclosurePolicy.FULL,
        budget_semantics="EXEMPT_UPPER_BOUND",
        observation_rendering="labelled_structural"),
}

ARM_ROLE = {
    "R0": ("no-new-evidence control — the model reasons from the original "
           "context alone, raw rendering"),
    "R1": ("governed delivery — bounded-leakage observation set through opaque "
           "handles, raw rendering"),
    "R2": "R1's evidence, rendered through the frozen legibility contract",
    "R3": ("exempt upper bound — every observation delivered without payment, "
           "so the delivered set is identical across units and discloses zero "
           "bits about the locus; raw rendering"),
    "R4": "R3's evidence, rendered through the frozen legibility contract",
}

GOVERNANCE_CLASS = {
    "R0": "NO_EVIDENCE_CONTROL", "R1": "GOVERNED", "R2": "GOVERNED",
    "R3": "EXEMPT_UPPER_BOUND", "R4": "EXEMPT_UPPER_BOUND",
}

#: Owner §6. A leaking channel is retained ONLY as a deterministic regression
#: fixture proving the leak is detected. It is not an experimental arm and is
#: not executed against the provider.
REGRESSION_FIXTURES = {
    "SINGLE_LEGACY_LEAK": ("deterministic fixture only; asserts PromptLeakGuard "
                           "fires on the historical bijection. NOT RUN as part "
                           "of the scientific experiment."),
}

#: Owner §7, from the audit's proposal, subsumed by the R-02 repair.
SUBSUMED_ARMS = {
    "context-preserved": ("state continuity became a harness invariant of every "
                          "arm, so it can no longer be a between-arm "
                          "manipulation; deleting an invariant to recreate an "
                          "arm would be a regression, not an experiment"),
}


# ── population (owner §8, §15) ───────────────────────────────────────────────

PRIMARY_POPULATION = {
    "definition": "gate-feasible determinate cases",
    "n_cases": 43,
    "why": ("the primary causal analysis must not be diluted by cases on which "
            "the frozen sufficiency rule cannot be satisfied at any affordable "
            "acquisition shape — those cases measure the rule, not the model"),
    "gate_infeasible_excluded": {
        "class": "verifier-locus cases",
        "disposition": "GATE_INFEASIBLE",
        "reported": "separately and descriptively; never pooled into the primary",
        "evidence": ("all 22 affordable acquisition shapes were enumerated "
                     "under max_payment=7; none achieves zero false positives; "
                     "the minimum other-locus false-positive count is 6"),
    },
    "corpus_adjudication": {
        "ag-10": "VALID_DESPITE_MISSING_SIGNATURE — retained",
        "sr-07": "VALID_DESPITE_MISSING_SIGNATURE — retained",
        "df-06": "VALID_DESPITE_MISSING_SIGNATURE — retained",
        "basis": ("exit 127 means the toolchain was not found, so no output "
                  "existed against which a signature could match; the fault is "
                  "present and observable. Materially different from the df-04 "
                  "class, where exit was 0."),
        "effect_on_n": "none — 43 stands",
    },
}

#: Owner §15. Do not fabricate a historical explanation for this unit.
EXCLUSIONS = {
    "c096c6b111e05554": {
        "status": "PROVENANCE_UNRESOLVED",
        "disposition": "EXCLUDED_FROM_CORRECTED_CAUSAL_ANALYSIS",
        "note": ("the provenance of this unit could not be established from "
                 "the immutable record. No historical explanation is offered, "
                 "because none is supported."),
    },
}


# ── endpoints (owner §9, §10) ────────────────────────────────────────────────

PRIMARY_ENDPOINT = {
    "name": "R_correct",
    "definition": ("the unit's FINAL diagnostic claim names the true locus, "
                   "evaluated on the primary population"),
    "unit": "case",
    "gate_handling": ("scored on the model's last claim, independent of the "
                      "sufficiency gate's verdict, so the endpoint measures the "
                      "model and not the rule"),
    "construct": METRIC_CONSTRUCT.get("gate_blind_accuracy"),
}

SECONDARY_ENDPOINTS = {
    "revision_rate": ("P(final claim != round-1 claim | round-1 claim wrong). "
                      "The endpoint the audit identified as the chain's most "
                      "surprising unmeasured result."),
    "gated_accuracy": "correct AND gate-qualifying evidence held",
    "invocation_rate": "units with >= 1 acquisition",
    "matched_evidence_use": "utilisation on units where both arms held evidence",
    "output_class_rates": ("VALID_DECISION / OUTPUT_TRUNCATED / "
                           "MODEL_OUTPUT_CONTRACT_FAILURE / PARSER_FAILURE, "
                           "reported separately and never collapsed into "
                           "'incorrect'"),
    "terminal_state_distribution": "all six Terminal values, per arm",
    "response_mode": "modal share and entropy — labelled POST-HOC, not a test",
}

#: Mandatory covariate: an arm that truncates more will look worse for reasons
#: that have nothing to do with the hypothesis (audit NEW-02).
MANDATORY_COVARIATES = ("per-arm output-truncation rate",
                        "per-arm MODEL_OUTPUT_CONTRACT_FAILURE rate")


# ── comparisons (owner §11) ──────────────────────────────────────────────────

COMPARISONS = {
    "C1_evidence_effect": {
        "arms": ("R0", "R1"),
        "question": ("does delivering evidence at all change the final "
                     "diagnostic claim? This is the direct test of "
                     "H-EVIDENCE-REVISION."),
        "family": "primary",
        "declared_differences": "disclosure policy only",
    },
    "C2_legibility_governed": {
        "arms": ("R1", "R2"),
        "question": ("does rendering the SAME evidence legibly change final "
                     "accuracy under the governed budget?"),
        "family": "primary",
        "declared_differences": "observation rendering only",
    },
    "C3_legibility_upper": {
        "arms": ("R3", "R4"),
        "question": ("the same legibility contrast with evidence coverage at "
                     "its upper bound, so a null in C2 cannot be attributed to "
                     "there being too little evidence to render"),
        "family": "primary",
        "declared_differences": "observation rendering only",
    },
    "C4_coverage_headroom": {
        "arms": ("R1", "R3"),
        "question": ("how much of the remaining gap is attributable to "
                     "evidence coverage rather than to revision?"),
        "family": "secondary",
        "declared_differences": ("TWO — disclosure AND budget_semantics. This "
                                 "comparison does NOT isolate a single "
                                 "variable and may never be described as if it "
                                 "did (audit A-06)."),
        "constraint": ("R3 is an EXEMPT_UPPER_BOUND; this comparison bounds "
                       "headroom and may not be reported as controller "
                       "performance, affordable navigation, or payment "
                       "optimality"),
    },
}

#: `arm_difference_report` is emitted with the contract and with every run. A
#: comparison may be described as isolating a single variable only where that
#: report says `isolates_single_variable: true` for the pair — the check that
#: audit A-06 existed for.
ISOLATION_CLAIMS_MUST_BE_BACKED_BY = "arm_differences"


# ── statistical plan (owner §13) ─────────────────────────────────────────────

STATISTICAL_PLAN = {
    "primary_unit": "case",
    "repetition_handling": ("repetitions aggregated within case before pairing "
                            "(majority over reps; ties resolved as incorrect); "
                            "(case, rep) pairing may be reported alongside but "
                            "never alone — pseudo-replication, audit A-05"),
    "paired_test": "McNemar, exact binomial when b + c < 25",
    "alpha": 0.05,
    "sided": "two-sided",
    "interval": "Wilson score, 95%",
    "p_value_floor": "never reported as 0; underflow reported as p < 1e-15",
    "multiple_comparisons": ("three primary comparisons; the family and its "
                             "size are reported with every p-value"),
    "mandatory_covariates": list(MANDATORY_COVARIATES),
    "prespecified_effect_threshold_pp": 10,
}


def exact_mcnemar_p(b: int, c: int) -> float:
    """Two-sided exact McNemar (binomial sign test on discordant pairs)."""
    m = b + c
    if m == 0:
        return 1.0
    tail = sum(math.comb(m, k) for k in range(0, min(b, c) + 1)) * 0.5 ** m
    return min(1.0, 2.0 * tail)


def _min_significant_discordance(alpha: float = 0.05) -> int:
    """Smallest one-directional discordant count reaching significance."""
    b = 0
    while exact_mcnemar_p(b, 0) > alpha:
        b += 1
    return b


def power_exact_mcnemar(n: int, p_improve: float, p_worsen: float = 0.0,
                        alpha: float = 0.05) -> float:
    """Exact power of the two-sided McNemar test.

    Each eligible case is trinomial: discordant-improving with probability
    `p_improve`, discordant-worsening with probability `p_worsen`, concordant
    otherwise. Enumerated exactly — no normal approximation, which is what makes
    a 43-case design look better powered than it is.
    """
    p_conc = 1.0 - p_improve - p_worsen
    if p_conc < 0:
        raise ValueError("improvement and worsening probabilities exceed 1")
    power = 0.0
    for b in range(n + 1):
        for c in range(n - b + 1):
            k = n - b - c
            prob = (math.factorial(n) / (math.factorial(b) * math.factorial(c)
                                         * math.factorial(k))
                    * p_improve ** b * p_worsen ** c * p_conc ** k)
            if prob and exact_mcnemar_p(b, c) <= alpha:
                power += prob
    return power


def mde(n: int, target_power: float = 0.80, p_worsen: float = 0.0,
        alpha: float = 0.05) -> float:
    """Smallest improvement rate reaching `target_power`, to 1pp resolution."""
    for pp in range(1, 101):
        delta = pp / 100.0
        if delta + p_worsen > 1.0:
            break
        if power_exact_mcnemar(n, delta, p_worsen, alpha) >= target_power:
            return delta
    return float("nan")


#: Cases already correct under V-like behaviour cannot contribute a discordant
#: pair in the improving direction, so they are not eligible to detect the
#: effect. Measured on the rescored historical record: 9 of 43.
ELIGIBLE_CASE_COUNT = {
    "n_primary_population": 43,
    "already_correct_pre_intervention": 9,
    "n_eligible_for_improvement": 34,
    "source": "analysis/post-audit-v2 rescoring of the immutable record",
}


def power_analysis(alpha: float = 0.05) -> dict:
    """The owner §12 sensitivity argument, computed rather than asserted."""
    n_all = ELIGIBLE_CASE_COUNT["n_primary_population"]
    n_elig = ELIGIBLE_CASE_COUNT["n_eligible_for_improvement"]
    delta = STATISTICAL_PLAN["prespecified_effect_threshold_pp"] / 100.0
    b_min = _min_significant_discordance(alpha)

    rows = {}
    for label, n in (("n=43 (whole primary population)", n_all),
                     ("n=34 (eligible for improvement)", n_elig)):
        rows[label] = {
            "n": n,
            "power_monotone": round(power_exact_mcnemar(n, delta, 0.0, alpha), 4),
            "power_with_2pp_regression": round(
                power_exact_mcnemar(n, delta, 0.02, alpha), 4),
            "mde_at_80pct_power_pp": round(mde(n, 0.80, 0.0, alpha) * 100),
        }
    return {
        "alpha": alpha,
        "test": "two-sided exact McNemar",
        "prespecified_threshold_pp": round(delta * 100),
        "min_discordant_pairs_for_significance": b_min,
        "results": rows,
        "interpretation": (
            f"At alpha={alpha} the exact test cannot reach significance with "
            f"fewer than {b_min} one-directional discordant pairs. A 10pp "
            f"effect produces {delta * n_elig:.1f} expected discordant pairs on "
            f"{n_elig} eligible cases. The design is therefore underpowered for "
            f"the prespecified threshold by a wide margin, and the monotone "
            f"figures are a best case — any regression in the treated arm makes "
            f"it worse."),
    }


#: Owner adjudication of the §12 STOP, recorded verbatim in effect.
#:
#: The owner accepted the finding that 10pp is underpowered, and declined every
#: remedy: the threshold stays, the corpus is not enlarged, and the experiment
#: is not to be run as an estimation exercise yet. The power question is
#: therefore ACKNOWLEDGED AND UNRESOLVED, not closed. The design and the power
#: analysis remain available for later adjudication.
OWNER_STATE = {
    "status": "FROZEN_NOT_STARTED",
    "execution_authorized": False,
    "additional_provider_calls_authorized": False,
    "power_issue": "OWNER_ACKNOWLEDGED_UNRESOLVED",
    "original_10pp_threshold_preserved": True,
    "provider_calls_to_date": {
        "scored": 0,
        "non_scored_preflight": 2,
        "note": ("the two non-scored preflight calls were declared sufficient; "
                 "no further provider contact of any kind is authorized — not "
                 "pilot, preflight, smoke, scored, unscored validation, or "
                 "capability checks"),
    },
    "remedies_declined_by_owner": (
        "raising or lowering the prespecified threshold",
        "enlarging the corpus",
        "running the experiment as an estimation exercise",
    ),
    "adjudication_is_not_authorization": (
        "discussion or adjudication of the power question does not constitute "
        "permission to execute"),
}

#: Owner §12 requires a STOP if the power analysis finds the prespecified
#: threshold inappropriate. It does.
STOP_CONDITION = {
    "triggered": True,
    "clause": ("owner §12: 'If you believe 10pp is statistically or "
               "scientifically inappropriate after power analysis on the final "
               "eligible case count, STOP BEFORE LIVE EXECUTION and return the "
               "power/effect-size argument for owner adjudication.'"),
    "finding": ("10pp is underpowered on this population. The design detects "
                "10pp with roughly one chance in four at best, and closer to "
                "one in eight on the cases actually eligible to improve."),
    "what_is_NOT_proposed": (
        "No threshold is altered here. Owner §12 also forbids altering "
        "thresholds after the pilot, and altering one before the pilot to make "
        "the design look adequate would be benchmark tuning. The threshold "
        "stays at 10pp until the owner adjudicates."),
    "options_for_the_owner": (
        "(a) accept the design as an estimation exercise and report intervals "
        "rather than a significance verdict; "
        "(b) raise the declared threshold to the MDE, accepting that only a "
        "large effect is detectable; "
        "(c) enlarge the corpus, which is a corpus-construction task with its "
        "own validity requirements and is not a repair; "
        "(d) increase repetitions — note this does NOT help, because the "
        "primary inferential unit is the case (owner §13) and repetitions "
        "reduce within-case noise without adding pairs."),
    "recommendation": ("(a). It is the only option that neither weakens the "
                       "evidential standard nor tunes the instrument, and the "
                       "corrected instrument's first job is to produce honest "
                       "intervals on a question the old one could not address "
                       "at all."),
}


# ── preflight gate (owner §3, §14) ───────────────────────────────────────────

PREFLIGHT = {
    "required_before_any_pilot_unit": True,
    "status": "PASSED — 6/6, 2 model calls, 2026-08-11",
    "artifacts": ("analysis/post-audit-v2/preflight/preflight-report.json",
                  "analysis/post-audit-v2/preflight/preflight-report-recomputed.json"),
    "completion_ceiling": {
        "requested_max_tokens": 8192,
        "max_completion_tokens_observed": 7914,
        "finish_reason_at_maximum": "stop",
        "route_ceiling_below_request": False,
        "finding": ("the historical 4096-token ceiling was the REQUESTED "
                    "parameter, not a provider cap. Owner §3's contingency "
                    "('if the effective ceiling remains 4096, freeze that as "
                    "the actual experimental constraint') is NOT triggered."),
        "scope_limit": ("no preflight call was truncated, so 8192 is not "
                        "demonstrated reachable — the cap is only bounded "
                        "below at 7914"),
    },
}

PILOT = {
    "gate": ("owner §14: if the protocol passes the pilot, STOP before full "
             "execution and return the frozen pilot assurance summary"),
    "integrity_suite_must_pass": ("gate_negative_control, model_uniformity, "
                                  "pilot_full_separation, oracle_isolation, "
                                  "raw_before_score, parse_failure_behaviour"),
}


def contract() -> dict:
    """The frozen contract, as a document."""
    return {
        "experiment_id": EXPERIMENT_ID,
        "hypothesis_id": HYPOTHESIS_ID,
        "hypothesis": HYPOTHESIS,
        "status": STATUS,
        "instrument_version": VERSION,
        "motivation_not_baseline": MOTIVATION_NOT_BASELINE,
        "not_tested_by_this_experiment": list(NOT_TESTED),
        "arms": {k: {"role": ARM_ROLE[k],
                     "governance_class": GOVERNANCE_CLASS[k],
                     "spec": v.__dict__ | {"disclosure": v.disclosure.value}}
                 for k, v in ARMS.items()},
        "arm_differences": arm_difference_report(ARMS.values()),
        "subsumed_arms": SUBSUMED_ARMS,
        "regression_fixtures_not_arms": REGRESSION_FIXTURES,
        "primary_population": PRIMARY_POPULATION,
        "exclusions": EXCLUSIONS,
        "primary_endpoint": PRIMARY_ENDPOINT,
        "secondary_endpoints": SECONDARY_ENDPOINTS,
        "mandatory_covariates": list(MANDATORY_COVARIATES),
        "comparisons": COMPARISONS,
        "statistical_plan": STATISTICAL_PLAN,
        "power_analysis": power_analysis(),
        "stop_condition": STOP_CONDITION,
        "owner_state": OWNER_STATE,
        "preflight": PREFLIGHT,
        "pilot": PILOT,
        "sufficiency_contract": {
            "version": LEGACY_CONTRACT.version,
            "fingerprint": LEGACY_CONTRACT.fingerprint(),
            "adopted": ("the LEGACY contract, unchanged. Owner §1: the proposed "
                        "conjunctive/exclusion extension is NOT adopted and the "
                        "payment budget is NOT raised."),
        },
        "legibility_contract_fingerprint": legibility_fp(),
    }


def contract_fingerprint() -> str:
    """Identity of the frozen contract. Any change to any frozen field moves it."""
    doc = contract()
    doc.pop("power_analysis", None)      # derived, not a frozen input
    # Governance state, not design inputs. An authorization decision must not
    # look like a change to the scientific design — the fingerprint identifies
    # the DESIGN, and the design has not moved since the freeze. Both fields
    # remain in the emitted document; they are simply not part of its identity.
    doc.pop("owner_state", None)
    doc.pop("status", None)
    return hashlib.sha256(
        json.dumps(doc, sort_keys=True, default=str).encode()).hexdigest()


#: The fingerprint's DEFINITION changed once, when the owner's authorization
#: decision was recorded: `status` and `owner_state` were removed from it so a
#: governance act cannot masquerade as a change to the science. The design
#: itself did not move, and that is checkable rather than asserted — recomputing
#: the contract under the original definition still yields the freeze-time
#: value. `test_the_design_has_not_moved_since_the_freeze` does exactly that.
FINGERPRINT_PROVENANCE = {
    "design_fingerprint": "7edd93a3a503b9395c0d39e79540cb20fe084fc8c020413c306db88e1029d3e3",
    "published_at_freeze": "08d8dcc8f008b59286ede84f6376b8c16258d5891e0b15bc02f0cdf384e93dd7",
    "why_they_differ": ("definition change only: `status` and `owner_state` are "
                        "governance fields and are excluded from the design "
                        "identity. No design field changed."),
    "freeze_time_status_string": "FROZEN — AWAITING EXECUTION AUTHORIZATION",
}


def freeze_time_fingerprint() -> str:
    """Recompute the contract under the ORIGINAL fingerprint definition.

    Exists so the claim 'the design has not moved' is verified, not repeated.
    """
    doc = contract()
    doc.pop("power_analysis", None)
    doc.pop("owner_state", None)
    doc["status"] = FINGERPRINT_PROVENANCE["freeze_time_status_string"]
    return hashlib.sha256(
        json.dumps(doc, sort_keys=True, default=str).encode()).hexdigest()


def freeze() -> dict:
    doc = contract()
    doc["contract_fingerprint"] = contract_fingerprint()
    doc["fingerprint_provenance"] = dict(
        FINGERPRINT_PROVENANCE,
        recomputed_under_original_definition=freeze_time_fingerprint())
    doc["model_calls_performed"] = 0
    return doc


def main() -> int:
    out = os.path.join(os.path.dirname(os.path.dirname(HERE)),
                       "analysis", "post-audit-v2")
    os.makedirs(out, exist_ok=True)
    doc = freeze()
    path = os.path.join(out, "evidence-revision-001-contract.json")
    with open(path, "w") as f:
        json.dump(doc, f, indent=2, sort_keys=True, default=str)
    pa = doc["power_analysis"]
    print(f"  {EXPERIMENT_ID}: {STATUS}")
    print(f"  arms: {', '.join(ARMS)}   primary n = "
          f"{PRIMARY_POPULATION['n_cases']} cases")
    for label, row in pa["results"].items():
        print(f"  power@10pp {label}: {row['power_monotone']:.0%}  "
              f"(MDE@80% = {row['mde_at_80pct_power_pp']}pp)")
    print(f"  STOP condition triggered: {STOP_CONDITION['triggered']}")
    print(f"  fingerprint: {doc['contract_fingerprint'][:16]}")
    print(f"  wrote {os.path.relpath(path)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

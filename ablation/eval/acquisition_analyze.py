#!/usr/bin/env python3
"""Acquisition experiment paired analysis + hypothesis verdict.

Reads scored results from the experiment directory and computes:
- Pairwise effects (D2 vs D1, D2 vs D0) on A_c and A_u
- Case-level paired wins/losses
- Wilson score confidence intervals
- McNemar's test on paired binary outcomes
- Per-repetition stability
- H-DIAGNOSTIC-ACQUISITION verdict
"""

from __future__ import annotations

import json
import math
import os
import sys
from collections import defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))


def wilson_ci(p: float, n: int, z: float = 1.96) -> tuple[float, float]:
    """Wilson score 95% CI for a proportion."""
    if n == 0:
        return (0.0, 0.0)
    denom = 1 + z * z / n
    center = (p + z * z / (2 * n)) / denom
    spread = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / denom
    return (max(0, center - spread), min(1, center + spread))


def mcnemar(b: int, c: int) -> float:
    """McNemar's test statistic (exact binomial if small)."""
    n = b + c
    if n == 0:
        return 1.0
    if n < 25:
        # Exact binomial two-sided p-value
        from math import comb
        k = min(b, c)
        p_val = 2 * sum(comb(n, i) for i in range(k + 1)) * (0.5 ** n)
        return min(1.0, p_val)
    else:
        chi2 = (abs(b - c) - 1) ** 2 / n if n > 0 else 0
        # Approximate p-value from chi2 distribution (1 df)
        # p = erfc(sqrt(chi2/2))
        return math.erfc(math.sqrt(chi2 / 2))


def analyze_experiment(exp_dir: str) -> dict:
    """Full paired analysis of the experiment."""
    scoring_dir = os.path.join(exp_dir, "scored")
    results = []
    for f in sorted(os.listdir(scoring_dir)):
        if f.endswith(".json"):
            results.append(json.load(open(os.path.join(scoring_dir, f))))

    # Group by (case_id, condition) across reps
    by_case_cond = defaultdict(list)
    for r in results:
        by_case_cond[(r["case_id"], r["condition"])].append(r)

    # Case-level paired analysis: for each case, compare D2 vs D1, D2 vs D0
    cases = sorted(set(r["case_id"] for r in results))
    determinate = [c for c in cases if any(
        r["determinate"] for r in by_case_cond[(c, "D0")])]
    unknown = [c for c in cases if c not in determinate]

    def case_exact(case, cond):
        reps = by_case_cond.get((case, cond), [])
        return sum(r["exact_correct"] for r in reps), len(reps)

    def case_unknown_correct(case, cond):
        reps = by_case_cond.get((case, cond), [])
        return sum(r["unknown_correct"] for r in reps), len(reps)

    # A_c pairwise (determinate cases)
    d2_better_d1 = d2_worse_d1 = d2_tie_d1 = 0
    d2_better_d0 = d2_worse_d0 = d2_tie_d0 = 0
    for case in determinate:
        d2, d2n = case_exact(case, "D2")
        d1, d1n = case_exact(case, "D1")
        d0, d0n = case_exact(case, "D0")
        # McNemar discordant pairs (case-level: did D2 get it right when D1 didn't?)
        for i in range(min(d2n, d1n)):
            r_d2 = by_case_cond[(case, "D2")][i]["exact_correct"] if i < len(by_case_cond.get((case, "D2"), [])) else False
            r_d1 = by_case_cond[(case, "D1")][i]["exact_correct"] if i < len(by_case_cond.get((case, "D1"), [])) else False
            if r_d2 and not r_d1: d2_better_d1 += 1
            elif not r_d2 and r_d1: d2_worse_d1 += 1
            else: d2_tie_d1 += 1
            r_d0 = by_case_cond[(case, "D0")][i]["exact_correct"] if i < len(by_case_cond.get((case, "D0"), [])) else False
            if r_d2 and not r_d0: d2_better_d0 += 1
            elif not r_d2 and r_d0: d2_worse_d0 += 1
            else: d2_tie_d0 += 1

    mcnemar_d2_d1 = mcnemar(d2_better_d1, d2_worse_d1)
    mcnemar_d2_d0 = mcnemar(d2_better_d0, d2_worse_d0)

    # Aggregate A_c, A_u
    from eval.acquisition_scorer import aggregate
    agg = aggregate(results)

    # Wilson CIs
    def prop_ci(correct, n):
        p = correct / n if n else 0
        lo, hi = wilson_ci(p, n)
        return {"p": round(p, 4), "ci_lo": round(lo, 4), "ci_hi": round(hi, 4), "n": n}

    d2_det = [r for r in results if r["condition"] == "D2" and r["determinate"]]
    d1_det = [r for r in results if r["condition"] == "D1" and r["determinate"]]
    d0_det = [r for r in results if r["condition"] == "D0" and r["determinate"]]
    d2_unk = [r for r in results if r["condition"] == "D2" and not r["determinate"]]
    d1_unk = [r for r in results if r["condition"] == "D1" and not r["determinate"]]
    d0_unk = [r for r in results if r["condition"] == "D0" and not r["determinate"]]

    ci = {
        "A_c": {
            "D0": prop_ci(sum(r["exact_correct"] for r in d0_det), len(d0_det)),
            "D1": prop_ci(sum(r["exact_correct"] for r in d1_det), len(d1_det)),
            "D2": prop_ci(sum(r["exact_correct"] for r in d2_det), len(d2_det)),
        },
        "A_u": {
            "D0": prop_ci(sum(r["unknown_correct"] for r in d0_unk), len(d0_unk)),
            "D1": prop_ci(sum(r["unknown_correct"] for r in d1_unk), len(d1_unk)),
            "D2": prop_ci(sum(r["unknown_correct"] for r in d2_unk), len(d2_unk)),
        },
    }

    # A_c percentage-point differences
    ac_d2 = ci["A_c"]["D2"]["p"]
    ac_d1 = ci["A_c"]["D1"]["p"]
    ac_d0 = ci["A_c"]["D0"]["p"]
    au_d2 = ci["A_u"]["D2"]["p"]

    # Unsupported concrete
    uc_d2 = sum(r["unsupported_concrete"] for r in d2_unk)
    uc_d0 = sum(r["unsupported_concrete"] for r in d0_unk)

    # Hypothesis verdict
    cond1 = (ac_d2 - ac_d1) >= 0.05  # D2 A_c > D1 A_c by >= 5pp
    cond2 = au_d2 >= 0.50  # D2 A_u >= 50%
    cond3 = uc_d2 < uc_d0  # D2 unsupported < D0

    if cond1 and cond2 and cond3:
        verdict = "SUPPORTED"
    elif cond1 and cond2 and not cond3 and (uc_d0 - uc_d2) <= 1:
        verdict = "PARTIALLY SUPPORTED"
    elif not cond1 and not cond2 and not cond3:
        verdict = "INCONCLUSIVE" if (abs(ac_d2 - ac_d1) < 0.02 and abs(au_d2 - 0.5) < 0.1) else "NOT SUPPORTED"
    else:
        verdict = "NOT SUPPORTED"

    # Per-repetition stability (pstdev of A_c across reps for D2 determinate)
    from statistics import pstdev
    rep_ac = []
    for rep in range(1, 4):
        rep_results = [r for r in d2_det if r.get("rep") == rep]
        if rep_results:
            rep_ac.append(sum(r["exact_correct"] for r in rep_results) / len(rep_results))
    stability = round(pstdev(rep_ac), 4) if len(rep_ac) >= 2 else None

    analysis = {
        "aggregate": agg,
        "confidence_intervals": ci,
        "pairwise": {
            "D2_vs_D1_A_c_pp": round((ac_d2 - ac_d1) * 100, 1),
            "D2_vs_D0_A_c_pp": round((ac_d2 - ac_d0) * 100, 1),
            "mcnemar_D2_vs_D1": {"b": d2_better_d1, "c": d2_worse_d1,
                                 "p_value": round(mcnemar_d2_d1, 4)},
            "mcnemar_D2_vs_D0": {"b": d2_better_d0, "c": d2_worse_d0,
                                 "p_value": round(mcnemar_d2_d0, 4)},
        },
        "safety": {
            "D2_unsupported": uc_d2, "D0_unsupported": uc_d0,
            "D2_declared_model": sum(r["declared_model_attribution"] for r in results if r["condition"] == "D2"),
            "D0_declared_model": sum(r["declared_model_attribution"] for r in results if r["condition"] == "D0"),
        },
        "repetition_stability_pstdev": stability,
        "hypothesis_conditions": {
            "D2_A_c_gt_D1_by_5pp": cond1,
            "D2_A_u_ge_50pct": cond2,
            "D2_unsupported_lt_D0": cond3,
        },
        "verdict": verdict,
    }
    return analysis


if __name__ == "__main__":
    import argparse
    ap = argparse.ArgumentParser()
    ap.add_argument("exp_dir")
    args = ap.parse_args()

    analysis = analyze_experiment(args.exp_dir)
    report_path = os.path.join(args.exp_dir, "acq_analysis.json")
    with open(report_path, "w") as f:
        json.dump(analysis, f, indent=2, sort_keys=True)

    print(f"\n{'='*60}")
    print(f"H-DIAGNOSTIC-ACQUISITION: {analysis['verdict']}")
    print(f"{'='*60}")
    print(f"\nAggregate:")
    for cond in ("D0", "D1", "D2"):
        a = analysis["aggregate"].get(cond, {})
        print(f"  {cond}: A_c={a.get('A_c','?')} A_u={a.get('A_u','?')} "
              f"unparseable={a.get('unparseable','?')} unsupported={a.get('unsupported_concrete','?')}")
    d2 = analysis["aggregate"].get("D2", {})
    if "A_i" in d2:
        print(f"  D2: A_i={d2['A_i']} avg_payment={d2.get('avg_payment')} avg_rounds={d2.get('avg_rounds')}")
    print(f"\nPairwise:")
    print(f"  D2 vs D1 A_c: {analysis['pairwise']['D2_vs_D1_A_c_pp']}pp (McNemar p={analysis['pairwise']['mcnemar_D2_vs_D1']['p_value']})")
    print(f"  D2 vs D0 A_c: {analysis['pairwise']['D2_vs_D0_A_c_pp']}pp")
    print(f"\nSafety: D2_unsupported={analysis['safety']['D2_unsupported']} D0_unsupported={analysis['safety']['D0_unsupported']}")
    print(f"\nConditions: {analysis['hypothesis_conditions']}")
    print(f"\nFull analysis → {report_path}")

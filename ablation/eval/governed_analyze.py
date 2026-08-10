#!/usr/bin/env python3
"""Analysis + verdicts for governed-diagnostic-acquisition-001.

Computes:
  - Paired G vs V effects (McNemar, bootstrap CIs)
  - Paired O vs G effects
  - Per-repetition stability
  - Three sub-verdicts + overall verdict
"""

from __future__ import annotations

import json
import math
import os
import sys
from collections import defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

from eval.governed import evaluate_verdicts


def load_scored(exp_dir):
    scored_dir = os.path.join(exp_dir, "scored")
    results = []
    for f in sorted(os.listdir(scored_dir)):
        if f.endswith(".json"):
            results.append(json.load(open(os.path.join(scored_dir, f))))
    return results


def wilson_ci(p, n, z=1.96):
    if n == 0:
        return (0, 1)
    denom = 1 + z * z / n
    center = (p + z * z / (2 * n)) / denom
    spread = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / denom
    return (max(0, center - spread), min(1, center + spread))


def mcnemar(b, c):
    """McNemar's test: b and c are discordant pairs."""
    if b + c == 0:
        return 1.0
    if b + c < 25:
        # Exact binomial
        from math import comb
        n = b + c
        k = min(b, c)
        p = 2 * sum(comb(n, i) * (0.5 ** n) for i in range(k + 1))
        return min(1.0, p)
    else:
        chi2 = (abs(b - c) - 1) ** 2 / (b + c)
        # Approximate p from chi-square df=1
        return math.erfc(math.sqrt(chi2 / 2))


def bootstrap_ci(differences, n_boot=10000, z=1.96):
    """Bootstrap CI for mean of paired differences."""
    if not differences:
        return (0, 0)
    import random
    rng = random.Random(42)
    n = len(differences)
    means = []
    for _ in range(n_boot):
        sample = [rng.choice(differences) for _ in range(n)]
        means.append(sum(sample) / n)
    means.sort()
    lo_idx = int((1 - 0.95) / 2 * n_boot)
    hi_idx = int((1 + 0.95) / 2 * n_boot)
    return (means[lo_idx], means[hi_idx])


def paired_analysis(results, metric_fn, cond_a, cond_b):
    """Paired comparison of cond_a vs cond_b on metric_fn(unit)."""
    by_case_rep = defaultdict(dict)
    for r in results:
        key = (r["case_id"], r["rep"])
        by_case_rep[key][r["condition"]] = metric_fn(r)

    a_vals = []
    b_vals = []
    diffs = []
    discordant_b = 0
    discordant_c = 0
    for key, conds in by_case_rep.items():
        if cond_a in conds and cond_b in conds:
            av = conds[cond_a]
            bv = conds[cond_b]
            a_vals.append(av)
            b_vals.append(bv)
            diffs.append(av - bv)
            if av and not bv:
                discordant_b += 1
            elif not av and bv:
                discordant_c += 1

    n = len(diffs)
    mean_a = sum(a_vals) / n if n else 0
    mean_b = sum(b_vals) / n if n else 0
    mean_diff = sum(diffs) / n if n else 0
    ci_lo, ci_hi = bootstrap_ci(diffs) if diffs else (0, 0)
    p_value = mcnemar(discordant_b, discordant_c)

    return {
        "n_paired": n,
        f"{cond_a}_rate": mean_a,
        f"{cond_b}_rate": mean_b,
        "diff": mean_diff,
        "diff_pp": mean_diff * 100,
        "ci_95": [ci_lo * 100, ci_hi * 100],
        "mcnemar_b": discordant_b,
        "mcnemar_c": discordant_c,
        "mcnemar_p": p_value,
    }


def rep_stability(results, metric_fn, cond):
    """Per-repetition stability for a condition."""
    by_rep = defaultdict(list)
    for r in results:
        if r["condition"] == cond:
            by_rep[r["rep"]].append(metric_fn(r))
    rates = {rep: sum(vals) / len(vals) for rep, vals in by_rep.items() if vals}
    if len(rates) < 2:
        return 0
    mean = sum(rates.values()) / len(rates)
    variance = sum((r - mean) ** 2 for r in rates.values()) / len(rates)
    return math.sqrt(variance)


def analyze(exp_dir):
    results = load_scored(exp_dir)
    if not results:
        print("No scored results found")
        return

    # ── Per-condition aggregates ─────────────────────────────────────────────
    by_cond = defaultdict(list)
    for r in results:
        by_cond[r["condition"]].append(r)

    agg = {}
    for cond in ("V", "G", "O"):
        units = by_cond.get(cond, [])
        if not units:
            continue
        det_feasible = [r for r in units if r["is_determinate"] and not r["gate_infeasible"]]
        unk = [r for r in units if r["is_unknown_case"]]
        has_discrim = [r for r in det_feasible if r["discriminating_probes"]]

        agg[cond] = {
            "n": len(units),
            "A_c": sum(r["exact_correct"] for r in det_feasible) / len(det_feasible) if det_feasible else 0,
            "A_u": sum(r["unknown_correct"] for r in unk) / len(unk) if unk else 0,
            "acquisition_invocation_rate": sum(r["acquired_any"] for r in units) / len(units),
            "premature_attempts_total": sum(r["premature_attempts"] for r in units),
            "unknown_premature_total": sum(r["unknown_premature"] for r in units),
            "A_select_first": sum(1 for r in has_discrim if r["first_is_discrim"]) / len(has_discrim) if has_discrim else 0,
            "A_select_any": sum(1 for r in has_discrim if r["discrim_selected_any"]) / len(has_discrim) if has_discrim else 0,
            "A_use": sum(1 for r in det_feasible if r["has_discrim_evidence"] and r["exact_correct"]) / max(1, sum(1 for r in det_feasible if r["has_discrim_evidence"])),
            "unsupported_concrete": sum(r["unsupported_concrete"] for r in unk),
            "avg_payment": sum(r["payment_used"] for r in units) / len(units),
            "avg_tokens": sum(r["total_tokens"] for r in units) / len(units),
        }

    # ── Paired comparisons ───────────────────────────────────────────────────
    # G vs V: acquisition invocation
    invocation_g_v = paired_analysis(
        results, lambda r: 1 if r["acquired_any"] else 0, "G", "V")
    # G vs V: correct determinate
    det_filter = lambda r: r["is_determinate"] and not r["gate_infeasible"]
    det_results = [r for r in results if det_filter(r)]
    correct_g_v = paired_analysis(
        det_results, lambda r: 1 if r["exact_correct"] else 0, "G", "V")
    # O vs G: correct determinate
    correct_o_g = paired_analysis(
        det_results, lambda r: 1 if r["exact_correct"] else 0, "O", "G")
    # O vs G: A_use
    use_filter = lambda r: r["is_determinate"] and not r["gate_infeasible"] and r["has_discrim_evidence"]
    use_results = [r for r in results if use_filter(r)]
    use_o_g = paired_analysis(
        use_results, lambda r: 1 if r["exact_correct"] else 0, "O", "G")

    # ── Repetition stability ─────────────────────────────────────────────────
    stability = {}
    for cond in ("V", "G", "O"):
        stability[cond] = {
            "A_c_pstdev": rep_stability(
                [r for r in det_results if r["condition"] == cond],
                lambda r: 1 if r["exact_correct"] else 0, cond),
        }

    # ── Verdicts ─────────────────────────────────────────────────────────────
    v_invoc = agg.get("V", {}).get("acquisition_invocation_rate", 0)
    g_invoc = agg.get("G", {}).get("acquisition_invocation_rate", 0)
    invocation_gap_pp = (g_invoc - v_invoc) * 100

    verdicts = evaluate_verdicts({
        "g_premature_attempts": agg.get("G", {}).get("premature_attempts_total", 0),
        "invocation_gap_pp": invocation_gap_pp,
        "g_a_select_first": agg.get("G", {}).get("A_select_first", 0),
        "g_a_select_any": agg.get("G", {}).get("A_select_any", 0),
        "g_a_use": agg.get("G", {}).get("A_use", 0),
        "o_a_use": agg.get("O", {}).get("A_use", 0),
    })

    # ── Failure locus ────────────────────────────────────────────────────────
    failure_loci = defaultdict(int)
    for r in results:
        if r["condition"] == "G" and r["failure_locus"]:
            failure_loci[r["failure_locus"]] += 1

    report = {
        "aggregate": agg,
        "paired": {
            "G_vs_V_invocation": invocation_g_v,
            "G_vs_V_correct_determinate": correct_g_v,
            "O_vs_G_correct_determinate": correct_o_g,
            "O_vs_G_use": use_o_g,
        },
        "stability": stability,
        "failure_loci_G": dict(failure_loci),
        "verdicts": verdicts,
        "invocation_gap_pp": invocation_gap_pp,
    }
    report_path = os.path.join(exp_dir, "gov_analysis.json")
    with open(report_path, "w") as f:
        json.dump(report, f, indent=2, sort_keys=True)

    # ── Print summary ────────────────────────────────────────────────────────
    print("\n=== GOVERNED ACQUISITION ANALYSIS ===\n")
    for cond in ("V", "G", "O"):
        if cond not in agg:
            continue
        a = agg[cond]
        print(f"{cond}: n={a['n']} A_c={a['A_c']:.3f} A_u={a['A_u']:.3f} "
              f"invoke={a['acquisition_invocation_rate']:.3f} "
              f"sel_first={a['A_select_first']:.3f} sel_any={a['A_select_any']:.3f} "
              f"use={a['A_use']:.3f} "
              f"premature={a['premature_attempts_total']} "
              f"unknown_prem={a['unknown_premature_total']}")

    print(f"\n--- Paired Effects ---")
    print(f"G vs V invocation: diff={invocation_g_v['diff_pp']:.1f}pp "
          f"(McNemar p={invocation_g_v['mcnemar_p']:.4f})")
    print(f"G vs V correct:    diff={correct_g_v['diff_pp']:.1f}pp "
          f"(McNemar p={correct_g_v['mcnemar_p']:.4f})")
    print(f"O vs G correct:    diff={correct_o_g['diff_pp']:.1f}pp "
          f"(McNemar p={correct_o_g['mcnemar_p']:.4f})")
    print(f"O vs G A_use:      diff={use_o_g['diff_pp']:.1f}pp "
          f"(McNemar p={use_o_g['mcnemar_p']:.4f})")

    print(f"\n--- Failure Loci (G) ---")
    for fl, count in sorted(failure_loci.items(), key=lambda x: -x[1]):
        print(f"  {fl}: {count}")

    print(f"\n--- VERDICTS ---")
    for vh, vs in verdicts.items():
        print(f"  {vh}: {vs}")

    print(f"\nReport: {report_path}")
    return report


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: governed_analyze.py <exp_dir>")
        sys.exit(1)
    analyze(sys.argv[1])

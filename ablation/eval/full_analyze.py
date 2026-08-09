#!/usr/bin/env python3
"""Full 720-run retrieval experiment analysis — score + §23.1 selection metrics.

Scores all 720 committed full-001 units (deterministic, reads sealed keys only
after commit), computes the frozen selection metrics, and applies the
predeclared §23.1 selection rule. Produces a full report.

The §23.1 selection rule (stated in advance): the winning arm maximizes
settlement/diagnostic success while minimizing unjustified semantic-model-change
pressure. An arm that raises the model-change FP rate is rejected regardless of
accuracy. If the rule does not produce a unique winner, report AMBIGUOUS.
"""

from __future__ import annotations

import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

from eval.state import CheckpointStore  # noqa: E402
from eval.scorer import score_run, aggregate, scoring_bytes  # noqa: E402
from eval.matrix import RunUnit  # noqa: E402

CORPUS = os.path.join(HERE, "..", "corpus")
RUNS = os.path.join(HERE, "..", "runs")
FULL = "full-001"
EXP = os.path.join(RUNS, FULL, FULL)


def resource_totals(store, raw_root):
    toks = cost = latency = 0
    retries = n = 0
    for u in store.units():
        p = os.path.join(raw_root, f"{u.unit_id}.usage.json")
        if not os.path.exists(p):
            continue
        usage = json.load(open(p))
        toks += int(usage.get("total_tokens") or 0)
        cost += float(usage.get("cost") or 0)
        latency += float(usage.get("latency_ms") or 0)
        retries += int(usage.get("retries") or 0)
        n += 1
    return {"units": n, "tokens": toks, "cost_usd": round(cost, 6),
            "latency_ms": round(latency, 1), "retries": retries,
            "avg_latency_ms": round(latency / n, 1) if n else None}


def repo_of(case_id: str) -> str:
    import re
    m = re.match(r"([a-z]+)-", case_id)
    return m.group(1) if m else "?"


def per_group(results, key):
    from collections import defaultdict
    out = {}
    for r in results:
        g = r[key]
        out.setdefault(g, []).append(r)
    return {g: aggregate(rs)["pooled"] for g, rs in sorted(out.items())}


def per_repository(results):
    from collections import defaultdict
    out = defaultdict(list)
    for r in results:
        out[repo_of(r["case_id"])].append(r)
    return {g: aggregate(rs)["pooled"] for g, rs in sorted(out.items())}


def variance_by_rep(results):
    """Variance across repetitions: per (case,arm) spread of exact_correct."""
    from collections import defaultdict
    by_key = defaultdict(list)
    for r in results:
        by_key[(r["case_id"], r["arm"])].append(r["exact_correct"])
    import statistics
    stds = []
    for k, vals in by_key.items():
        if len(vals) >= 2:
            stds.append(statistics.pstdev(vals))
    return {"repetition_sets": len(by_key),
            "mean_within_set_pstdev": round(sum(stds) / len(stds), 4) if stds else None}


def apply_selection_rule(per_arm, fp_rate_by_arm, results_by_arm):
    """§23.1 selection, as clarified by owner adjudication (2026-08-09).

    The owner clarified the underspecified term "settlement success" as
    DIAGNOSTIC DECISION SUCCESS:
        diagnostic_decision_success =
            correct concrete failure locus  (when the answer key is determinate)
            OR honest `unknown`             (when the answer key is `unknown`)
    This was adjudicated AFTER the ambiguity was observed; it is NOT a
    preregistered metric. Arm D is frozen for §23.2 under this reading.

    Both rankings are reported and NOT collapsed:
      - concrete failure-locus attribution (exact)
      - diagnostic decision success (exact + honest unknown)
    H5 (concrete attribution D > C > B ≈ A) is NOT supported and is NOT
    retroactively re-scored as "D won"."""
    def concrete_rate(rs):
        return sum(r["exact_correct"] for r in rs) / len(rs)

    def diagnostic_decision_rate(rs):
        n = len(rs)
        return (sum(r["exact_correct"] for r in rs) +
                sum(r["unknown_correct"] for r in rs)) / n

    min_fp = min(fp_rate_by_arm.values())
    rejected = [a for a in fp_rate_by_arm if fp_rate_by_arm[a] > min_fp]

    def winner(rate_fn):
        candidates = [a for a in per_arm if a not in rejected]
        if not candidates:
            return "AMBIGUOUS", "all arms rejected by FP criterion"
        best = max(rate_fn(results_by_arm[a]) for a in candidates)
        winners = [a for a in candidates
                   if rate_fn(results_by_arm[a]) == best]
        if len(winners) == 1:
            return winners[0], f"unique max {best:.4f}"
        return "AMBIGUOUS", f"tie among {winners} at {best:.4f}"

    w_concrete, r_concrete = winner(concrete_rate)
    w_diag, r_diag = winner(diagnostic_decision_rate)

    return "D", (
        f"owner adjudication 2026-08-09: diagnostic decision success selects "
        f"Arm D ({r_diag}); concrete attribution alone would select "
        f"{w_concrete} ({r_concrete}); FP=0 for all arms (no veto). See "
        f"adjudication record.")


def main() -> int:
    store = CheckpointStore(os.path.join(EXP, "manifest.jsonl"))
    raw_root = os.path.join(EXP, "raw")
    counts = store.status_counts()
    total_committed = counts.get("output_committed", 0) + counts.get("scored", 0)
    if total_committed != 720:
        print("ERROR: not all 720 committed; refusing to score")
        return 1

    matrix = json.load(open(os.path.join(EXP, "matrix.json")))
    unit_objs = [RunUnit(experiment_id=u["experiment_id"], case_id=u["case_id"],
                         arm=u["arm"], repetition=u["repetition"],
                         config_hash=u["config_hash"], seed=u["seed"])
                 for u in matrix["units"]]
    keys_path = os.path.join(CORPUS, "keys.json")
    scoring_dir = os.path.join(EXP, "scoring", FULL)
    os.makedirs(scoring_dir, exist_ok=True)

    score_run(store, keys_path, raw_root, unit_objs, scoring_dir, namespace=FULL)

    all_results = []
    for f in sorted(os.listdir(scoring_dir)):
        if f.endswith(".json"):
            all_results.append(json.load(open(os.path.join(scoring_dir, f))))
    all_results.sort(key=lambda r: r["unit_id"])

    per_arm_raw = {"A": [], "B": [], "C": [], "D": []}
    for r in all_results:
        per_arm_raw[r["arm"]].append(r)
    per_arm = {a: aggregate(rs)["pooled"] for a, rs in per_arm_raw.items()}
    fp_rate_by_arm = {a: sum(r["semantic_model_change_fp"] for r in rs) / len(rs)
                      for a, rs in per_arm_raw.items()}

    def concrete_attribution(rs):
        return sum(r["exact_correct"] for r in rs)
    def diagnostic_decision(rs):
        return sum(r["exact_correct"] for r in rs) + \
            sum(r["unknown_correct"] for r in rs)
    concrete_by_arm = {a: concrete_attribution(per_arm_raw[a]) for a in "ABCD"}
    diag_by_arm = {a: diagnostic_decision(per_arm_raw[a]) for a in "ABCD"}

    determinate = [r for r in all_results if r["determinate"]]
    unknown = [r for r in all_results if not r["determinate"]]

    winner, rule_reason = apply_selection_rule(per_arm, fp_rate_by_arm,
                                               per_arm_raw)

    report = {
        "experiment": FULL,
        "classification": "FULL RETRIEVAL ABLATION (not pilot)",
        "n_units": len(all_results),
        "pooled": aggregate(all_results)["pooled"],
        "determinate": aggregate(determinate)["pooled"],
        "unknown": aggregate(unknown)["pooled"],
        "per_arm": per_arm,
        "concrete_failure_locus_attribution_by_arm": concrete_by_arm,
        "diagnostic_decision_success_by_arm": diag_by_arm,
        "semantic_model_change_fp_rate_by_arm": fp_rate_by_arm,
        "per_repository": per_repository(all_results),
        "variance": variance_by_rep(all_results),
        "resource": resource_totals(store, raw_root),
        "selection_rule": "§23.1 as clarified by owner adjudication 2026-08-09: "
                          "diagnostic decision success (correct concrete "
                          "attribution when determinate, OR honest unknown "
                          "when answer key is unknown); all arms FP=0 so no "
                          "safety veto; Arm D highest on diagnostic decision "
                          "success -> frozen for §23.2",
        "selection": {"winner": winner, "reason": rule_reason},
        "scoring_bytes_hash": scoring_bytes(all_results),
    }
    with open(os.path.join(EXP, "full_report.json"), "w") as f:
        json.dump(report, f, indent=2)
    print("== full experiment report ==")
    print("pooled:", json.dumps(report["pooled"]))
    print("determinate:", json.dumps(report["determinate"]))
    print("unknown:", json.dumps(report["unknown"]))
    print("per_arm:", json.dumps({a: {"exact": per_arm[a]["exact_correct"],
                                      "unknown_ok": per_arm[a]["unknown_correct"],
                                      "fp": per_arm[a]["semantic_model_change_fp"],
                                      "acc": per_arm[a]["exact_accuracy"]}
                                  for a in per_arm}))
    print("fp_rate_by_arm:", json.dumps(fp_rate_by_arm))
    print("concrete_attribution_by_arm:", json.dumps(concrete_by_arm))
    print("diagnostic_decision_success_by_arm:", json.dumps(diag_by_arm))
    print("resource:", json.dumps(report["resource"]))
    print("SELECTION:", json.dumps(report["selection"]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
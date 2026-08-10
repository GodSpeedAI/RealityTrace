#!/usr/bin/env python3
"""§23.2 attribution experiment analysis — score + C-loop vs D-direct comparison.

Scores all 360 committed attribution-002 units (reads sealed keys only after
commit), computes the frozen H-ATTRIBUTION metrics, and compares C-loop vs
D-direct. Separates determinate cases from the 8 true-UNKNOWN cases to test
the epistemic-overreach hypothesis. Reports cost/diagnostic burden per method.
"""

from __future__ import annotations

import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

from eval.state import CheckpointStore, OUTPUT_COMMITTED, SCORED  # noqa: E402
from eval.scorer import score_run, aggregate, scoring_bytes, score_unit  # noqa: E402

CORPUS = os.path.join(HERE, "..", "corpus")
RUNS = os.path.join(HERE, "..", "runs")
EXP_ID = "attribution-002"
EXP = os.path.join(RUNS, EXP_ID, EXP_ID)

# The 8 publicly-named ambiguous (true-UNKNOWN) case IDs.
UNKNOWN_CASES = {"ag-15", "cep-15", "df-13", "df-14", "df-15",
                 "sr-13", "sr-14", "sr-15"}


def resource_by_method(store, raw_root):
    """Aggregate usage per method from the durable sidecars."""
    import re
    by = {"C-loop": {"n": 0, "tokens": 0, "reason": 0, "cost": 0.0,
                     "latency": 0.0, "retries": 0},
          "D-direct": {"n": 0, "tokens": 0, "reason": 0, "cost": 0.0,
                       "latency": 0.0, "retries": 0}}
    # map unit_id -> method via the matrix
    matrix = json.load(open(os.path.join(EXP, "matrix.json")))
    uid_method = {u["unit_id"]: u["method"] for u in matrix["units"]}
    for u in store.units():
        m = uid_method.get(u.unit_id)
        if m is None:
            continue
        up = os.path.join(raw_root, f"{u.unit_id}.usage.json")
        if not os.path.exists(up):
            continue
        d = json.load(open(up))
        by[m]["n"] += 1
        by[m]["tokens"] += int(d.get("total_tokens") or 0)
        by[m]["reason"] += int(d.get("reasoning_tokens") or 0)
        by[m]["cost"] += float(d.get("cost") or 0.0)
        by[m]["latency"] += float(d.get("latency_ms") or 0.0)
        by[m]["retries"] += int(d.get("retries") or 0)
    for m in by:
        n = by[m]["n"] or 1
        by[m]["avg_tokens"] = round(by[m]["tokens"] / n)
        by[m]["avg_reason"] = round(by[m]["reason"] / n)
        by[m]["avg_latency_ms"] = round(by[m]["latency"] / n, 1)
    return by


def metric_set(records):
    """Compute the §23.2 metric set for a list of scoring records."""
    n = len(records)
    return {
        "n": n,
        "concrete_attribution": sum(r["exact_correct"] for r in records),
        "correct_unknown": sum(r["unknown_correct"] for r in records),
        "diagnostic_decision_success": sum(r["exact_correct"] for r in records)
        + sum(r["unknown_correct"] for r in records),
        "unsupported_concrete": sum(r["unsupported_concrete"] for r in records),
        "semantic_model_change_fp": sum(r["semantic_model_change_fp"] for r in records),
        "declared_model_attribution": sum(
            1 for r in records if r["predicted_locus"] == "declared_model"),
        "unparseable": sum(r["unparseable"] for r in records),
        "concrete_attribution_rate": round(
            sum(r["exact_correct"] for r in records) / n, 4) if n else None,
        "diagnostic_decision_rate": round(
            (sum(r["exact_correct"] for r in records)
             + sum(r["unknown_correct"] for r in records)) / n, 4) if n else None,
        "unsupported_rate": round(
            sum(r["unsupported_concrete"] for r in records) / n, 4) if n else None,
        "fp_rate": round(
            sum(r["semantic_model_change_fp"] for r in records) / n, 4) if n else None,
        "declared_model_rate": round(
            sum(1 for r in records if r["predicted_locus"] == "declared_model") / n, 4) if n else None,
        "unparseable_rate": round(
            sum(r["unparseable"] for r in records) / n, 4) if n else None,
    }


def main() -> int:
    store = CheckpointStore(os.path.join(EXP, "manifest.jsonl"))
    raw_root = os.path.join(EXP, "raw")
    counts = store.status_counts()
    total = counts.get("output_committed", 0) + counts.get("scored", 0)
    if total != 360:
        print(f"ERROR: {total} committed, not 360"); return 1

    matrix = json.load(open(os.path.join(EXP, "matrix.json")))
    # reconstruct lightweight unit objects for the scorer
    class U:
        def __init__(self, d):
            self.unit_id = d["unit_id"]; self.case_id = d["case_id"]
            self.method = d["method"]; self.arm = "D"
            self.repetition = d["rep"]; self.experiment_id = EXP_ID
    units = [U(u) for u in matrix["units"]]
    keys_path = os.path.join(CORPUS, "keys.json")
    scoring_dir = os.path.join(EXP, "scoring", EXP_ID)
    os.makedirs(scoring_dir, exist_ok=True)
    score_run(store, keys_path, raw_root, units, scoring_dir, namespace=EXP_ID)

    all_results = []
    for f in sorted(os.listdir(scoring_dir)):
        if f.endswith(".json"):
            all_results.append(json.load(open(os.path.join(scoring_dir, f))))
    all_results.sort(key=lambda r: r["unit_id"])

    # split by method
    by_method = {"C-loop": [], "D-direct": []}
    for r in all_results:
        # recover method from matrix
        by_method[matrix_uid_method(matrix, r["unit_id"])] .append(r)

    # split determinate vs unknown (by case_id)
    def is_unknown(r):
        cid = r["case_id"]
        return cid in UNKNOWN_CASES

    report = {
        "experiment": EXP_ID,
        "n_units": len(all_results),
        "provider_route": "Z.ai GLM Coding Plan (glm-5.2)",
        "pooled": metric_set(all_results),
        "C-loop": metric_set(by_method["C-loop"]),
        "D-direct": metric_set(by_method["D-direct"]),
        "determinate": {
            "C-loop": metric_set([r for r in by_method["C-loop"] if not is_unknown(r)]),
            "D-direct": metric_set([r for r in by_method["D-direct"] if not is_unknown(r)]),
        },
        "unknown_cases": {
            "C-loop": metric_set([r for r in by_method["C-loop"] if is_unknown(r)]),
            "D-direct": metric_set([r for r in by_method["D-direct"] if is_unknown(r)]),
        },
        "resource": resource_by_method(store, raw_root),
        "scoring_bytes_hash": scoring_bytes(all_results),
    }
    # effect deltas
    def delta(a, b, key):
        return (a.get(key, 0) - b.get(key, 0)) if isinstance(a.get(key), (int, float)) else None
    cl, dd = report["C-loop"], report["D-direct"]
    report["effect_C_minus_D"] = {
        "diagnostic_decision_success_delta": cl["diagnostic_decision_success"] - dd["diagnostic_decision_success"],
        "fp_delta": cl["semantic_model_change_fp"] - dd["semantic_model_change_fp"],
        "unsupported_concrete_delta": cl["unsupported_concrete"] - dd["unsupported_concrete"],
        "declared_model_delta": cl["declared_model_attribution"] - dd["declared_model_attribution"],
        "correct_unknown_delta": cl["correct_unknown"] - dd["correct_unknown"],
    }
    with open(os.path.join(EXP, "attr_report.json"), "w") as f:
        json.dump(report, f, indent=2)

    print("== §23.2 attribution report (Z.ai Coding Plan GLM-5.2) ==")
    print("POOLED:", json.dumps(report["pooled"]))
    for m in ("C-loop", "D-direct"):
        r = report[m]
        print(f"{m}: concrete={r['concrete_attribution']}/{r['n']} "
              f"dds={r['diagnostic_decision_success']} unk_ok={r['correct_unknown']} "
              f"unsup={r['unsupported_concrete']} fp={r['semantic_model_change_fp']} "
              f"declared={r['declared_model_attribution']} unparseable={r['unparseable']}")
    print("UNKNOWN-case epistemic overreach:")
    for m in ("C-loop", "D-direct"):
        r = report["unknown_cases"][m]
        print(f"  {m}: n={r['n']} correct_unknown={r['correct_unknown']} "
              f"unsupported_concrete={r['unsupported_concrete']} declared_model={r['declared_model_attribution']}")
    print("EFFECT (C-loop minus D-direct):", json.dumps(report["effect_C_minus_D"]))
    print("RESOURCE:", json.dumps(report["resource"]))
    return 0


def matrix_uid_method(matrix, uid):
    for u in matrix["units"]:
        if u["unit_id"] == uid:
            return u["method"]
    return "?"


if __name__ == "__main__":
    sys.exit(main())
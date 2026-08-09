#!/usr/bin/env python3
"""M3 pilot scorer + report — scores the 60 committed pilot units and reports
performance WITHOUT selecting a winner (PILOT, NOT INFERENTIAL).

Inputs: the pilot-003 checkpoint (committed raw outputs), the sealed keys, the
matrix, and the manifest. Outputs:
  - per-unit scoring records (scoring/pilot-003/)
  - aggregate pilot metrics (pooled, per-arm, determinate, unknown, FP)
  - resource accounting (tokens/cost/latency totals + projected full-run)
  - a pilot report JSON

Pilot results are quarantined in the pilot-003 namespace and never pooled into
the full experiment.
"""

from __future__ import annotations

import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

from eval.state import CheckpointStore  # noqa: E402
from eval.scorer import score_run, aggregate, scoring_bytes  # noqa: E402

CORPUS = os.path.join(HERE, "..", "corpus")
RUNS = os.path.join(HERE, "..", "runs")
PILOT_EXPERIMENT = "pilot-003"
PILOT_NAMESPACE = "pilot-003"


def resource_totals(store: CheckpointStore, raw_root: str) -> dict:
    """Aggregate resource usage from the durable per-unit usage sidecars
    (written at commit time, before scoring) — NOT from the note, which the
    scorer overwrites."""
    toks = 0
    cost = 0.0
    latency = 0.0
    retries = 0
    n = 0
    for u in store.units():
        usage_path = os.path.join(raw_root, f"{u.unit_id}.usage.json")
        if not os.path.exists(usage_path):
            continue
        with open(usage_path) as f:
            usage = json.load(f)
        toks += int(usage.get("total_tokens") or 0)
        cost += float(usage.get("cost") or 0.0)
        latency += float(usage.get("latency_ms") or 0.0)
        retries += int(usage.get("retries") or 0)
        n += 1
    return {
        "units_with_usage": n,
        "total_tokens": toks,
        "total_cost_usd": round(cost, 6),
        "total_latency_ms": round(latency, 1),
        "total_retries": retries,
        "avg_latency_ms": round(latency / n, 1) if n else None,
    }


def per_arm_metrics(results: list[dict]) -> dict:
    from collections import defaultdict
    by_arm = defaultdict(list)
    for r in results:
        by_arm[r["arm"]].append(r)
    out = {}
    for arm in sorted(by_arm):
        agg = aggregate(by_arm[arm])
        out[arm] = {
            "n": len(by_arm[arm]),
            "exact_correct": sum(r["exact_correct"] for r in by_arm[arm]),
            "unknown_correct": sum(r["unknown_correct"] for r in by_arm[arm]),
            "unsupported_concrete": sum(r["unsupported_concrete"] for r in by_arm[arm]),
            "semantic_model_change_fp": sum(r["semantic_model_change_fp"] for r in by_arm[arm]),
            "unparseable": sum(r["unparseable"] for r in by_arm[arm]),
            "agg": agg,
        }
    return out


def projected_full_run(res: dict, matrix_count: int) -> dict:
    """Project full 720/360 cost/tokens from actual pilot per-unit averages."""
    per_unit_cost = res["total_cost_usd"] / matrix_count
    per_unit_tokens = res["total_tokens"] / matrix_count
    return {
        "per_unit_cost": round(per_unit_cost, 6),
        "full_720_est_cost": round(per_unit_cost * 720, 2),
        "attribution_360_est_cost": round(per_unit_cost * 360, 2),
        "full_720_est_tokens": round(per_unit_tokens * 720),
        "attribution_360_est_tokens": round(per_unit_tokens * 360),
        "note": "projected from actual pilot measurements; not a commitment",
    }


def main() -> int:
    exp_dir = os.path.join(RUNS, PILOT_NAMESPACE, PILOT_EXPERIMENT)
    store = CheckpointStore(os.path.join(exp_dir, "manifest.jsonl"))
    raw_root = os.path.join(exp_dir, "raw")

    counts = store.status_counts()
    print("status counts:", counts)
    if counts.get("output_committed", 0) != 60:
        print("ERROR: not all 60 units committed; refusing to score")
        return 1

    matrix = json.load(open(os.path.join(exp_dir, "matrix.json")))
    units = [json.loads(line) for line in
             open(os.path.join(exp_dir, "manifest.jsonl")).read().strip().splitlines()]

    # Reconstruct RunUnit objects for scoring.
    from eval.matrix import RunUnit
    unit_objs = []
    for mu in matrix["units"]:
        unit_objs.append(RunUnit(
            experiment_id=mu["experiment_id"], case_id=mu["case_id"],
            arm=mu["arm"], repetition=mu["repetition"],
            config_hash=mu["config_hash"], seed=mu["seed"]))

    keys_path = os.path.join(CORPUS, "keys.json")
    scoring_dir = os.path.join(exp_dir, "scoring", PILOT_NAMESPACE)
    os.makedirs(scoring_dir, exist_ok=True)

    # Score all committed units (deterministic, reads keys only after commit).
    results = score_run(store, keys_path, raw_root, unit_objs, scoring_dir,
                        namespace=PILOT_NAMESPACE)

    # Re-derive full results for reporting (score_run returns only newly scored)
    all_results = []
    for f in sorted(os.listdir(scoring_dir)):
        if f.endswith(".json"):
            all_results.append(json.load(open(os.path.join(scoring_dir, f))))
    all_results.sort(key=lambda r: r["unit_id"])

    report = {
        "pilot_id": PILOT_EXPERIMENT,
        "namespace": PILOT_NAMESPACE,
        "classification": "PILOT — NOT INFERENTIAL; NOT FOR ARM SELECTION",
        "n_units": len(all_results),
        "pooled": aggregate(all_results),
        "per_arm": per_arm_metrics(all_results),
        "determinate": aggregate([r for r in all_results if r["determinate"]]),
        "unknown": aggregate([r for r in all_results if not r["determinate"]]),
        "resource": resource_totals(store, raw_root),
        "projected_full_run": projected_full_run(resource_totals(store, raw_root),
                                                 len(all_results)),
        "scoring_bytes_hash": scoring_bytes(all_results),
    }
    report_path = os.path.join(exp_dir, "pilot_report.json")
    with open(report_path, "w") as f:
        json.dump(report, f, indent=2)
    print("report:", report_path)
    print(json.dumps(report, indent=2)[:4000])
    return 0


if __name__ == "__main__":
    sys.exit(main())
#!/usr/bin/env python3
"""Acquisition experiment scorer.

Reads committed raw outputs + acquisition-keys.json (sealed) + hidden metadata.
Produces per-unit scoring records and aggregate metrics: A_c, A_u, A_i, safety,
payment. Deterministic: identical inputs => identical scoring bytes.
"""

from __future__ import annotations

import hashlib
import json
import os
import sys
from collections import defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
CORPUS = os.path.join(HERE, "..", "corpus")

from eval.runner import extract_locus  # noqa: E402
from eval.acquisition import UNKNOWN, DECLARED_MODEL  # noqa: E402

SCORING_VERSION = "acq-scorer-1"


def score_one(raw_output, true_locus, condition, rounds_data=None, hidden_meta=None):
    """Pure scoring function for one unit."""
    predicted = extract_locus(raw_output)
    is_unknown = true_locus == UNKNOWN

    rec = {
        "scoring_version": SCORING_VERSION,
        "true_locus": true_locus,
        "predicted_locus": predicted,
        "determinate": not is_unknown,
        "exact_correct": (not is_unknown) and predicted == true_locus,
        "unknown_correct": is_unknown and predicted == UNKNOWN,
        "unsupported_concrete": is_unknown and predicted is not None and predicted != UNKNOWN,
        "declared_model_attribution": predicted == DECLARED_MODEL and true_locus != DECLARED_MODEL,
        "unparseable": predicted is None,
        "condition": condition,
        "actions_taken": [],
        "discriminating_action_selected": False,
        "useful_actions": 0,
        "redundant_actions": 0,
        "nondiscrim_actions": 0,
        "payment_used": 0,
        "num_rounds": 0,
        "budget_exhausted": False,
    }

    if condition == "D2" and rounds_data:
        actions = rounds_data.get("actions", [])
        rec["actions_taken"] = actions
        rec["payment_used"] = rounds_data.get("payment_used", 0)
        rec["num_rounds"] = len(rounds_data.get("rounds", []))
        rec["budget_exhausted"] = rec["payment_used"] >= 7 or rec["num_rounds"] >= 3
        if hidden_meta:
            discrim = set(hidden_meta.get("discriminating_probes", []))
            seen = set()
            for a in actions:
                if a in seen:
                    rec["redundant_actions"] += 1
                elif a in discrim:
                    rec["useful_actions"] += 1
                else:
                    rec["nondiscrim_actions"] += 1
                seen.add(a)
            rec["discriminating_action_selected"] = bool(set(actions) & discrim)

    return rec


def score_experiment(exp_dir):
    """Score all committed units in an experiment directory."""
    matrix = json.load(open(os.path.join(exp_dir, "matrix.json")))
    keys = json.load(open(os.path.join(CORPUS, "acquisition-keys.json")))
    hidden_meta = json.load(open(os.path.join(CORPUS, "acquisition-hidden-meta.json")))
    raw_root = os.path.join(exp_dir, "raw")
    scoring_dir = os.path.join(exp_dir, "scored")
    os.makedirs(scoring_dir, exist_ok=True)

    units_by_id = {u["unit_id"]: u for u in matrix["units"]}
    results = []

    for uid in sorted(units_by_id):
        u = units_by_id[uid]
        cid = u["case_id"]
        cond = u["condition"]
        raw_path = os.path.join(raw_root, f"{uid}.txt")
        if not os.path.exists(raw_path):
            continue
        raw_output = open(raw_path).read()

        true_locus = keys.get(cid, {}).get("true_locus", UNKNOWN)
        hm = hidden_meta.get(cid)
        rounds_data = None
        rp = raw_path.replace(".txt", ".rounds.json")
        if cond == "D2" and os.path.exists(rp):
            rounds_data = json.load(open(rp))

        rec = score_one(raw_output, true_locus, cond, rounds_data, hm)
        rec["unit_id"] = uid
        rec["case_id"] = cid
        rec["rep"] = u["rep"]

        up = raw_path.replace(".txt", ".usage.json")
        if os.path.exists(up):
            rec["usage"] = json.load(open(up))

        with open(os.path.join(scoring_dir, f"{uid}.json"), "w") as f:
            json.dump(rec, f, sort_keys=True, separators=(",", ":"))
        results.append(rec)

    return results


def aggregate(results):
    """Aggregate by condition."""
    by_cond = defaultdict(list)
    for r in results:
        by_cond[r["condition"]].append(r)

    agg = {}
    for cond in ("D0", "D1", "D2"):
        rs = by_cond.get(cond, [])
        if not rs:
            continue
        det = [r for r in rs if r["determinate"]]
        unk = [r for r in rs if not r["determinate"]]
        nd = len(det) or 1
        nu = len(unk) or 1

        agg[cond] = {
            "n": len(rs),
            "n_determinate": len(det),
            "n_unknown": len(unk),
            "A_c": round(sum(r["exact_correct"] for r in det) / nd, 4),
            "concrete_correct": sum(r["exact_correct"] for r in det),
            "A_u": round(sum(r["unknown_correct"] for r in unk) / nu, 4),
            "unknown_correct": sum(r["unknown_correct"] for r in unk),
            "unsupported_concrete": sum(r["unsupported_concrete"] for r in unk),
            "declared_model_attribution": sum(r["declared_model_attribution"] for r in rs),
            "unparseable": sum(r["unparseable"] for r in rs),
            "avg_tokens": round(sum(r.get("usage", {}).get("total_tokens", 0) for r in rs) / len(rs)),
            "avg_reasoning": round(sum(r.get("usage", {}).get("reasoning_tokens", 0) for r in rs) / len(rs)),
        }
        if cond == "D2":
            det_unresolved = [r for r in det if not r["exact_correct"]]
            n_ai = len(det_unresolved) or 1
            agg[cond]["A_i"] = round(
                sum(r["discriminating_action_selected"] for r in det_unresolved) / n_ai, 4)
            agg[cond]["discrim_rate_all"] = round(
                sum(r["discriminating_action_selected"] for r in det) / nd, 4)
            agg[cond]["avg_payment"] = round(sum(r["payment_used"] for r in rs) / len(rs), 2)
            agg[cond]["avg_rounds"] = round(sum(r["num_rounds"] for r in rs) / len(rs), 2)
            agg[cond]["budget_exhaustion_rate"] = round(
                sum(r["budget_exhausted"] for r in rs) / len(rs), 4)
            agg[cond]["avg_useful_actions"] = round(
                sum(r["useful_actions"] for r in rs) / len(rs), 2)
            agg[cond]["avg_redundant_actions"] = round(
                sum(r["redundant_actions"] for r in rs) / len(rs), 2)

    return agg


if __name__ == "__main__":
    import argparse
    ap = argparse.ArgumentParser()
    ap.add_argument("exp_dir")
    ap.add_argument("--report", action="store_true")
    args = ap.parse_args()

    results = score_experiment(args.exp_dir)
    agg = aggregate(results)
    report = {"scoring_version": SCORING_VERSION,
              "n_units": len(results),
              "aggregate": agg}

    report_path = os.path.join(args.exp_dir, "acq_report.json")
    with open(report_path, "w") as f:
        json.dump(report, f, indent=2, sort_keys=True)

    if args.report:
        print(json.dumps(agg, indent=2))
    else:
        print(f"Scored {len(results)} units → {report_path}")

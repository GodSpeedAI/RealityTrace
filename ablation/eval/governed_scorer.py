#!/usr/bin/env python3
"""Scorer for governed-diagnostic-acquisition-001.

Computes per-unit and aggregate metrics:
  A_trigger: acquisition triggering (premature attempts, invocation rate)
  A_select: action selection (first/any discriminating)
  A_use: evidence utilization (correct settlement | discriminating evidence)
  A_c, A_u: system-level outcomes
  Failure locus classification (§28)
"""

from __future__ import annotations

import json
import os
import sys
from collections import defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

from eval.acquisition import PROBE_BY_ID, LOCUS_VOCABULARY, MAX_ROUNDS, MAX_PAYMENT
from eval.governed import SIGNAL_RULES, BEARS_ON, GATE_INFEASIBLE_LOCI

SCORING_VERSION = "gov-scorer-1"
VALID_LOCI = set(LOCUS_VOCABULARY)


def load_scored(exp_dir):
    scored_dir = os.path.join(exp_dir, "scored")
    results = []
    if not os.path.isdir(scored_dir):
        return results
    for fname in sorted(os.listdir(scored_dir)):
        if fname.endswith(".json"):
            with open(os.path.join(scored_dir, fname)) as f:
                results.append(json.load(f))
    return results


def load_trajectories(raw_dir):
    results = {}
    for fname in os.listdir(raw_dir):
        if fname.endswith(".trajectory.json"):
            with open(os.path.join(raw_dir, fname)) as f:
                d = json.load(f)
                results[d["unit_id"]] = d
    return results


def score_unit(trajectory, usage, governed_keys, hidden_meta):
    """Score one unit."""
    cid = trajectory["case_id"]
    cond = trajectory["condition"]
    key = governed_keys.get(cid, {})
    true_locus = key.get("true_locus", "unknown")
    gate_infeasible = key.get("gate_infeasible", False)
    discriminating = set(hidden_meta.get(cid, {}).get("discriminating_probes", []))

    final_locus = trajectory.get("final_locus", "unknown")
    actions = trajectory.get("actions", [])
    action_ids = [a["action"] for a in actions]
    acquired = trajectory.get("acquired_probes", [])
    observations = trajectory.get("observations", {})
    premature = trajectory.get("premature_attempts", [])
    unknown_premature = trajectory.get("unknown_premature", 0)
    terminal = trajectory.get("terminal_state", "")

    is_unknown_case = true_locus == "unknown"
    is_determinate = not is_unknown_case

    # ── correctness ──────────────────────────────────────────────────────────
    if is_unknown_case:
        exact_correct = False
        unknown_correct = final_locus == "unknown"
        unsupported_concrete = final_locus != "unknown"
        semantic_model_fp = False
    else:
        exact_correct = final_locus == true_locus
        unknown_correct = False
        unsupported_concrete = False
        semantic_model_fp = (true_locus != "declared_model"
                             and final_locus == "declared_model")

    # diagnostic-decision success
    diag_success = unknown_correct or exact_correct

    # ── acquisition triggering ───────────────────────────────────────────────
    acquired_any = len(acquired) > 0
    rounds_before_first = None
    for i, a in enumerate(actions):
        if a.get("round"):
            rounds_before_first = a["round"] - 1
            break

    # ── action selection ─────────────────────────────────────────────────────
    discrim_selected_any = bool(set(action_ids) & discriminating) if discriminating else False
    first_action = action_ids[0] if action_ids else None
    first_is_discrim = first_action in discriminating if (first_action and discriminating) else False

    # ── evidence utilization ─────────────────────────────────────────────────
    has_discrim_evidence = discrim_selected_any and discriminating
    if has_discrim_evidence:
        correct_use = exact_correct if is_determinate else unknown_correct
    else:
        correct_use = None

    # ── governed-system safety ───────────────────────────────────────────────
    # In G/O, unsupported concrete settlement is structurally prevented.
    # If gate prevents it, this is GOVERNED SAFETY, not model restraint.
    governed_safe_concrete = 0
    if cond in ("G", "O") and is_unknown_case:
        # If terminal is UNKNOWN, the gate prevented concrete settlement
        if final_locus == "unknown":
            governed_safe_concrete = 1

    # ── budget ───────────────────────────────────────────────────────────────
    budget_exhausted = usage.get("payment_used", 0) >= MAX_PAYMENT

    # ── failure locus classification (§28) ───────────────────────────────────
    failure_locus = None
    if is_determinate and not exact_correct:
        if cond in ("G", "O") and gate_infeasible:
            failure_locus = "TRUE_HORIZON_EXHAUSTION"
        elif not acquired_any:
            failure_locus = "SUFFICIENCY_DETECTION_FAILURE"
        elif not discrim_selected_any:
            if budget_exhausted:
                failure_locus = "BUDGET_FAILURE"
            else:
                failure_locus = "ACTION_SELECTION_FAILURE"
        elif has_discrim_evidence and not correct_use:
            failure_locus = "EVIDENCE_USE_FAILURE"
        elif final_locus not in VALID_LOCI:
            failure_locus = "CONTRACT_FAILURE"
        else:
            failure_locus = "EVIDENCE_USE_FAILURE"
    elif is_unknown_case and not unknown_correct:
        if cond == "V":
            failure_locus = "SUFFICIENCY_DETECTION_FAILURE"
        else:
            failure_locus = "SUFFICIENCY_DETECTION_FAILURE"

    return {
        "unit_id": trajectory["unit_id"],
        "case_id": cid,
        "condition": cond,
        "rep": trajectory.get("rep", 1),
        "true_locus": true_locus,
        "gate_infeasible": gate_infeasible,
        "final_locus": final_locus,
        "terminal_state": terminal,
        "scoring_version": SCORING_VERSION,
        "exact_correct": exact_correct,
        "unknown_correct": unknown_correct,
        "diag_success": diag_success,
        "unsupported_concrete": unsupported_concrete,
        "semantic_model_fp": semantic_model_fp,
        "is_determinate": is_determinate,
        "is_unknown_case": is_unknown_case,
        "acquired_any": acquired_any,
        "num_actions": len(action_ids),
        "action_ids": action_ids,
        "acquired_probes": acquired,
        "discriminating_probes": sorted(discriminating),
        "discrim_selected_any": discrim_selected_any,
        "first_action": first_action,
        "first_is_discrim": first_is_discrim,
        "has_discrim_evidence": has_discrim_evidence,
        "correct_use": correct_use,
        "premature_attempts": len(premature),
        "unknown_premature": unknown_premature,
        "rounds_before_first": rounds_before_first,
        "payment_used": usage.get("payment_used", 0),
        "budget_exhausted": budget_exhausted,
        "total_tokens": usage.get("total_tokens", 0),
        "reasoning_tokens": usage.get("reasoning_tokens", 0),
        "model_calls": usage.get("model_calls", 1),
        "latency_ms": usage.get("latency_ms", 0),
        "governed_safe_concrete": governed_safe_concrete,
        "failure_locus": failure_locus,
    }


def score_all(exp_dir, governed_keys, hidden_meta):
    raw_dir = os.path.join(exp_dir, "raw")
    scored_dir = os.path.join(exp_dir, "scored")
    os.makedirs(scored_dir, exist_ok=True)

    trajectories = load_trajectories(raw_dir)
    results = []
    for uid, traj in trajectories.items():
        usage_path = os.path.join(raw_dir, f"{uid}.usage.json")
        usage = json.load(open(usage_path)) if os.path.exists(usage_path) else {}
        scored = score_unit(traj, usage, governed_keys, hidden_meta)
        with open(os.path.join(scored_dir, f"{uid}.json"), "w") as f:
            json.dump(scored, f, indent=2, sort_keys=True, default=list)
        results.append(scored)
    return results


def aggregate(results):
    """Compute aggregate metrics by condition."""
    by_cond = defaultdict(list)
    for r in results:
        by_cond[r["condition"]].append(r)

    agg = {}
    for cond in ("V", "G", "O"):
        units = by_cond.get(cond, [])
        if not units:
            continue
        n = len(units)
        det = [r for r in units if r["is_determinate"] and not r["gate_infeasible"]]
        unk = [r for r in units if r["is_unknown_case"]]
        det_all = [r for r in units if r["is_determinate"]]
        unk_all = [r for r in units if r["is_unknown_case"]]

        # Cases with discriminating path (for A_select denominator)
        has_discrim = [r for r in det if r["discriminating_probes"]]

        agg[cond] = {
            "n": n,
            "n_determinate_feasible": len(det),
            "n_determinate_infeasible": len(det_all) - len(det),
            "n_unknown": len(unk),
            "A_c": sum(r["exact_correct"] for r in det) / len(det) if det else 0,
            "A_u": sum(r["unknown_correct"] for r in unk) / len(unk) if unk else 0,
            "diag_success": sum(r["diag_success"] for r in det + unk) / len(det + unk) if (det or unk) else 0,
            "unsupported_concrete": sum(r["unsupported_concrete"] for r in unk),
            "semantic_model_fp": sum(r["semantic_model_fp"] for r in det_all),
            "acquisition_invocation_rate": sum(r["acquired_any"] for r in units) / n,
            "premature_attempts_total": sum(r["premature_attempts"] for r in units),
            "unknown_premature_total": sum(r["unknown_premature"] for r in units),
            "A_select_first": (sum(1 for r in has_discrim if r["first_is_discrim"]) / len(has_discrim)
                              if has_discrim else 0),
            "A_select_any": (sum(1 for r in has_discrim if r["discrim_selected_any"]) / len(has_discrim)
                            if has_discrim else 0),
            "avg_actions": sum(r["num_actions"] for r in units) / n,
            "avg_payment": sum(r["payment_used"] for r in units) / n,
            "budget_exhaustion_rate": sum(r["budget_exhausted"] for r in units) / n,
            "avg_tokens": sum(r["total_tokens"] for r in units) / n,
            "avg_model_calls": sum(r["model_calls"] for r in units) / n,
            "avg_rounds_before_first": (sum(r["rounds_before_first"] for r in units if r["rounds_before_first"] is not None)
                                        / max(1, sum(1 for r in units if r["rounds_before_first"] is not None))),
            # A_use: correct | discriminating evidence acquired
            "A_use": (sum(1 for r in det if r["has_discrim_evidence"] and r["exact_correct"])
                      / max(1, sum(1 for r in det if r["has_discrim_evidence"]))),
            "n_with_discrim_evidence": sum(1 for r in det if r["has_discrim_evidence"]),
            "governed_safe_unknown": sum(r["governed_safe_concrete"] for r in unk),
        }

    # Failure locus breakdown for G
    g_units = by_cond.get("G", [])
    if g_units:
        fl = defaultdict(int)
        for r in g_units:
            if r["failure_locus"]:
                fl[r["failure_locus"]] += 1
        agg["G_failure_loci"] = dict(fl)

    return agg


def main():
    if len(sys.argv) < 2:
        print("Usage: governed_scorer.py <exp_dir>")
        sys.exit(1)
    exp_dir = sys.argv[1]
    corpus = os.path.join(HERE, "..", "corpus")
    governed_keys = json.load(open(os.path.join(corpus, "governed-keys.json")))
    hidden_meta = json.load(open(os.path.join(corpus, "acquisition-hidden-meta.json")))
    results = score_all(exp_dir, governed_keys, hidden_meta)
    agg = aggregate(results)
    report_path = os.path.join(exp_dir, "gov_scored.json")
    with open(report_path, "w") as f:
        json.dump({"scoring_version": SCORING_VERSION, "aggregate": agg,
                   "n_units": len(results)}, f, indent=2, sort_keys=True)
    print(f"Scored {len(results)} units")
    for cond in ("V", "G", "O"):
        if cond in agg:
            a = agg[cond]
            print(f"  {cond}: A_c={a['A_c']:.3f} A_u={a['A_u']:.3f} "
                  f"invoke={a['acquisition_invocation_rate']:.3f} "
                  f"sel_first={a['A_select_first']:.3f} sel_any={a['A_select_any']:.3f} "
                  f"use={a['A_use']:.3f}")
    print(f"Report: {report_path}")


if __name__ == "__main__":
    main()

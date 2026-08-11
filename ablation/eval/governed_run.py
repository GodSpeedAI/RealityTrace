#!/usr/bin/env python3
"""H-GOVERNED-ACQUISITION experiment driver (540 units: 3 conditions x 60 x 3).

Three controller conditions:
  V — voluntary acquisition (no gate, fresh D2 replication)
  G — governed self-selected acquisition (structural sufficiency gate)
  O — governed oracle-probe upper bound (harness selects probes)

The harness owns the state machine. The model proposes actions/settlements.
Gate checks are controller-level, not prompt-level.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

from eval.state import CheckpointStore  # noqa: E402
from eval.report import experiment_dir  # noqa: E402
from eval.acquisition import (  # noqa: E402
    PROBES, PROBE_BY_ID, PROBE_IDS, MAX_ROUNDS, MAX_PAYMENT,
    LOCUS_VOCABULARY, SYSTEM_PROMPT, action_catalogue_text, scaffold_hash,
)
from eval.governed import (  # noqa: E402
    EXPERIMENT_ID, SEED, CONDITIONS,
    V_G_SCAFFOLD_ROUND1, V_G_SCAFFOLD_ROUNDN, V_G_SCAFFOLD_BUDGET_EXHAUSTED,
    G_SCAFFOLD_REJECTION_SETTLEMENT, G_SCAFFOLD_REJECTION_UNKNOWN,
    O_SCAFFOLD_ROUND1, O_SCAFFOLD_OBSERVATION,
    SIGNAL_RULES, BEARS_ON, GATE_INFEASIBLE_LOCI,
    settlement_eligible, unknown_navigable, affordable_actions,
    acquisition_contract_dict, contract_fingerprint, evaluate_verdicts,
    HYPOTHESIS_RULE,
)
from eval.context_packer import pack_context, estimate_tokens  # noqa: E402
from eval.zai_client import call_coding_plan, ModelClientError  # noqa: E402
from eval.release import verify_release  # noqa: E402
from eval.attribution_run import load_artifacts  # noqa: E402
from eval.store import build_store, load_case_rows  # noqa: E402
from eval.arms import build_arm_context, validate_arm_context  # noqa: E402

CORPUS = os.path.join(HERE, "..", "corpus")
SXR_ROOT = os.path.join(HERE, "..", "..")
REPS = 3
ARM_D = "D"
MAX_MODEL_CALLS = {"V": 6, "G": 10, "O": 6}

VALID_LOCI = set(LOCUS_VOCABULARY)


class _Unit:
    def __init__(self, unit_id, case_id, condition, rep):
        self.unit_id = unit_id
        self.case_id = case_id
        self.condition = condition
        self.rep = rep


def unit_id(exp, case, condition, rep, fingerprint):
    payload = "|".join((exp, case, condition, str(rep), fingerprint))
    return hashlib.sha256(payload.encode()).hexdigest()[:16]


def load_probe_results():
    path = os.path.join(CORPUS, "acquisition-probes.jsonl")
    results = {}
    with open(path) as f:
        for line in f:
            d = json.loads(line)
            if "case_id" in d:
                results[d["case_id"]] = d
    return results


def load_hidden_meta():
    with open(os.path.join(CORPUS, "acquisition-hidden-meta.json")) as f:
        return json.load(f)


def load_governed_keys():
    with open(os.path.join(CORPUS, "governed-keys.json")) as f:
        return json.load(f)


def format_observation(probe_id, probe_result):
    p = probe_result.get("probes", {}).get(probe_id, {})
    if "error" in p:
        return f"[probe error: {p['error']}]"
    return json.dumps(p, sort_keys=True, default=str)[:2000]


def parse_round_response(text):
    m = re.search(r'\{[^{}]*"candidate_loci"[^{}]*\}', text, re.DOTALL)
    if not m:
        m = re.search(r'\{[^{}]+\}', text, re.DOTALL)
    if not m:
        return None
    try:
        return json.loads(m.group())
    except json.JSONDecodeError:
        return None


def extract_locus(text):
    from eval.runner import extract_locus as _el
    return _el(text)


def oracle_select(case_id, acquired_probes, payment_used, hidden_meta):
    """Select lowest-payment discriminating probe. Uses hidden meta ONLY for selection."""
    m = hidden_meta.get(case_id, {})
    discriminating = m.get("discriminating_probes", [])
    candidates = [
        pid for pid in discriminating
        if pid not in acquired_probes
        and payment_used + PROBE_BY_ID[pid]["cost"] <= MAX_PAYMENT
    ]
    if candidates:
        return min(candidates, key=lambda p: PROBE_BY_ID[p]["cost"])
    all_affordable = [
        pid for pid in PROBE_IDS
        if pid not in acquired_probes
        and payment_used + PROBE_BY_ID[pid]["cost"] <= MAX_PAYMENT
    ]
    if all_affordable:
        return min(all_affordable, key=lambda p: PROBE_BY_ID[p]["cost"])
    return None


def _model_call(prompt, condition):
    return call_coding_plan(
        model="glm-5.2", system_prompt=SYSTEM_PROMPT,
        user_prompt=prompt, temperature=0.0, max_tokens=8192)


def freeze_contract(exp_dir):
    manifest = {
        "experiment_id": EXPERIMENT_ID,
        "kind": "governed-diagnostic-acquisition-H-GOVERNED-ACQUISITION",
        "derived_from": "diagnostic-acquisition-001 (NOT SUPPORTED)",
        "parent_report": ".agents/reports/8-diagnostic-acquisition.md",
        "conditions": list(CONDITIONS),
        "reps": REPS,
        "model": {"model_id": "glm-5.2", "provider": "zai-coding-plan",
                  "endpoint": "https://api.z.ai/api/coding/paas/v4",
                  "temperature": 0.0, "context_budget_tokens": 32_000,
                  "max_tokens": 4096},
        "contract_fingerprint": contract_fingerprint(),
        "acquisition_budget": {"max_rounds": MAX_ROUNDS, "max_payment": MAX_PAYMENT},
        "probe_catalogue_hash": hashlib.sha256(
            json.dumps(PROBES, sort_keys=True).encode()).hexdigest(),
        "hypothesis": HYPOTHESIS_RULE,
    }
    paths = {
        "corpus_cases": os.path.join(CORPUS, "cases.jsonl"),
        "sealed_keys_sha256": os.path.join(CORPUS, "keys.sha256"),
        "governed_keys": os.path.join(CORPUS, "governed-keys.json"),
        "governed_sufficiency_contract": os.path.join(CORPUS, "governed-sufficiency-contract.json"),
        "acquisition_hidden_meta": os.path.join(CORPUS, "acquisition-hidden-meta.json"),
        "acquisition_probes": os.path.join(CORPUS, "acquisition-probes.jsonl"),
    }
    manifest["hashes"] = {}
    for name, path in paths.items():
        if os.path.exists(path):
            if name == "sealed_keys_sha256":
                manifest["hashes"][name] = open(path).read().strip().split()[0]
            else:
                manifest["hashes"][name] = hashlib.sha256(open(path, "rb").read()).hexdigest()
    manifest["hashes"]["sxr_git_head"] = subprocess.run(
        ["git", "-C", SXR_ROOT, "rev-parse", "HEAD"],
        capture_output=True, text=True).stdout.strip()
    blob = json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
    manifest["contract_hash"] = hashlib.sha256(blob).hexdigest()
    os.makedirs(exp_dir, exist_ok=True)
    with open(os.path.join(exp_dir, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=2)
    return manifest


def run_unit(unit, case_row, ctx_sections, probe_results, hidden_meta,
             store, raw_root):
    """Execute one experimental unit (V, G, or O)."""
    uid = unit.unit_id
    cid = unit.case_id
    cond = unit.condition
    contract = acquisition_contract_dict(cond)
    budget = int(contract["context_budget_tokens"])
    case_probes = probe_results.get(cid, {})

    pc = pack_context(dict(ctx_sections), SYSTEM_PROMPT, cid, ARM_D, budget)
    ctx_text = "\n\n".join("[%s]\n%s" % (c, pc.sections[c])
                           for c in sorted(pc.sections))
    catalogue = action_catalogue_text()

    acquired = ()
    observations = {}
    payment_used = 0
    all_rounds = []
    all_calls = []
    premature_attempts = []
    unknown_premature_count = 0
    actions_taken = []
    final_locus = None
    terminal_state = None
    total_tokens = 0
    total_reasoning = 0
    call_count = 0
    max_calls = MAX_MODEL_CALLS[cond]

    def do_call(prompt):
        nonlocal call_count, total_tokens, total_reasoning
        call_count += 1
        t0 = time.monotonic()
        mc = _model_call(prompt, cond)
        elapsed = (time.monotonic() - t0) * 1000
        u = mc.usage or {}
        total_tokens += int(u.get("total_tokens") or 0)
        total_reasoning += int(u.get("reasoning_tokens") or 0)
        call_data = {
            "call_num": call_count,
            "latency_ms": getattr(mc, 'latency_ms', round(elapsed, 1)),
            "prompt_tokens": int(u.get("prompt_tokens") or 0),
            "completion_tokens": int(u.get("completion_tokens") or 0),
            "response_preview": (mc.text or "")[:200],
        }
        all_calls.append(call_data)
        return mc

    def execute_action(action_id):
        nonlocal payment_used
        cost = PROBE_BY_ID[action_id]["cost"]
        payment_used += cost
        obs_text = format_observation(action_id, case_probes)
        obs_data = case_probes.get("probes", {}).get(action_id, {})
        observations[action_id] = obs_data
        return action_id, obs_text, cost

    # ── Round 1 ──────────────────────────────────────────────────────────────
    if cond in ("V", "G"):
        prompt = V_G_SCAFFOLD_ROUND1.format(
            case_id=cid, context=ctx_text, action_catalogue=catalogue,
            max_rounds=MAX_ROUNDS, max_payment=MAX_PAYMENT)
    else:  # O
        prompt = O_SCAFFOLD_ROUND1.format(case_id=cid, context=ctx_text)

    if estimate_tokens(prompt) > budget:
        prompt = prompt[:budget * 3]

    prev_candidates = ""
    prev_unresolved = ""
    prev_action = "(none)"

    for round_num in range(1, MAX_ROUNDS + 2):
        if call_count >= max_calls:
            terminal_state = "MAX_CALLS_REACHED"
            final_locus = final_locus or "unknown"
            break

        mc = do_call(prompt)
        response = mc.text
        round_data = {"round": round_num, "calls_in_round": 1}

        # Try to parse structured response
        parsed = parse_round_response(response)
        locus = extract_locus(response)

        # Check for settle
        settle = None
        if parsed and isinstance(parsed.get("settle"), dict):
            settle = parsed["settle"]

        if not settle and locus:
            settle = {"locus": locus}

        if settle and settle.get("locus"):
            slocus = settle["locus"]
            if slocus not in VALID_LOCI:
                slocus = "unknown"

            if cond == "V":
                final_locus = slocus
                terminal_state = "CONCRETE_SETTLEMENT" if slocus != "unknown" else "UNKNOWN_SETTLEMENT"
                round_data["settled"] = slocus
                break

            # G or O: gate check
            if slocus == "unknown":
                # NAVIGABILITY_GATE
                if unknown_navigable(acquired, payment_used):
                    unknown_premature_count += 1
                    round_data["unknown_rejected"] = True
                    avail = affordable_actions(acquired, payment_used)
                    if cond == "O":
                        # Oracle selects probe automatically
                        action = oracle_select(cid, acquired, payment_used, hidden_meta)
                        if action:
                            aid, obs_text, cost = execute_action(action)
                            actions_taken.append({"action": aid, "source": "oracle", "round": round_num})
                            acquired += (aid,)
                            prev_action = aid
                            prev_candidates = ", ".join(parsed.get("candidate_loci", [])) if parsed else ""
                            prev_unresolved = parsed.get("unresolved_distinction", "") if parsed else ""
                            prompt = O_SCAFFOLD_OBSERVATION.format(
                                round_num=round_num + 1, max_rounds=MAX_ROUNDS,
                                payment_used=payment_used, max_payment=MAX_PAYMENT,
                                prev_candidates=prev_candidates,
                                prev_unresolved=prev_unresolved,
                                probe_id=aid, observation=obs_text)
                            all_rounds.append(round_data)
                            continue
                    else:
                        # G: force action selection
                        prompt = G_SCAFFOLD_REJECTION_UNKNOWN.format(
                            payment_used=payment_used, max_payment=MAX_PAYMENT,
                            action_catalogue=catalogue)
                        mc2 = do_call(prompt)
                        parsed2 = parse_round_response(mc2.text)
                        if parsed2 and parsed2.get("selected_action") in PROBE_BY_ID:
                            action = parsed2["selected_action"]
                            if payment_used + PROBE_BY_ID[action]["cost"] <= MAX_PAYMENT:
                                aid, obs_text, cost = execute_action(action)
                                actions_taken.append({"action": aid, "source": "model", "round": round_num})
                                acquired += (aid,)
                                prev_action = aid
                                prev_candidates = ", ".join(parsed2.get("candidate_loci", prev_candidates.split(", ")))
                                prev_unresolved = parsed2.get("unresolved_distinction", prev_unresolved)
                                prompt = V_G_SCAFFOLD_ROUNDN.format(
                                    round_num=round_num + 1, max_rounds=MAX_ROUNDS,
                                    payment_used=payment_used, max_payment=MAX_PAYMENT,
                                    prev_candidates=prev_candidates, prev_unresolved=prev_unresolved,
                                    prev_action=aid, observation=obs_text,
                                    action_catalogue=catalogue)
                                round_data["calls_in_round"] += 1
                                round_data["forced_action"] = aid
                                all_rounds.append(round_data)
                                continue
                        # Can't parse or can't afford
                        terminal_state = "BUDGET_EXHAUSTED"
                        final_locus = "unknown"
                        all_rounds.append(round_data)
                        break
                else:
                    final_locus = "unknown"
                    terminal_state = "TERMINAL_UNKNOWN"
                    round_data["terminal_unknown"] = True
                    all_rounds.append(round_data)
                    break
            else:
                # SUFFICIENCY_GATE for concrete locus
                eligible = settlement_eligible(slocus, acquired, observations)
                if eligible:
                    final_locus = slocus
                    terminal_state = "CONCRETE_SETTLEMENT"
                    round_data["settled"] = slocus
                    round_data["gate_passed"] = True
                    all_rounds.append(round_data)
                    break
                else:
                    premature_attempts.append({"round": round_num, "locus": slocus})
                    round_data["settlement_rejected"] = slocus
                    if cond == "O":
                        action = oracle_select(cid, acquired, payment_used, hidden_meta)
                        if action:
                            aid, obs_text, cost = execute_action(action)
                            actions_taken.append({"action": aid, "source": "oracle", "round": round_num})
                            acquired += (aid,)
                            prev_action = aid
                            prev_candidates = ", ".join(parsed.get("candidate_loci", [])) if parsed else ""
                            prev_unresolved = parsed.get("unresolved_distinction", "") if parsed else ""
                            prompt = O_SCAFFOLD_OBSERVATION.format(
                                round_num=round_num + 1, max_rounds=MAX_ROUNDS,
                                payment_used=payment_used, max_payment=MAX_PAYMENT,
                                prev_candidates=prev_candidates,
                                prev_unresolved=prev_unresolved,
                                probe_id=aid, observation=obs_text)
                            all_rounds.append(round_data)
                            continue
                    else:
                        prompt = G_SCAFFOLD_REJECTION_SETTLEMENT.format(
                            rejected_locus=slocus, action_catalogue=catalogue)
                        mc2 = do_call(prompt)
                        parsed2 = parse_round_response(mc2.text)
                        if parsed2 and parsed2.get("selected_action") in PROBE_BY_ID:
                            action = parsed2["selected_action"]
                            if payment_used + PROBE_BY_ID[action]["cost"] <= MAX_PAYMENT:
                                aid, obs_text, cost = execute_action(action)
                                actions_taken.append({"action": aid, "source": "model", "round": round_num})
                                acquired += (aid,)
                                prev_action = aid
                                prev_candidates = ", ".join(parsed2.get("candidate_loci", prev_candidates.split(", ")))
                                prev_unresolved = parsed2.get("unresolved_distinction", prev_unresolved)
                                prompt = V_G_SCAFFOLD_ROUNDN.format(
                                    round_num=round_num + 1, max_rounds=MAX_ROUNDS,
                                    payment_used=payment_used, max_payment=MAX_PAYMENT,
                                    prev_candidates=prev_candidates, prev_unresolved=prev_unresolved,
                                    prev_action=aid, observation=obs_text,
                                    action_catalogue=catalogue)
                                round_data["calls_in_round"] += 1
                                round_data["forced_action"] = aid
                                all_rounds.append(round_data)
                                continue
                        terminal_state = "BUDGET_EXHAUSTED"
                        final_locus = "unknown"
                        all_rounds.append(round_data)
                        break

        # Check for action selection
        selected = parsed.get("selected_action") if parsed else None
        if cond == "O" and not settle:
            # O: if model didn't settle, harness selects probe
            action = oracle_select(cid, acquired, payment_used, hidden_meta)
            if action:
                aid, obs_text, cost = execute_action(action)
                actions_taken.append({"action": aid, "source": "oracle", "round": round_num})
                acquired += (aid,)
                prev_action = aid
                prev_candidates = ", ".join(parsed.get("candidate_loci", [])) if parsed else ""
                prev_unresolved = parsed.get("unresolved_distinction", "") if parsed else ""
                prompt = O_SCAFFOLD_OBSERVATION.format(
                    round_num=round_num + 1, max_rounds=MAX_ROUNDS,
                    payment_used=payment_used, max_payment=MAX_PAYMENT,
                    prev_candidates=prev_candidates, prev_unresolved=prev_unresolved,
                    probe_id=aid, observation=obs_text)
                round_data["oracle_action"] = aid
                all_rounds.append(round_data)
                continue
            else:
                terminal_state = "BUDGET_EXHAUSTED"
                final_locus = "unknown"
                all_rounds.append(round_data)
                break

        if selected and selected in PROBE_BY_ID:
            cost = PROBE_BY_ID[selected]["cost"]
            if payment_used + cost <= MAX_PAYMENT:
                aid, obs_text, acost = execute_action(selected)
                actions_taken.append({"action": aid, "source": "model", "round": round_num})
                acquired += (aid,)
                prev_action = aid
                prev_candidates = ", ".join(parsed.get("candidate_loci", [])) if parsed else ""
                prev_unresolved = parsed.get("unresolved_distinction", "") if parsed else ""
                if round_num >= MAX_ROUNDS or payment_used >= MAX_PAYMENT:
                    obs_summary = "\n".join(f"[{a['action']}]: {format_observation(a['action'], case_probes)}"
                                           for a in actions_taken[-3:])
                    prompt = V_G_SCAFFOLD_BUDGET_EXHAUSTED.format(
                        payment_used=payment_used, max_payment=MAX_PAYMENT,
                        round_num=round_num, max_rounds=MAX_ROUNDS,
                        observations_summary=obs_summary)
                else:
                    prompt = V_G_SCAFFOLD_ROUNDN.format(
                        round_num=round_num + 1, max_rounds=MAX_ROUNDS,
                        payment_used=payment_used, max_payment=MAX_PAYMENT,
                        prev_candidates=prev_candidates, prev_unresolved=prev_unresolved,
                        prev_action=aid, observation=obs_text,
                        action_catalogue=catalogue)
                round_data["action"] = aid
                all_rounds.append(round_data)
                continue
            else:
                round_data["budget_blocked"] = selected
        else:
            round_data["no_valid_action"] = True

        # If response was useless (empty/unparseable), force final answer
        # immediately — don't loop with the same prompt
        if round_data.get("no_valid_action") and not round_data.get("action"):
            obs_summary = "\n".join(f"[{a['action']}]: {format_observation(a['action'], case_probes)}"
                                   for a in actions_taken[-3:]) or "(none)"
            prompt = V_G_SCAFFOLD_BUDGET_EXHAUSTED.format(
                payment_used=payment_used, max_payment=MAX_PAYMENT,
                round_num=round_num, max_rounds=MAX_ROUNDS,
                observations_summary=obs_summary)
            mc_final = do_call(prompt)
            locus_final = extract_locus(mc_final.text)
            if locus_final and locus_final in VALID_LOCI:
                if cond == "V":
                    final_locus = locus_final
                else:
                    if locus_final == "unknown":
                        final_locus = "unknown"
                    elif settlement_eligible(locus_final, acquired, observations):
                        final_locus = locus_final
                    else:
                        final_locus = "unknown"
            else:
                final_locus = "unknown"
            terminal_state = "BUDGET_EXHAUSTED"
            round_data["budget_exhausted"] = True
            round_data["final_locus"] = final_locus
            all_rounds.append(round_data)
            break

        # Budget exhausted or can't parse
        if round_num >= MAX_ROUNDS or payment_used >= MAX_PAYMENT or not affordable_actions(acquired, payment_used):
            obs_summary = "\n".join(f"[{a['action']}]: {format_observation(a['action'], case_probes)}"
                                   for a in actions_taken[-3:]) or "(none)"
            prompt = V_G_SCAFFOLD_BUDGET_EXHAUSTED.format(
                payment_used=payment_used, max_payment=MAX_PAYMENT,
                round_num=round_num, max_rounds=MAX_ROUNDS,
                observations_summary=obs_summary)
            mc_final = do_call(prompt)
            locus_final = extract_locus(mc_final.text)
            if locus_final and locus_final in VALID_LOCI:
                if cond == "V":
                    final_locus = locus_final
                else:
                    if locus_final == "unknown":
                        final_locus = "unknown"
                    elif settlement_eligible(locus_final, acquired, observations):
                        final_locus = locus_final
                    else:
                        final_locus = "unknown"
            else:
                final_locus = "unknown"
            terminal_state = "BUDGET_EXHAUSTED"
            round_data["budget_exhausted"] = True
            round_data["final_locus"] = final_locus
            all_rounds.append(round_data)
            break

        all_rounds.append(round_data)

    if final_locus is None:
        final_locus = "unknown"
        terminal_state = terminal_state or "CONTRACT_FAILURE"

    # Compose raw output
    raw_output = f"Locus: {final_locus}\n"
    raw_output += f"Terminal: {terminal_state}\n"
    raw_output += f"Actions: {', '.join(a['action'] for a in actions_taken)}\n"
    raw_output += f"Payment: {payment_used}/{MAX_PAYMENT}\n"
    raw_output += f"Rounds: {len(all_rounds)}\n"
    raw_output += f"Premature attempts: {len(premature_attempts)}\n"
    raw_output += f"Unknown premature: {unknown_premature_count}\n"

    raw_path = os.path.join(raw_root, f"{uid}.txt")
    with open(raw_path, "w") as f:
        f.write(raw_output)
    usage = {
        "total_tokens": total_tokens, "prompt_tokens": 0,
        "completion_tokens": 0, "reasoning_tokens": total_reasoning,
        "cost": 0.0, "latency_ms": sum(c.get("latency_ms", 0) for c in all_calls),
        "retries": 0, "resolved_model": "glm-5.2",
        "condition": cond, "rounds": len(all_rounds),
        "model_calls": call_count,
        "actions_taken": [a["action"] for a in actions_taken],
        "action_sources": [a["source"] for a in actions_taken],
        "payment_used": payment_used,
        "premature_attempts": len(premature_attempts),
        "unknown_premature": unknown_premature_count,
        "terminal_state": terminal_state,
        "acquired_probes": list(acquired),
    }
    with open(raw_path.replace(".txt", ".usage.json"), "w") as f:
        json.dump(usage, f, sort_keys=True)
    trajectory = {
        "unit_id": uid, "case_id": cid, "condition": cond, "rep": unit.rep,
        "rounds": all_rounds, "calls": all_calls,
        "actions": actions_taken,
        "observations": {pid: observations.get(pid, {}) for pid in acquired},
        "premature_attempts": premature_attempts,
        "unknown_premature": unknown_premature_count,
        "final_locus": final_locus, "terminal_state": terminal_state,
        "payment_used": payment_used, "acquired_probes": list(acquired),
    }
    with open(raw_path.replace(".txt", ".trajectory.json"), "w") as f:
        json.dump(trajectory, f, indent=2, sort_keys=True)
    h = hashlib.sha256(raw_output.encode()).hexdigest()
    store.transition(uid, "output_committed", raw_output_path=raw_path,
                     raw_output_hash=h,
                     note=f"gov:{cond};tokens={total_tokens};calls={call_count}")
    return raw_output


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seed", type=int, default=SEED)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--pilot", action="store_true")
    ap.add_argument("--worker", type=int, default=0)
    ap.add_argument("--num-workers", type=int, default=1)
    args = ap.parse_args()

    rows = load_case_rows(os.path.join(CORPUS, "cases.jsonl"))
    ids = [r["case_id"] for r in rows]
    assert len(ids) == 60

    namespace = "gov-pilot" if args.pilot else "gov-full"
    exp_name = f"{EXPERIMENT_ID}-pilot" if args.pilot else EXPERIMENT_ID
    exp_dir = experiment_dir("runs", namespace, exp_name)
    os.makedirs(exp_dir, exist_ok=True)
    store = CheckpointStore(os.path.join(exp_dir, "manifest.jsonl"))
    raw_root = os.path.join(exp_dir, "raw")
    os.makedirs(raw_root, exist_ok=True)

    probe_results = load_probe_results()
    hidden_meta = load_hidden_meta()
    env = dict(os.environ)
    verify_release(EXPERIMENT_ID, env)

    if not os.path.exists(os.path.join(exp_dir, "manifest.json")):
        freeze_contract(exp_dir)
        print("contract frozen")
    else:
        print("contract already frozen")

    fp = contract_fingerprint()

    pilot_cases = None
    if args.pilot:
        pilot_cases = {"ag-06", "ag-09", "cep-11", "cep-15", "sr-11"}

    units = []
    for cond in CONDITIONS:
        for case in sorted(ids):
            if pilot_cases and case not in pilot_cases:
                continue
            for rep in range(1, REPS + 1):
                units.append(_Unit(unit_id(EXPERIMENT_ID, case, cond, rep, fp),
                                   case, cond, rep))
    import random
    rng = random.Random(args.seed)
    order = [u.unit_id for u in units]
    rng.shuffle(order)
    if args.limit:
        order = order[:args.limit]
    # Worker partitioning: each worker processes every Nth unit
    if args.num_workers > 1:
        order = [uid for i, uid in enumerate(order) if i % args.num_workers == args.worker]
    by_unit = {u.unit_id: u for u in units}
    with open(os.path.join(exp_dir, "matrix.json"), "w") as f:
        json.dump({"seed": args.seed, "fingerprint": fp, "order": order,
                   "units": [{"unit_id": u.unit_id, "case_id": u.case_id,
                              "condition": u.condition, "rep": u.rep}
                             for u in units]}, f, indent=1)

    artifacts = load_artifacts(rows)
    store_map = build_store(rows, artifacts)
    rows_by_id = {r["case_id"]: r for r in rows}

    total = len(order)
    executed = skipped = failed = 0
    for uid in order:
        u = by_unit[uid]
        st = store.get(uid)
        if st is not None and st.status in ("output_committed", "scored"):
            skipped += 1
            continue
        store.ensure(u)
        if st is not None and st.status == "running":
            store.transition(uid, "retryable")
        seed = int(hashlib.sha256(u.case_id.encode()).hexdigest(), 16) % (2**31)
        store.transition(uid, "running", seed=seed)
        try:
            ctx = build_arm_context(ARM_D, u.case_id, store_map[u.case_id])
            validate_arm_context(ctx)
            run_unit(u, rows_by_id[u.case_id], dict(ctx.sections),
                     probe_results, hidden_meta, store, raw_root)
            executed += 1
            if executed % 10 == 0:
                print(f"  progress: {executed}/{total} executed, {skipped} skipped")
        except Exception as e:
            failed += 1
            store.transition(uid, "failed",
                             note=f"error: {type(e).__name__}: {str(e)[:100]}")
            print(f"  FAILED {uid} ({u.condition}/{u.case_id}): {e}")
        if args.limit and executed >= args.limit:
            break

    print(f"\ngoverned {exp_name}: executed={executed} skipped={skipped} failed={failed}")
    print("status counts:", store.status_counts())
    print("exp_dir:", exp_dir)


if __name__ == "__main__":
    main()

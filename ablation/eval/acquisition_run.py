#!/usr/bin/env python3
"""H-DIAGNOSTIC-ACQUISITION experiment driver (540 units: 3 conditions × 60 × 3).

Three controller conditions over frozen Arm D retrieval:
  D0 — direct attribution (single-shot)
  D1 — passive diagnostic (single-shot with decomposition)
  D2 — active diagnostic acquisition (multi-round with probe execution)

Uses the Z.ai GLM Coding Plan route (GLM-5.2). D2's probe observations come
from the precomputed acquisition-probes.jsonl (real executed probes, bound by
content hash — plan §5). The harness owns D2's state machine, budget, and
evidence isolation (plan §18). Raw output committed before scoring.
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
    EXPERIMENT_ID, SEED, CONDITIONS, SYSTEM_PROMPT,
    D0_SCAFFOLD, D1_SCAFFOLD, D2_SCAFFOLD_ROUND1, D2_SCAFFOLD_ROUNDN,
    D2_SCAFFOLD_BUDGET_EXHAUSTED, PROBES, PROBE_BY_ID, PROBE_IDS,
    MAX_ROUNDS, MAX_PAYMENT, action_catalogue_text, acquisition_contract_dict,
    contract_fingerprint, scaffold_hash,
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
    """Load precomputed probe observations for all 60 cases."""
    path = os.path.join(CORPUS, "acquisition-probes.jsonl")
    results = {}
    with open(path) as f:
        for line in f:
            d = json.loads(line)
            if "case_id" in d:
                results[d["case_id"]] = d
    return results


def format_observation(probe_id: str, probe_result: dict) -> str:
    """Format a probe's result as a bounded observation for the model."""
    p = probe_result.get("probes", {}).get(probe_id, {})
    if "error" in p:
        return f"[probe error: {p['error']}]"
    return json.dumps(p, sort_keys=True, default=str)[:2000]


def parse_round_response(text: str) -> dict | None:
    """Parse D2's structured JSON round response."""
    # Find JSON block in the response
    m = re.search(r'\{[^{}]*"candidate_loci"[^{}]*\}', text, re.DOTALL)
    if not m:
        # Try to find any JSON block
        m = re.search(r'\{[^{}]+\}', text, re.DOTALL)
    if not m:
        return None
    try:
        d = json.loads(m.group())
        return d
    except json.JSONDecodeError:
        return None


def extract_locus(text: str) -> str | None:
    """Extract Locus: <locus> from model output."""
    from eval.runner import extract_locus as _el
    return _el(text)


def freeze_contract(exp_dir, probe_results):
    """Freeze the experiment manifest."""
    manifest = {
        "experiment_id": EXPERIMENT_ID,
        "kind": "diagnostic-acquisition-H-DIAGNOSTIC-ACQUISITION",
        "derived_from": "Slice 7 §23.2 (H-ATTRIBUTION partially supported)",
        "parent_report": ".agents/reports/7-sxr-retrieval-ablation.md",
        "retrieval_substrate": "Arm D (SQL+FTS+graph+vector) frozen from §23.1",
        "conditions": list(CONDITIONS),
        "reps": REPS,
        "model": {"model_id": "glm-5.2", "provider": "zai-coding-plan",
                  "endpoint": "https://api.z.ai/api/coding/paas/v4",
                  "temperature": 0.0, "context_budget_tokens": 32_000,
                  "max_tokens": 4096},
        "contract_fingerprint": contract_fingerprint(),
        "acquisition_budget": {"max_rounds": MAX_ROUNDS,
                               "max_payment": MAX_PAYMENT},
        "probe_catalogue_hash": hashlib.sha256(
            json.dumps(PROBES, sort_keys=True).encode()).hexdigest(),
        "hypothesis": "H-DIAGNOSTIC-ACQUISITION: D2 active acquisition recovers "
                      "A_c while preserving A_u, relative to D1 and D0",
    }
    case_path = os.path.join(CORPUS, "cases.jsonl")
    acq_keys_path = os.path.join(CORPUS, "acquisition-keys.json")
    acq_meta_path = os.path.join(CORPUS, "acquisition-hidden-meta.json")
    reclass_path = os.path.join(CORPUS, "acquisition-reclassifications.json")
    manifest["hashes"] = {
        "corpus_cases": hashlib.sha256(open(case_path, "rb").read()).hexdigest(),
        "sealed_keys": open(os.path.join(CORPUS, "keys.sha256")).read().strip().split()[0],
        "acquisition_keys": hashlib.sha256(open(acq_keys_path, "rb").read()).hexdigest(),
        "acquisition_hidden_meta": hashlib.sha256(open(acq_meta_path, "rb").read()).hexdigest(),
        "acquisition_reclassifications": hashlib.sha256(open(reclass_path, "rb").read()).hexdigest(),
        "acquisition_probes": hashlib.sha256(
            open(os.path.join(CORPUS, "acquisition-probes.jsonl"), "rb").read()).hexdigest(),
        "sxr_git_head": subprocess.run(
            ["git", "-C", SXR_ROOT, "rev-parse", "HEAD"],
            capture_output=True, text=True).stdout.strip(),
    }
    blob = json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
    manifest["contract_hash"] = hashlib.sha256(blob).hexdigest()
    os.makedirs(exp_dir, exist_ok=True)
    with open(os.path.join(exp_dir, "manifest.json"), "w") as f:
        f.write(json.dumps(manifest, indent=2) + "\n")
    return manifest


def run_d0_d1(unit, case_row, ctx_sections, store, raw_root):
    """Execute a D0 or D1 unit (single-shot)."""
    uid = unit.unit_id
    cond = unit.condition
    scaffold = D0_SCAFFOLD if cond == "D0" else D1_SCAFFOLD
    contract = acquisition_contract_dict(cond)
    budget = int(contract["context_budget_tokens"])

    pc = pack_context(dict(ctx_sections), SYSTEM_PROMPT,
                      unit.case_id, ARM_D, budget)
    ctx_text = "\n\n".join("[%s]\n%s" % (c, pc.sections[c])
                           for c in sorted(pc.sections))
    prompt = scaffold.format(case_id=unit.case_id, context=ctx_text)
    if estimate_tokens(prompt) > budget:
        raise RuntimeError(f"prompt exceeds budget for {uid}")

    t0 = time.monotonic()
    mc = call_coding_plan(model="glm-5.2", system_prompt=SYSTEM_PROMPT,
                          user_prompt=prompt, temperature=0.0, max_tokens=4096)
    elapsed = (time.monotonic() - t0) * 1000

    raw_path = os.path.join(raw_root, f"{uid}.txt")
    with open(raw_path, "w") as f:
        f.write(mc.text)
    usage = {
        "total_tokens": int(mc.usage.get("total_tokens") or 0),
        "prompt_tokens": int(mc.usage.get("prompt_tokens") or 0),
        "completion_tokens": int(mc.usage.get("completion_tokens") or 0),
        "reasoning_tokens": int(mc.usage.get("reasoning_tokens") or 0),
        "cost": mc.cost, "latency_ms": mc.latency_ms,
        "retries": mc.retries, "resolved_model": mc.model,
        "condition": cond, "rounds": 1,
    }
    with open(raw_path.replace(".txt", ".usage.json"), "w") as f:
        json.dump(usage, f, sort_keys=True)
    h = hashlib.sha256(mc.text.encode()).hexdigest()
    store.transition(uid, "output_committed", raw_output_path=raw_path,
                     raw_output_hash=h,
                     note=f"acq:{cond};tokens={usage['total_tokens']}")
    return mc.text


def run_d2(unit, case_row, ctx_sections, probe_results, store, raw_root):
    """Execute a D2 unit (multi-round active acquisition)."""
    uid = unit.unit_id
    cid = unit.case_id
    contract = acquisition_contract_dict("D2")
    budget = int(contract["context_budget_tokens"])
    case_probes = probe_results.get(cid, {})

    # Build round 1 prompt
    pc = pack_context(dict(ctx_sections), SYSTEM_PROMPT, cid, ARM_D, budget)
    ctx_text = "\n\n".join("[%s]\n%s" % (c, pc.sections[c])
                           for c in sorted(pc.sections))
    catalogue = action_catalogue_text()
    prompt = D2_SCAFFOLD_ROUND1.format(
        case_id=cid, context=ctx_text, action_catalogue=catalogue,
        max_rounds=MAX_ROUNDS, max_payment=MAX_PAYMENT)

    all_rounds = []
    total_tokens = 0
    total_reasoning = 0
    payment_used = 0
    actions_taken = []
    observations = []
    final_locus = None

    for round_num in range(1, MAX_ROUNDS + 2):  # +1 for budget_exhausted
        if estimate_tokens(prompt) > budget:
            # Truncate context if needed
            prompt = prompt[:budget * 3]

        t0 = time.monotonic()
        try:
            mc = call_coding_plan(model="glm-5.2", system_prompt=SYSTEM_PROMPT,
                                  user_prompt=prompt, temperature=0.0,
                                  max_tokens=4096)
        except ModelClientError as e:
            raise RuntimeError(f"D2 call failed for {uid} round {round_num}: {e}")
        elapsed = (time.monotonic() - t0) * 1000
        total_tokens += int(mc.usage.get("total_tokens") or 0)
        total_reasoning += int(mc.usage.get("reasoning_tokens") or 0)

        response = mc.text
        round_data = {"round": round_num, "prompt_tokens": int(mc.usage.get("prompt_tokens") or 0),
                      "completion_tokens": int(mc.usage.get("completion_tokens") or 0),
                      "latency_ms": mc.latency_ms}
        all_rounds.append(round_data)

        # Check for settle
        locus = extract_locus(response)
        if locus:
            final_locus = locus
            all_rounds[-1]["settled"] = True
            break

        # Parse JSON response for action selection
        parsed = parse_round_response(response)
        if not parsed:
            # Can't parse — try to extract locus directly
            if locus:
                final_locus = locus
                break
            # Force final answer on next round
            all_rounds[-1]["parse_failed"] = True
            if round_num >= MAX_ROUNDS:
                final_locus = "unknown"
                break
            prompt = D2_SCAFFOLD_BUDGET_EXHAUSTED.format(
                payment_used=payment_used, max_payment=MAX_PAYMENT,
                round_num=round_num, max_rounds=MAX_ROUNDS,
                observations_summary="\n".join(observations[-3:]) or "(none)")
            continue

        settle = parsed.get("settle")
        if settle and isinstance(settle, dict) and settle.get("locus"):
            final_locus = settle["locus"]
            if final_locus not in ("verifier", "verification_plan", "projection",
                                   "implementation", "environment", "authority",
                                   "evidence_collection", "declared_model", "unknown"):
                final_locus = "unknown"
            all_rounds[-1]["settled"] = True
            break

        selected = parsed.get("selected_action")
        if not selected or selected not in PROBE_BY_ID:
            all_rounds[-1]["no_valid_action"] = True
            if round_num >= MAX_ROUNDS:
                final_locus = "unknown"
                break
            prompt = D2_SCAFFOLD_BUDGET_EXHAUSTED.format(
                payment_used=payment_used, max_payment=MAX_PAYMENT,
                round_num=round_num, max_rounds=MAX_ROUNDS,
                observations_summary="\n".join(observations[-3:]) or "(none)")
            continue

        # Check budget
        action_cost = PROBE_BY_ID[selected]["cost"]
        if payment_used + action_cost > MAX_PAYMENT:
            all_rounds[-1]["budget_blocked"] = selected
            prompt = D2_SCAFFOLD_BUDGET_EXHAUSTED.format(
                payment_used=payment_used, max_payment=MAX_PAYMENT,
                round_num=round_num, max_rounds=MAX_ROUNDS,
                observations_summary="\n".join(observations[-3:]) or "(none)")
            continue

        payment_used += action_cost
        actions_taken.append(selected)
        obs = format_observation(selected, case_probes)
        observations.append(f"[{selected}]: {obs}")
        all_rounds[-1]["action"] = selected
        all_rounds[-1]["cost"] = action_cost

        # Check if budget exhausted
        if round_num >= MAX_ROUNDS or payment_used >= MAX_PAYMENT:
            prompt = D2_SCAFFOLD_BUDGET_EXHAUSTED.format(
                payment_used=payment_used, max_payment=MAX_PAYMENT,
                round_num=round_num, max_rounds=MAX_ROUNDS,
                observations_summary="\n".join(observations[-3:]))
        else:
            prompt = D2_SCAFFOLD_ROUNDN.format(
                round_num=round_num + 1, max_rounds=MAX_ROUNDS,
                payment_used=payment_used, max_payment=MAX_PAYMENT,
                prev_candidates=", ".join(parsed.get("candidate_loci", [])),
                prev_unresolved=parsed.get("unresolved_distinction", ""),
                prev_action=selected,
                observation=obs,
                action_catalogue=catalogue)

    if final_locus is None:
        final_locus = "unknown"

    # Compose raw output: all rounds + final locus
    raw_output = f"Locus: {final_locus}\n"
    raw_output += f"Actions: {', '.join(actions_taken)}\n"
    raw_output += f"Payment: {payment_used}/{MAX_PAYMENT}\n"
    raw_output += f"Rounds: {len(all_rounds)}\n"
    for i, obs in enumerate(observations):
        raw_output += f"Observation[{i}]: {obs}\n"

    raw_path = os.path.join(raw_root, f"{uid}.txt")
    with open(raw_path, "w") as f:
        f.write(raw_output)
    usage = {
        "total_tokens": total_tokens, "prompt_tokens": 0,
        "completion_tokens": 0, "reasoning_tokens": total_reasoning,
        "cost": 0.0, "latency_ms": sum(r.get("latency_ms", 0) for r in all_rounds),
        "retries": 0, "resolved_model": mc.model,
        "condition": "D2", "rounds": len(all_rounds),
        "actions_taken": actions_taken, "payment_used": payment_used,
    }
    with open(raw_path.replace(".txt", ".usage.json"), "w") as f:
        json.dump(usage, f, sort_keys=True)
    # Persist full round detail as developmental evidence
    with open(raw_path.replace(".txt", ".rounds.json"), "w") as f:
        json.dump({"rounds": all_rounds, "actions": actions_taken,
                   "observations": observations, "final_locus": final_locus,
                   "payment_used": payment_used}, f, sort_keys=True, indent=2)
    h = hashlib.sha256(raw_output.encode()).hexdigest()
    store.transition(uid, "output_committed", raw_output_path=raw_path,
                     raw_output_hash=h,
                     note=f"acq:D2;tokens={total_tokens};actions={len(actions_taken)}")
    return raw_output


def main():
    import subprocess
    ap = argparse.ArgumentParser()
    ap.add_argument("--seed", type=int, default=SEED)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--pilot", action="store_true")
    args = ap.parse_args()

    rows = load_case_rows(os.path.join(CORPUS, "cases.jsonl"))
    ids = [r["case_id"] for r in rows]
    assert len(ids) == 60

    namespace = "acq-pilot" if args.pilot else "acq-full"
    exp_name = f"{EXPERIMENT_ID}-pilot" if args.pilot else EXPERIMENT_ID
    exp_dir = experiment_dir("runs", namespace, exp_name)
    os.makedirs(exp_dir, exist_ok=True)
    store = CheckpointStore(os.path.join(exp_dir, "manifest.jsonl"))
    raw_root = os.path.join(exp_dir, "raw")
    os.makedirs(raw_root, exist_ok=True)

    probe_results = load_probe_results()
    env = dict(os.environ)

    # Release gate (owner-authorized)
    verify_release(EXPERIMENT_ID, env)

    if not os.path.exists(os.path.join(exp_dir, "manifest.json")):
        freeze_contract(exp_dir, probe_results)
        print("contract frozen")
    else:
        print("contract already frozen")

    fp = contract_fingerprint()

    # Build matrix
    pilot_cases = None
    if args.pilot:
        # 5 cases: ag-01(verifier), ag-06(implementation), cep-09(environment),
        # cep-11(authority), cep-15(unknown) — deterministic from public metadata
        pilot_cases = {"ag-01", "ag-06", "cep-09", "cep-11", "cep-15"}

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
    by_unit = {u.unit_id: u for u in units}
    with open(os.path.join(exp_dir, "matrix.json"), "w") as f:
        json.dump({"seed": args.seed, "fingerprint": fp, "order": order,
                   "units": [{"unit_id": u.unit_id, "case_id": u.case_id,
                              "condition": u.condition, "rep": u.rep}
                             for u in units]}, f, indent=1)

    # Load retrieval context
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
            if u.condition in ("D0", "D1"):
                run_d0_d1(u, rows_by_id[u.case_id], dict(ctx.sections),
                          store, raw_root)
            else:
                run_d2(u, rows_by_id[u.case_id], dict(ctx.sections),
                       probe_results, store, raw_root)
            executed += 1
            if executed % 10 == 0:
                print(f"  progress: {executed}/{total} executed, {skipped} skipped")
        except Exception as e:
            failed += 1
            store.transition(uid, "failed", note=f"error: {type(e).__name__}: {str(e)[:100]}")
            print(f"  FAILED {uid} ({u.condition}/{u.case_id}): {e}")
        if args.limit and executed >= args.limit:
            break

    print(f"\nacquisition {exp_name}: executed={executed} skipped={skipped} failed={failed}")
    print("status counts:", store.status_counts())
    print("exp_dir:", exp_dir)


if __name__ == "__main__":
    main()

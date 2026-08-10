#!/usr/bin/env python3
"""§23.2 attribution-loop ablation driver (360 units: 2 methods x 60 x 3).

Two attribution methods (C-loop, D-direct) over the FROZEN Arm D retrieval
substrate, 60 cases x 3 reps = 180 per method = 360 total, under the live
backend. Freezes the §23.2 contract first, then executes in the frozen seeded
randomized order, committing raw outputs before scoring with durable usage
sidecars and crash-resume.

Namespace attribution-001; release marker authorizes only this experiment id.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

from eval.arms import build_arm_context, validate_arm_context  # noqa: E402
from eval.contract import live_contract  # noqa: E402
from eval.state import CheckpointStore  # noqa: E402
from eval.store import build_store, load_case_rows  # noqa: E402
from eval.report import experiment_dir  # noqa: E402
from eval.attribution import (attribution_prompt, attribution_contract_dict,
                              METHODS)  # noqa: E402
from eval.context_packer import estimate_tokens  # noqa: E402
from eval.bundle import assert_prompt_sanitized  # noqa: E402
from eval.release import verify_release  # noqa: E402
from eval.zai_client import call_coding_plan, ModelClientError  # noqa: E402

CORPUS = os.path.join(HERE, "..", "corpus")
SXR_ROOT = os.path.join(HERE, "..", "..")
ATTR_EXPERIMENT = "attribution-002"
ATTR_NAMESPACE = "attribution-002"
SUPERSEDED_EXPERIMENT = "attribution-001"
SUPERSEDED_REASON = ("Clinepass monthly quota exhausted before first scored "
                     "unit (0/360 committed); provider-route substitution to "
                     "Z.ai GLM Coding Plan per owner directive 2026-08-10. "
                     "Only provider/access route changed; all experimental "
                     "semantics frozen (GLM-5.2, Arm D, C-loop/D-direct, 32K "
                     "pack-v1, prompts, scorer, unknown semantics, FP def).")
ARM_D = "D"
REPS = 3


class _Unit:
    """Minimal unit object with a unit_id for CheckpointStore.ensure."""
    def __init__(self, unit_id):
        self.unit_id = unit_id


def load_artifacts(rows):
    out = {}
    import subprocess
    store_root = os.path.expanduser("~/.local/share/sxr/repos")
    for r in rows:
        res = subprocess.run(
            ["/home/sprime01/projects/sxr/target/debug/sxr", "ledger",
             "export", "--repo", r["repo_id"]], capture_output=True, text=True)
        try:
            d = json.loads(res.stdout)
        except json.JSONDecodeError:
            continue
        recs = d if isinstance(d, list) else d.get("records", [])
        for rec in recs:
            if rec.get("record_kind") != "observation.recorded":
                continue
            p = rec["payload"] if isinstance(rec["payload"], dict) else json.loads(rec["payload"])
            for rep in p.get("representations", []):
                if rep.get("representation_id") == f"rep:obs:{r['observation_id']}":
                    h = rep.get("content_hash", "").replace("sha256:", "")
                    ap = os.path.join(store_root, r["repo_id"], "artifacts", f"{h}.stdout")
                    if os.path.exists(ap):
                        out[r["case_id"]] = open(ap, errors="replace").read()
                    break
            break
    return out


def unit_id(exp, case, method, rep, config_hash):
    import hashlib
    payload = "|".join((exp, case, method, str(rep), config_hash))
    return hashlib.sha256(payload.encode()).hexdigest()[:16]


def freeze_contract(exp_dir, cfg):
    from eval.attribution import method_scaffold_hash
    from eval.scorer import SCORING_VERSION
    from eval.context_packer import PACKING_VERSION
    import subprocess
    manifest = {
        "experiment_id": ATTR_EXPERIMENT,
        "namespace": ATTR_NAMESPACE,
        "kind": "attribution-loop-ablation-H-ATTRIBUTION",
        "retrieval_substrate": "Arm D (SQL+FTS+graph+vector) frozen from §23.1",
        "arm_D_frozen_reason": "owner adjudication 2026-08-09: diagnostic "
                               "decision success (see owner_adjudication_23-1.json)",
        "supersedes": SUPERSEDED_EXPERIMENT,
        "supersession": {
            "superseded_experiment": SUPERSEDED_EXPERIMENT,
            "superseded_provider": "clinepass",
            "superseded_status": "superseded_before_execution",
            "committed_units_under_superseded": 0,
            "reason": SUPERSEDED_REASON,
        },
        "provider_route": {
            "provider": "zai-coding-plan",
            "endpoint": "https://api.z.ai/api/coding/paas/v4/chat/completions",
            "route_kind": "Z.ai GLM Coding Plan (dedicated endpoint, not "
                          "pay-as-you-go)",
            "auth": "Zhipu signed-JWT HS256 (sign_type=SIGN)",
        },
        "methods": list(METHODS),
        "reps": REPS,
        "model_identity_tag": cfg.model_identity_tag(),
        "model": {"model_id": cfg.model_id, "provider": cfg.provider,
                  "temperature": cfg.temperature,
                  "context_budget_tokens": cfg.context_budget_tokens,
                  "max_tokens": 4096},
        "scaffold_hashes": {m: method_scaffold_hash(m) for m in METHODS},
        "primary_measure": "semantic-model-change false-positive rate",
        "secondary_measures": ["unknown termination rate (honest non-answer = "
                               "success)", "time/tokens to a supported locus"],
        "h_attribution": "C-loop (diagnostic loop) has materially lower FP "
                         "than D-direct (direct classification) at acceptable "
                         "extra cost",
        "scorer_version": SCORING_VERSION,
        "packer_version": PACKING_VERSION,
        "interpretation_caveat": "§23.1 used Clinepass (finalProvider "
                                 "fireworks); §23.2 uses Z.ai Coding Plan. "
                                 "Cross-experiment absolute rates are NOT "
                                 "directly comparable; only the internal "
                                 "C-loop vs D-direct comparison is causal.",
    }
    case_path = os.path.join(CORPUS, "cases.jsonl")
    key_manifest = open(os.path.join(CORPUS, "keys.sha256")).read().strip().split()[0]
    import hashlib
    manifest["hashes"] = {
        "corpus_cases": hashlib.sha256(open(case_path, "rb").read()).hexdigest(),
        "sealed_key_manifest": key_manifest,
        "sxr_git_head": subprocess.run(
            ["git", "-C", SXR_ROOT, "rev-parse", "HEAD"],
            capture_output=True, text=True).stdout.strip(),
    }
    import json as j
    blob = j.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
    manifest["contract_hash"] = hashlib.sha256(blob).hexdigest()
    os.makedirs(exp_dir, exist_ok=True)
    with open(os.path.join(exp_dir, "manifest.json"), "w") as f:
        f.write(j.dumps(manifest, indent=2) + "\n")
    return manifest


def run_one(unit, method, case_row, ctx_sections, ctx_arm, cfg, store,
            raw_root, bundle_root, env):
    """Execute one §23.2 unit and commit its raw output."""
    uid = unit
    state = store.get(uid)
    if state is None:
        store.ensure(_Unit(uid))
    state = store.get(uid)
    raw_path = os.path.join(raw_root, f"{uid}.txt")
    # idempotency
    if state.status in ("output_committed", "scored") and state.raw_output_hash \
            and os.path.exists(raw_path):
        import hashlib
        if hashlib.sha256(open(raw_path).read().encode()).hexdigest() == state.raw_output_hash:
            return
    if state.status == "running":
        store.transition(uid, "retryable")
    import hashlib as _h
    seed = int(_h.sha256(case_row["case_id"].encode()).hexdigest(), 16) % (2**31)
    store.transition(uid, "running", seed=seed)

    contract = attribution_contract_dict(method)
    verify_release(ATTR_EXPERIMENT, env)
    # Pack the Arm D context into the frozen 32K budget (arm-independent).
    from eval.context_packer import pack_context
    from eval.attribution import METHOD_SCAFFOLDS
    from eval.context_packer import estimate_tokens
    budget = int(contract["context_budget_tokens"])
    # Use the generic packer to bound the context; then build the method prompt.
    pc = pack_context(dict(ctx_sections), contract["system_prompt"],
                      case_row["case_id"], "D", budget)
    ctx_text = "\n\n".join("[%s]\n%s" % (c, pc.sections[c])
                           for c in sorted(pc.sections))
    prompt = attribution_prompt(method, case_row["case_id"], ctx_text)
    assert_prompt_sanitized(prompt, case_row["case_id"])
    if estimate_tokens(prompt) > budget:
        raise RuntimeError(f"prompt exceeds budget for {uid}")
    try:
        mc = call_coding_plan(model=contract["model_id"],
                              system_prompt=contract["system_prompt"],
                              user_prompt=prompt, temperature=0.0,
                              max_tokens=4096)
    except ModelClientError as e:
        raise RuntimeError(f"live call failed: {e}") from e
    if int(mc.usage.get("prompt_tokens") or 0) > budget:
        raise RuntimeError(f"provider prompt {mc.usage['prompt_tokens']} > budget")
    os.makedirs(raw_root, exist_ok=True)
    with open(raw_path, "w") as f:
        f.write(mc.text)
    usage = {"total_tokens": int(mc.usage.get("total_tokens") or 0),
             "prompt_tokens": int(mc.usage.get("prompt_tokens") or 0),
             "completion_tokens": int(mc.usage.get("completion_tokens") or 0),
             "reasoning_tokens": int(mc.usage.get("reasoning_tokens") or 0),
             "cost": mc.cost, "latency_ms": mc.latency_ms,
             "retries": mc.retries, "generation_id": mc.generation_id,
             "resolved_model": mc.model, "method": method,
             "provider_route": "zai-coding-plan"}
    with open(raw_path.replace(".txt", ".usage.json"), "w") as f:
        f.write(json.dumps(usage, sort_keys=True))
    import hashlib
    h = hashlib.sha256(mc.text.encode()).hexdigest()
    store.transition(uid, "output_committed", raw_output_path=raw_path,
                     raw_output_hash=h,
                     note=f"attr:{method};tokens={usage['total_tokens']}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seed", type=int, default=20260809)
    ap.add_argument("--limit", type=int, default=0)
    args = ap.parse_args()

    rows = load_case_rows(os.path.join(CORPUS, "cases.jsonl"))
    ids = [r["case_id"] for r in rows]
    assert len(ids) == 60
    cfg = live_contract()
    config_hash = cfg.model_identity_tag()

    exp_dir = experiment_dir("runs", ATTR_NAMESPACE, ATTR_EXPERIMENT)
    os.makedirs(exp_dir, exist_ok=True)
    store = CheckpointStore(os.path.join(exp_dir, "manifest.jsonl"))
    raw_root = os.path.join(exp_dir, "raw")
    bundle_root = os.path.join(exp_dir, "bundles")
    os.makedirs(raw_root, exist_ok=True)
    os.makedirs(bundle_root, exist_ok=True)

    if not os.path.exists(os.path.join(exp_dir, "manifest.json")):
        freeze_contract(exp_dir, cfg)
        print("contract frozen")
    else:
        print("contract already frozen")

    # build matrix: 2 methods x 60 cases x 3 reps, seeded order
    units = []
    for method in METHODS:
        for case in sorted(ids):
            for rep in range(1, REPS + 1):
                units.append({"unit_id": unit_id(ATTR_EXPERIMENT, case, method,
                                                  rep, config_hash),
                              "case_id": case, "method": method, "rep": rep})
    import random
    rng = random.Random(args.seed)
    order = [u["unit_id"] for u in units]
    rng.shuffle(order)
    if args.limit:
        order = order[:args.limit]
    by_unit = {u["unit_id"]: u for u in units}
    with open(os.path.join(exp_dir, "matrix.json"), "w") as f:
        json.dump({"seed": args.seed, "config_hash": config_hash,
                   "order": order, "units": units}, f, indent=1)

    artifacts = load_artifacts(rows)
    store_map = build_store(rows, artifacts)
    rows_by_id = {r["case_id"]: r for r in rows}
    env = dict(os.environ)

    executed = skipped = 0
    for uid in order:
        u = by_unit[uid]
        st0 = store.get(uid)
        if st0 is not None and st0.status in ("output_committed", "scored"):
            skipped += 1
            continue
        # Arm D context (frozen substrate)
        ctx = build_arm_context(ARM_D, u["case_id"], store_map[u["case_id"]])
        validate_arm_context(ctx)
        run_one(uid, u["method"], rows_by_id[u["case_id"]], dict(ctx.sections),
                ARM_D, cfg, store, raw_root, bundle_root, env)
        executed += 1
        if args.limit and executed >= args.limit:
            break
    print(f"attribution {ATTR_EXPERIMENT}: executed={executed} skipped={skipped}")
    print("status counts:", store.status_counts())
    print("exp_dir:", exp_dir)


if __name__ == "__main__":
    main()
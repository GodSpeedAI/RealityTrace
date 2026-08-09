#!/usr/bin/env python3
"""M3 pre-flight: prove isolation from the ACTUAL live evaluator environment,
then make exactly ONE live model call to validate the pipeline before the
full 60-unit run.

Mirrors run_evaluator's live path step-for-step: build bundle, scrub env,
assert bundle sanitized, probe key paths, compose prompt, assert prompt
sanitized, then call the model once. Exits non-zero on any isolation failure
BEFORE spending a call.
"""

from __future__ import annotations

import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

from eval.contract import live_contract  # noqa: E402
from eval.bundle import (build_bundle, scrub_env, probe_key_paths,  # noqa: E402
                         assert_bundle_sanitized, assert_prompt_sanitized)
from eval.matrix import RunMatrix  # noqa: E402
from eval.store import build_store, load_case_rows  # noqa: E402
from eval.arms import build_arm_context  # noqa: E402
from eval.runner import extract_locus  # noqa: E402
from eval.context_packer import pack_context  # noqa: E402

CORPUS = os.path.join(HERE, "..", "corpus")


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


def main():
    rows = load_case_rows(os.path.join(CORPUS, "cases.jsonl"))
    selected = ["ag-01", "cep-01", "df-01", "sr-10", "sr-13"]
    rows = [r for r in rows if r["case_id"] in selected]
    artifacts = load_artifacts(rows)
    store_map = build_store(rows, artifacts)
    cfg = live_contract()

    # Pick the first unit of the frozen 60-unit matrix (case, arm, rep).
    matrix = RunMatrix.expand("pilot-001", selected, 20260809, cfg.fingerprint())
    unit = matrix.units[0]
    ctx = build_arm_context(unit.arm, unit.case_id, store_map[unit.case_id])
    case_row = next(r for r in rows if r["case_id"] == unit.case_id)

    bundle = build_bundle("/tmp/opencode/preflight-bundles", unit.unit_id,
                          case_row, ctx.to_dict(), cfg.to_dict())
    scrubbed = scrub_env(dict(os.environ), bundle.path)
    assert_bundle_sanitized(bundle, scrubbed)
    resolved = probe_key_paths(scrubbed, bundle.path)
    print(f"[isolation] probe resolved: {resolved or 'nothing'}")
    if resolved:
        sys.exit("ISOLATION FAIL: live evaluator env can reach key paths")

    # Budget: prove ALL 20 (5 cases x 4 arms) fit the frozen 32K budget.
    budget = int(cfg.context_budget_tokens)
    all_fit = True
    for cid in selected:
        for arm in "ABCD":
            c = build_arm_context(arm, cid, store_map[cid])
            pc = pack_context(c.sections, cfg.system_prompt, cid, arm, budget)
            ok = pc.estimated_prompt_tokens <= budget
            all_fit = all_fit and ok
            if not ok:
                print(f"[budget] FAIL {cid} {arm}: {pc.estimated_prompt_tokens}> {budget}")
    print(f"[budget] all 20 combos fit 32K: {all_fit}")
    if not all_fit:
        sys.exit("BUDGET FAIL: some case/arm exceeds frozen 32K")

    pc = pack_context(ctx.sections, cfg.system_prompt, unit.case_id, unit.arm,
                      budget)
    prompt = pc.prompt
    assert_prompt_sanitized(prompt, unit.case_id)
    print(f"[isolation] prompt sanitized (est {pc.estimated_prompt_tokens} tok)")
    print(f"[unit] {unit.unit_id[:12]} case={unit.case_id} arm={unit.arm} rep={unit.repetition}")

    # ONE compliant live call
    from eval.model_client import call_model
    from eval.release import verify_release
    marker = os.environ.get("SXR_ABLATION_RELEASE_MARKER")
    verify_release(unit.experiment_id, dict(os.environ))
    model = "%s/%s" % (cfg.provider, cfg.model_id)
    mc = call_model(model=model, system_prompt=cfg.system_prompt,
                    user_prompt=prompt, temperature=cfg.temperature)
    print(f"[live] resolved_model={mc.model}")
    print(f"[live] tokens={mc.usage.get('total_tokens')} "
          f"prompt={mc.usage.get('prompt_tokens')} cost=${mc.cost:.6f} "
          f"latency={mc.latency_ms}ms retries={mc.retries}")
    if int(mc.usage.get("prompt_tokens") or 0) > budget:
        sys.exit(f"PROVIDER BUDGET VIOLATION: prompt {mc.usage['prompt_tokens']} > {budget}")
    print(f"[live] extracted_locus={extract_locus(mc.text)!r}")
    print("[preflight] ALL ISOLATION + BUDGET CHECKS PASS + one compliant live call")


if __name__ == "__main__":
    main()
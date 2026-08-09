#!/usr/bin/env python3
"""M3 pilot driver — the 5-case x 4-arm x 3-rep = 60 live model-backed runs.

Freezes the experiment contract and selection record FIRST (before any live
call), then executes the 60 units in the frozen seeded randomized order under
the live backend, durably committing each raw output before any scoring, with
per-unit token/cost/latency accounting and crash-resume.

The pilot uses a distinct immutable namespace (pilot-003) so its results are
quarantined from the full experiment. The FULL experiment is NOT launched: the
release marker authorizes only this pilot experiment id.

Usage:
    SXR_ABLATION_RELEASE=<token> SXR_ABLATION_RELEASE_MARKER=<path> \\
        uv run --python 3.12 -m eval.pilot_run [--limit N]
"""

from __future__ import annotations

import argparse
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

from eval.arms import build_arm_context, validate_arm_context  # noqa: E402
from eval.matrix import RunMatrix  # noqa: E402
from eval.contract import live_contract  # noqa: E402
from eval.state import CheckpointStore  # noqa: E402
from eval.runner import run_evaluator  # noqa: E402
from eval.store import build_store, load_case_rows  # noqa: E402
from eval.report import experiment_dir  # noqa: E402
from eval.select_pilot import select_pilot  # noqa: E402
from eval.pilot_manifest import build_pilot_manifest, write_manifest  # noqa: E402

CORPUS = os.path.join(HERE, "..", "corpus")
SXR_ROOT = os.path.join(HERE, "..", "..")
PILOT_EXPERIMENT = "pilot-003"
PILOT_NAMESPACE = "pilot-003"


def load_artifacts(rows):
    out = {}
    import subprocess
    store_root = os.path.expanduser("~/.local/share/sxr/repos")
    for r in rows:
        res = subprocess.run(
            ["/home/sprime01/projects/sxr/target/debug/sxr", "ledger",
             "export", "--repo", r["repo_id"]],
            capture_output=True, text=True)
        try:
            d = json.loads(res.stdout)
        except json.JSONDecodeError:
            continue
        recs = d if isinstance(d, list) else d.get("records", [])
        for rec in recs:
            if rec.get("record_kind") != "observation.recorded":
                continue
            p = rec["payload"] if isinstance(rec["payload"], dict) \
                else json.loads(rec["payload"])
            for rep in p.get("representations", []):
                if rep.get("representation_id") == f"rep:obs:{r['observation_id']}":
                    h = rep.get("content_hash", "").replace("sha256:", "")
                    ap = os.path.join(store_root, r["repo_id"], "artifacts",
                                      f"{h}.stdout")
                    if os.path.exists(ap):
                        out[r["case_id"]] = open(ap, errors="replace").read()
                    break
            break
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seed", type=int, default=20260809)
    ap.add_argument("--limit", type=int, default=0, help="0 = full 60-unit pilot")
    args = ap.parse_args()

    all_rows = load_case_rows(os.path.join(CORPUS, "cases.jsonl"))
    sel = select_pilot(all_rows)
    selected = sel.selected
    rows = [r for r in all_rows if r["case_id"] in selected]
    assert len(rows) == 5 == len(selected)

    cfg = live_contract()
    matrix = RunMatrix.expand(PILOT_EXPERIMENT, selected, args.seed,
                              cfg.fingerprint())
    assert len(matrix.units) == 5 * 4 * 3 == 60
    order = matrix.order[:args.limit] if args.limit else matrix.order
    by_id = {u.unit_id: u for u in matrix.units}

    exp_dir = experiment_dir("runs", PILOT_NAMESPACE, PILOT_EXPERIMENT)
    os.makedirs(exp_dir, exist_ok=True)
    store = CheckpointStore(os.path.join(exp_dir, "manifest.jsonl"))
    raw_root = os.path.join(exp_dir, "raw")
    bundle_root = os.path.join(exp_dir, "bundles")
    os.makedirs(raw_root, exist_ok=True)
    os.makedirs(bundle_root, exist_ok=True)

    # FREEZE the contract + selection BEFORE any live call (idempotent).
    manifest_path = os.path.join(exp_dir, "manifest.json")
    if not os.path.exists(manifest_path):
        m = build_pilot_manifest(cfg, sel.to_dict(), SXR_ROOT)
        write_manifest(manifest_path, m)
        print("contract frozen:", hash_manifest := open(manifest_path).read()
              and json.load(open(manifest_path))["contract_hash"])
    else:
        print("manifest already frozen:",
              json.load(open(manifest_path))["contract_hash"])

    sel_path = os.path.join(exp_dir, "selection.json")
    if not os.path.exists(sel_path):
        from eval.select_pilot import write_selection_record
        write_selection_record(sel_path, sel)
        print("selection record written")

    # matrix record (immutable per experiment)
    matrix_path = os.path.join(exp_dir, "matrix.json")
    if not os.path.exists(matrix_path):
        with open(matrix_path, "w") as f:
            json.dump(matrix.to_dict(), f, indent=1)

    artifacts = load_artifacts(rows)
    store_map = build_store(rows, artifacts)
    rows_by_id = {r["case_id"]: r for r in rows}

    executed = 0
    skipped = 0
    for uid in order:
        u = by_id[uid]
        st0 = store.get(uid)
        if st0 is not None and st0.status in ("output_committed", "scored"):
            skipped += 1
            continue
        # A unit left in "running" by a crash mid-call must be rolled back to
        # retryable before re-running (running -> running is invalid).
        if st0 is not None and st0.status == "running":
            store.transition(uid, "retryable")
        ctx = build_arm_context(u.arm, u.case_id, store_map[u.case_id])
        validate_arm_context(ctx)
        run_evaluator(u, bundle_root, rows_by_id[u.case_id], ctx.to_dict(),
                      cfg.to_dict(), u.seed, store, raw_root=raw_root)
        executed += 1
        if args.limit and executed >= args.limit:
            break
    print(f"pilot {PILOT_EXPERIMENT}: executed={executed} skipped={skipped}")
    print("status counts:", store.status_counts())
    print("exp_dir:", exp_dir)


if __name__ == "__main__":
    main()
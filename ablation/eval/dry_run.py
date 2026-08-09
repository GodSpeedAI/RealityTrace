#!/usr/bin/env python3
"""M2 dry-run batch driver — the operational entry point for evaluation runs.

Expands the real 60x4x3 matrix over the recorded corpus evidence, executes
units in the seeded randomized order using the SIMULATED evaluator (never a
model), durably commits raw outputs, and supports resume: a second invocation
with the same experiment identity skips already-committed/scored units.

Usage:
    uv run --python 3.12 -m eval.dry_run --namespace full-001 \
        --experiment exp-001 --seed 20260809 [--limit N]

Live model-backed execution is structurally impossible here (release.py).
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
from eval.contract import frozen_contract  # noqa: E402
from eval.state import CheckpointStore  # noqa: E402
from eval.runner import run_evaluator  # noqa: E402
from eval.store import build_store, load_case_rows  # noqa: E402
from eval.report import namespace_root, experiment_dir  # noqa: E402

CORPUS = os.path.join(HERE, "..", "corpus")


def load_artifacts(rows):
    out = {}
    store_root = os.path.expanduser("~/.local/share/sxr/repos")
    import subprocess
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
    ap.add_argument("--namespace", required=True)
    ap.add_argument("--experiment", required=True)
    ap.add_argument("--seed", type=int, default=20260809)
    ap.add_argument("--limit", type=int, default=0, help="0 = full matrix")
    args = ap.parse_args()

    rows = load_case_rows(os.path.join(CORPUS, "cases.jsonl"))
    ids = [r["case_id"] for r in rows]
    cfg = frozen_contract()
    matrix = RunMatrix.expand(args.experiment, ids, args.seed,
                              cfg.fingerprint())
    order = matrix.order[:args.limit] if args.limit else matrix.order
    by_id = {u.unit_id: u for u in matrix.units}

    exp_dir = experiment_dir("runs", args.namespace, args.experiment)
    store = CheckpointStore(os.path.join(exp_dir, "manifest.jsonl"))
    raw_root = os.path.join(exp_dir, "raw")
    bundle_root = os.path.join(exp_dir, "bundles")
    os.makedirs(raw_root, exist_ok=True)
    os.makedirs(bundle_root, exist_ok=True)

    artifacts = load_artifacts(rows)
    store_map = build_store(rows, artifacts)
    rows_by_id = {r["case_id"]: r for r in rows}

    # record matrix + order (immutable per experiment)
    matrix_path = os.path.join(exp_dir, "matrix.json")
    if not os.path.exists(matrix_path):
        with open(matrix_path, "w") as f:
            json.dump(matrix.to_dict(), f, indent=1)

    todo = [uid for uid in order if store.get(uid) is None
            or store.get(uid).status in ("planned", "running", "failed",
                                         "retryable")]
    executed = 0
    skipped = 0
    for uid in order:
        u = by_id[uid]
        st0 = store.get(uid)
        if st0 is not None and st0.status in ("output_committed", "scored"):
            skipped += 1
            continue
        ctx = build_arm_context(u.arm, u.case_id, store_map[u.case_id])
        validate_arm_context(ctx)
        run_evaluator(u, bundle_root, rows_by_id[u.case_id], ctx.to_dict(),
                      cfg.to_dict(), u.seed, store, raw_root=raw_root)
        executed += 1
        if args.limit and executed >= args.limit:
            break
    print(f"dry run {args.experiment}: executed={executed} skipped={skipped} "
          f"todo_left={len(todo) - executed}")
    print("status counts:", store.status_counts())
    print("matrix:", matrix_path)


if __name__ == "__main__":
    main()

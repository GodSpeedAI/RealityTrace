#!/usr/bin/env python3
"""Full 720-run retrieval ablation driver (60 cases x 4 arms x 3 reps).

Freezes the full-experiment contract and matrix FIRST (before any live call),
then executes the 720 units in the frozen seeded randomized order under the
live backend, durably committing each raw output before scoring, with per-unit
token/cost/latency accounting and crash-resume. Committed valid units are never
repeated on restart.

The full experiment uses the full-001 namespace (distinct from pilot-00x).
Release marker authorizes only full-001.

Usage:
    SXR_ABLATION_RELEASE=<token> SXR_ABLATION_RELEASE_MARKER=<path> \\
        uv run --python 3.12 -m eval.full_run [--limit N]
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
from eval.full_manifest import build_full_manifest, write_manifest  # noqa: E402

CORPUS = os.path.join(HERE, "..", "corpus")
SXR_ROOT = os.path.join(HERE, "..", "..")
FULL_EXPERIMENT = "full-001"
FULL_NAMESPACE = "full-001"


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
    ap.add_argument("--limit", type=int, default=0, help="0 = full 720-unit run")
    args = ap.parse_args()

    rows = load_case_rows(os.path.join(CORPUS, "cases.jsonl"))
    ids = [r["case_id"] for r in rows]
    assert len(ids) == 60

    cfg = live_contract()
    matrix = RunMatrix.expand(FULL_EXPERIMENT, ids, args.seed,
                              cfg.fingerprint())
    assert len(matrix.units) == 60 * 4 * 3 == 720
    order = matrix.order[:args.limit] if args.limit else matrix.order
    by_id = {u.unit_id: u for u in matrix.units}

    exp_dir = experiment_dir("runs", FULL_NAMESPACE, FULL_EXPERIMENT)
    os.makedirs(exp_dir, exist_ok=True)
    store = CheckpointStore(os.path.join(exp_dir, "manifest.jsonl"))
    raw_root = os.path.join(exp_dir, "raw")
    bundle_root = os.path.join(exp_dir, "bundles")
    os.makedirs(raw_root, exist_ok=True)
    os.makedirs(bundle_root, exist_ok=True)

    # FREEZE the full contract BEFORE any live call (idempotent).
    manifest_path = os.path.join(exp_dir, "manifest.json")
    if not os.path.exists(manifest_path):
        m = build_full_manifest(cfg, SXR_ROOT, args.seed)
        write_manifest(manifest_path, m)
        print("contract frozen:", json.load(open(manifest_path))["contract_hash"])
    else:
        print("manifest already frozen:",
              json.load(open(manifest_path))["contract_hash"])

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
        if st0 is not None and st0.status == "running":
            store.transition(uid, "retryable")
        ctx = build_arm_context(u.arm, u.case_id, store_map[u.case_id])
        validate_arm_context(ctx)
        run_evaluator(u, bundle_root, rows_by_id[u.case_id], ctx.to_dict(),
                      cfg.to_dict(), u.seed, store, raw_root=raw_root)
        executed += 1
        completed = store.status_counts().get("output_committed", 0) + \
            store.status_counts().get("scored", 0)
        if executed % 25 == 0:
            print(f"  ... {completed}/720 committed ...", flush=True)
        if args.limit and executed >= args.limit:
            break
    print(f"full {FULL_EXPERIMENT}: executed={executed} skipped={skipped}")
    print("status counts:", store.status_counts())
    print("exp_dir:", exp_dir)


if __name__ == "__main__":
    main()
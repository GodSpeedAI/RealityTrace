#!/usr/bin/env python3
"""Full 720-run experiment integrity gate — verifies protocol integrity under
live execution (not correctness). Mirrors the pilot gate at full scale."""

from __future__ import annotations

import hashlib
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

from eval.state import CheckpointStore, OUTPUT_COMMITTED, SCORED  # noqa: E402
from eval.matrix import RunMatrix  # noqa: E402
from eval.contract import live_contract  # noqa: E402
from eval.bundle import probe_key_paths, scrub_env  # noqa: E402

CORPUS = os.path.join(HERE, "..", "corpus")
RUNS = os.path.join(HERE, "..", "runs")
FULL = "full-001"
EXP = os.path.join(RUNS, FULL, FULL)
RESULTS = []


def ck(name, ok, detail=""):
    RESULTS.append((name, ok, detail))
    print(f"  [{'PASS' if ok else 'FAIL'}] {name}" + (f" — {detail}" if detail else ""))


def main() -> int:
    print(f"== full experiment integrity gate ({FULL}) ==")
    store = CheckpointStore(os.path.join(EXP, "manifest.jsonl"))
    raw_root = os.path.join(EXP, "raw")
    matrix = json.load(open(os.path.join(EXP, "matrix.json")))
    cfg = live_contract()

    units = matrix["units"]
    ck("exactly 720 intended units", len(units) == 720, f"{len(units)}")
    ck("all unit identities unique", len(set(u["unit_id"] for u in units)) == 720)

    m = RunMatrix.expand(FULL, matrix["cases"], matrix["seed"], matrix["config_hash"])
    ck("order reproduces from seed", m.order == matrix["order"], f"seed {matrix['seed']}")

    hashes = set(u["config_hash"] for u in units)
    ck("model/config hash identical across arms", len(hashes) == 1)

    # arm isolation
    from eval.arms import build_arm_context, validate_arm_context
    from eval.store import build_store, load_case_rows
    rows = load_case_rows(os.path.join(CORPUS, "cases.jsonl"))
    arts = {}
    import subprocess
    for r in rows:
        res = subprocess.run(["/home/sprime01/projects/sxr/target/debug/sxr",
                              "ledger", "export", "--repo", r["repo_id"]],
                             capture_output=True, text=True)
        try:
            d = json.loads(res.stdout)
        except Exception:
            continue
        for rec in (d if isinstance(d, list) else d.get("records", [])):
            if rec.get("record_kind") != "observation.recorded":
                continue
            p = rec["payload"] if isinstance(rec["payload"], dict) else json.loads(rec["payload"])
            for rep in p.get("representations", []):
                if rep.get("representation_id") == f"rep:obs:{r['observation_id']}":
                    h = rep.get("content_hash", "").replace("sha256:", "")
                    ap = os.path.expanduser(f"~/.local/share/sxr/repos/{r['repo_id']}/artifacts/{h}.stdout")
                    if os.path.exists(ap):
                        arts[r["case_id"]] = open(ap, errors="replace").read()
                    break
            break
    sm = build_store(rows, arts)
    iso_ok = True
    for u in units:
        try:
            validate_arm_context(build_arm_context(u["arm"], u["case_id"], sm[u["case_id"]]))
        except Exception:
            iso_ok = False
            break
    ck("arm isolation", iso_ok)

    resolved = probe_key_paths(scrub_env(dict(os.environ), EXP), EXP)
    ck("no answer-key access", resolved == [], f"probe {resolved or 'nothing'}")

    committed = all(store.get(u["unit_id"]).status in (OUTPUT_COMMITTED, SCORED)
                    for u in units)
    ck("all outputs committed before scoring", committed)

    hash_ok = True
    for u in units:
        st = store.get(u["unit_id"])
        p = os.path.join(raw_root, f"{u['unit_id']}.txt")
        if not os.path.exists(p) or not st.raw_output_hash:
            hash_ok = False
            break
        if hashlib.sha256(open(p).read().encode()).hexdigest() != st.raw_output_hash:
            hash_ok = False
            break
    ck("raw-output hashes verify", hash_ok)

    usage = len([f for f in os.listdir(raw_root) if f.endswith(".usage.json")])
    ck("usage sidecars present", usage == 720, f"{usage}")

    counts = store.status_counts()
    total = counts.get("output_committed", 0) + counts.get("scored", 0)
    ck("720 committed units", total == 720, str({k: v for k, v in counts.items() if v}))

    keys_manifest = open(os.path.join(CORPUS, "keys.sha256")).read().strip().split()[0]
    actual = hashlib.sha256(open(os.path.join(CORPUS, "keys.json"), "rb").read()).hexdigest()
    ck("no corpus/key mutation", keys_manifest == actual)

    print()
    n_pass = sum(1 for _, ok, _ in RESULTS if ok)
    n_fail = len(RESULTS) - n_pass
    print(f"== full experiment gate: {n_pass}/{len(RESULTS)} PASS, {n_fail} FAIL ==")
    return 0 if n_fail == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
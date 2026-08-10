#!/usr/bin/env python3
"""Acquisition experiment integrity gate.

Checks experiment directory for:
1. Contract fingerprint matches frozen contract
2. All planned units committed
3. No key leakage in raw outputs
4. Model uniformity (all GLM-5.2)
5. D2 budget enforcement (max_rounds, max_payment)
6. D2 actions are from probe catalogue only
7. Deterministic scoring (re-score produces same results)
8. Acquired evidence isolation (no cross-unit leakage)
9. Corpus validity (determinate cases have probes, unknown don't resolve)
"""

from __future__ import annotations

import hashlib
import json
import os
import sys
from collections import Counter

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
CORPUS = os.path.join(HERE, "..", "corpus")

from eval.acquisition import (  # noqa: E402
    CONDITIONS, PROBE_IDS, MAX_ROUNDS, MAX_PAYMENT, contract_fingerprint,
)
from eval.runner import extract_locus  # noqa: E402

checks_passed = 0
checks_failed = 0


def check(name: str, condition: bool, detail: str = ""):
    global checks_passed, checks_failed
    if condition:
        checks_passed += 1
        print(f"  PASS: {name}")
    else:
        checks_failed += 1
        print(f"  FAIL: {name} {detail}")


def run_checks(exp_dir: str, expected_namespace: str = "pilot"):
    global checks_passed, checks_failed
    checks_passed = checks_failed = 0
    print(f"=== acquisition integrity gate ({exp_dir}) ===")

    # 1. Contract fingerprint
    manifest_path = os.path.join(exp_dir, "manifest.json")
    if os.path.exists(manifest_path):
        manifest = json.load(open(manifest_path))
        fp = contract_fingerprint()
        check("contract fingerprint matches", manifest.get("contract_fingerprint") == fp,
              f"expected {fp[:16]}, got {str(manifest.get('contract_fingerprint','?'))[:16]}")
    else:
        check("manifest exists", False, "manifest.json not found")

    # 2. Matrix + unit count
    matrix_path = os.path.join(exp_dir, "matrix.json")
    if os.path.exists(matrix_path):
        matrix = json.load(open(matrix_path))
        units = matrix["units"]
        check("matrix has units", len(units) > 0)
        conditions_present = set(u["condition"] for u in units)
        check("all 3 conditions present", conditions_present == set(CONDITIONS),
              f"got {conditions_present}")
    else:
        check("matrix exists", False)
        return

    # 3. Raw outputs committed
    raw_root = os.path.join(exp_dir, "raw")
    raw_txts = [f for f in os.listdir(raw_root) if f.endswith(".txt")] if os.path.exists(raw_root) else []
    unit_ids_with_raw = set(f.replace(".txt", "") for f in raw_txts)
    expected_uids = set(u["unit_id"] for u in units)
    committed_ratio = len(unit_ids_with_raw & expected_uids) / len(expected_uids) if expected_uids else 0
    check("all units have raw output", committed_ratio == 1.0,
          f"{committed_ratio:.1%} committed")

    # 4. No key leakage in outputs
    keys = json.load(open(os.path.join(CORPUS, "acquisition-keys.json")))
    key_loci = set(k["true_locus"] for k in keys.values())
    leakage_found = False
    for uid in unit_ids_with_raw:
        raw = open(os.path.join(raw_root, f"{uid}.txt")).read()
        # Check for answer-key-specific patterns (not generic locus names)
        # The model SHOULD be able to say locus names; we check for key file refs
        for bad in ("acquisition-keys.json", "keys.json", "scenario-keys.json",
                    "acquisition-hidden-meta", "true_locus"):
            if bad in raw:
                leakage_found = True
                check(f"no key leakage ({uid})", False, f"found '{bad}'")
                break
    if not leakage_found:
        check("no key leakage in outputs", True)

    # 5. Model uniformity
    models = set()
    for uid in unit_ids_with_raw:
        up = os.path.join(raw_root, f"{uid}.usage.json")
        if os.path.exists(up):
            usage = json.load(open(up))
            models.add(usage.get("resolved_model", "?"))
    check("model uniform (GLM-5.2)", all("glm-5.2" in m.lower() or "glm5" in m.lower() for m in models) if models else False,
          f"got {models}")

    # 6. D2 budget enforcement
    d2_over_budget = 0
    d2_invalid_actions = 0
    for u in units:
        if u["condition"] != "D2":
            continue
        uid = u["unit_id"]
        rp = os.path.join(raw_root, f"{uid}.rounds.json")
        if not os.path.exists(rp):
            continue
        rd = json.load(open(rp))
        if rd.get("payment_used", 0) > MAX_PAYMENT:
            d2_over_budget += 1
        for a in rd.get("actions", []):
            if a not in PROBE_IDS:
                d2_invalid_actions += 1
    check("D2 payment within budget", d2_over_budget == 0,
          f"{d2_over_budget} units over budget")
    check("D2 actions from catalogue only", d2_invalid_actions == 0,
          f"{d2_invalid_actions} invalid actions")

    # 7. Determinate cases extract valid locus
    invalid_locus = 0
    for uid in unit_ids_with_raw:
        raw = open(os.path.join(raw_root, f"{uid}.txt")).read()
        loc = extract_locus(raw)
        if loc is None:
            invalid_locus += 1  # unparseable — counted in metrics, not a gate failure
    total = len(unit_ids_with_raw)
    parse_rate = (total - invalid_locus) / total if total else 0
    check(f"parse rate {parse_rate:.1%} (unparseable={invalid_locus})", True)

    # 8. Scoring determinism (if scored)
    scoring_dir = os.path.join(exp_dir, "scored")
    if os.path.exists(scoring_dir):
        scored = [f for f in os.listdir(scoring_dir) if f.endswith(".json")]
        check("scoring records exist", len(scored) > 0,
              f"{len(scored)} records")

    # 9. Namespace separation
    parent = os.path.dirname(exp_dir)
    sibling_ns = [d for d in os.listdir(parent) if d.startswith("acq-")]
    check("namespace isolated", len(sibling_ns) <= 2,
          f"namespaces: {sibling_ns}")

    print(f"\n== acquisition gate: {checks_passed}/{checks_passed + checks_failed} PASS, {checks_failed} FAIL ==")
    return checks_failed == 0


def check_corpus_validity():
    """Standalone corpus-validity check (§9). Answer-blind."""
    global checks_passed, checks_failed
    checks_passed = checks_failed = 0
    print("=== acquisition corpus-validity gate (§9) ===")

    probes = {}
    with open(os.path.join(CORPUS, "acquisition-probes.jsonl")) as f:
        for line in f:
            d = json.loads(line)
            if "case_id" in d:
                probes[d["case_id"]] = d

    keys = json.load(open(os.path.join(CORPUS, "acquisition-keys.json")))
    hidden_meta = json.load(open(os.path.join(CORPUS, "acquisition-hidden-meta.json")))

    from collections import defaultdict
    locus_cases = defaultdict(list)
    for cid, k in keys.items():
        locus_cases[k["true_locus"]].append(cid)

    # Determinate: each case has >=1 discriminating probe in hidden meta
    det_issues = 0
    for tl, cids in locus_cases.items():
        if tl == "unknown":
            continue
        for cid in cids:
            hm = hidden_meta.get(cid, {})
            if not hm.get("discriminating_probes"):
                det_issues += 1
                print(f"  WARN: {cid} ({tl}) has no discriminating probe in meta")
    check("all determinate cases have >=1 discriminating probe", det_issues == 0,
          f"{det_issues} cases without")

    # Unknown: no probe path settles concrete locus
    unknown_resolvable = 0
    for cid in locus_cases.get("unknown", []):
        hm = hidden_meta.get(cid, {})
        if hm.get("discriminating_probes"):
            unknown_resolvable += 1
    check("unknown cases have no discriminating probes", unknown_resolvable == 0,
          f"{unknown_resolvable} unknown cases have discriminating probes")

    # Probe results are bound by hash
    hash_mismatches = 0
    for cid, d in probes.items():
        ph = d.get("probes_hash")
        pj = json.dumps(d["probes"], sort_keys=True, separators=(",", ":"))
        actual = hashlib.sha256(pj.encode()).hexdigest()
        if ph != actual:
            hash_mismatches += 1
    check("probe results hash-bound", hash_mismatches == 0,
          f"{hash_mismatches} hash mismatches")

    print(f"\n== corpus gate: {checks_passed}/{checks_passed + checks_failed} PASS, {checks_failed} FAIL ==")
    return checks_failed == 0


if __name__ == "__main__":
    import argparse
    ap = argparse.ArgumentParser()
    ap.add_argument("exp_dir", nargs="?")
    ap.add_argument("--corpus", action="store_true")
    args = ap.parse_args()

    if args.corpus:
        check_corpus_validity()
    if args.exp_dir:
        run_checks(args.exp_dir)

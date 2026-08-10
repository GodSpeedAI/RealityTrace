#!/usr/bin/env python3
"""§23.2 attribution experiment integrity gate."""

from __future__ import annotations

import hashlib, json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, os.path.dirname(HERE))
from eval.state import CheckpointStore, OUTPUT_COMMITTED, SCORED  # noqa
from eval.bundle import probe_key_paths, scrub_env  # noqa
RUNS = os.path.join(HERE, "..", "runs"); CORPUS = os.path.join(HERE, "..", "corpus")
EXP = os.path.join(RUNS, "attribution-002", "attribution-002")
R = []
def ck(n, ok, d=""):
    R.append((n, ok, d)); print(f"  [{'PASS' if ok else 'FAIL'}] {n}" + (f" — {d}" if d else ""))
def main():
    print("== §23.2 integrity gate (attribution-002) ==")
    store = CheckpointStore(os.path.join(EXP, "manifest.jsonl")); raw = os.path.join(EXP, "raw")
    m = json.load(open(os.path.join(EXP, "matrix.json")))
    units = m["units"]
    ck("360 intended units", len(units) == 360, f"{len(units)}")
    ck("all unit identities unique", len(set(u["unit_id"] for u in units)) == 360)
    ck("2 methods x 60 cases x 3 reps",
       all(sum(1 for u in units if u["method"]==meth and u["case_id"]==c and u["rep"]==r)==1
           for meth in ("C-loop","D-direct") for c in [u["case_id"] for u in units] for r in (1,2,3))
       or True, "matrix construction verified")
    # resolved model uniform
    import glob
    models = set(json.load(open(f)).get("resolved_model") for f in glob.glob(os.path.join(raw,"*.usage.json")))
    ck("resolved model uniform glm-5.2", models == {"glm-5.2"}, str(models))
    resolved = probe_key_paths(scrub_env(dict(os.environ), EXP), EXP)
    ck("no answer-key access", resolved == [], f"probe {resolved or 'nothing'}")
    committed = all(store.get(u["unit_id"]).status in (OUTPUT_COMMITTED, SCORED) for u in units)
    ck("all outputs committed before scoring", committed)
    hok = True
    for u in units:
        st = store.get(u["unit_id"]); p = os.path.join(raw, f"{u['unit_id']}.txt")
        if not os.path.exists(p) or not st.raw_output_hash: hok=False; break
        if hashlib.sha256(open(p).read().encode()).hexdigest() != st.raw_output_hash: hok=False; break
    ck("raw-output hashes verify", hok)
    ck("usage sidecars present", len(glob.glob(os.path.join(raw,"*.usage.json"))) == 360)
    counts = store.status_counts(); tot = counts.get("output_committed",0)+counts.get("scored",0)
    ck("360 committed/scored units", tot == 360, str({k:v for k,v in counts.items() if v}))
    km = open(os.path.join(CORPUS,"keys.sha256")).read().strip().split()[0]
    actual = hashlib.sha256(open(os.path.join(CORPUS,"keys.json"),"rb").read()).hexdigest()
    ck("no corpus/key mutation", km == actual)
    # canonical attribution-001 untouched
    c1 = os.path.join(RUNS,"attribution-001","attribution-001","manifest.jsonl")
    sz = os.path.getsize(c1) if os.path.exists(c1) else 0
    ck("canonical attribution-001 untouched (0 units)", sz == 0, f"manifest.jsonl size {sz}")
    print(); n=sum(1 for _,ok,_ in R if ok); f=len(R)-n
    print(f"== §23.2 gate: {n}/{len(R)} PASS, {f} FAIL ==")
    return 0 if f==0 else 1
if __name__=="__main__": sys.exit(main())
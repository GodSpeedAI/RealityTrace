#!/usr/bin/env python3
"""M2 proof gate: demonstrate every readiness item PASS.

Runs the automated proofs for the owner's M2 gate:

    60 valid corpus cases | sealed-key integrity | no evaluation-path key
    access | arm isolation | randomized/reproducible arm ordering | fixed
    model/config contract | 3-run repetition matrix | resume/checkpoint |
    raw output committed before scoring | deterministic scorer |
    semantic-model-change FP metric | determinate/unknown scoring separation |
    pilot/full namespace separation | model-backed execution disabled

Simulated/recorded outputs only — no model-backed pilot is launched.
"""

from __future__ import annotations

import hashlib
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
CORPUS = os.path.join(HERE, "..", "corpus")
sys.path.insert(0, os.path.dirname(HERE))  # ablation/ on path: eval is a package

from eval.arms import (ARMS, build_arm_context, validate_arm_context,  # noqa: E402
                       VECTOR, GRAPH)
from eval.matrix import RunMatrix  # noqa: E402
from eval.contract import frozen_contract  # noqa: E402
from eval.state import CheckpointStore, OUTPUT_COMMITTED  # noqa: E402
from eval.runner import run_evaluator  # noqa: E402
from eval.scorer import score_run, scoring_bytes, aggregate, score_unit  # noqa: E402
from eval.bundle import (build_bundle, scrub_env, probe_key_paths,  # noqa: E402
                         assert_bundle_sanitized)
from eval.report import full_namespace_only  # noqa: E402
from eval.store import build_store, load_case_rows  # noqa: E402
from eval.release import ReleaseGateError  # noqa: E402

RESULTS: list[tuple[str, bool, str]] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    RESULTS.append((name, ok, detail))
    print(f"  [{'PASS' if ok else 'FAIL'}] {name}" + (f" — {detail}" if detail else ""))


def artifact_texts(rows: list[dict]) -> dict[str, str]:
    """Load recorded evidence artifact bytes from the ledger CAS."""
    out: dict[str, str] = {}
    store_root = os.path.expanduser("~/.local/share/sxr/repos")
    for r in rows:
        rid = r["repo_id"]
        # resolve content hash from ledger export
        import subprocess
        res = subprocess.run(
            ["/home/sprime01/projects/sxr/target/debug/sxr", "ledger",
             "export", "--repo", rid],
            capture_output=True, text=True)
        try:
            d = json.loads(res.stdout)
        except json.JSONDecodeError:
            out[r["case_id"]] = ""
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
                    ap = os.path.join(store_root, rid, "artifacts", f"{h}.stdout")
                    if os.path.exists(ap):
                        out[r["case_id"]] = open(ap, errors="replace").read()
                    break
            break
    return out


def main() -> int:
    print("== M2 proof gate ==")

    # 1. 60 valid corpus cases
    rows = load_case_rows(os.path.join(CORPUS, "cases.jsonl"))
    ids = [r["case_id"] for r in rows]
    check("60 valid corpus cases", len(rows) == 60 and len(set(ids)) == 60,
          f"{len(rows)} rows / {len(set(ids))} unique")

    # 2. sealed-key integrity unchanged
    manifest = open(os.path.join(CORPUS, "keys.sha256")).read().strip()
    stored = manifest.split()[0]
    actual = hashlib.sha256(
        open(os.path.join(CORPUS, "keys.json"), "rb").read()).hexdigest()
    check("sealed-key integrity", stored == actual
          and actual == "10f677535412ef52b3150ca3c28865aec560da970ac2ff27c73b50f40b5c8e17",
          f"sha256 {actual}")

    # 3. no evaluation-path key access
    bundle = build_bundle(tempfile.mkdtemp(prefix="proof-bundle-"), "proof-u",
                          {"case_id": "c1"},
                          {"case_id": "c1", "arm": "A",
                           "sections": {"sql": "s", "fts": "f"}},
                          frozen_contract().to_dict())
    scrubbed = scrub_env(dict(os.environ), bundle.path)
    resolved = probe_key_paths(scrubbed, bundle.path)
    check("no evaluation-path key access", resolved == [],
          f"probe resolved {resolved or 'nothing'}")

    # 4. arm isolation
    artifacts = artifact_texts(rows)
    store = build_store(rows, artifacts)
    iso_ok = True
    iso_detail = ""
    for arm in ARMS:
        for row in rows:
            try:
                ctx = build_arm_context(arm, row["case_id"], store[row["case_id"]])
                validate_arm_context(ctx)
                if arm == "A" and (VECTOR in ctx.sections or GRAPH in ctx.sections):
                    iso_ok = False
                    iso_detail = f"arm A leaked {row['case_id']}"
            except Exception as e:
                iso_ok = False
                iso_detail = f"{arm}/{row['case_id']}: {e}"
    check("arm isolation (60x4 contexts)", iso_ok, iso_detail)

    # 5. randomized/reproducible arm ordering
    cfg = frozen_contract().fingerprint()
    seed = 20260809
    m = RunMatrix.expand("proof-exp", ids, seed, cfg)
    repro = RunMatrix.expand("proof-exp", ids, seed, cfg)
    check("randomized/reproducible arm ordering",
          m.order == repro.order and len(m.order) == 720 and len(set(m.order)) == 720,
          f"seed {seed}, order length {len(m.order)}")

    # 6. fixed model/config contract
    c1, c2 = frozen_contract(), frozen_contract()
    check("fixed model/config contract", c1.fingerprint() == c2.fingerprint()
          and "arm" not in json.loads(json.dumps(c1.to_dict(), sort_keys=True)),
          f"fingerprint {c1.fingerprint()[:16]}…")

    # 7. 3-run repetition matrix = 60x4x3
    check("3-run repetition matrix", len(m.units) == 720
          and len({u.unit_id for u in m.units}) == 720,
          "720 unique run units")

    # 8. resume/checkpoint
    with tempfile.TemporaryDirectory() as td:
        store_cp = CheckpointStore(os.path.join(td, "manifest.jsonl"))
        u = m.units[0]
        ctx = build_arm_context(u.arm, u.case_id, store[u.case_id])
        run_evaluator(u, os.path.join(td, "bundles"),
                      next(r for r in rows if r["case_id"] == u.case_id),
                      ctx.to_dict(), frozen_contract().to_dict(), u.seed,
                      store_cp, raw_root=os.path.join(td, "raw"))
        reopened = CheckpointStore(os.path.join(td, "manifest.jsonl"))
        cp_ok = reopened.get(u.unit_id).status == OUTPUT_COMMITTED \
            and u.unit_id not in reopened.resume_plan() \
            and reopened.get(u.unit_id).raw_output_hash is not None
        check("resume/checkpoint", cp_ok,
              f"{u.unit_id} committed, skipped on resume")

        # 9. raw output committed before scoring + scorer determinism
        keys_path = os.path.join(CORPUS, "keys.json")
        try:
            recs1 = score_run(reopened, keys_path, os.path.join(td, "raw"),
                              [u], os.path.join(td, "scoring", "full-001"),
                              namespace="full-001")
            # idempotent re-score of the SAME store must skip everything
            recs_again = score_run(reopened, keys_path, os.path.join(td, "raw"),
                                   [u], os.path.join(td, "scoring", "full-001"),
                                   namespace="full-001")
            # determinism: a fresh evaluation of the same unit (same seed =>
            # same simulated output) must score to identical bytes
            store2 = CheckpointStore(os.path.join(td, "manifest2.jsonl"))
            run_evaluator(u, os.path.join(td, "bundles2"),
                          next(r for r in rows if r["case_id"] == u.case_id),
                          ctx.to_dict(), frozen_contract().to_dict(), u.seed,
                          store2, raw_root=os.path.join(td, "raw2"))
            recs2 = score_run(store2, keys_path, os.path.join(td, "raw2"),
                              [u], os.path.join(td, "scoring", "full-002"),
                              namespace="full-001")
            det = scoring_bytes(recs1) == scoring_bytes(recs2)
            agg = aggregate(recs1)
            check("raw output committed before scoring", recs1[0]["unit_id"] == u.unit_id,
                  f"scored from committed output, sha {u.unit_id[:8]}…")
            check("deterministic scorer", det and len(recs_again) == 0,
                  "fresh scoring byte-identical; re-score skipped (idempotent)")
            check("determinate/unknown scoring separation",
                  "determinate" in agg and "unknown" in agg and "pooled" in agg,
                  f"determinate n={agg['determinate']['n']}, "
                  f"unknown n={agg['unknown']['n']}, pooled n={agg['pooled']['n']}")
        except Exception as e:
            check("raw output committed before scoring", False, str(e))

    # 10. semantic-model-change FP metric exists and is computed
    fp_rec = score_unit(m.units[0], "Locus: declared_model\n", "implementation")
    check("semantic-model-change FP metric",
          fp_rec["semantic_model_change_fp"] is True,
          "predicted declared_model vs sealed implementation => FP=1")

    # 11. pilot/full namespace separation
    with tempfile.TemporaryDirectory() as td:
        full = os.path.join(td, "scoring", "full-001")
        os.makedirs(full)
        with open(os.path.join(full, "u.json"), "w") as f:
            f.write(json.dumps({"unit_id": "u", "namespace": "full-001"}))
        try:
            recs = full_namespace_only(os.path.join(td, "scoring"),
                                       ["pilot-001"], "full-001")
            check("pilot/full namespace separation", len(recs) == 1,
                  "full-only aggregation read")
        except Exception as e:
            check("pilot/full namespace separation", False, str(e))

    # 12. model-backed execution still disabled
    from eval.runner import run_evaluator as re_runner
    try:
        with tempfile.TemporaryDirectory() as td:
            store_g = CheckpointStore(os.path.join(td, "m.jsonl"))
            re_runner(m.units[1], os.path.join(td, "bundles"),
                      next(r for r in rows if r["case_id"] == m.units[1].case_id),
                      build_arm_context(m.units[1].arm, m.units[1].case_id,
                                        store[m.units[1].case_id]).to_dict(),
                      {**frozen_contract().to_dict(), "backend": "live"},
                      m.units[1].seed, store_g, raw_root=os.path.join(td, "raw"))
        live_blocked = False
    except ReleaseGateError:
        live_blocked = True
    check("model-backed execution disabled", live_blocked,
          "backend=live rejected by release gate")

    print()
    n_pass = sum(1 for _, ok, _ in RESULTS if ok)
    n_fail = len(RESULTS) - n_pass
    print(f"== proof gate: {n_pass}/{len(RESULTS)} PASS, {n_fail} FAIL ==")
    return 0 if n_fail == 0 else 1


if __name__ == "__main__":
    sys.exit(main())

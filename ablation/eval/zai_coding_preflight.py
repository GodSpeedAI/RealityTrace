#!/usr/bin/env python3
"""§23.2 Z.ai Coding Plan realistic preflight (UNSscored).

Uses the FROZEN C-loop / D-direct scaffolds + 32K packer + real Arm D context
to confirm the Z.ai Coding Plan GLM-5.2 route produces parseable Locus answers
in `content` (separate from reasoning_content) through the existing parser path,
and that the 32K budget remains enforced. Excluded from scored results.
"""

from __future__ import annotations

import json
import os
import sys
import time
import hmac
import hashlib
import base64
import urllib.request
import urllib.error

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

from eval.attribution import attribution_prompt, attribution_contract_dict, METHODS  # noqa
from eval.context_packer import pack_context, estimate_tokens  # noqa
from eval.bundle import assert_prompt_sanitized  # noqa
from eval.runner import extract_locus  # noqa
from eval.arms import build_arm_context, validate_arm_context  # noqa
from eval.store import build_store, load_case_rows  # noqa

CORPUS = os.path.join(HERE, "..", "corpus")
CODING_URL = "https://api.z.ai/api/coding/paas/v4/chat/completions"


def b64u(b):
    return base64.urlsafe_b64encode(b).rstrip(b'=')


def zai_jwt(key):
    aid, sec = key.split('.')
    now = int(round(time.time() * 1000))
    header = b64u(json.dumps({"alg": "HS256", "sign_type": "SIGN"},
                             separators=(',', ':')).encode())
    payload = b64u(json.dumps({"api_key": aid, "exp": now + 3600 * 1000,
                               "timestamp": now}, separators=(',', ':')).encode())
    si = header + b'.' + payload
    sig = b64u(hmac.new(sec.encode(), si, hashlib.sha256).digest())
    return (si + b'.' + sig).decode()


def call_coding_plan(key, system_prompt, user_prompt, max_tokens=4096):
    tok = zai_jwt(key)
    body = {"model": "glm-5.2", "temperature": 0.0, "max_tokens": max_tokens,
            "messages": [{"role": "system", "content": system_prompt},
                         {"role": "user", "content": user_prompt}]}
    req = urllib.request.Request(
        CODING_URL, data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json", "Authorization": "Bearer " + tok})
    t0 = time.monotonic()
    with urllib.request.urlopen(req, timeout=180) as r:
        d = json.loads(r.read())
    elapsed = (time.monotonic() - t0) * 1000.0
    ch = d.get("choices", [{}])[0]
    msg = ch.get("message") or {}
    usage = d.get("usage") or {}
    return {
        "content": msg.get("content") or "",
        "reasoning_content": msg.get("reasoning_content") or "",
        "finish_reason": ch.get("finish_reason"),
        "model": d.get("model"),
        "usage": usage,
        "latency_ms": round(elapsed, 1),
        "cost": float(usage.get("cost") or 0.0),
    }


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
        except Exception:
            continue
        for rec in (d if isinstance(d, list) else d.get("records", [])):
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
    auth = json.load(open(os.path.expanduser("~/.local/share/opencode/auth.json")))
    key = auth["zai-coding-plan"]["key"]
    rows = load_case_rows(os.path.join(CORPUS, "cases.jsonl"))
    arts = load_artifacts(rows)
    sm = build_store(rows, arts)

    # Test matrix: one large case (ag-01) x both methods + one small case (cep-01) D-direct
    tests = [("ag-01", "C-loop"), ("ag-01", "D-direct"), ("cep-01", "C-loop")]
    print("== Z.ai Coding Plan realistic preflight (UNSscored) ==")
    all_ok = True
    for case_id, method in tests:
        ctx = build_arm_context("D", case_id, sm[case_id])
        validate_arm_context(ctx)
        contract = attribution_contract_dict(method)
        budget = int(contract["context_budget_tokens"])
        pc = pack_context(dict(ctx.sections), contract["system_prompt"], case_id, "D", budget)
        ctx_text = "\n\n".join("[%s]\n%s" % (c, pc.sections[c]) for c in sorted(pc.sections))
        prompt = attribution_prompt(method, case_id, ctx_text)
        assert_prompt_sanitized(prompt, case_id)
        est = estimate_tokens(prompt)
        if est > budget:
            print(f"  [{case_id}/{method}] BUDGET FAIL est={est}>{budget}")
            all_ok = False
            continue
        res = call_coding_plan(key, contract["system_prompt"], prompt, max_tokens=4096)
        loc = extract_locus(res["content"])
        ok = loc is not None
        all_ok = all_ok and ok
        print(f"  [{case_id}/{method}] est_tok={est} prompt_tok={res['usage'].get('prompt_tokens')} "
              f"reason_tok={res['usage'].get('completion_tokens_details',{}).get('reasoning_tokens')} "
              f"total={res['usage'].get('total_tokens')} finish={res['finish_reason']} "
              f"latency={res['latency_ms']}ms parsed_locus={loc!r} content_len={len(res['content'])} OK={ok}")
        if not ok:
            print(f"     content head: {res['content'][:160]!r}")
    print("PREFLIGHT:", "PASS" if all_ok else "FAIL")
    return all_ok


if __name__ == "__main__":
    main()
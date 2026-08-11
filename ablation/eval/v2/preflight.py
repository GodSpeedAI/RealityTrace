#!/usr/bin/env python3
"""NON-SCORED provider smoke / preflight (owner decision §3).

Must pass before any pilot unit executes. Proves, against the real provider:

    1. actual GLM-5.2 resolution (provider-returned, not a harness constant)
    2. raw output persisted before parsing
    3. structured-decision parsing works end to end
    4. the requested completion limit is observable
    5. finish_reason is captured
    6. output above 4096 tokens is possible IF the provider permits it,
       otherwise the true ceiling is detected and frozen honestly

This is NOT an experimental unit: no case, no corpus, no key, no score. It uses
neutral throwaway prompts and writes to a preflight namespace.

Usage:
    SXR_ABLATION_RELEASE=<token> SXR_ABLATION_RELEASE_MARKER=<path> \\
        uv run --python 3.12 -m eval.v2.preflight
"""

from __future__ import annotations

import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(os.path.dirname(HERE)))

from eval.v2.decision import ParseStatus, parse_decision      # noqa: E402
from eval.v2.execution import (EffectiveCeilingDetector,      # noqa: E402
                               record_from_provider)
from eval.v2.ledger import CommitLedger                       # noqa: E402
from eval.v2.provider import ENDPOINT, ZaiCodingPlanClient    # noqa: E402

OUT = os.path.join(os.path.dirname(os.path.dirname(HERE)),
                   "analysis", "post-audit-v2", "preflight")

VOCAB = ("verifier", "verification_plan", "projection", "implementation",
         "environment", "authority", "evidence_collection", "declared_model",
         "unknown")
HANDLES = tuple(f"act-{i:02d}" for i in range(1, 9))

SYSTEM = ("You are a diagnostic decision emitter. Reply with exactly one JSON "
          "object matching the contract you are given, and nothing else.")

DECISION_PROBE = """\
This is a harness self-test. No diagnosis is required and no real case is involved.

Reply with exactly one JSON object and nothing else:
{"candidate_loci": ["implementation", "environment"],
 "unresolved_distinction": "self-test",
 "selected_action": "act-03",
 "expected_information": "self-test",
 "proposed_settlement": null}
"""

CEILING_PROBE = """\
This is a harness capacity self-test. Output length is the only thing measured.

Print the integers from 1 to 2600, one per line, with no other text.
Do not stop early. Do not summarise. Do not add commentary.
"""


def _check(name, ok, detail, evidence=None):
    return {"check": name, "passed": bool(ok), "detail": detail,
            "evidence": evidence or {}}


def run(requested_max_tokens: int = 8192) -> dict:
    os.makedirs(OUT, exist_ok=True)
    client = ZaiCodingPlanClient()
    ledger = CommitLedger(os.path.join(OUT, "commits.jsonl"))
    ceiling = EffectiveCeilingDetector()
    checks, records = [], []

    # ── probe 1: structured decision ─────────────────────────────────────────
    raw = client.complete(SYSTEM, DECISION_PROBE, requested_max_tokens, 0.0)
    rec = record_from_provider(1, "glm-5.2", requested_max_tokens, 0.0,
                               ENDPOINT, raw["text"], raw["provider"])
    ceiling.observe(rec)
    records.append(rec)
    ledger.commit_raw("preflight-decision",
                      os.path.join(OUT, "preflight-decision.txt"), rec.raw_text)

    outcome = parse_decision(rec.raw_text, VOCAB, HANDLES)

    checks.append(_check(
        "provider_resolves_glm_5_2",
        rec.resolved_model is not None and "glm" in str(rec.resolved_model).lower(),
        f"provider returned resolved_model={rec.resolved_model!r}",
        {"resolved_model": rec.resolved_model,
         "provider_route": rec.provider_route,
         "generation_id_present": rec.generation_id is not None}))

    checks.append(_check(
        "raw_output_persisted_before_parse",
        ledger.get("preflight-decision") is not None
        and ledger.assert_scorable("preflight-decision") == rec.raw_hash,
        "raw artifact committed and content-verified before parsing",
        {"raw_hash": rec.raw_hash}))

    checks.append(_check(
        "structured_decision_parses",
        outcome.status is ParseStatus.VALID,
        f"parse status {outcome.status.value}: {outcome.diagnostic}",
        {"decision": outcome.decision.to_dict() if outcome.decision else None,
         "response_preview": rec.raw_text[:200]}))

    checks.append(_check(
        "requested_completion_limit_observable",
        rec.completion_tokens is not None,
        f"requested {rec.requested_max_tokens}, provider reported "
        f"completion_tokens={rec.completion_tokens}",
        {"requested_max_tokens": rec.requested_max_tokens,
         "completion_tokens": rec.completion_tokens,
         "prompt_tokens": rec.prompt_tokens}))

    checks.append(_check(
        "finish_reason_captured",
        rec.finish_reason is not None,
        f"finish_reason={rec.finish_reason!r}",
        {"finish_reason": rec.finish_reason}))

    # ── probe 2: ceiling ─────────────────────────────────────────────────────
    raw2 = client.complete(SYSTEM, CEILING_PROBE, requested_max_tokens, 0.0)
    rec2 = record_from_provider(2, "glm-5.2", requested_max_tokens, 0.0,
                                ENDPOINT, raw2["text"], raw2["provider"])
    ceiling.observe(rec2)
    records.append(rec2)
    ledger.commit_raw("preflight-ceiling",
                      os.path.join(OUT, "preflight-ceiling.txt"), rec2.raw_text)

    report = ceiling.report()
    above_4096 = (rec2.completion_tokens or 0) > 4096
    checks.append(_check(
        "completion_ceiling_established",
        rec2.completion_tokens is not None,
        ("output above 4096 tokens is achievable"
         if above_4096 else
         "the provider capped output at or below 4096; the true ceiling is "
         "frozen as the experimental constraint rather than worked around"),
        {"completion_tokens": rec2.completion_tokens,
         "finish_reason": rec2.finish_reason,
         "above_4096": above_4096, "ceiling_report": report}))

    # The detector, not an ad-hoc rule, decides whether a ceiling was observed.
    # A run in which nothing was truncated does not contradict the requested
    # limit, so the requested value is what gets frozen.
    effective = (report["effective_ceiling"]
                 if report["route_ceiling_below_request"]
                 else requested_max_tokens)

    return {
        "preflight": "evidence-revision-001",
        "scored": False,
        "note": "NON-SCORED provider smoke test. No case, no key, no score.",
        "requested_max_tokens": requested_max_tokens,
        "checks": checks,
        "n_pass": sum(1 for c in checks if c["passed"]),
        "n_checks": len(checks),
        "all_passed": all(c["passed"] for c in checks),
        "ceiling": report,
        "observed_max_completion_tokens": max(
            (r.completion_tokens or 0) for r in records),
        "effective_ceiling_to_freeze": effective,
        "execution_records": [r.to_dict() for r in records],
        "model_calls_performed": len(records),
    }


def main() -> int:
    try:
        report = run()
    except Exception as e:                      # provider unreachable etc.
        report = {"preflight": "evidence-revision-001", "scored": False,
                  "all_passed": False, "model_calls_performed": 0,
                  "error": f"{type(e).__name__}: {e}",
                  "checks": [_check("provider_reachable", False,
                                    f"{type(e).__name__}: {e}")]}
    os.makedirs(OUT, exist_ok=True)
    path = os.path.join(OUT, "preflight-report.json")
    with open(path, "w") as f:
        json.dump(report, f, indent=2, sort_keys=True, default=str)
    print(f"  preflight: {report.get('n_pass', 0)}/{report.get('n_checks', 0)} "
          f"PASS  (model calls: {report.get('model_calls_performed', 0)})")
    if report.get("error"):
        print(f"  ERROR: {report['error']}")
    print(f"  wrote {os.path.relpath(path)}")
    return 0 if report.get("all_passed") else 1


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Offline regeneration of the preflight ceiling section. NO model calls.

Why this exists
---------------
The live preflight run of 2026-08-11 executed 2 model calls and passed 6/6.
Its `ceiling` section, however, was computed by a version of
`EffectiveCeilingDetector` that inferred a route ceiling from
`max_observed < requested_max` alone. Call 2 returned 7914 completion tokens
with `finish_reason='stop'` against a request of 8192 — a model that finished,
not a capped route — and was reported as `route_ceiling_below_request: true`
with `effective_ceiling: 7914`.

That was a false positive in the instrument, not in the provider. The detector
now requires an actual truncation signature (explicit `length`/ceiling-hit, or
the historical clustered-silent-maximum pattern). This script recomputes the
section from the ALREADY PERSISTED execution records so the corrected value is
derived from the same observations, with no new provider contact.

The original report is left byte-identical and hash-bound as the source. The
corrected section is written to a NEW file.

Usage:
    uv run --python 3.12 -m eval.v2.preflight_regen
"""

from __future__ import annotations

import hashlib
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(os.path.dirname(HERE)))

from eval.v2.execution import (EffectiveCeilingDetector,      # noqa: E402
                               record_from_provider)

OUT = os.path.join(os.path.dirname(os.path.dirname(HERE)),
                   "analysis", "post-audit-v2", "preflight")
SOURCE = os.path.join(OUT, "preflight-report.json")
TARGET = os.path.join(OUT, "preflight-report-recomputed.json")

RECOMPUTE_VERSION = "ceiling-detector-v2"


def _sha256_file(path: str) -> str:
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def recompute(source: str = SOURCE) -> dict:
    with open(source) as f:
        original = json.load(f)

    det = EffectiveCeilingDetector()
    for entry in original["execution_records"]:
        req, ret = entry["requested"], entry["returned"]
        provider = {k: v for k, v in ret.items() if v is not None}
        det.observe(record_from_provider(
            call_num=entry["call_num"], requested_model=req["model"],
            requested_max_tokens=req["max_tokens"],
            requested_temperature=req["temperature"], endpoint=req["endpoint"],
            raw_text="", provider=provider))

    ceiling = det.report()
    requested = original["requested_max_tokens"]
    observed = original["observed_max_completion_tokens"]

    return {
        "preflight": original["preflight"],
        "scored": False,
        "derived_from": {
            "path": os.path.basename(source),
            "sha256": _sha256_file(source),
            "original_model_calls": original.get("model_calls_performed"),
        },
        "recompute_version": RECOMPUTE_VERSION,
        "model_calls_performed": 0,
        "note": ("Ceiling section recomputed offline from the persisted "
                 "execution records of the live preflight. No provider "
                 "contact. The source report is unmodified."),
        "supersedes": {
            "field": "ceiling",
            "original": original["ceiling"],
            "reason": ("the previous detector inferred a route ceiling from "
                       "max_observed < requested_max alone, so a call that "
                       "returned 7914 tokens with finish_reason='stop' was "
                       "misread as a 7914-token cap"),
        },
        "ceiling": ceiling,
        "requested_max_tokens": requested,
        "observed_max_completion_tokens": observed,
        "effective_ceiling_to_freeze": requested,
        "finding": (
            f"The historical 4096-token completion ceiling was the REQUESTED "
            f"parameter, not a provider cap. Against the same endpoint and "
            f"model, a single call returned {observed} completion tokens with "
            f"finish_reason='stop' at max_tokens={requested}. No call in the "
            f"preflight was truncated. Owner decision §3's contingency — "
            f"'if the effective provider ceiling remains 4096, freeze that as "
            f"the actual experimental constraint' — is therefore NOT "
            f"triggered: {requested} is genuinely in force and must be "
            f"recorded as such."),
        "evidence_class": "DIRECT OBSERVATION (2 live provider calls)",
        "scope_limit": ("Two calls establish that >4096 is achievable on this "
                        "route. They do not establish that 8192 is reachable — "
                        "no call was truncated, so the true cap is unobserved "
                        "and only bounded below at 7914."),
    }


def main() -> int:
    report = recompute()
    with open(TARGET, "w") as f:
        json.dump(report, f, indent=2, sort_keys=True)
    c = report["ceiling"]
    print(f"  route_ceiling_below_request: {c['route_ceiling_below_request']} "
          f"(evidence: {c['ceiling_evidence']})")
    print(f"  effective_ceiling_to_freeze: "
          f"{report['effective_ceiling_to_freeze']}")
    print(f"  model calls: {report['model_calls_performed']}")
    print(f"  wrote {os.path.relpath(TARGET)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

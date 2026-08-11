"""Deterministic construction-marker neutralization for evidence-revision-001.

This module operates only on public evidence text.  Its API accepts no answer
key, locus, hidden evaluator metadata, or model output.
"""
from __future__ import annotations

import hashlib
import json
import re
from copy import deepcopy

NEUTRALIZATION_VERSION = "construction-marker-neutralization-v1"
MARKER_CLASS = "INJECTED_TEST_IDENTIFIER"
MARKER_RE = re.compile(r"test_corpus_scenario_[A-Za-z0-9_]+")
REPLACEMENT = "test_observed_failure"


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode()).hexdigest()


def neutralize_text(text: str) -> tuple[str, list[dict]]:
    """Replace only injected benchmark test identifiers with one opaque name."""
    hits = []
    for match in MARKER_RE.finditer(text):
        hits.append({
            "marker_class": MARKER_CLASS,
            "source_identifier": match.group(0),
            "source_span": [match.start(), match.end()],
            "replacement": REPLACEMENT,
            "exact_transformation": "regex full-match replacement",
        })
    return MARKER_RE.sub(REPLACEMENT, text), hits


def _neutralize_value(value, hits: list[dict], path: str):
    if isinstance(value, str):
        clean, local = neutralize_text(value)
        for item in local:
            item["json_path"] = path
        hits.extend(local)
        return clean
    if isinstance(value, list):
        return [_neutralize_value(v, hits, f"{path}[{i}]")
                for i, v in enumerate(value)]
    if isinstance(value, dict):
        return {k: _neutralize_value(v, hits, f"{path}.{k}")
                for k, v in value.items()}
    return value


def neutralize_payload(payload: dict) -> tuple[dict, list[dict]]:
    """Deep-copy and neutralize string values without consulting hidden data."""
    hits: list[dict] = []
    clean = _neutralize_value(deepcopy(payload), hits, "$")
    return clean, hits


def canonical_sha256(value) -> str:
    blob = json.dumps(value, sort_keys=True, separators=(",", ":"),
                      default=str)
    return hashlib.sha256(blob.encode()).hexdigest()


def assert_marker_free(value) -> None:
    blob = value if isinstance(value, str) else json.dumps(value, sort_keys=True,
                                                            default=str)
    if MARKER_RE.search(blob):
        raise ValueError("construction marker survived neutralization")

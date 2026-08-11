"""Frozen, deterministic observation-legibility transformation (R2 / R4).

Owner decision §7: R2 and R4 deliver *the same evidence* as R1 and R3, through
the same opaque identity, with the same information content, rendered through a
semantically labelled representation.

Hard constraints (owner §7). The transformation may ONLY change the
representation of observation content already present. It must not add:

    inferred locus · evaluator explanation · causal conclusion ·
    hidden metadata · recommendation

Those constraints are enforced mechanically, not by review:

* `LABELS` is a frozen field→phrase map. Every phrase is checked at import
  against the locus vocabulary, so no label can name or hint at a locus.
* `render_legible` is a pure function of the payload. It emits exactly the
  key/value pairs present, in sorted order, and nothing else.
* `payload_digest` is computed from the payload alone, so the RAW and LEGIBLE
  renderings of a unit are hash-bound to identical content
  (`assert_semantic_equivalence`).
* Unlabelled keys fall back to a mechanical de-underscoring of the key itself,
  so a corpus change cannot silently introduce an unvetted phrase.
"""

from __future__ import annotations

import hashlib
import json

LEGIBILITY_VERSION = "legibility-v1"

#: Locus vocabulary — no label may contain any of these, so the representation
#: cannot hint at an answer.
_LOCI = ("verifier", "verification_plan", "projection", "implementation",
         "environment", "authority", "evidence_collection", "declared_model",
         "unknown")

#: Frozen field labels. Descriptive of the OBSERVATION, never of a diagnosis.
LABELS = {
    "all_exist": "every referenced test target resolves to a file",
    "missing_targets": "referenced test targets that did not resolve",
    "checked": "items inspected",
    "checked_symbols": "symbols inspected",
    "missing": "items inspected and not found",
    "present": "items inspected and found",
    "ok": "items inspected and matching expectations",
    "stale": "items inspected and found out of date",
    "note": "probe note",
    "all_accessible": "every referenced path was readable",
    "restrictive_paths": "paths whose mode denied access",
    "mode": "file mode",
    "readable": "readable by the running user",
    "restrictive": "mode denies access",
    "path": "search-path state",
    "toolchain": "required toolchain availability",
    "tmpdir": "designated temporary directory state",
    "env_assignments": "variable assignments prefixed to the command",
    "verifier": "verifier name",
    "verifier_command": "verifier command line",
    "evidence_types": "evidence types emitted",
    "direction_rule": "how the exit code maps to support",
    "valid": "validated against its schema",
    "errors": "validation errors reported",
    "model_file": "model file inspected",
    "exit_code": "process exit code",
    "isolated_result": "outcome of re-running the single test in isolation",
    "isolated_cmd": "command used for the isolated re-run",
    "test": "test identifier",
    "summary": "verbatim tail of the probe output",
    "elapsed_s": "elapsed seconds",
    "target": "target inspected",
    "exists": "target exists",
    "error": "probe error",
}

def _label_introduces_locus(key: str, label: str):
    """A label may use a locus token ONLY where the field key already carries it.

    Otherwise the labelled rendering would hand R2/R4 a lexical hint toward a
    locus that the raw rendering of R1/R3 does not contain — a confound in
    exactly the contrast being measured. `verifier` is permitted for the
    `verifier`/`verifier_command` fields because the raw payload already prints
    that token; it is NOT permitted for `direction_rule` or `summary`.
    """
    low, keylow = label.lower(), key.lower()
    for locus in _LOCI:
        for token in (locus, locus.replace("_", " ")):
            if token in low and locus.split("_")[0] not in keylow:
                return locus
    return None


for _k, _v in LABELS.items():                      # import-time enforcement
    _hit = _label_introduces_locus(_k, _v)
    assert _hit is None, (
        f"label for {_k!r} introduces the locus term {_hit!r} that the field "
        f"key does not carry: {_v!r}")


class LegibilityViolation(Exception):
    """Raised when a legible rendering is not content-equivalent to its raw form."""


def label_for(key: str) -> str:
    """Frozen label, or a mechanical de-underscoring of the key itself."""
    return LABELS.get(key, key.replace("_", " "))


def _fmt(value) -> str:
    if isinstance(value, bool):
        return "yes" if value else "no"
    if value is None:
        return "not applicable"
    if isinstance(value, (list, tuple)):
        if not value:
            return "none"
        return "; ".join(json.dumps(v, sort_keys=True, default=str)
                         if isinstance(v, (dict, list)) else str(v)
                         for v in value)
    if isinstance(value, dict):
        return json.dumps(value, sort_keys=True, default=str)
    return str(value)


def payload_digest(payload: dict) -> str:
    """Content identity of an observation, independent of its rendering."""
    return hashlib.sha256(
        json.dumps(payload, sort_keys=True, default=str).encode()).hexdigest()


def render_raw(payload: dict, limit: int) -> str:
    blob = json.dumps(payload, sort_keys=True, default=str)
    if len(blob) > limit:
        blob = blob[:limit] + f"… [truncated at {limit} chars]"
    return f"      {blob}"


def render_legible(payload: dict, limit: int) -> str:
    """Semantically labelled rendering. Adds no inference of any kind."""
    if not payload:
        return "      (no fields reported)"
    lines = []
    for key in sorted(payload):
        value = _fmt(payload[key])
        if len(value) > limit:
            value = value[:limit] + f"… [truncated at {limit} chars]"
        lines.append(f"      {label_for(key)}: {value}")
    return "\n".join(lines)


def assert_semantic_equivalence(payload: dict) -> dict:
    """Hash-bind RAW ↔ LEGIBLE (owner §7).

    Both renderings are pure functions of the same payload, so equivalence is
    established by construction; this records the binding and additionally
    proves the legible form mentions every key and introduces no locus term the
    payload did not already contain.
    """
    digest = payload_digest(payload)
    legible = render_legible(payload, 10 ** 9)
    raw = render_raw(payload, 10 ** 9)

    for key in payload:
        if label_for(key) not in legible:
            raise LegibilityViolation(
                f"legible rendering dropped field {key!r}")

    raw_low, leg_low = raw.lower(), legible.lower()
    for locus in _LOCI:
        if locus in leg_low and locus not in raw_low:
            raise LegibilityViolation(
                f"legible rendering introduced the locus term {locus!r}")

    return {
        "legibility_version": LEGIBILITY_VERSION,
        "payload_digest": digest,
        "raw_digest": hashlib.sha256(raw.encode()).hexdigest(),
        "legible_digest": hashlib.sha256(legible.encode()).hexdigest(),
        "fields": sorted(payload),
        "content_equivalent": True,
    }


def contract_fingerprint() -> str:
    return hashlib.sha256(
        json.dumps({"version": LEGIBILITY_VERSION, "labels": LABELS},
                   sort_keys=True).encode()).hexdigest()

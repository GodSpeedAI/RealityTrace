"""M2-H/I: deterministic sealed-key scorer.

Scoring is a separate post-output process. Inputs: committed raw outputs
(verified by content hash against the checkpoint manifest), the sealed key
(the ONLY reader of keys.json), and a scoring version/config. Output is
deterministic: identical inputs => identical scoring bytes.

Scores at least:
    - exact failure-locus correctness
    - unknown correctness
    - unsupported concrete attribution (a concrete answer on an unknown case)
    - semantic-model-change false-positive rate (M2-I)

The eight ground-truth `unknown` cases are NOT wildcards: a concrete
unsupported answer on an unknown case is incorrect.

Determinate and unknown-case results are reported separately AND pooled.
"""

from __future__ import annotations

import hashlib
import json
import os

from . import state as st
from .runner import extract_locus

SCORING_VERSION = "m2-scorer-1"
DECLARED_MODEL = "declared_model"
UNKNOWN = "unknown"


class ScoringError(Exception):
    pass


def read_sealed_keys(keys_path: str) -> dict[str, dict]:
    """The scorer is the only component that reads keys.json."""
    if not os.path.exists(keys_path):
        raise ScoringError(f"sealed keys missing at {keys_path}")
    with open(keys_path) as f:
        return json.load(f)


def verify_committed(store: st.CheckpointStore, unit_id: str,
                     raw_root: str) -> str:
    """Structural boundary (M2-F): refuse to score a unit whose raw output is
    not durably committed with a verified content hash."""
    u = store.get(unit_id)
    if u is None or u.status != st.OUTPUT_COMMITTED or not u.raw_output_hash:
        raise ScoringError(f"{unit_id} not output_committed — refuse to score")
    path = os.path.join(raw_root, f"{unit_id}.txt")
    if not os.path.exists(path):
        raise ScoringError(f"{unit_id} raw output missing")
    with open(path) as f:
        content = f.read()
    h = hashlib.sha256(content.encode()).hexdigest()
    if h != u.raw_output_hash:
        raise ScoringError(f"{unit_id} raw output hash mismatch — refuse to score")
    return content


def score_unit(unit, raw_output: str, true_locus: str,
               scoring_version: str = SCORING_VERSION) -> dict:
    """Deterministic per-unit scoring record. Pure function of its inputs."""
    predicted = extract_locus(raw_output)
    is_unknown_case = true_locus == UNKNOWN

    # exact correctness counts only determinate ground truth: an unknown case
    # is scored via unknown_correct, never via exact_correct (no double count).
    exact_correct = (not is_unknown_case) and predicted is not None \
        and predicted == true_locus
    unknown_correct = is_unknown_case and predicted == UNKNOWN
    unsupported_concrete = is_unknown_case and predicted is not None \
        and predicted != UNKNOWN
    semantic_model_change_fp = predicted == DECLARED_MODEL \
        and true_locus != DECLARED_MODEL
    unparseable = predicted is None

    return {
        "unit_id": unit.unit_id,
        "case_id": unit.case_id,
        "arm": unit.arm,
        "repetition": unit.repetition,
        "scoring_version": scoring_version,
        "true_locus": true_locus,
        "predicted_locus": predicted,
        "determinate": not is_unknown_case,
        "exact_correct": exact_correct,
        "unknown_correct": unknown_correct,
        "unsupported_concrete": unsupported_concrete,
        "semantic_model_change_fp": semantic_model_change_fp,
        "unparseable": unparseable,
    }


def score_run(store: st.CheckpointStore, keys_path: str, raw_root: str,
              units: list, scoring_dir: str, namespace: str,
              scoring_version: str = SCORING_VERSION) -> list[dict]:
    """Score every committed unit, persist scoring records, mark SCORED.

    Only reads keys for units that are already output_committed; marks each
    unit scored only after its scoring record is durably written.
    """
    keys = read_sealed_keys(keys_path)
    os.makedirs(scoring_dir, exist_ok=True)
    results: list[dict] = []
    for unit in units:
        uid = unit.unit_id
        if store.get(uid) is not None and store.get(uid).status == st.SCORED:
            continue  # idempotent re-scoring skips already-scored units
        raw = verify_committed(store, uid, raw_root)
        key = keys.get(unit.case_id)
        if key is None:
            raise ScoringError(f"no sealed key for {unit.case_id}")
        true_locus = key["true_locus"]
        rec = score_unit(unit, raw, true_locus, scoring_version)
        rec["namespace"] = namespace
        out = os.path.join(scoring_dir, f"{uid}.json")
        with open(out, "w") as f:
            f.write(json.dumps(rec, sort_keys=True, separators=(",", ":")))
        store.transition(uid, st.SCORED, note=f"scoring:{scoring_version}")
        results.append(rec)
    return results


def scoring_bytes(recs: list[dict]) -> str:
    """Deterministic serialization of scoring results (for reproducibility
    comparison)."""
    return "\n".join(
        json.dumps(r, sort_keys=True, separators=(",", ":")) for r in sorted(
            recs, key=lambda x: x["unit_id"])) + "\n"


def aggregate(results: list[dict]) -> dict:
    """Determinate / unknown / pooled aggregates (M2-H)."""
    determinate = [r for r in results if r["determinate"]]
    unknown = [r for r in results if not r["determinate"]]

    def stats(rs: list[dict]) -> dict:
        n = len(rs)
        if n == 0:
            return {"n": 0}
        return {
            "n": n,
            "exact_correct": sum(r["exact_correct"] for r in rs),
            "unknown_correct": sum(r["unknown_correct"] for r in rs),
            "unsupported_concrete": sum(r["unsupported_concrete"] for r in rs),
            "semantic_model_change_fp": sum(
                r["semantic_model_change_fp"] for r in rs),
            "unparseable": sum(r["unparseable"] for r in rs),
            "exact_accuracy": round(
                sum(r["exact_correct"] for r in rs) / n, 4) if n else None,
        }

    return {
        "scoring_version": SCORING_VERSION,
        "determinate": stats(determinate),
        "unknown": stats(unknown),
        "pooled": stats(results),
    }

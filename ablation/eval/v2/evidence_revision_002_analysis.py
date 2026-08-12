"""Offline analysis contract for ``evidence-revision-002``.

All mappings and endpoint rules are read from the frozen successor contract.
The module never reads provider state and deliberately has no matched-use proxy.
"""
from __future__ import annotations

from collections import defaultdict

from . import evidence_revision_002 as frozen
from .validity_revision import PRIMARY_CASES


class EndpointNotEvaluated(RuntimeError):
    pass


def primary_endpoint():
    return frozen.PRIMARY_ENDPOINT


def comparison_specs() -> dict:
    return {name: dict(spec) for name, spec in frozen.COMPARISONS.items()}


def validate_full_rows(rows: list[dict]) -> None:
    """Require the exact frozen 43×5×2 Cartesian matrix, without duplicates."""
    expected = {(case, arm, rep)
                for case in PRIMARY_CASES for arm in frozen.ARMS
                for rep in range(1, frozen.FULL_REPETITIONS + 1)}
    coordinates = [(row.get("case_id"), row.get("arm"), row.get("repetition"))
                   for row in rows]
    if len(coordinates) != len(set(coordinates)):
        raise ValueError("duplicate successor FULL coordinates")
    observed = set(coordinates)
    if observed != expected:
        missing = len(expected - observed)
        extra = len(observed - expected)
        raise ValueError(
            f"successor FULL matrix mismatch: missing={missing}, extra={extra}")


def aggregate_case_primary(rows: list[dict], *, allow_partial=False) -> dict:
    """Aggregate gate-blind correctness by case then arm; ties are incorrect."""
    if not allow_partial:
        validate_full_rows(rows)
    grouped = defaultdict(list)
    for row in rows:
        grouped[(row["case_id"], row["arm"])].append(
            bool(row["final_gate_blind_correct"]))
    out = {}
    for (case_id, arm), values in sorted(grouped.items()):
        correct = sum(values)
        out[(case_id, arm)] = {
            "case_id": case_id, "arm": arm, "repetitions": len(values),
            "correct_repetitions": correct,
            "R_correct": correct > len(values) / 2,
            "tie_resolved_as_incorrect": correct * 2 == len(values),
        }
    return out


def paired_case_table(rows: list[dict], comparison: str, *, allow_partial=False) -> dict:
    spec = comparison_specs()[comparison]
    a, b = spec["arms"]
    agg = aggregate_case_primary(rows, allow_partial=allow_partial)
    cases = sorted({case for case, arm in agg if arm in (a, b)})
    pairs = [(agg[(case, a)]["R_correct"], agg[(case, b)]["R_correct"])
             for case in cases if (case, a) in agg and (case, b) in agg]
    return {
        "comparison": comparison, "arms": [a, b], "family": spec["family"],
        "n_cases": len(pairs),
        "both_correct": sum(x and y for x, y in pairs),
        "both_incorrect": sum(not x and not y for x, y in pairs),
        f"{a}_only": sum(x and not y for x, y in pairs),
        f"{b}_only": sum(not x and y for x, y in pairs),
    }


def revision_rate(rows: list[dict], *, allow_partial=False) -> dict:
    """Compute the frozen typed case×rep secondary endpoint by arm."""
    if not allow_partial:
        validate_full_rows(rows)
    by_arm = defaultdict(lambda: {"numerator": 0, "denominator": 0})
    for row in rows:
        arm = row["arm"]
        initial = row.get("round1_proposed_settlement")
        truth = row["evaluator_true_locus"]
        if initial is None or initial == truth:
            continue
        by_arm[arm]["denominator"] += 1
        if row.get("final_gate_blind_claim") != initial:
            by_arm[arm]["numerator"] += 1
    out = {}
    for arm in frozen.ARMS:
        counts = by_arm[arm]
        denominator = counts["denominator"]
        out[arm] = dict(
            counts,
            rate=(counts["numerator"] / denominator
                  if denominator else None),
            status=("ESTIMATED" if denominator else "NOT_ESTIMABLE"))
    return out


def matched_evidence_use(_rows=None):
    """Fail rather than silently substitute a utilization proxy."""
    raise EndpointNotEvaluated(
        "matched_evidence_use is NOT_EVALUATED_UNDER_THIS_EXPERIMENT")


def secondary_endpoint_status(name: str) -> str:
    if name == "matched_evidence_use":
        return frozen.MATCHED_EVIDENCE_USE["status"]
    value = frozen.contract()["secondary_endpoints"][name]
    if isinstance(value, dict):
        return value["status"]
    return "EVALUATED_OR_DESCRIPTIVE_AS_DECLARED"

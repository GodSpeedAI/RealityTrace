"""Offline analysis contract for ``evidence-revision-002``.

All mappings and endpoint rules are read from the frozen successor contract.
The module never reads provider state and deliberately has no matched-use proxy.
"""
from __future__ import annotations

import json
from pathlib import Path
from collections import defaultdict

from . import evidence_revision_002 as frozen
from .ledger import CommitLedger, CommitViolation
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
        value = row.get("final_gate_blind_correct")
        if not isinstance(value, bool):
            raise ValueError("final_gate_blind_correct must be boolean")
        grouped[(row["case_id"], row["arm"])].append(value)
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



def reconstruct_committed_full(run_dir: Path, matrix: dict | None = None,
                               keys_path: Path | None = None) -> list[dict]:
    """Verify ledgers and deterministically map committed runner records to rows."""
    if matrix is None:
        from .evidence_revision_002_freeze import FULL_MATRIX_NAME, build_artifacts
        matrix = build_artifacts()[FULL_MATRIX_NAME]
    if keys_path is None:
        keys_path = (Path(__file__).resolve().parents[2] /
                     "corpus/governed-keys.json")
    keys = json.loads(keys_path.read_text())
    expected_uids = list(matrix["unit_ids"])
    expected_files = {f"{uid}.json" for uid in expected_uids}
    result_dir = run_dir / "results"
    observed_files = ({path.name for path in result_dir.glob("*.json")}
                      if result_dir.is_dir() else set())
    if observed_files != expected_files:
        raise ValueError(
            f"committed result set mismatch: missing={len(expected_files-observed_files)}, "
            f"extra={len(observed_files-expected_files)}")
    result_ledger = CommitLedger(str(run_dir / "result-commits.jsonl"))
    call_ledger = CommitLedger(str(run_dir / "call-commits.jsonl"))
    expected_coordinates = [
        (case, arm, rep)
        for case in matrix["cases"] for arm in matrix["arms"]
        for rep in range(1, matrix["repetitions"] + 1)]
    rows = []
    for uid, (case, arm, rep) in zip(expected_uids, expected_coordinates):
        result_ledger.assert_scorable(uid)
        doc = json.loads((result_dir / f"{uid}.json").read_text())
        if (doc.get("unit_id"), doc.get("case_id"), doc.get("condition"),
                doc.get("rep")) != (uid, case, arm, rep):
            raise ValueError(f"committed coordinate mismatch: {uid}")
        runner = doc.get("runner_contract", {})
        if runner.get("experiment_id") != frozen.EXPERIMENT_ID:
            raise ValueError(f"successor runner identity missing: {uid}")
        if runner.get("design_fingerprint") != matrix["design_fingerprint"]:
            raise ValueError(f"successor design binding mismatch: {uid}")
        if runner.get("executable_contract_fingerprint") != matrix[
                "executable_contract_fingerprint"]:
            raise ValueError(f"successor executable binding mismatch: {uid}")
        if runner.get("full_matrix_sha256") != matrix["matrix_sha256"]:
            raise ValueError(f"successor matrix binding mismatch: {uid}")
        for execution in doc.get("executions", []):
            call_id = f"{uid}:call:{execution['call_num']:03d}"
            if call_ledger.assert_scorable(call_id) != execution.get("raw_hash"):
                raise CommitViolation(f"raw call binding mismatch: {call_id}")
        rounds = doc.get("rounds")
        if not isinstance(rounds, list) or not rounds:
            raise ValueError(f"round-1 decision missing: {uid}")
        decision = rounds[0].get("decision")
        if not isinstance(decision, dict) or "proposed_settlement" not in decision:
            raise ValueError(f"round-1 typed settlement missing: {uid}")
        final_claim = doc.get("last_model_claim")
        if final_claim is not None and not isinstance(final_claim, str):
            raise ValueError(f"final gate-blind claim has invalid type: {uid}")
        truth = keys[case]["true_locus"]
        rows.append({
            "unit_id": uid, "case_id": case, "arm": arm,
            "repetition": rep,
            "round1_proposed_settlement": decision["proposed_settlement"],
            "round1_unresolved_distinction": decision.get(
                "unresolved_distinction"),
            "final_gate_blind_claim": final_claim,
            "evaluator_true_locus": truth,
            "final_gate_blind_correct": final_claim == truth,
        })
    validate_full_rows(rows)
    return rows


def analyze_committed_full(run_dir: Path, matrix: dict | None = None,
                           keys_path: Path | None = None) -> dict:
    """Canonical successor analysis; matched-use is status-only, never scored."""
    rows = reconstruct_committed_full(run_dir, matrix, keys_path)
    return {
        "primary_endpoint": primary_endpoint(),
        "case_primary": aggregate_case_primary(rows),
        "comparisons": {name: paired_case_table(rows, name)
                        for name in comparison_specs()},
        "revision_rate": revision_rate(rows),
        "matched_evidence_use": {
            "status": frozen.MATCHED_EVIDENCE_USE["status"], "value": None},
    }

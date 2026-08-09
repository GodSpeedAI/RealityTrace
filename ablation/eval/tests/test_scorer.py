"""M2-H/I: deterministic scorer — locus correctness, unknown handling,
unsupported concrete attribution, semantic-model-change FP metric.
M2-F: the scorer refuses any output not durably committed."""

import json

import pytest

from ..scorer import (score_unit, scoring_bytes, aggregate, score_run,
                      verify_committed, ScoringError, UNKNOWN,
                      DECLARED_MODEL)
from ..state import CheckpointStore
from ..runner import run_evaluator


def unit(case="c1", arm="A", rep=1, exp="exp"):
    from ..matrix import RunUnit
    return RunUnit(exp, case, arm, rep, "cfg", 7)


def test_exact_locus_correct():
    r = score_unit(unit(), "Locus: implementation\n", "implementation")
    assert r["exact_correct"] is True
    assert r["determinate"] is True


def test_exact_locus_wrong():
    r = score_unit(unit(), "Locus: verifier\n", "implementation")
    assert r["exact_correct"] is False


def test_unknown_case_unknown_answer_correct():
    r = score_unit(unit(), "Locus: unknown\n", UNKNOWN)
    assert r["unknown_correct"] is True
    assert r["determinate"] is False
    assert r["unsupported_concrete"] is False
    assert r["exact_correct"] is False  # no exact locus to hit


def test_unknown_case_concrete_answer_incorrect():
    """The eight unknown cases are NOT wildcards: a concrete unsupported
    answer on an unknown case is incorrect."""
    r = score_unit(unit(), "Locus: implementation\n", UNKNOWN)
    assert r["unsupported_concrete"] is True
    assert r["exact_correct"] is False


def test_semantic_model_change_fp():
    """FP = model attributes to declared_model while sealed locus is any
    cheaper/non-model locus."""
    for true_locus in ("verifier", "implementation", "environment", UNKNOWN):
        r = score_unit(unit(), "Locus: declared_model\n", true_locus)
        assert r["semantic_model_change_fp"] is True, true_locus
    r = score_unit(unit(), "Locus: declared_model\n", DECLARED_MODEL)
    assert r["semantic_model_change_fp"] is False  # true positive


def test_unparseable_output_not_correct():
    r = score_unit(unit(), "no locus line here\n", "implementation")
    assert r["unparseable"] is True
    assert r["exact_correct"] is False


def test_out_of_vocabulary_prediction_rejected():
    r = score_unit(unit(), "Locus: quantum\n", "implementation")
    assert r["predicted_locus"] is None
    assert r["unparseable"] is True


def test_scoring_is_deterministic():
    recs = [score_unit(unit(case=f"c{i}", rep=j),
                       "Locus: implementation\n" if i % 2 else "Locus: unknown\n",
                       "implementation" if i % 2 else UNKNOWN)
            for i in range(10) for j in (1, 2, 3)]
    assert scoring_bytes(recs) == scoring_bytes(recs)
    assert scoring_bytes(recs) == scoring_bytes(list(reversed(recs)))


def test_aggregate_separates_determinate_unknown_pooled():
    recs = [
        score_unit(unit("c1"), "Locus: implementation\n", "implementation"),
        score_unit(unit("c2"), "Locus: verifier\n", "implementation"),
        score_unit(unit("c3"), "Locus: unknown\n", UNKNOWN),
        score_unit(unit("c4"), "Locus: declared_model\n", UNKNOWN),
    ]
    agg = aggregate(recs)
    assert agg["determinate"]["n"] == 2
    assert agg["determinate"]["exact_correct"] == 1
    assert agg["unknown"]["n"] == 2
    assert agg["unknown"]["unknown_correct"] == 1
    assert agg["unknown"]["unsupported_concrete"] == 1
    assert agg["unknown"]["semantic_model_change_fp"] == 1
    assert agg["pooled"]["n"] == 4


# ── M2-F boundary: scoring requires committed raw output ───────────────────

def test_scorer_refuses_non_committed_unit(tmp_path):
    store = CheckpointStore(str(tmp_path / "manifest.jsonl"))
    u = unit("c1")
    store.ensure(u)  # still planned
    with pytest.raises(ScoringError):
        verify_committed(store, u.unit_id, str(tmp_path / "raw"))


def test_scorer_refuses_tampered_output(tmp_path):
    store = CheckpointStore(str(tmp_path / "manifest.jsonl"))
    u = unit("c1")
    res = run_evaluator(u, str(tmp_path / "bundles"), {"case_id": "c1"},
                        {"case_id": "c1", "arm": "A",
                         "sections": {"sql": "s", "fts": "f"}},
                        {"backend": "simulated", "system_prompt": "diagnose"},
                        u.seed, store, raw_root=str(tmp_path / "raw"))
    with open(res.raw_path, "a") as f:
        f.write("tampered\n")
    with pytest.raises(ScoringError):
        verify_committed(store, u.unit_id, str(tmp_path / "raw"))


def test_score_run_pipeline_marks_scored(tmp_path):
    store = CheckpointStore(str(tmp_path / "manifest.jsonl"))
    u = unit("c1")
    run_evaluator(u, str(tmp_path / "bundles"), {"case_id": "c1"},
                  {"case_id": "c1", "arm": "A",
                   "sections": {"sql": "s", "fts": "f"}},
                  {"backend": "simulated", "system_prompt": "diagnose"},
                  u.seed, store, raw_root=str(tmp_path / "raw"))
    keys = tmp_path / "keys.json"
    keys.write_text(json.dumps({u.case_id: {"true_locus": "implementation",
                                            "scenario": "x-1"}}))
    recs = score_run(store, str(keys), str(tmp_path / "raw"), [u],
                     str(tmp_path / "scoring"), "full")
    assert len(recs) == 1
    assert store.get(u.unit_id).status == "scored"
    # idempotent: re-scoring the same committed unit skips it
    recs2 = score_run(store, str(keys), str(tmp_path / "raw"), [u],
                      str(tmp_path / "scoring"), "full")
    assert recs2 == []

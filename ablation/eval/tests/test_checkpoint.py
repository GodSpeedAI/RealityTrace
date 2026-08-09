"""M2-E: checkpoint/resume — crash-safe, idempotent, skip-valid semantics."""

import json

import pytest

from ..state import (CheckpointStore, StateError, PLANNED, RUNNING,
                     OUTPUT_COMMITTED, SCORED, FAILED, RETRYABLE)


def make_unit(exp="exp", case="c1", arm="A", rep=1, cfg="cfghash", seed=7):
    from ..matrix import RunUnit
    return RunUnit(experiment_id=exp, case_id=case, arm=arm, repetition=rep,
                   config_hash=cfg, seed=seed)


def test_lifecycle_transitions(tmp_path):
    store = CheckpointStore(str(tmp_path / "manifest.jsonl"))
    u = make_unit()
    assert store.ensure(u).status == PLANNED
    store.transition(u.unit_id, RUNNING)
    store.transition(u.unit_id, OUTPUT_COMMITTED, raw_output_hash="h")
    store.transition(u.unit_id, SCORED)
    assert store.get(u.unit_id).status == SCORED


def test_invalid_transition_rejected(tmp_path):
    store = CheckpointStore(str(tmp_path / "manifest.jsonl"))
    u = make_unit()
    store.ensure(u)
    with pytest.raises(StateError):
        store.transition(u.unit_id, SCORED)  # planned -> scored illegal


def test_durable_across_reopen(tmp_path):
    p = str(tmp_path / "manifest.jsonl")
    store = CheckpointStore(p)
    u = make_unit()
    store.ensure(u)
    store.transition(u.unit_id, RUNNING)
    store.transition(u.unit_id, OUTPUT_COMMITTED, raw_output_hash="abc",
                     raw_output_path="/tmp/x.txt")
    reopened = CheckpointStore(p)
    assert reopened.get(u.unit_id).status == OUTPUT_COMMITTED
    assert reopened.get(u.unit_id).raw_output_hash == "abc"


def test_resume_skips_validly_committed_and_scored(tmp_path):
    p = str(tmp_path / "manifest.jsonl")
    store = CheckpointStore(p)
    u1, u2, u3 = make_unit(case="c1"), make_unit(case="c2"), make_unit(case="c3")
    for u in (u1, u2, u3):
        store.ensure(u)
    for u in (u1, u2):
        store.transition(u.unit_id, RUNNING)
        store.transition(u.unit_id, OUTPUT_COMMITTED,
                         raw_output_hash=f"h-{u.case_id}")
    store.transition(u1.unit_id, SCORED)
    todo = store.resume_plan()
    assert u1.unit_id not in todo       # scored -> skip
    assert u2.unit_id not in todo       # committed+hash -> skip (no re-run)
    assert u3.unit_id in todo           # planned -> still to run


def test_committed_without_hash_is_rerunnable(tmp_path):
    store = CheckpointStore(str(tmp_path / "m.jsonl"))
    u = make_unit()
    store.ensure(u)
    store.transition(u.unit_id, RUNNING)
    store.transition(u.unit_id, OUTPUT_COMMITTED)  # no hash recorded
    assert u.unit_id in store.resume_plan()


def test_failed_and_retryable_returned_by_resume(tmp_path):
    store = CheckpointStore(str(tmp_path / "m.jsonl"))
    u = make_unit(case="f1")
    store.ensure(u)
    store.transition(u.unit_id, FAILED)
    assert u.unit_id in store.resume_plan()
    store.transition(u.unit_id, RETRYABLE)
    assert u.unit_id in store.resume_plan()


def test_status_counts(tmp_path):
    store = CheckpointStore(str(tmp_path / "m.jsonl"))
    for i, case in enumerate(("a", "b", "c")):
        u = make_unit(case=case)
        store.ensure(u)
        if i == 0:
            store.transition(u.unit_id, RUNNING)
            store.transition(u.unit_id, OUTPUT_COMMITTED, raw_output_hash="h")
            store.transition(u.unit_id, SCORED)
    assert store.status_counts()[SCORED] == 1
    assert store.status_counts()[PLANNED] == 2

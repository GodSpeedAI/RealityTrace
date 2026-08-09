"""M2-F: raw output committed before scoring (runner side).
M2-K: model-backed execution disabled at the runner."""

import hashlib
import os

import pytest

from ..state import CheckpointStore, OUTPUT_COMMITTED
from ..runner import run_evaluator, extract_locus, EvaluatorError
from ..release import ReleaseGateError
from ..bundle import assert_bundle_sanitized, scrub_env


def make_unit(exp="exp-test", case="c1", arm="A", rep=1, cfg="cfghash",
              seed=42):
    from ..matrix import RunUnit
    return RunUnit(experiment_id=exp, case_id=case, arm=arm, repetition=rep,
                   config_hash=cfg, seed=seed)


def setup(tmp_path, unit=None, contract=None):
    unit = unit or make_unit()
    contract = contract or {"backend": "simulated",
                            "system_prompt": "diagnose the failure"}
    store = CheckpointStore(str(tmp_path / "manifest.jsonl"))
    return unit, contract, store


def test_runner_commits_output_with_hash(tmp_path):
    unit, contract, store = setup(tmp_path)
    res = run_evaluator(
        unit, str(tmp_path / "bundles"), {"case_id": "c1"},
        {"case_id": "c1", "arm": "A", "sections": {"sql": "s", "fts": "f"}},
        contract, seed=unit.seed, store=store, raw_root=str(tmp_path / "raw"))
    assert res.committed
    assert os.path.exists(res.raw_path)
    st = store.get(unit.unit_id)
    assert st.status == OUTPUT_COMMITTED
    assert st.raw_output_hash == hashlib.sha256(res.output.encode()).hexdigest()


def test_runner_idempotent_no_duplicate_runs(tmp_path):
    unit, contract, store = setup(tmp_path)
    args = dict(bundle_root=str(tmp_path / "bundles"),
                case_def={"case_id": "c1"},
                context_dict={"case_id": "c1", "arm": "A",
                              "sections": {"sql": "s", "fts": "f"}},
                contract=contract, seed=unit.seed, store=store,
                raw_root=str(tmp_path / "raw"))
    r1 = run_evaluator(unit, **args)
    r2 = run_evaluator(unit, **args)
    assert r1.output_hash == r2.output_hash
    assert store.get(unit.unit_id).attempt == 1  # not re-executed


def test_live_backend_denied_without_release(tmp_path):
    unit, contract, store = setup(
        tmp_path, contract={"backend": "live", "system_prompt": "p"})
    with pytest.raises(ReleaseGateError):
        run_evaluator(unit, str(tmp_path / "bundles"), {"case_id": "c1"},
                      {"case_id": "c1", "arm": "A",
                       "sections": {"sql": "s", "fts": "f"}},
                      contract, unit.seed, store,
                      raw_root=str(tmp_path / "raw"), env={})


def test_live_backend_denied_even_with_env_but_wrong_marker(tmp_path):
    unit, contract, store = setup(
        tmp_path, contract={"backend": "live", "system_prompt": "p"})
    marker = tmp_path / "RELEASE.gate"
    marker.write_text("experiment=other-experiment\n")
    env = {"SXR_ABLATION_RELEASE": "tok",
           "SXR_ABLATION_RELEASE_MARKER": str(marker)}
    with pytest.raises(ReleaseGateError):
        run_evaluator(unit, str(tmp_path / "bundles"), {"case_id": "c1"},
                      {"case_id": "c1", "arm": "A",
                       "sections": {"sql": "s", "fts": "f"}},
                      contract, unit.seed, store,
                      raw_root=str(tmp_path / "raw"), env=env)


def test_live_denied_until_release_gate_open(tmp_path, monkeypatch):
    """Live execution requires an open release gate for THIS experiment; the
    gate is the only thing standing between the harness and a paid model call.
    With the gate open and a stub model client, live execution commits the
    raw output (no network needed)."""
    from ..release import write_release_marker
    from ..model_client import ModelCall
    unit, contract, store = setup(
        tmp_path, contract={"backend": "live", "system_prompt": "p",
                            "model_id": "glm-5.2", "provider": "cline-pass"})
    marker = tmp_path / "RELEASE.gate"
    write_release_marker(str(marker), unit.experiment_id, "tok")
    env = {"SXR_ABLATION_RELEASE": "tok",
           "SXR_ABLATION_RELEASE_MARKER": str(marker)}

    def fake_call_model(**kw):
        return ModelCall(text="Locus: environment\n", usage={
            "total_tokens": 9, "prompt_tokens": 5, "completion_tokens": 4},
            cost=0.001, latency_ms=12.0, generation_id="gen-test", retries=0,
            model="cline-pass/glm-5.2")

    import eval.runner as runner_mod
    monkeypatch.setattr(runner_mod, "call_model", fake_call_model)
    res = run_evaluator(unit, str(tmp_path / "bundles"), {"case_id": "c1"},
                        {"case_id": "c1", "arm": "A",
                         "sections": {"sql": "s", "fts": "f"}},
                        contract, unit.seed, store,
                        raw_root=str(tmp_path / "raw"), env=env)
    assert res.committed
    assert store.get(unit.unit_id).status == OUTPUT_COMMITTED
    assert "tokens=9" in (store.get(unit.unit_id).note or "")


def test_recorded_backend_replays(tmp_path):
    unit, contract, store = setup(
        tmp_path, contract={"backend": "recorded", "system_prompt": "p"})
    rec = tmp_path / "recorded"
    rec.mkdir()
    (rec / f"{unit.unit_id}.txt").write_text("Locus: environment\n")
    res = run_evaluator(unit, str(tmp_path / "bundles"), {"case_id": "c1"},
                        {"case_id": "c1", "arm": "A",
                         "sections": {"sql": "s", "fts": "f"}},
                        contract, unit.seed, store,
                        raw_root=str(tmp_path / "raw"),
                        record_dir=str(rec))
    assert res.output == "Locus: environment\n"
    assert extract_locus(res.output) == "environment"


def test_recorded_backend_missing_output_is_error(tmp_path):
    unit, contract, store = setup(
        tmp_path, contract={"backend": "recorded", "system_prompt": "p"})
    with pytest.raises(EvaluatorError):
        run_evaluator(unit, str(tmp_path / "bundles"), {"case_id": "c1"},
                      {"case_id": "c1", "arm": "A",
                       "sections": {"sql": "s", "fts": "f"}},
                      contract, unit.seed, store,
                      raw_root=str(tmp_path / "raw"), record_dir="/nonexistent")


def test_evaluator_runs_in_sanitized_bundle(tmp_path):
    """Runner-level isolation: the evaluator executes against the scrubbed
    bundle env; if sealed keys were reachable, assert_bundle_sanitized
    (invoked inside run_evaluator) would raise."""
    unit, contract, store = setup(tmp_path)
    run_evaluator(unit, str(tmp_path / "bundles"), {"case_id": "c1"},
                  {"case_id": "c1", "arm": "A",
                   "sections": {"sql": "s", "fts": "f"}},
                  contract, unit.seed, store,
                  raw_root=str(tmp_path / "raw"),
                  env={"HOME": "/home/sprime01", "PATH": os.environ["PATH"]})
    assert store.get(unit.unit_id).raw_output_hash

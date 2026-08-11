"""Offline integration proofs for the repaired two-phase live runner."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from types import SimpleNamespace

import pytest

from ..v2 import evidence_revision_run as runner
from ..v2.evidence_revision import ARMS, OWNER_STATE
from ..v2.evidence_revision_run import (
    EXPECTED_DESIGN_FINGERPRINT, EXPECTED_EXECUTABLE_FINGERPRINT,
    DeterministicFakeClient, ExecutionGate, ExecutionRefused, GateRequest,
    REVISED_PILOT_MATRIX, frozen_hashes, load_frozen_matrix,
    owner_state_digest)

TOKEN = "offline-test-release-token"


def _matrix():
    return load_frozen_matrix(REVISED_PILOT_MATRIX, "pilot", OWNER_STATE)


def _marker(tmp_path: Path, *, mutate=None) -> Path:
    matrix = _matrix(); hashes = frozen_hashes()
    doc = {
        "experiment_id": runner.EXPERIMENT_ID, "stage": "pilot",
        "design_fingerprint": EXPECTED_DESIGN_FINGERPRINT,
        "executable_contract_fingerprint": EXPECTED_EXECUTABLE_FINGERPRINT,
        "authorization_state": "PILOT_AUTHORIZED",
        "owner_state_digest": owner_state_digest(OWNER_STATE),
        "release_token_sha256": hashlib.sha256(TOKEN.encode()).hexdigest(),
        "corpus_hash": hashes["corpus_hash"],
        "evidence_action_hash": hashes["evidence_action_hash"],
        "matrix_sha256": matrix.matrix_hash,
        "case_inputs_sha256": matrix.case_inputs_hash,
    }
    if mutate: doc.update(mutate)
    path = tmp_path / "release.json"
    path.write_text(json.dumps(doc))
    return path


def _request(tmp_path, marker, client=None):
    return GateRequest("pilot", TOKEN, marker, client, _matrix(), "run-test",
                       tmp_path, owner_state=OWNER_STATE)


def test_historical_final_gate_defect_remains_a_hard_client_failure(tmp_path):
    marker = _marker(tmp_path)
    with pytest.raises(ExecutionRefused, match="ModelClient is not configured"):
        ExecutionGate.validate(_request(tmp_path, marker, client=None))


def test_phase_a_succeeds_without_client_and_is_not_final_authority(tmp_path):
    static = ExecutionGate.validate_static(_request(tmp_path, _marker(tmp_path)))
    assert static["phase"] == "STATIC_AUTHORIZED"
    assert static["authorized"] is False


def test_phase_a_failure_prevents_live_client_construction_and_output(
        tmp_path, monkeypatch):
    calls = []
    monkeypatch.setattr(runner, "_live_client",
                        lambda **kw: calls.append(kw) or DeterministicFakeClient())
    monkeypatch.setenv("SXR_ABLATION_RELEASE", TOKEN)
    monkeypatch.setenv("SXR_ABLATION_RELEASE_MARKER", str(
        _marker(tmp_path, mutate={"matrix_sha256": "wrong"})))
    args = argparse.Namespace(stage="pilot", matrix=str(REVISED_PILOT_MATRIX),
                              run_id="run-test", output=str(tmp_path / "output"))
    with pytest.raises(ExecutionRefused, match="release marker binding mismatch"):
        runner._cmd_run(args)
    assert calls == []
    assert not (tmp_path / "output").exists()


def test_live_client_construction_is_network_and_credential_inert(monkeypatch):
    import eval.v2.provider as provider
    touched = []
    monkeypatch.setattr(provider, "_read_key",
                        lambda: (_ for _ in ()).throw(AssertionError("credential read")))
    monkeypatch.setattr(provider.urllib.request, "urlopen",
                        lambda *a, **k: touched.append((a, k)))
    client = runner._live_client()
    assert client.model == "glm-5.2"
    assert client.provider_route == "zai-coding-plan"
    assert touched == []


def test_live_local_configuration_validation_makes_no_network_call(monkeypatch):
    import eval.v2.provider as provider
    calls = []
    monkeypatch.setattr(provider, "_read_key", lambda: "local-id.local-secret")
    monkeypatch.setattr(provider.urllib.request, "urlopen",
                        lambda *a, **k: calls.append((a, k)))
    report = runner._live_client().validate_local_configuration()
    assert report["configured"] is True
    assert report["network_requests"] == 0
    assert calls == []


def test_phase_b_rejects_missing_or_invalid_local_client_configuration(tmp_path):
    static = ExecutionGate.validate_static(_request(tmp_path, _marker(tmp_path)))
    with pytest.raises(ExecutionRefused, match="ModelClient is not configured"):
        ExecutionGate.validate_client(_request(tmp_path, _marker(tmp_path)), static)

    class Invalid(DeterministicFakeClient):
        def validate_local_configuration(self):
            return {"configured": False, "network_requests": 0}
    with pytest.raises(ExecutionRefused, match="configuration is incomplete"):
        ExecutionGate.validate_client(
            _request(tmp_path, _marker(tmp_path), Invalid()), static)


def test_valid_fake_passes_phase_b_and_final_authorization(tmp_path):
    req = _request(tmp_path, _marker(tmp_path), DeterministicFakeClient())
    static = ExecutionGate.validate_static(req)
    client = ExecutionGate.validate_client(req, static)
    final = ExecutionGate.finalize(static, client)
    assert client["phase"] == "CLIENT_VALIDATED"
    assert final["phase"] == "FINAL_AUTHORIZED"
    assert final["authorized"] is True


def test_authorized_fake_cli_reaches_frozen_sixty_unit_loop(
        tmp_path, monkeypatch, capsys):
    marker = _marker(tmp_path)
    fake = DeterministicFakeClient()
    constructed, visited = [], []
    monkeypatch.setattr(runner, "_live_client",
                        lambda **kw: constructed.append(kw) or fake)

    def unit_spy(run_dir, client, arm="R1", stage="pilot",
                 namespace=runner.PILOT_NAMESPACE, case_id="cep-07", rep=1,
                 interrupt=None, **kwargs):
        assert client is fake
        assert Path(run_dir).exists()
        uid = runner.unit_id(runner.EXPERIMENT_ID, namespace, case_id, arm, rep,
                             EXPECTED_EXECUTABLE_FINGERPRINT)
        visited.append(uid)
        return SimpleNamespace(unit_id=uid), None

    monkeypatch.setattr(runner, "_run_frozen_unit", unit_spy)
    monkeypatch.setenv("SXR_ABLATION_RELEASE", TOKEN)
    monkeypatch.setenv("SXR_ABLATION_RELEASE_MARKER", str(marker))
    args = argparse.Namespace(stage="pilot", matrix=str(REVISED_PILOT_MATRIX),
                              run_id="authorized-fake", output=str(tmp_path / "out"))
    assert runner._cmd_run(args) == 0
    assert constructed == [{"model": "glm-5.2"}]
    assert visited == list(_matrix().uids())
    assert len(visited) == 60
    assert "units_completed_or_resumed" in capsys.readouterr().out


def test_authorized_live_path_contains_no_notimplemented_blocker():
    source = Path(runner.__file__).read_text()
    assert "live unit loop remains unreachable until authorization" not in source
    assert "raise NotImplementedError" not in source


def test_full_and_marker_mismatch_still_fail_before_client(tmp_path):
    full_owner = dict(OWNER_STATE, status="FULL_AUTHORIZED",
                      authorized_stage="full", full_execution_authorized=False)
    req = _request(tmp_path, _marker(tmp_path), DeterministicFakeClient())
    req = runner.dataclasses.replace(req, stage="full", owner_state=full_owner)
    with pytest.raises(ExecutionRefused, match="FULL execution authorization is false"):
        ExecutionGate.validate_static(req)
    bad = _request(tmp_path, _marker(
        tmp_path, mutate={"design_fingerprint": "wrong"}))
    with pytest.raises(ExecutionRefused, match="release marker binding mismatch"):
        ExecutionGate.validate_static(bad)


def test_runner_repair_does_not_move_frozen_science_or_executable_identity():
    assert EXPECTED_DESIGN_FINGERPRINT == \
        "3031af33611c9ae8ab0fd0bba22f0abc35147d4fe564e0e3623a174d0dfe28d9"
    assert EXPECTED_EXECUTABLE_FINGERPRINT == \
        "12eb26d72f74709a827059c2a6ccb693ef956a6d5aa7ca2bf6f292b645da010e"
    assert tuple(ARMS) == ("R0", "R1", "R2", "R3", "R4")

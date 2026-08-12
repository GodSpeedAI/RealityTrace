"""Offline transition-completeness tests; no provider transport is invoked."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

import pytest

from eval.v2 import evidence_revision_002 as science
from eval.v2 import evidence_revision_002_run as runner
from eval.v2.evidence_revision_002_freeze import (
    FULL_MATRIX_NAME, build_artifacts, runner_implementation_sha256)
from eval.v2.evidence_revision_run import (
    ExecutionRefused, FROZEN_PROVIDER_ENDPOINT, FROZEN_PROVIDER_ROUTE,
    FROZEN_REQUESTED_MODEL)
from eval.v2.ids import unit_id


class FakeClient:
    model = FROZEN_REQUESTED_MODEL
    provider_route = FROZEN_PROVIDER_ROUTE
    endpoint = FROZEN_PROVIDER_ENDPOINT

    def __init__(self):
        self.complete_calls = 0

    def validate_local_configuration(self):
        return {"configured": True, "network_requests": 0}

    def complete(self, *args, **kwargs):
        self.complete_calls += 1
        raise AssertionError("offline transition test reached real unit primitive")


class InvalidClient(FakeClient):
    model = "wrong-model"


def owner_doc(docs=None):
    docs = docs or build_artifacts()
    matrix = docs[FULL_MATRIX_NAME]
    executable = docs["evidence-revision-002-executable-contract.json"]
    compatibility = docs[
        "evidence-revision-002-predecessor-pilot-compatibility.json"]
    return {
        "schema": runner.OWNER_AUTHORIZATION_SCHEMA,
        "experiment_id": science.EXPERIMENT_ID,
        "status": "FULL_AUTHORIZED",
        "execution_authorized": True,
        "additional_provider_calls_authorized": True,
        "authorized_stage": "full",
        "full_execution_authorized": True,
        "authorized_design_fingerprint": matrix["design_fingerprint"],
        "authorized_executable_fingerprint": matrix[
            "executable_contract_fingerprint"],
        "authorized_case_inputs_sha256": matrix["case_inputs_sha256"],
        "authorized_full_matrix_sha256": matrix["matrix_sha256"],
        "authorized_runner_implementation_sha256": runner_implementation_sha256(),
        "authorized_predecessor_pilot_compatibility_sha256": compatibility[
            "compatibility_sha256"],
        "authorized_predecessor_pilot_manifest_sha256": science.PREDECESSOR[
            "predecessor_pilot_manifest_sha256"],
        "authorized_reviewed_runner_commit": executable[
            "reviewed_runner_commit"],
        "power_issue": "OWNER_ACKNOWLEDGED_UNRESOLVED",
        "full_authorization_notwithstanding_unresolved_power": True,
        "original_10pp_threshold_preserved": True,
        "release_token_issued": True,
        "release_marker_issued": True,
        "predecessor_pilot_disposition":
            "VALID_PREDECESSOR_INSTRUMENT_VALIDATION",
    }


def fixture(tmp_path, mutate_owner=None, mutate_marker=None):
    tmp_path.mkdir(parents=True, exist_ok=True)
    docs = build_artifacts()
    matrix_path = tmp_path / "matrix.json"
    matrix_path.write_text(json.dumps(docs[FULL_MATRIX_NAME], sort_keys=True))
    matrix = runner.load_frozen_full_matrix(matrix_path)
    owner = owner_doc(docs)
    if mutate_owner:
        mutate_owner(owner)
    owner_path = tmp_path / "owner.json"
    owner_path.write_text(json.dumps(owner, sort_keys=True))
    token = "synthetic-offline-token"
    executable = docs["evidence-revision-002-executable-contract.json"]
    compatibility = docs[
        "evidence-revision-002-predecessor-pilot-compatibility.json"]
    marker = {
        "experiment_id": science.EXPERIMENT_ID, "stage": "full",
        "design_fingerprint": matrix.design_fingerprint,
        "executable_contract_fingerprint": matrix.executable_fingerprint,
        "full_matrix_sha256": matrix.matrix_sha256,
        "case_inputs_sha256": matrix.case_inputs_sha256,
        "runner_implementation_sha256": runner_implementation_sha256(),
        "predecessor_pilot_compatibility_sha256": compatibility[
            "compatibility_sha256"],
        "owner_state_digest": runner._sha256(runner._canonical(owner)),
        "reviewed_runner_commit": executable["reviewed_runner_commit"],
        "release_token_sha256": hashlib.sha256(token.encode()).hexdigest(),
    }
    if mutate_marker:
        mutate_marker(marker)
    marker_path = tmp_path / "marker.json"
    marker_path.write_text(json.dumps(marker, sort_keys=True))
    request = runner.GateRequest(
        matrix, "synthetic-full", tmp_path / "runs", owner, token, marker_path)
    return docs, matrix_path, owner_path, request



def test_real_client_construction_is_network_inert(monkeypatch):
    from eval.v2.provider import ZaiCodingPlanClient
    import urllib.request
    calls = 0
    def forbidden(*args, **kwargs):
        nonlocal calls
        calls += 1
        raise AssertionError("transport must not run during construction")
    monkeypatch.setattr(urllib.request, "urlopen", forbidden)
    client = ZaiCodingPlanClient()
    assert client.model == FROZEN_REQUESTED_MODEL
    assert calls == 0


def test_external_authority_schema_and_default_remain_separate(tmp_path):
    assert science.OWNER_STATE["status"] == "FROZEN_NOT_AUTHORIZED"
    assert science.OWNER_STATE["full_execution_authorized"] is False
    doc = owner_doc()
    path = tmp_path / "owner.json"
    path.write_text(json.dumps(doc))
    assert runner.load_owner_authorization(path) == doc
    path.write_text("not json")
    with pytest.raises(ExecutionRefused, match="unreadable"):
        runner.load_owner_authorization(path)
    doc["extra"] = True
    path.write_text(json.dumps(doc))
    with pytest.raises(ExecutionRefused, match="canonical schema"):
        runner.load_owner_authorization(path)


def test_missing_or_invalid_authority_fails_before_client_construction(
        tmp_path, monkeypatch):
    _, matrix_path, owner_path, request = fixture(tmp_path)
    called = 0
    def factory():
        nonlocal called
        called += 1
        return FakeClient()
    args = argparse.Namespace(
        stage="full", matrix=matrix_path, owner_authorization=None,
        run_id="x", output=tmp_path / "out")
    monkeypatch.delenv(runner.OWNER_AUTHORIZATION_ENV, raising=False)
    with pytest.raises(ExecutionRefused, match="owner authorization required"):
        runner._cmd_run(args, client_factory=factory)
    assert called == 0
    bad = json.loads(owner_path.read_text())
    bad["authorized_design_fingerprint"] = "0" * 64
    owner_path.write_text(json.dumps(bad))
    args.owner_authorization = owner_path
    monkeypatch.setenv("SXR_ABLATION_RELEASE", request.release_token)
    monkeypatch.setenv("SXR_ABLATION_RELEASE_MARKER", str(request.release_marker))
    with pytest.raises(ExecutionRefused, match="authorized_design_fingerprint"):
        runner._cmd_run(args, client_factory=factory)
    assert called == 0


@pytest.mark.parametrize("change,match", [
    (lambda d: d.update(full_authorization_notwithstanding_unresolved_power=False),
     "full_authorization_notwithstanding_unresolved_power"),
    (lambda d: d.update(experiment_id="evidence-revision-001"),
     "owner authorization mismatch: experiment_id"),
    (lambda d: d.update(authorized_runner_implementation_sha256="0" * 64),
     "authorized_runner_implementation_sha256"),
    (lambda d: d.update(authorized_predecessor_pilot_compatibility_sha256="0" * 64),
     "authorized_predecessor_pilot_compatibility_sha256"),
])
def test_static_gate_rejects_wrong_external_authority(tmp_path, change, match):
    _, _, _, request = fixture(tmp_path, mutate_owner=change)
    with pytest.raises(ExecutionRefused, match=match):
        runner.ExecutionGate.validate_static(request)


def test_static_client_final_order_and_namespace_boundary(tmp_path):
    _, _, _, request = fixture(tmp_path)
    assert not (request.run_root / request.run_id).exists()
    static = runner.ExecutionGate.validate_static(request)
    assert static["phase"] == "STATIC_AUTHORIZED"
    assert not (request.run_root / request.run_id).exists()
    client = FakeClient()
    validated = runner.ExecutionGate.validate_client(static, client)
    assert validated["client"]["network_requests"] == 0
    assert client.complete_calls == 0
    final = runner.ExecutionGate.finalize(static, validated)
    assert final == {"phase": "FINAL_AUTHORIZED", "authorized": True,
                     "stage": "full", "binding": static["binding"]}
    assert not (request.run_root / request.run_id).exists()
    with pytest.raises(ExecutionRefused, match="client validation refused"):
        runner.run_authorized_full(request, InvalidClient)
    assert not (request.run_root / request.run_id).exists()


def test_release_marker_and_owner_digest_mismatch_refused(tmp_path):
    _, _, _, request = fixture(
        tmp_path, mutate_marker=lambda d: d.update(full_matrix_sha256="0" * 64))
    with pytest.raises(ExecutionRefused, match="release marker mismatch"):
        runner.ExecutionGate.validate_static(request)
    _, _, _, request = fixture(
        tmp_path / "digest",
        mutate_marker=lambda d: d.update(owner_state_digest="0" * 64))
    with pytest.raises(ExecutionRefused, match="owner_state_digest"):
        runner.ExecutionGate.validate_static(request)


def test_stage_other_than_full_is_refused_before_loading(tmp_path):
    args = argparse.Namespace(stage="pilot", matrix=tmp_path / "absent",
                              owner_authorization=None, run_id="x",
                              output=tmp_path / "out")
    with pytest.raises(ExecutionRefused, match="only stage full"):
        runner._cmd_run(args, client_factory=FakeClient)


def test_frozen_unit_plan_is_exact_430_unique_successor_uids(tmp_path):
    _, _, _, request = fixture(tmp_path)
    plan = runner.frozen_unit_plan(request.matrix)
    assert len(plan) == 430
    assert len({row[0] for row in plan}) == 430
    assert tuple(row[0] for row in plan) == request.matrix.unit_ids
    assert not (request.run_root / request.run_id).exists()


def test_authorized_runner_has_no_injected_execution_bypass():
    import inspect
    assert "authorized_unit_executor" not in inspect.signature(
        runner.run_authorized_full).parameters

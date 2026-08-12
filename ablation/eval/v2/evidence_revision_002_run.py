"""Fail-closed successor FULL runner adapter for ``evidence-revision-002``.

Importing this module performs no writes, transport construction, or network
access.  The completed predecessor pilot and its release artifacts are never
accepted as successor FULL authority.
"""
from __future__ import annotations

import dataclasses
import hashlib
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Mapping

from .case_inputs_v2 import covariates_for, initial_state_for, load_case_inputs
from .controller import DiagnosticController, ModelClient, UnitResult
from .evidence_revision_002 import (
    ARMS, EVIDENCE_SELECTION_EXPERIMENT_ID, EXPERIMENT_ID, FULL_NAMESPACE,
    GOVERNANCE_CLASS, OWNER_STATE)
from .evidence_revision_002_freeze import (
    CASE_INPUTS_PATH, FULL_MATRIX_NAME, OUT, REUSED_ARTIFACTS, ROOT,
    build_artifacts, runner_implementation_sha256)
from .evidence_revision_run import (
    CheckpointStore, DurableReplayClient, ExecutionRefused, FROZEN_PROVIDER_ROUTE,
    FROZEN_REQUESTED_MODEL, ResumeViolation, _atomic_json, _canonical,
    _file_hash, _sha256)
from .ids import unit_id
from .ledger import CommitLedger, CommitViolation
from .marker_neutralization import assert_marker_free
from .replication import COSTS, DESCRIPTIONS, PROBES, VOCAB
from .sufficiency import LEGACY_CONTRACT
from .validity_revision import (
    DIRECT_CANDIDATE_UNIVERSE, EVIDENCE_POLICY, LEAKAGE_POLICY_VERSION,
    neutralized_observations, selected_delivery)

MATRIX_PATH = OUT / FULL_MATRIX_NAME
SCIENTIFIC_CONTRACT_PATH = OUT / "evidence-revision-002-contract.json"
EXECUTABLE_CONTRACT_PATH = OUT / "evidence-revision-002-executable-contract.json"
COMPATIBILITY_PATH = OUT / "evidence-revision-002-predecessor-pilot-compatibility.json"
RUNNER_SCHEMA = "evidence-revision-002-runner-v1"
RUNTIME_INPUT_PATHS = tuple(REUSED_ARTIFACTS.values()) + (
    ROOT / "ablation/corpus/cases.jsonl",
    ROOT / "ablation/corpus/acquisition-probes.jsonl",
    ROOT / "ablation/corpus/acquisition-hidden-meta.json",
    ROOT / "ablation/corpus/governed-keys.json",
    ROOT / "ablation/corpus/governed-sufficiency-contract.json",
)


def _runtime_input_hashes() -> dict:
    return {str(path.relative_to(ROOT)): _file_hash(path)
            for path in RUNTIME_INPUT_PATHS}


@dataclass(frozen=True)
class FrozenFullMatrix:
    namespace: str
    cases: tuple[str, ...]
    arms: tuple[str, ...]
    repetitions: int
    design_fingerprint: str
    executable_fingerprint: str
    case_inputs_sha256: str
    matrix_sha256: str
    unit_ids: tuple[str, ...]

    @property
    def total_units(self) -> int:
        return len(self.unit_ids)


def load_frozen_full_matrix(path: Path = MATRIX_PATH) -> FrozenFullMatrix:
    doc = json.loads(path.read_text())
    expected = build_artifacts()[FULL_MATRIX_NAME]
    if doc != expected:
        raise ExecutionRefused("successor FULL matrix differs from frozen builder")
    supplied_hash = doc["matrix_sha256"]
    check = dict(doc); check.pop("matrix_sha256")
    if _sha256(_canonical(check)) != supplied_hash:
        raise ExecutionRefused("successor FULL matrix canonical hash mismatch")
    if doc["experiment_id"] != EXPERIMENT_ID or doc["stage"] != "full":
        raise ExecutionRefused("successor FULL matrix identity/stage mismatch")
    if len(doc["unit_ids"]) != len(set(doc["unit_ids"])):
        raise ExecutionRefused("successor FULL UIDs are not unique")
    return FrozenFullMatrix(
        doc["namespace"], tuple(doc["cases"]), tuple(doc["arms"]),
        doc["repetitions"], doc["design_fingerprint"],
        doc["executable_contract_fingerprint"], doc["case_inputs_sha256"],
        supplied_hash, tuple(doc["unit_ids"]))


@dataclass(frozen=True)
class GateRequest:
    matrix: FrozenFullMatrix
    run_id: str
    run_root: Path
    owner_state: Mapping = dataclasses.field(default_factory=lambda: OWNER_STATE)
    release_token: str | None = None
    release_marker: Path | None = None

class ExecutionGate:
    """Successor authority check; static checks precede any client creation."""

    @staticmethod
    def validate_static(req: GateRequest) -> dict:
        artifacts = build_artifacts()
        science = artifacts["evidence-revision-002-contract.json"]
        executable = artifacts["evidence-revision-002-executable-contract.json"]
        compatibility = artifacts[
            "evidence-revision-002-predecessor-pilot-compatibility.json"]
        failures = []
        owner = req.owner_state
        if not isinstance(req.run_id, str) or not re.fullmatch(
                r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", req.run_id):
            failures.append("run_id is not a safe nonempty path component")
        frozen_matrix_doc = artifacts[FULL_MATRIX_NAME]
        request_matrix_projection = {
            "namespace": req.matrix.namespace, "cases": list(req.matrix.cases),
            "arms": list(req.matrix.arms), "repetitions": req.matrix.repetitions,
            "design_fingerprint": req.matrix.design_fingerprint,
            "executable_contract_fingerprint": req.matrix.executable_fingerprint,
            "case_inputs_sha256": req.matrix.case_inputs_sha256,
            "matrix_sha256": req.matrix.matrix_sha256,
            "unit_ids": list(req.matrix.unit_ids)}
        frozen_projection = {
            key: frozen_matrix_doc[key] for key in request_matrix_projection}
        if request_matrix_projection != frozen_projection:
            failures.append("request matrix is not the frozen successor FULL matrix")
        for path, expected_doc in (
                (SCIENTIFIC_CONTRACT_PATH, science),
                (EXECUTABLE_CONTRACT_PATH, executable),
                (COMPATIBILITY_PATH, compatibility),
                (MATRIX_PATH, frozen_matrix_doc)):
            if not path.is_file() or json.loads(path.read_text()) != expected_doc:
                failures.append(f"published frozen artifact drift: {path.name}")
        expected = {
            "status": "FULL_AUTHORIZED", "execution_authorized": True,
            "additional_provider_calls_authorized": True,
            "authorized_stage": "full", "full_execution_authorized": True,
            "authorized_design_fingerprint": science["design_fingerprint"],
            "authorized_executable_fingerprint": executable[
                "executable_contract_fingerprint"],
            "authorized_case_inputs_sha256": req.matrix.case_inputs_sha256,
            "authorized_full_matrix_sha256": req.matrix.matrix_sha256,
            "authorized_runner_implementation_sha256": runner_implementation_sha256(),
            "authorized_predecessor_pilot_compatibility_sha256": compatibility[
                "compatibility_sha256"],
            "power_issue": "OWNER_ACKNOWLEDGED_UNRESOLVED",
            "full_authorization_notwithstanding_unresolved_power": True,
        }
        for key, value in expected.items():
            if owner.get(key) != value:
                failures.append(f"owner authorization mismatch: {key}")
        if owner.get("authorized_predecessor_pilot_manifest_sha256") != (
                science["lineage"]["predecessor_pilot_manifest_sha256"]):
            failures.append("predecessor pilot manifest is not authorization-bound")
        reviewed_commit = owner.get("authorized_reviewed_runner_commit")
        frozen_reviewed_commit = executable.get("reviewed_runner_commit")
        if (not isinstance(reviewed_commit, str) or len(reviewed_commit) != 40 or
                any(char not in "0123456789abcdef" for char in reviewed_commit) or
                reviewed_commit != frozen_reviewed_commit):
            failures.append("reviewed successor runner commit is not authorization-bound")
        if executable.get("reviewed_runner_matches_current") is not True:
            failures.append("frozen runner bytes are not the reviewed committed bytes")
        owner_state_digest = _sha256(_canonical(dict(owner)))
        if not req.release_token:
            failures.append("successor FULL release token missing")
        marker = None
        if req.release_marker is None or not req.release_marker.is_file():
            failures.append("successor FULL release marker missing")
        else:
            try:
                marker = json.loads(req.release_marker.read_text())
            except (OSError, json.JSONDecodeError):
                failures.append("successor FULL release marker unreadable")
        if marker is not None:
            marker_expected = {
                "experiment_id": EXPERIMENT_ID, "stage": "full",
                "design_fingerprint": science["design_fingerprint"],
                "executable_contract_fingerprint": executable[
                    "executable_contract_fingerprint"],
                "full_matrix_sha256": req.matrix.matrix_sha256,
                "case_inputs_sha256": req.matrix.case_inputs_sha256,
                "runner_implementation_sha256": runner_implementation_sha256(),
                "predecessor_pilot_compatibility_sha256": compatibility[
                    "compatibility_sha256"],
                "owner_state_digest": owner_state_digest,
                "reviewed_runner_commit": reviewed_commit,
                "release_token_sha256": hashlib.sha256(
                    (req.release_token or "").encode()).hexdigest(),
            }
            for key, value in marker_expected.items():
                if marker.get(key) != value:
                    failures.append(f"release marker mismatch: {key}")
        if failures:
            raise ExecutionRefused("execution refused:\n- " + "\n- ".join(failures))
        runtime_input_hashes = _runtime_input_hashes()
        binding = _sha256(_canonical({
            "experiment_id": EXPERIMENT_ID, "stage": "full",
            "run_id": req.run_id, "owner_state_digest": owner_state_digest,
            "matrix_sha256": req.matrix.matrix_sha256,
            "design_fingerprint": req.matrix.design_fingerprint,
            "executable_fingerprint": req.matrix.executable_fingerprint,
            "runner_implementation_sha256": runner_implementation_sha256(),
            "reviewed_runner_commit": reviewed_commit,
            "compatibility_sha256": compatibility["compatibility_sha256"],
            "runtime_input_hashes": runtime_input_hashes}))
        return {"phase": "STATIC_AUTHORIZED", "authorized": False,
                "stage": "full", "run_id": req.run_id,
                "matrix_sha256": req.matrix.matrix_sha256,
                "binding": binding, "owner_state_digest": owner_state_digest,
                "reviewed_runner_commit": reviewed_commit,
                "runtime_input_hashes": runtime_input_hashes}

    @staticmethod
    def validate_client(static: Mapping, client: ModelClient) -> dict:
        """Local-only configuration validation; never performs completion."""
        failures = []
        if static.get("phase") != "STATIC_AUTHORIZED":
            failures.append("valid static authorization result is absent")
        if not callable(getattr(client, "complete", None)):
            failures.append("ModelClient.complete is not implemented")
        if getattr(client, "model", None) != FROZEN_REQUESTED_MODEL:
            failures.append("requested model differs from frozen model")
        if getattr(client, "provider_route", None) != FROZEN_PROVIDER_ROUTE:
            failures.append("provider route differs from frozen route")
        from .evidence_revision_run import FROZEN_PROVIDER_ENDPOINT
        if getattr(client, "endpoint", None) != FROZEN_PROVIDER_ENDPOINT:
            failures.append("provider endpoint differs from frozen route")
        validator = getattr(client, "validate_local_configuration", None)
        if not callable(validator):
            failures.append("local provider configuration validator is absent")
        else:
            try:
                report = validator()
                if report.get("configured") is not True:
                    failures.append("provider configuration is incomplete")
                if report.get("network_requests") != 0:
                    failures.append("client construction/config validation used network")
            except Exception as exc:
                failures.append(
                    f"provider configuration invalid: {type(exc).__name__}: {exc}")
        if failures:
            raise ExecutionRefused(
                "client validation refused:\n- " + "\n- ".join(failures))
        return {"phase": "CLIENT_VALIDATED", "authorized": False,
                "stage": "full", "binding": static["binding"],
                "client": {"model": client.model,
                           "provider_route": client.provider_route,
                           "endpoint": client.endpoint,
                           "network_requests": 0}}

    @staticmethod
    def finalize(static: Mapping, client_result: Mapping) -> dict:
        failures = []
        if static.get("phase") != "STATIC_AUTHORIZED":
            failures.append("static authorization is absent")
        if client_result.get("phase") != "CLIENT_VALIDATED":
            failures.append("client validation is absent")
        if static.get("binding") != client_result.get("binding"):
            failures.append("gate phase bindings differ")
        if failures:
            raise ExecutionRefused(
                "final authorization refused:\n- " + "\n- ".join(failures))
        return {"phase": "FINAL_AUTHORIZED", "authorized": True,
                "stage": "full", "binding": static["binding"]}


def _unit_binding(matrix: FrozenFullMatrix, case_id: str, arm: str, rep: int) -> str:
    return _sha256(_canonical({
        "experiment": EXPERIMENT_ID, "fingerprint": matrix.executable_fingerprint,
        "stage": "full", "namespace": matrix.namespace,
        "case_id": case_id, "arm": arm, "rep": rep}))


def _commit_result(run_dir: Path, result: UnitResult,
                   call_ledger: CommitLedger, matrix: FrozenFullMatrix) -> str:
    for execution in result.executions:
        call_id = f"{result.unit_id}:call:{execution['call_num']:03d}"
        actual = call_ledger.assert_scorable(call_id)
        if actual != execution["raw_hash"]:
            raise CommitViolation("controller execution lost raw binding")
    result_path = run_dir / "results" / f"{result.unit_id}.json"
    ledger = CommitLedger(str(run_dir / "result-commits.jsonl"))
    doc = result.to_dict()
    artifacts = build_artifacts()
    compatibility = artifacts[
        "evidence-revision-002-predecessor-pilot-compatibility.json"]
    doc["runner_contract"] = {
        "schema": RUNNER_SCHEMA, "experiment_id": EXPERIMENT_ID,
        "design_fingerprint": matrix.design_fingerprint,
        "executable_contract_fingerprint": matrix.executable_fingerprint,
        "full_matrix_sha256": matrix.matrix_sha256,
        "runner_implementation_sha256": runner_implementation_sha256(),
        "predecessor_pilot_compatibility_sha256": compatibility[
            "compatibility_sha256"],
        "governance_class": GOVERNANCE_CLASS[result.condition],
        "budget_semantics": ARMS[result.condition].budget_semantics,
        "actual_implied_evidence_payment": sum(
            int(round_record.get("payment_charged", 0)
                if isinstance(round_record, dict)
                else round_record.payment_charged)
            for round_record in result.rounds),
        "gate_blind_last_model_claim": result.last_model_claim,
        "governed_final_settlement": result.final_locus,
        "arm_spec": dataclasses.asdict(ARMS[result.condition]) | {
            "disclosure": ARMS[result.condition].disclosure.value},
        "evidence_selection_seed_identity": EVIDENCE_SELECTION_EXPERIMENT_ID,
        "leakage_policy_version": LEAKAGE_POLICY_VERSION,
    }
    if result.case_id != "fixture-01":
        inputs = load_case_inputs()
        doc["runner_contract"].update(covariates_for(result.case_id, inputs))
        observations = neutralized_observations()[0]
        hidden = json.loads((Path(__file__).resolve().parents[2] /
                             "corpus/acquisition-hidden-meta.json").read_text())
        _, delivery = selected_delivery(result.case_id, result.rep,
                                        result.condition, observations, hidden)
        payloads = [observation.payload for observation in delivery.observations]
        doc["runner_contract"].update({
            "masked_k_implied_cost": (delivery.payment_charged
                                      if EVIDENCE_POLICY[result.condition] == "MASKED_K"
                                      else None),
            "masked_k_realized_k": (len(delivery.delivered_probes)
                                     if EVIDENCE_POLICY[result.condition] == "MASKED_K"
                                     else None),
            "evidence_policy": EVIDENCE_POLICY[result.condition],
            "evidence_membership": list(delivery.delivered_probes),
            "evidence_set_hash": _sha256(_canonical({
                "probes": delivery.delivered_probes,
                "handles": [observation.handle for observation in delivery.observations],
                "payloads": payloads})),
            "semantic_evidence_hash": _sha256(_canonical(payloads)),
            "case_input_binding_sha256": matrix.case_inputs_sha256})
    payload = json.dumps(doc, sort_keys=True, separators=(",", ":"), default=str)
    return ledger.commit_raw(result.unit_id, str(result_path), payload).raw_hash or ""


def run_authorized_full(req: GateRequest,
                        client_factory: Callable[[], ModelClient]) -> dict:
    """Execute only after static authority, then client validation, then namespace creation.

    No caller in the frozen successor owner state can pass the first line.
    ``client_factory`` is intentionally invoked only after static authorization.
    """
    static = ExecutionGate.validate_static(req)
    client = client_factory()
    client_result = ExecutionGate.validate_client(static, client)
    final = ExecutionGate.finalize(static, client_result)
    if final.get("phase") != "FINAL_AUTHORIZED" or final.get("authorized") is not True:
        raise ExecutionRefused("final successor authorization is not authoritative")
    def run_one(run_dir: Path, case_id: str, arm: str, rep: int):
        """Lexically scoped live primitive, reachable only after finalization."""
        if _runtime_input_hashes() != static["runtime_input_hashes"]:
            raise ExecutionRefused("runtime inputs changed after authorization")
        if case_id not in req.matrix.cases or arm not in req.matrix.arms:
            raise ExecutionRefused("unit is outside frozen successor FULL matrix")
        uid = unit_id(EXPERIMENT_ID, req.matrix.namespace, case_id, arm, rep,
                      req.matrix.executable_fingerprint)
        if uid not in req.matrix.unit_ids:
            raise ExecutionRefused("unit UID is outside frozen successor FULL matrix")
        checkpoint = CheckpointStore(run_dir / "checkpoint.json")
        binding = _unit_binding(req.matrix, case_id, arm, rep)
        if checkpoint.status(uid) == "PLANNED":
            checkpoint.transition(uid, "PLANNED", unit_binding=binding)
        replay = DurableReplayClient(client, uid, run_dir, checkpoint=checkpoint)
        if checkpoint.status(uid) in ("RESULT_COMMITTED", "SCORED"):
            result_ledger = CommitLedger(str(run_dir / "result-commits.jsonl"))
            result_ledger.assert_scorable(uid)
            doc = json.loads((run_dir / "results" / f"{uid}.json").read_text())
            for execution in doc.get("executions", []):
                actual = replay.ledger.assert_scorable(
                    f"{uid}:call:{execution['call_num']:03d}")
                if actual != execution["raw_hash"]:
                    raise CommitViolation("completed result lost its raw-response binding")
            doc.pop("runner_contract", None)
            return UnitResult(**doc), replay
        initial = initial_state_for(case_id)
        observations = neutralized_observations()[0][case_id]
        hidden = json.loads((Path(__file__).resolve().parents[2] /
                             "corpus/acquisition-hidden-meta.json").read_text())
        discriminating = tuple(hidden[case_id]["discriminating_probes"])
        assert_marker_free(initial); assert_marker_free(observations)
        ctrl = DiagnosticController(
            ARMS[arm], LEGACY_CONTRACT, PROBES, DESCRIPTIONS, COSTS, VOCAB,
            replay, endpoint=FROZEN_PROVIDER_ROUTE, requested_model=FROZEN_REQUESTED_MODEL,
            evidence_selection_policy=EVIDENCE_POLICY[arm], disclosure_k=3,
            masked_candidate_universe=(DIRECT_CANDIDATE_UNIVERSE
                                       if EVIDENCE_POLICY[arm] == "MASKED_K" else ()))
        # Controller's experiment_id argument is exclusively the frozen evidence
        # seed domain. Keeping the predecessor domain preserves handles/delivery.
        result = ctrl.run_unit(
            uid, EVIDENCE_SELECTION_EXPERIMENT_ID, req.matrix.namespace, case_id, rep,
            initial["question"], initial["initial_evidence"], observations,
            discriminating)
        checkpoint.transition(uid, "PARSED", unit_binding=binding)
        _commit_result(run_dir, result, replay.ledger, req.matrix)
        checkpoint.transition(uid, "RESULT_COMMITTED", unit_binding=binding,
                              result_hash=_file_hash(run_dir / "results" / f"{uid}.json"))
        return result, replay

    artifacts = build_artifacts()
    executable = artifacts["evidence-revision-002-executable-contract.json"]
    compatibility = artifacts[
        "evidence-revision-002-predecessor-pilot-compatibility.json"]
    source_hashes = {
        "full_matrix_sha256": req.matrix.matrix_sha256,
        "case_inputs_sha256": req.matrix.case_inputs_sha256,
        "design_fingerprint": req.matrix.design_fingerprint,
        "executable_contract_fingerprint": req.matrix.executable_fingerprint,
        "runner_implementation_sha256": runner_implementation_sha256(),
        "predecessor_pilot_compatibility_sha256": compatibility[
            "compatibility_sha256"],
        "predecessor_pilot_manifest_sha256": (
            executable["owner_state"]["predecessor_pilot_manifest_sha256"]),
        "owner_state_digest": _sha256(_canonical(dict(req.owner_state))),
        "reviewed_runner_commit": req.owner_state["authorized_reviewed_runner_commit"],
        "release_marker_sha256": _file_hash(req.release_marker),
        "release_token_sha256": hashlib.sha256(req.release_token.encode()).hexdigest(),
        "runtime_input_hashes": static["runtime_input_hashes"],
    }
    run_dir = req.run_root / req.run_id
    manifest_path = run_dir / "run.json"
    identity = {
        "schema": RUNNER_SCHEMA, "experiment_id": EXPERIMENT_ID,
        "stage": "full", "run_id": req.run_id,
        "design_fingerprint": req.matrix.design_fingerprint,
        "executable_contract_fingerprint": req.matrix.executable_fingerprint,
        "source_hashes": source_hashes,
    }
    if manifest_path.exists():
        if json.loads(manifest_path.read_text()) != identity:
            raise ResumeViolation("existing successor run namespace is incompatible")
    else:
        if run_dir.exists() and any(run_dir.iterdir()):
            raise ResumeViolation("non-empty successor namespace has no identity manifest")
        _atomic_json(manifest_path, identity)
    checkpoint = CheckpointStore(run_dir / "checkpoint.json")
    completed = resumed = 0
    for case_id in req.matrix.cases:
        for arm in req.matrix.arms:
            for rep in range(1, req.matrix.repetitions + 1):
                uid = unit_id(EXPERIMENT_ID, req.matrix.namespace, case_id, arm,
                              rep, req.matrix.executable_fingerprint)
                before = checkpoint.status(uid)
                result, _ = run_one(run_dir, case_id, arm, rep)
                if result.unit_id != uid:
                    raise ResumeViolation("successor loop UID differs from matrix")
                completed += 1
                resumed += int(before in ("RESULT_COMMITTED", "SCORED"))
                _atomic_json(run_dir / "execution-summary.json", {
                    "schema": "evidence-revision-002-execution-summary-v1",
                    "experiment_id": EXPERIMENT_ID, "stage": "full",
                    "run_id": req.run_id,
                    "matrix_sha256": req.matrix.matrix_sha256,
                    "units_planned": req.matrix.total_units,
                    "units_completed_or_resumed": completed,
                    "units_resumed": resumed,
                    "provider_calls_performed": CommitLedger(
                        str(run_dir / "call-commits.jsonl")).provenance_summary()["verified"],
                    "model_calls_performed": CommitLedger(
                        str(run_dir / "call-commits.jsonl")).provenance_summary()["verified"]})
    return json.loads((run_dir / "execution-summary.json").read_text())

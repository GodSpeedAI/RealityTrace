#!/usr/bin/env python3
"""Execution orchestration for frozen ``evidence-revision-001``.

This module does not own scientific semantics.  It imports the frozen arms,
controller, parser, evidence channel, state renderer, provenance structures,
and commit ledger.  Live execution fails closed until the owner state and the
missing frozen pilot matrix are explicitly resolved.

Safe offline commands::

    python -m eval.v2.evidence_revision_run inspect-matrix --stage pilot
    python -m eval.v2.evidence_revision_run dry-run --output /tmp/er-dry

The provider module is deliberately imported only inside ``_live_client`` and
is unreachable from dry-run and inspection commands.
"""
from __future__ import annotations

import argparse
import dataclasses
import hashlib
import json
import os
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Mapping

from .channel import PromptLeakGuard
from .controller import DiagnosticController, ModelClient, UnitResult, arm_difference_report
from .dstate import missing_state_sections
from .evidence_revision import (ARMS, EXPERIMENT_ID, GOVERNANCE_CLASS,
                                OWNER_STATE, PRIMARY_POPULATION)
from .case_inputs_v2 import (covariates_for, initial_state_for,
                             load_case_inputs as load_case_inputs_v2)
from .marker_neutralization import assert_marker_free
from .validity_revision import (
    DESIGN_VERSION, DIRECT_CANDIDATE_UNIVERSE, EVIDENCE_POLICY,
    EXECUTABLE_VERSION, FULL_NAMESPACE,
    LEAKAGE_POLICY_VERSION, PILOT_NAMESPACE, PILOT_REPETITIONS,
    PREDECESSOR_DESIGN, PREDECESSOR_EXECUTABLE, build_artifacts,
    design_fingerprint, feasibility_report, leakage_report,
    neutralized_observations, pair_equality_report, selected_delivery)
from .ids import unit_id
from .ledger import CommitLedger, CommitViolation
from .legibility import assert_semantic_equivalence
from .replication import COSTS, DESCRIPTIONS, PROBES, VOCAB, _fixture
from .sufficiency import LEGACY_CONTRACT, derive_topology

EXPECTED_DESIGN_FINGERPRINT = "3031af33611c9ae8ab0fd0bba22f0abc35147d4fe564e0e3623a174d0dfe28d9"
EXPECTED_EXECUTABLE_FINGERPRINT = "12eb26d72f74709a827059c2a6ccb693ef956a6d5aa7ca2bf6f292b645da010e"
# Backward-compatible name for consumers that mean executable identity.
EXPECTED_FINGERPRINT = EXPECTED_EXECUTABLE_FINGERPRINT
RUNNER_SCHEMA = "evidence-revision-runner-v2"
MATRIX_SCHEMA = "evidence-revision-matrix-v2"
CALL_SCHEMA = "evidence-revision-call-v1"
VALID_STAGES = ("pilot", "full")
FROZEN_REQUESTED_MODEL = "glm-5.2"
FROZEN_PROVIDER_ROUTE = "zai-coding-plan"
FROZEN_PROVIDER_ENDPOINT = "https://api.z.ai/api/coding/paas/v4/chat/completions"
REPO_ROOT = Path(__file__).resolve().parents[3]
CORPUS = REPO_ROOT / "ablation" / "corpus"
REVISION_DIR = REPO_ROOT / "ablation" / "analysis" / "post-audit-v2"
REVISED_SCIENTIFIC_CONTRACT = REVISION_DIR / "evidence-revision-001-revision-002-contract.json"
REVISED_EXECUTABLE_CONTRACT = REVISION_DIR / "evidence-revision-001-revision-002-executable-contract.json"
REVISED_PILOT_MATRIX = REVISION_DIR / "evidence-revision-001-pilot-matrix-v2.json"
REVISED_CASE_INPUTS = REVISION_DIR / "case-inputs-v2.json"
REVISED_MARKER_MANIFEST = REVISION_DIR / "evidence-revision-001-marker-neutralization-v2.json"
REVISED_FEASIBILITY = REVISION_DIR / "evidence-revision-001-masked-k-feasibility-v2.json"
REVISED_LEAKAGE = REVISION_DIR / "evidence-revision-001-masked-k-leakage-v2.json"

CORPUS_FILES = {
    "cases": CORPUS / "cases.jsonl",
    "probes": CORPUS / "acquisition-probes.jsonl",
    "hidden_meta": CORPUS / "acquisition-hidden-meta.json",
    "keys": CORPUS / "governed-keys.json",
    "sufficiency": CORPUS / "governed-sufficiency-contract.json",
}
SOURCE_FILES = {
    "runner_module": Path(__file__),
    "provider_module": Path(__file__).with_name("provider.py"),
    "scientific_contract": REVISED_SCIENTIFIC_CONTRACT,
    "executable_contract": REVISED_EXECUTABLE_CONTRACT,
    "pilot_matrix": REVISED_PILOT_MATRIX,
    "case_inputs": REVISED_CASE_INPUTS,
    "marker_manifest": REVISED_MARKER_MANIFEST,
    "masked_k_feasibility": REVISED_FEASIBILITY,
    "masked_k_leakage": REVISED_LEAKAGE,
    "validity_revision_module": Path(__file__).with_name("validity_revision.py"),
    "case_inputs_module": Path(__file__).with_name("case_inputs_v2.py"),
    "marker_neutralizer_module": Path(__file__).with_name("marker_neutralization.py"),
    "controller_module": Path(__file__).with_name("controller.py"),
    "channel_module": Path(__file__).with_name("channel.py"),
    "decision_module": Path(__file__).with_name("decision.py"),
    "state_module": Path(__file__).with_name("dstate.py"),
    "legibility_module": Path(__file__).with_name("legibility.py"),
    "ledger_module": Path(__file__).with_name("ledger.py"),
}

# Separate from scientific/executable identity: these are the payment-bearing
# implementation files whose exact bytes the owner reviews and releases.
RUNNER_IMPLEMENTATION_FILES = (
    "ablation/eval/v2/evidence_revision_run.py",
    "ablation/eval/v2/provider.py",
    "ablation/eval/v2/controller.py",
    "ablation/eval/v2/channel.py",
    "ablation/eval/v2/dstate.py",
    "ablation/eval/v2/decision.py",
    "ablation/eval/v2/execution.py",
    "ablation/eval/v2/ledger.py",
    "ablation/eval/v2/legibility.py",
    "ablation/eval/v2/ids.py",
    "ablation/eval/v2/case_inputs_v2.py",
    "ablation/eval/v2/marker_neutralization.py",
    "ablation/eval/v2/sufficiency.py",
    "ablation/eval/v2/replication.py",
    "ablation/eval/v2/validity_revision.py",
)
RUNNER_IMPLEMENTATION_SCHEMA = "evidence-revision-runner-implementation-v1"



class ExecutionRefused(RuntimeError):
    """A centralized execution-gate failure."""


class ResumeViolation(RuntimeError):
    """A checkpoint or replay artifact conflicts with its unit/request."""


class InjectedInterruption(RuntimeError):
    """Offline crash-injection hook used to prove resume boundaries."""


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _file_hash(path: Path) -> str:
    return _sha256(path.read_bytes())


def _canonical(value) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"),
                      default=str).encode()


def runner_implementation_manifest(root: Path = REPO_ROOT) -> dict:
    """Canonical byte identity of all payment-bearing runner implementation."""
    files = {}
    for relative in RUNNER_IMPLEMENTATION_FILES:
        path = root / relative
        if not path.is_file():
            raise ExecutionRefused(f"bound runner implementation file missing: {relative}")
        files[relative] = _file_hash(path)
    return {"schema": RUNNER_IMPLEMENTATION_SCHEMA, "files": files}


def runner_implementation_sha256(root: Path = REPO_ROOT) -> str:
    return _sha256(_canonical(runner_implementation_manifest(root)))


def runner_implementation_sha256_at_commit(commit: str,
                                            root: Path = REPO_ROOT) -> str:
    """Reconstruct the bound manifest from a locally reviewed Git commit."""
    if (len(commit) != 40 or any(c not in "0123456789abcdef" for c in commit)):
        raise ExecutionRefused("reviewed_runner_commit is not a full lowercase SHA")
    files = {}
    for relative in RUNNER_IMPLEMENTATION_FILES:
        try:
            payload = subprocess.check_output(
                ["git", "-C", str(root), "show", f"{commit}:{relative}"],
                stderr=subprocess.DEVNULL)
        except (OSError, subprocess.CalledProcessError) as exc:
            raise ExecutionRefused(
                f"reviewed runner commit cannot resolve {relative}") from exc
        files[relative] = _sha256(payload)
    return _sha256(_canonical(
        {"schema": RUNNER_IMPLEMENTATION_SCHEMA, "files": files}))


def latest_runner_implementation_commit(root: Path = REPO_ROOT) -> str:
    try:
        return subprocess.check_output(
            ["git", "-C", str(root), "log", "-1", "--format=%H", "--",
             *RUNNER_IMPLEMENTATION_FILES], text=True,
            stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.CalledProcessError) as exc:
        raise ExecutionRefused("cannot resolve reviewed runner commit") from exc


def _atomic_json(path: Path, value) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = _canonical(value) + b"\n"
    fd, tmp = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as f:
            f.write(payload)
            f.flush()
            os.fsync(f.fileno())
        os.replace(tmp, path)
        dfd = os.open(path.parent, os.O_DIRECTORY)
        try:
            os.fsync(dfd)
        finally:
            os.close(dfd)
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)


def frozen_hashes() -> dict:
    """Hash the actual frozen inputs; no transport is imported or contacted."""
    missing = [str(p) for p in (*CORPUS_FILES.values(), *SOURCE_FILES.values())
               if not p.is_file()]
    if missing:
        raise ExecutionRefused(f"missing frozen inputs: {missing}")
    corpus = {name: _file_hash(path) for name, path in CORPUS_FILES.items()}
    sources = {name: _file_hash(path) for name, path in SOURCE_FILES.items()}
    return {
        "corpus": corpus,
        "sources": sources,
        "corpus_hash": _sha256(_canonical(corpus)),
        "evidence_action_hash": _sha256(_canonical({
            "probes": corpus["probes"], "hidden_meta": corpus["hidden_meta"],
            "channel": sources["channel_module"]})),
    }


def _load_jsonl(path: Path) -> dict:
    out = {}
    for line in path.read_text().splitlines():
        if line.strip():
            row = json.loads(line)
            out[row["case_id"]] = row
    return out


def eligible_cases() -> tuple[str, ...]:
    """Re-derive the frozen primary population from the adopted gate."""
    probes = {cid: row["probes"] for cid, row in
              _load_jsonl(CORPUS_FILES["probes"]).items()}
    keys = json.loads(CORPUS_FILES["keys"].read_text())
    topology = derive_topology(probes, LEGACY_CONTRACT, COSTS, 7)
    cases = tuple(sorted(cid for cid, key in keys.items()
                         if key["true_locus"] != "unknown"
                         and bool(topology.get(cid))))
    if len(cases) != PRIMARY_POPULATION["n_cases"]:
        raise ExecutionRefused(
            f"primary population drift: derived {len(cases)}, frozen "
            f"{PRIMARY_POPULATION['n_cases']}")
    return cases


def _revised_contracts() -> tuple[dict, dict, dict]:
    science = json.loads(REVISED_SCIENTIFIC_CONTRACT.read_text())
    executable = json.loads(REVISED_EXECUTABLE_CONTRACT.read_text())
    matrix = json.loads(REVISED_PILOT_MATRIX.read_text())
    return science, executable, matrix


def inspect_matrix(stage: str) -> dict:
    """Safe offline inspection of contract-owned execution coordinates."""
    if stage not in VALID_STAGES:
        raise ValueError(stage)
    hashes = frozen_hashes()
    science, executable, matrix = _revised_contracts()
    if science["design_fingerprint"] != EXPECTED_DESIGN_FINGERPRINT:
        raise ExecutionRefused("revised scientific fingerprint drift")
    if executable["executable_contract_fingerprint"] != EXPECTED_EXECUTABLE_FINGERPRINT:
        raise ExecutionRefused("revised executable fingerprint drift")
    if stage == "pilot":
        return {
            "experiment_id": EXPERIMENT_ID, "stage": stage,
            "design_fingerprint": EXPECTED_DESIGN_FINGERPRINT,
            "executable_contract_fingerprint": EXPECTED_EXECUTABLE_FINGERPRINT,
            "cases": matrix["cases"], "arms": matrix["arms"],
            "repetitions": matrix["repetitions"],
            "total_units": matrix["total_units"], "uids": matrix["unit_ids"],
            "namespace": matrix["namespace"],
            "matrix_sha256": matrix["matrix_sha256"],
            "case_inputs_sha256": matrix["case_inputs_sha256"],
            "corpus_hash": hashes["corpus_hash"],
            "evidence_action_hash": hashes["evidence_action_hash"],
            "source_hashes": hashes["sources"], "executable": True,
            "blocker": None, "execution_authorized": False,
            "model_calls_performed": 0, "provider_calls_performed": 0}
    return {
        "experiment_id": EXPERIMENT_ID, "stage": stage,
        "design_fingerprint": EXPECTED_DESIGN_FINGERPRINT,
        "executable_contract_fingerprint": EXPECTED_EXECUTABLE_FINGERPRINT,
        "cases": list(eligible_cases()), "arms": list(ARMS),
        "repetitions": None, "total_units": None, "uids": [],
        "namespace": FULL_NAMESPACE, "executable": False,
        "blocker": "FULL matrix is intentionally not frozen or authorized",
        "execution_authorized": False, "model_calls_performed": 0,
        "provider_calls_performed": 0}


@dataclass(frozen=True)
class FrozenMatrix:
    stage: str
    namespace: str
    cases: tuple[str, ...]
    arms: tuple[str, ...]
    repetitions: int
    design_fingerprint: str
    executable_fingerprint: str
    case_inputs_hash: str
    matrix_hash: str

    @property
    def total_units(self) -> int:
        return len(self.cases) * len(self.arms) * self.repetitions

    def uids(self, namespace: str | None = None) -> tuple[str, ...]:
        namespace = namespace or self.namespace
        return tuple(unit_id(EXPERIMENT_ID, namespace, case, arm, rep,
                             self.executable_fingerprint)
                     for case in self.cases for arm in self.arms
                     for rep in range(1, self.repetitions + 1))


def load_frozen_matrix(path: Path, stage: str,
                       owner_state: Mapping | None = None) -> FrozenMatrix:
    """Load a matrix whose canonical hash is owned by the scientific contract.

    ``owner_state`` is accepted only for API compatibility and is never read:
    authorization state cannot own scientific matrix semantics.
    """
    doc = json.loads(path.read_text())
    _, executable, frozen_doc = _revised_contracts()
    expected = executable.get("pilot_matrix_sha256") if stage == "pilot" else None
    if not expected:
        raise ExecutionRefused(f"scientific contract has no frozen {stage} matrix")
    canonical = _sha256(_canonical({k: v for k, v in doc.items()
                                    if k != "matrix_sha256"}))
    if canonical != doc.get("matrix_sha256") or canonical != expected:
        raise ExecutionRefused(f"{stage} matrix canonical hash mismatch")
    if doc.get("schema") != MATRIX_SCHEMA or doc.get("experiment_id") != EXPERIMENT_ID:
        raise ExecutionRefused("matrix identity/schema mismatch")
    if doc.get("stage") != stage:
        raise ExecutionRefused("matrix stage mismatch")
    if doc.get("design_fingerprint") != EXPECTED_DESIGN_FINGERPRINT:
        raise ExecutionRefused("matrix design fingerprint mismatch")
    if doc.get("executable_contract_fingerprint") != EXPECTED_EXECUTABLE_FINGERPRINT:
        raise ExecutionRefused("matrix executable fingerprint mismatch")
    inputs = load_case_inputs_v2()
    if doc.get("case_inputs_sha256") != inputs["case_inputs_sha256"]:
        raise ExecutionRefused("matrix case-input binding mismatch")
    cases, arms = tuple(doc.get("cases", ())), tuple(doc.get("arms", ()))
    reps = doc.get("repetitions")
    if not cases or len(cases) != len(set(cases)) or not isinstance(reps, int) or reps < 1:
        raise ExecutionRefused("matrix cases/repetitions invalid")
    if arms != tuple(ARMS) or not set(cases).issubset(eligible_cases()):
        raise ExecutionRefused("matrix arms/population mismatch")
    matrix = FrozenMatrix(stage, doc["namespace"], cases, arms, reps,
                          EXPECTED_DESIGN_FINGERPRINT,
                          EXPECTED_EXECUTABLE_FINGERPRINT,
                          inputs["case_inputs_sha256"], canonical)
    if list(matrix.uids()) != doc.get("unit_ids"):
        raise ExecutionRefused("matrix unit IDs are not executable-bound")
    return matrix


def owner_state_digest(owner_state: Mapping) -> str:
    return _sha256(_canonical(dict(owner_state)))


@dataclass(frozen=True)
class GateRequest:
    stage: str
    release_token: str | None
    release_marker: Path | None
    client: ModelClient | None
    matrix: FrozenMatrix | None
    run_id: str
    run_root: Path
    owner_state: Mapping = dataclasses.field(default_factory=lambda: OWNER_STATE)
    observed_fingerprint: str = EXPECTED_EXECUTABLE_FINGERPRINT
    observed_design_fingerprint: str = EXPECTED_DESIGN_FINGERPRINT


class ExecutionGate:
    """Two-phase fail-closed gate for every non-dry provider execution."""

    AUTHORIZED_STATUS = {"pilot": "PILOT_AUTHORIZED", "full": "FULL_AUTHORIZED"}

    @staticmethod
    def _refuse(failures) -> None:
        if failures:
            raise ExecutionRefused("execution refused:\n- " + "\n- ".join(failures))

    @classmethod
    def validate_static(cls, req: GateRequest) -> dict:
        """Phase A: validate all authority/science without touching a client."""
        failures = []
        if req.stage not in VALID_STAGES:
            failures.append("requested stage is invalid")
        if req.observed_design_fingerprint != EXPECTED_DESIGN_FINGERPRINT:
            failures.append("frozen design fingerprint mismatch")
        if req.observed_fingerprint != EXPECTED_EXECUTABLE_FINGERPRINT:
            failures.append("frozen executable fingerprint mismatch")
        expected_status = cls.AUTHORIZED_STATUS.get(req.stage)
        owner = req.owner_state
        if owner.get("status") != expected_status:
            failures.append(f"owner state is not {expected_status}")
        if owner.get("execution_authorized") is not True:
            failures.append("owner execution authorization is false")
        if owner.get("additional_provider_calls_authorized") is not True:
            failures.append("owner provider-call authorization is false")
        if owner.get("authorized_stage") != req.stage:
            failures.append("requested stage is not separately authorized")
        if req.stage == "pilot" and owner.get("full_execution_authorized") is not False:
            failures.append("pilot authority must leave FULL explicitly unauthorized")
        if req.stage == "full" and owner.get("full_execution_authorized") is not True:
            failures.append("FULL execution authorization is false")
        for key, expected in (
                ("authorized_design_fingerprint", EXPECTED_DESIGN_FINGERPRINT),
                ("authorized_executable_fingerprint", EXPECTED_EXECUTABLE_FINGERPRINT)):
            if owner.get(key) != expected:
                failures.append(f"owner authorization binding mismatch: {key}")
        if not req.release_token:
            failures.append("release token missing")
        marker = None
        if not req.release_marker or not req.release_marker.is_file():
            failures.append("release marker missing")
        else:
            try:
                marker = json.loads(req.release_marker.read_text())
            except (OSError, json.JSONDecodeError):
                failures.append("release marker is not valid JSON")
        hashes = frozen_hashes()
        implementation_sha = runner_implementation_sha256()
        if marker is not None:
            expected_marker = {
                "experiment_id": EXPERIMENT_ID,
                "stage": req.stage,
                "design_fingerprint": EXPECTED_DESIGN_FINGERPRINT,
                "executable_contract_fingerprint": EXPECTED_EXECUTABLE_FINGERPRINT,
                "authorization_state": expected_status,
                "owner_state_digest": owner_state_digest(owner),
                "release_token_sha256": _sha256((req.release_token or "").encode()),
                "corpus_hash": hashes["corpus_hash"],
                "evidence_action_hash": hashes["evidence_action_hash"],
                "matrix_sha256": req.matrix.matrix_hash if req.matrix else None,
                "case_inputs_sha256": req.matrix.case_inputs_hash if req.matrix else None,
                "runner_implementation_sha256": implementation_sha,
            }
            for key, value in expected_marker.items():
                if marker.get(key) != value:
                    failures.append(f"release marker binding mismatch: {key}")
            reviewed_commit = marker.get("reviewed_runner_commit")
            try:
                reviewed_sha = runner_implementation_sha256_at_commit(
                    reviewed_commit or "")
                if reviewed_sha != implementation_sha:
                    failures.append("reviewed runner commit does not bind current implementation")
            except ExecutionRefused as exc:
                failures.append(str(exc))
        if req.matrix is None:
            failures.append("frozen stage matrix is unavailable")
        else:
            expected_namespace = PILOT_NAMESPACE if req.stage == "pilot" else FULL_NAMESPACE
            if req.matrix.stage != req.stage:
                failures.append("matrix stage mismatch")
            if req.matrix.namespace != expected_namespace:
                failures.append("pilot/full namespace separation mismatch")
            if req.matrix.design_fingerprint != EXPECTED_DESIGN_FINGERPRINT:
                failures.append("matrix design fingerprint mismatch")
            if req.matrix.executable_fingerprint != EXPECTED_EXECUTABLE_FINGERPRINT:
                failures.append("matrix executable fingerprint mismatch")
            if owner.get("authorized_case_inputs_sha256") != req.matrix.case_inputs_hash:
                failures.append("owner case-input authorization mismatch")
            if req.stage == "pilot" and owner.get(
                    "authorized_pilot_matrix_sha256") != req.matrix.matrix_hash:
                failures.append("owner pilot-matrix authorization mismatch")
        if (not req.run_id or "/" in req.run_id or req.run_id in (".", "..")
                or req.run_id.strip() != req.run_id):
            failures.append("unique run id is invalid")
        diffs = arm_difference_report(ARMS.values())
        for pair in ("R1_vs_R2", "R3_vs_R4"):
            if (diffs[pair]["n_declared_differences"] != 1
                    or not diffs[pair]["isolates_single_variable"]):
                failures.append(f"arm-isolation drift: {pair}")
        try:
            pairs = pair_equality_report()
            for pair in ("R1_vs_R2", "R3_vs_R4"):
                if not pairs[pair]["underlying_evidence_equal"]:
                    failures.append(f"paired evidence mismatch: {pair}")
            feasibility = feasibility_report()
            leakage = leakage_report()
            if not feasibility["all_cases_feasible"]:
                failures.append("MASKED_K structural affordability failure")
            if any(row["selected_k"] != 3 or row["selected_cost"] > 7
                   for row in feasibility["rows"]):
                failures.append("MASKED_K exact-K/payment invariant failure")
            if leakage["missingness_count"] or leakage["singleton_equivalent_count"]:
                failures.append("MASKED_K missingness/singleton invariant failure")
            assert_semantic_equivalence({"runner_gate": "operational"})
            PromptLeakGuard(PROBES).assert_clean("opaque final transport bytes")
        except Exception as exc:  # pragma: no cover - mutation target
            failures.append(f"rendering/leak/invariant guard not operational: {exc}")
        cls._refuse(failures)
        binding = _sha256(_canonical({
            "stage": req.stage, "run_id": req.run_id,
            "owner_state_digest": owner_state_digest(owner),
            "matrix_hash": req.matrix.matrix_hash,
            "design": EXPECTED_DESIGN_FINGERPRINT,
            "executable": EXPECTED_EXECUTABLE_FINGERPRINT,
            "runner_implementation_sha256": implementation_sha,
            "reviewed_runner_commit": marker.get("reviewed_runner_commit")
                if marker else None}))
        return {"phase": "STATIC_AUTHORIZED", "authorized": False,
                "stage": req.stage, "binding": binding, "hashes": hashes,
                "runner_implementation_sha256": implementation_sha,
                "reviewed_runner_commit": marker["reviewed_runner_commit"]}

    @classmethod
    def validate_client(cls, req: GateRequest, static_result: Mapping) -> dict:
        """Phase B: local client/config validation; never performs completion."""
        failures = []
        if static_result.get("phase") != "STATIC_AUTHORIZED":
            failures.append("valid static authorization result is absent")
        client = req.client
        if client is None:
            failures.append("ModelClient is not configured")
        else:
            if not callable(getattr(client, "complete", None)):
                failures.append("ModelClient.complete is not implemented")
            if getattr(client, "model", None) != FROZEN_REQUESTED_MODEL:
                failures.append("requested model differs from frozen model")
            if getattr(client, "provider_route", None) != FROZEN_PROVIDER_ROUTE:
                failures.append("provider route differs from frozen route")
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
                    failures.append(f"provider configuration invalid: {type(exc).__name__}: {exc}")
        cls._refuse(failures)
        return {"phase": "CLIENT_VALIDATED", "authorized": False,
                "stage": req.stage, "binding": static_result["binding"],
                "client": {"model": client.model,
                           "provider_route": client.provider_route,
                           "endpoint": client.endpoint,
                           "network_requests": 0}}

    @classmethod
    def finalize(cls, static_result: Mapping,
                 client_result: Mapping) -> dict:
        """Phase C: combine matching successful phases into final authority."""
        failures = []
        if static_result.get("phase") != "STATIC_AUTHORIZED":
            failures.append("static authorization is absent")
        if client_result.get("phase") != "CLIENT_VALIDATED":
            failures.append("client validation is absent")
        if static_result.get("binding") != client_result.get("binding"):
            failures.append("gate phase bindings differ")
        if static_result.get("stage") != client_result.get("stage"):
            failures.append("gate phase stages differ")
        cls._refuse(failures)
        return {"phase": "FINAL_AUTHORIZED", "authorized": True,
                "stage": static_result["stage"],
                "binding": static_result["binding"],
                "hashes": static_result["hashes"],
                "client": client_result["client"]}

    @classmethod
    def validate(cls, req: GateRequest) -> dict:
        """Compatibility entry point: perform all phases without weakening any."""
        static = cls.validate_static(req)
        client = cls.validate_client(req, static)
        return cls.finalize(static, client)


class RunNamespace:
    """Immutable run identity; incompatible append attempts are rejected."""

    def __init__(self, root: Path, run_id: str, stage: str, source_hashes: Mapping):
        self.path = root / run_id
        self.manifest_path = self.path / "run.json"
        self.identity = {
            "schema": RUNNER_SCHEMA, "experiment_id": EXPERIMENT_ID,
            "design_fingerprint": EXPECTED_DESIGN_FINGERPRINT,
            "executable_contract_fingerprint": EXPECTED_EXECUTABLE_FINGERPRINT,
            "stage": stage,
            "run_id": run_id, "source_hashes": dict(source_hashes),
        }

    def create_or_verify(self) -> Path:
        if self.manifest_path.exists():
            got = json.loads(self.manifest_path.read_text())
            if got != self.identity:
                raise ResumeViolation("existing run namespace is incompatible")
        else:
            if self.path.exists() and any(self.path.iterdir()):
                raise ResumeViolation("non-empty run namespace has no identity manifest")
            _atomic_json(self.manifest_path, self.identity)
        return self.path


class CheckpointStore:
    """Durable per-unit status snapshots with legal-transition enforcement."""
    ORDER = {"PLANNED": 0, "CALLING": 1, "RAW_COMMITTED": 2,
             "PARSED": 3, "RESULT_COMMITTED": 4, "SCORED": 5}
    ALLOWED = {
        "PLANNED": {"PLANNED", "CALLING"},
        "CALLING": {"CALLING", "RAW_COMMITTED"},
        "RAW_COMMITTED": {"RAW_COMMITTED", "CALLING", "PARSED"},
        "PARSED": {"PARSED", "RESULT_COMMITTED"},
        "RESULT_COMMITTED": {"RESULT_COMMITTED", "SCORED"},
        "SCORED": {"SCORED"},
    }

    def __init__(self, path: Path):
        self.path = path
        self.data = json.loads(path.read_text()) if path.exists() else {}

    def status(self, uid: str) -> str:
        return self.data.get(uid, {}).get("status", "PLANNED")

    def transition(self, uid: str, status: str, **details) -> None:
        old = self.status(uid)
        if status not in self.ORDER or status not in self.ALLOWED[old]:
            raise ResumeViolation(f"illegal checkpoint transition {old} -> {status}")
        current = self.data.get(uid, {})
        if current.get("unit_binding") and details.get("unit_binding") not in (
                None, current["unit_binding"]):
            raise ResumeViolation("checkpoint belongs to another arm/stage/unit")
        self.data[uid] = {**current, **details, "status": status}
        _atomic_json(self.path, self.data)


class DurableReplayClient:
    """Commit every raw response before exposing it to parser/controller.

    A verified committed response is replayed after checking the exact request
    identity.  Thus restarting the deterministic controller cannot duplicate a
    committed provider call or parse different bytes for that call.
    """

    def __init__(self, inner: ModelClient, unit_id_: str, run_dir: Path,
                 canonical_probe_ids=PROBES, checkpoint: CheckpointStore | None = None,
                 interrupt: Callable[[str, int], None] | None = None):
        self.inner, self.unit_id, self.run_dir = inner, unit_id_, run_dir
        self.guard = PromptLeakGuard(canonical_probe_ids)
        self.calls = 0
        self.ledger = CommitLedger(str(run_dir / "call-commits.jsonl"))
        self.checkpoint = checkpoint
        self.interrupt = interrupt or (lambda _where, _n: None)

    def _request(self, n, system, user, requested_max_tokens, temperature):
        return {"schema": CALL_SCHEMA, "unit_id": self.unit_id, "call_num": n,
                "system_sha256": _sha256(system.encode()),
                "user_sha256": _sha256(user.encode()),
                "requested_max_tokens": requested_max_tokens,
                "temperature": temperature}

    def complete(self, system: str, user: str, requested_max_tokens: int,
                 temperature: float) -> dict:
        self.calls += 1
        n = self.calls
        # This is the final rendered byte string passed to transport.
        self.guard.assert_clean(user, where="final transport prompt")
        missing = missing_state_sections(user)
        if missing:
            raise ResumeViolation(f"final transport prompt lacks state: {missing}")
        request = self._request(n, system, user, requested_max_tokens, temperature)
        call_id = f"{self.unit_id}:call:{n:03d}"
        raw_path = self.run_dir / "raw" / self.unit_id / f"{n:03d}.txt"
        sidecar = self.run_dir / "calls" / self.unit_id / f"{n:03d}.json"
        entry = self.ledger.get(call_id)
        if entry:
            raw_hash = self.ledger.assert_scorable(call_id)
            if not sidecar.is_file():
                raise ResumeViolation("raw call is committed but sidecar is missing")
            saved = json.loads(sidecar.read_text())
            if saved.get("request") != request or saved.get("raw_hash") != raw_hash:
                raise ResumeViolation("committed call request/raw identity drift")
            text = raw_path.read_text()
            if _sha256(text.encode()) != raw_hash:
                raise ResumeViolation("committed raw bytes changed")
            return {"text": text, "provider": saved["provider"]}
        if raw_path.exists() or sidecar.exists():
            raise ResumeViolation("partial uncommitted call artifact; refuse overwrite")
        if self.checkpoint:
            self.checkpoint.transition(self.unit_id, "CALLING")
        self.interrupt("before_call", n)
        # Durable call intent prevents an ambiguous post-transport crash from
        # silently issuing the same request again.  Without provider-side
        # idempotency, such an intent must be adjudicated rather than retried.
        _atomic_json(sidecar, {"request": request, "status": "CALL_IN_FLIGHT"})
        response = self.inner.complete(system, user, requested_max_tokens, temperature)
        self.interrupt("after_provider_result", n)
        if not isinstance(response, dict) or not isinstance(response.get("text"), str):
            raise ResumeViolation("ModelClient returned an invalid response shape")
        provider = response.get("provider", {})
        if not isinstance(provider, dict):
            raise ResumeViolation("ModelClient provider provenance is not a mapping")
        predicted_hash = _sha256(response["text"].encode())
        # The provenance sidecar is durable before the raw ledger transaction;
        # once the ledger commit exists, replay has all required metadata.
        _atomic_json(sidecar, {"request": request, "raw_hash": predicted_hash,
                              "provider": provider,
                              "prompt_guard": {"inspected_user_sha256": request["user_sha256"],
                                               "findings": []}})
        committed = self.ledger.commit_raw(call_id, str(raw_path), response["text"])
        if committed.raw_hash != predicted_hash:
            raise ResumeViolation("on-disk raw hash differs from provider response")
        self.interrupt("after_raw_persist", n)
        # commit_raw computes the hash from disk and appends+fsyncs its manifest.
        self.interrupt("after_raw_hash", n)
        self.interrupt("after_manifest_commit", n)
        if self.checkpoint:
            self.checkpoint.transition(self.unit_id, "RAW_COMMITTED",
                                       last_call=n, raw_hash=committed.raw_hash)
        return {"text": raw_path.read_text(), "provider": provider}


class DeterministicFakeClient:
    """Offline-only scripted client. It has no network imports or transport."""
    offline_only = True
    model = FROZEN_REQUESTED_MODEL
    provider_route = FROZEN_PROVIDER_ROUTE
    endpoint = FROZEN_PROVIDER_ENDPOINT

    def validate_local_configuration(self) -> dict:
        return {"configured": True, "model": self.model,
                "provider_route": self.provider_route,
                "endpoint": self.endpoint, "network_requests": 0}

    def __init__(self, script=None, provider=None):
        self.script = list(script or [])
        self.provider = dict(provider or {
            "resolved_model": "fake/deterministic", "provider_route": "offline",
            "generation_id": "fake-generation", "finish_reason": "stop",
            "prompt_tokens": 101, "completion_tokens": 37, "total_tokens": 138,
            "retries": 0, "latency_ms": 1.25, "cost": None})
        self.call_count = 0

    def complete(self, system, user, requested_max_tokens, temperature):
        self.call_count += 1
        text = self.script.pop(0) if self.script else (
            '{"candidate_loci":["authority","environment"],'
            '"unresolved_distinction":null,"selected_action":null,'
            '"expected_information":null,"proposed_settlement":"authority"}')
        return {"text": text, "provider": dict(self.provider)}


def _unit_binding(stage: str, namespace: str, case_id: str, arm: str, rep: int) -> str:
    return _sha256(_canonical({"experiment": EXPERIMENT_ID, "fingerprint": EXPECTED_EXECUTABLE_FINGERPRINT,
                               "stage": stage, "namespace": namespace,
                               "case_id": case_id, "arm": arm, "rep": rep}))


def _run_frozen_unit(run_dir: Path, client: ModelClient, arm="R1",
                     stage="pilot", namespace=PILOT_NAMESPACE, case_id="cep-07",
                     rep=1, interrupt=None, *, require_offline=False,
                     endpoint=FROZEN_PROVIDER_ROUTE,
                     requested_model=FROZEN_REQUESTED_MODEL
                     ) -> tuple[UnitResult, DurableReplayClient]:
    """Run one frozen unit through the shared durable orchestration path."""
    if require_offline and not getattr(client, "offline_only", False):
        raise ExecutionRefused("offline dry-run accepts only offline_only clients")
    if arm not in ARMS:
        raise ExecutionRefused("arm is not frozen")
    uid = unit_id(EXPERIMENT_ID, namespace, case_id, arm, rep,
                  EXPECTED_EXECUTABLE_FINGERPRINT)
    checkpoint = CheckpointStore(run_dir / "checkpoint.json")
    binding = _unit_binding(stage, namespace, case_id, arm, rep)
    if checkpoint.status(uid) == "PLANNED":
        checkpoint.transition(uid, "PLANNED", unit_binding=binding)
    replay = DurableReplayClient(client, uid, run_dir, checkpoint=checkpoint,
                                 interrupt=interrupt)
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
    if case_id == "fixture-01":  # retained solely for predecessor unit tests
        initial = {"question": "Which failure locus does the recorded failure belong to?",
                   "initial_evidence": "offline fixture: verifier exit 101; permission denied"}
        observations, discriminating = _fixture(), ("inspect_permissions",)
    else:
        initial = initial_state_for(case_id)
        observations = neutralized_observations()[0][case_id]
        hidden = json.loads(CORPUS_FILES["hidden_meta"].read_text())
        discriminating = tuple(hidden[case_id]["discriminating_probes"])
        assert_marker_free(initial["question"])
        assert_marker_free(initial["initial_evidence"])
        assert_marker_free(observations)
    ctrl = DiagnosticController(
        ARMS[arm], LEGACY_CONTRACT, PROBES, DESCRIPTIONS, COSTS, VOCAB,
        replay, endpoint=endpoint, requested_model=requested_model,
        evidence_selection_policy=EVIDENCE_POLICY[arm], disclosure_k=3,
        masked_candidate_universe=(
            DIRECT_CANDIDATE_UNIVERSE
            if EVIDENCE_POLICY[arm] == "MASKED_K" else ()))
    result = ctrl.run_unit(uid, EXPERIMENT_ID, namespace, case_id, rep,
                           initial["question"], initial["initial_evidence"],
                           observations, discriminating)
    checkpoint.transition(uid, "PARSED", unit_binding=binding)
    if interrupt:
        interrupt("after_parse", replay.calls)
    commit_result(run_dir, result, replay.ledger)
    checkpoint.transition(uid, "RESULT_COMMITTED", unit_binding=binding,
                          result_hash=_file_hash(run_dir / "results" / f"{uid}.json"))
    return result, replay


def run_offline_unit(run_dir: Path, client: ModelClient, arm="R1",
                     stage="pilot", namespace=PILOT_NAMESPACE, case_id="cep-07",
                     rep=1, interrupt=None) -> tuple[UnitResult, DurableReplayClient]:
    """Exercise the frozen path with a client structurally unable to network."""
    return _run_frozen_unit(
        run_dir, client, arm, stage, namespace, case_id, rep, interrupt,
        require_offline=True, endpoint="offline",
        requested_model="fake/deterministic")


def commit_result(run_dir: Path, result: UnitResult, call_ledger: CommitLedger) -> str:
    """Commit an analysis-ready result only after every raw call verifies."""
    for execution in result.executions:
        n = execution["call_num"]
        call_id = f"{result.unit_id}:call:{n:03d}"
        actual = call_ledger.assert_scorable(call_id)
        if actual != execution["raw_hash"]:
            raise CommitViolation("controller execution is not bound to committed raw bytes")
    result_path = run_dir / "results" / f"{result.unit_id}.json"
    ledger = CommitLedger(str(run_dir / "result-commits.jsonl"))
    result_doc = result.to_dict()
    result_doc["runner_contract"] = {
        "experiment_id": EXPERIMENT_ID,
        "design_fingerprint": EXPECTED_DESIGN_FINGERPRINT,
        "executable_contract_fingerprint": EXPECTED_EXECUTABLE_FINGERPRINT,
        "governance_class": GOVERNANCE_CLASS[result.condition],
        "budget_semantics": ARMS[result.condition].budget_semantics,
        "actual_implied_evidence_payment": sum(
            int(r.get("payment_charged", 0) if isinstance(r, dict)
                else r.payment_charged) for r in result.rounds),
        "gate_blind_last_model_claim": result.last_model_claim,
        "governed_final_settlement": result.final_locus,
        "arm_spec": dataclasses.asdict(ARMS[result.condition]) | {
            "disclosure": ARMS[result.condition].disclosure.value},
        "leakage_policy_version": LEAKAGE_POLICY_VERSION,
    }
    if result.case_id != "fixture-01":
        inputs = load_case_inputs_v2()
        result_doc["runner_contract"].update(covariates_for(result.case_id, inputs))
        observations = neutralized_observations()[0]
        hidden = json.loads(CORPUS_FILES["hidden_meta"].read_text())
        _, delivery = selected_delivery(result.case_id, result.rep,
                                        result.condition, observations, hidden)
        payloads = [o.payload for o in delivery.observations]
        result_doc["runner_contract"].update({
            "masked_k_implied_cost": (delivery.payment_charged
                                      if EVIDENCE_POLICY[result.condition] == "MASKED_K" else None),
            "masked_k_realized_k": (len(delivery.delivered_probes)
                                     if EVIDENCE_POLICY[result.condition] == "MASKED_K" else None),
            "evidence_policy": EVIDENCE_POLICY[result.condition],
            "evidence_membership": list(delivery.delivered_probes),
            "evidence_set_hash": _sha256(_canonical({
                "probes": delivery.delivered_probes,
                "handles": [o.handle for o in delivery.observations],
                "payloads": payloads})),
            "semantic_evidence_hash": _sha256(_canonical(payloads)),
            "case_input_binding_sha256": inputs["case_inputs_sha256"],
        })
    payload = json.dumps(result_doc, sort_keys=True, separators=(",", ":"),
                         default=str)
    entry = ledger.commit_raw(result.unit_id, str(result_path), payload)
    return entry.raw_hash or ""


def score_committed_result(run_dir: Path, uid: str, scorer: Callable[[dict], dict]) -> dict:
    """Scoring handoff: consume verified disk bytes and commit one score only."""
    ledger = CommitLedger(str(run_dir / "result-commits.jsonl"))
    ledger.assert_scorable(uid)
    result = json.loads((run_dir / "results" / f"{uid}.json").read_text())
    # Re-verify all raw response identities before scientific scoring.
    calls = CommitLedger(str(run_dir / "call-commits.jsonl"))
    for execution in result.get("executions", []):
        n = execution["call_num"]
        actual = calls.assert_scorable(f"{uid}:call:{n:03d}")
        if actual != execution["raw_hash"]:
            raise CommitViolation("score refused: raw response hash mismatch")
    score_path = run_dir / "scores" / f"{uid}.json"
    scores = CommitLedger(str(run_dir / "score-commits.jsonl"))
    if scores.get(uid):
        scores.assert_scorable(uid)
        return json.loads(score_path.read_text())
    score = scorer(result)
    scores.commit_raw(uid, str(score_path), json.dumps(
        score, sort_keys=True, separators=(",", ":"), default=str))
    checkpoint = CheckpointStore(run_dir / "checkpoint.json")
    checkpoint.transition(uid, "SCORED")
    return score


def dry_run(output: Path) -> dict:
    """Offline validation only; cannot instantiate the live client."""
    hashes = frozen_hashes()
    ns = RunNamespace(output, "offline-validation", "pilot", hashes["sources"])
    run_dir = ns.create_or_verify()
    fake = DeterministicFakeClient()
    result, replay = run_offline_unit(run_dir, fake)
    scored = score_committed_result(run_dir, result.unit_id,
                                    lambda r: {"analysis_handoff": True,
                                               "last_model_claim": r["last_model_claim"],
                                               "final_settlement": r["final_locus"]})
    return {"mode": "OFFLINE_DRY_RUN", "network_transport_instantiated": False,
            "model_calls_performed": 0, "provider_calls_performed": 0,
            "fake_client_calls": fake.call_count, "unit_id": result.unit_id,
            "terminal_state": result.terminal_state,
            "raw_calls_committed": replay.ledger.provenance_summary()["verified"],
            "scoring_handoff": scored,
            "design_fingerprint": EXPECTED_DESIGN_FINGERPRINT,
            "executable_contract_fingerprint": EXPECTED_EXECUTABLE_FINGERPRINT}


def _live_client(model=FROZEN_REQUESTED_MODEL):  # pragma: no cover - stubbed in tests
    """Construct the existing provider adapter; construction performs no I/O."""
    from .provider import ZaiCodingPlanClient
    return ZaiCodingPlanClient(model=model)


def _cmd_run(args) -> int:  # pragma: no cover - exercised with fakes in tests
    matrix = load_frozen_matrix(Path(args.matrix), args.stage, OWNER_STATE)
    provisional = GateRequest(
        args.stage, os.getenv("SXR_ABLATION_RELEASE"),
        Path(os.environ["SXR_ABLATION_RELEASE_MARKER"])
        if os.getenv("SXR_ABLATION_RELEASE_MARKER") else None,
        None, matrix, args.run_id, Path(args.output))

    # Phase A must complete before a network-capable object even exists.
    static = ExecutionGate.validate_static(provisional)
    client = _live_client(model=FROZEN_REQUESTED_MODEL)
    final_request = dataclasses.replace(provisional, client=client)
    client_gate = ExecutionGate.validate_client(final_request, static)
    final = ExecutionGate.finalize(static, client_gate)
    if final.get("phase") != "FINAL_AUTHORIZED" or final.get("authorized") is not True:
        raise ExecutionRefused("final authorization result is not authoritative")

    # No filesystem namespace or unit exists before final authorization.
    namespace_hashes = dict(final["hashes"]["sources"])
    namespace_hashes.update({
        "matrix_sha256": matrix.matrix_hash,
        "case_inputs_sha256": matrix.case_inputs_hash,
        "owner_state_digest": owner_state_digest(OWNER_STATE),
        "release_marker_sha256": _file_hash(final_request.release_marker),
        "release_token_sha256": _sha256(final_request.release_token.encode()),
        "final_authorization_binding": final["binding"],
        "runner_implementation_sha256": static["runner_implementation_sha256"],
        "reviewed_runner_commit": static["reviewed_runner_commit"],
    })
    ns = RunNamespace(Path(args.output), args.run_id, args.stage,
                      namespace_hashes)
    run_dir = ns.create_or_verify()
    checkpoint = CheckpointStore(run_dir / "checkpoint.json")
    expected_uids = iter(matrix.uids())
    completed, resumed = 0, 0
    for case_id in matrix.cases:
        for arm in matrix.arms:
            for rep in range(1, matrix.repetitions + 1):
                expected_uid = next(expected_uids)
                status_before = checkpoint.status(expected_uid)
                result, _ = _run_frozen_unit(
                    run_dir, client, arm=arm, stage=args.stage,
                    namespace=matrix.namespace, case_id=case_id, rep=rep,
                    endpoint=FROZEN_PROVIDER_ROUTE,
                    requested_model=FROZEN_REQUESTED_MODEL)
                if result.unit_id != expected_uid:
                    raise ResumeViolation("unit loop UID differs from frozen matrix")
                completed += 1
                resumed += int(status_before in ("RESULT_COMMITTED", "SCORED"))
                _atomic_json(run_dir / "execution-summary.json", {
                    "schema": "evidence-revision-execution-summary-v2",
                    "experiment_id": EXPERIMENT_ID, "stage": args.stage,
                    "run_id": args.run_id, "matrix_sha256": matrix.matrix_hash,
                    "executable_contract_fingerprint": EXPECTED_EXECUTABLE_FINGERPRINT,
                    "units_planned": matrix.total_units,
                    "units_completed_or_resumed": completed,
                    "units_resumed": resumed,
                    "provider_calls_performed": CommitLedger(
                        str(run_dir / "call-commits.jsonl")).provenance_summary()["verified"],
                    "model_calls_performed": CommitLedger(
                        str(run_dir / "call-commits.jsonl")).provenance_summary()["verified"],
                    "final_authorization_binding": final["binding"]})
    summary = json.loads((run_dir / "execution-summary.json").read_text())
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    inspect = sub.add_parser("inspect-matrix", help="safe, offline frozen-input inspection")
    inspect.add_argument("--stage", choices=VALID_STAGES, default="pilot")
    dry = sub.add_parser("dry-run", help="offline deterministic orchestration validation")
    dry.add_argument("--output", type=Path, required=True)
    run = sub.add_parser("run", help="live path; owner/release gate applies")
    run.add_argument("--stage", choices=VALID_STAGES, required=True)
    run.add_argument("--matrix", required=True)
    run.add_argument("--run-id", required=True)
    run.add_argument("--output", required=True)
    args = parser.parse_args(argv)
    if args.command == "inspect-matrix":
        print(json.dumps(inspect_matrix(args.stage), indent=2, sort_keys=True))
        return 0
    if args.command == "dry-run":
        print(json.dumps(dry_run(args.output), indent=2, sort_keys=True))
        return 0
    return _cmd_run(args)


if __name__ == "__main__":
    raise SystemExit(main())

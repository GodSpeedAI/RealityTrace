"""Offline artifact builder and compatibility proof for evidence-revision-002."""
from __future__ import annotations

import copy
import functools
import hashlib
import json
import subprocess
from pathlib import Path

from .case_inputs_v2 import initial_state_for, load_case_inputs
from .channel import ActionCatalogue, ObservationChannel, unit_seed
from .controller import TASK_INSTRUCTION
from .dstate import DiagnosticState, missing_state_sections
from .evidence_revision_002 import (
    ARMS, COMPARISONS, EVIDENCE_SELECTION_EXPERIMENT_ID, EXPERIMENT_ID,
    FULL_NAMESPACE, FULL_REPETITIONS, MATCHED_EVIDENCE_USE, OWNER_STATE,
    PREDECESSOR, PRIMARY_ENDPOINT, PRIMARY_POPULATION, REVISION_RATE,
    STATISTICAL_PLAN, contract_fingerprint, freeze as freeze_science)
from .evidence_revision_run import (
    FROZEN_PROVIDER_ENDPOINT, FROZEN_PROVIDER_ROUTE, FROZEN_REQUESTED_MODEL,
    RUNNER_IMPLEMENTATION_FILES, eligible_cases,
    runner_implementation_sha256_at_commit as predecessor_runner_sha_at_commit)
from .ids import unit_id
from .legibility import assert_semantic_equivalence
from .marker_neutralization import assert_marker_free
from .replication import COSTS, DESCRIPTIONS, PROBES, VOCAB
from .validity_revision import (
    DIRECT_CANDIDATE_UNIVERSE, EVIDENCE_POLICY, K, MAX_PAYMENT, PRIMARY_CASES,
    build_artifacts as build_predecessor_artifacts, leakage_report,
    neutralized_observations, pair_equality_report, selected_delivery)

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
OUT = ROOT / "ablation" / "analysis" / "post-audit-v2"
PREDECESSOR_REVIEWED_COMMIT = PREDECESSOR["predecessor_reviewed_runner_commit"]
CASE_INPUTS_PATH = OUT / "case-inputs-v2.json"
PREDECESSOR_MATRIX_PATH = OUT / "evidence-revision-001-pilot-matrix-v2.json"
PREDECESSOR_SCIENCE_PATH = (
    OUT / "evidence-revision-001-revision-002-contract.json")
PREDECESSOR_RUN_JSON_PATH = (
    ROOT / "ablation/runs/evidence-revision-001-revision-002" /
    PREDECESSOR["predecessor_pilot_run"] / "run.json")
FULL_MATRIX_NAME = "evidence-revision-002-full-matrix.json"

REUSED_ARTIFACTS = {
    "case_inputs": OUT / "case-inputs-v2.json",
    "marker_neutralization": OUT / "evidence-revision-001-marker-neutralization-v2.json",
    "masked_k_feasibility": OUT / "evidence-revision-001-masked-k-feasibility-v2.json",
    "masked_k_leakage": OUT / "evidence-revision-001-masked-k-leakage-v2.json",
    "pair_equality": OUT / "evidence-revision-001-pair-equality-v2.json",
}

SUCCESSOR_RUNNER_IMPLEMENTATION_FILES = tuple(RUNNER_IMPLEMENTATION_FILES) + (
    "ablation/eval/v2/evidence_revision.py",
    "ablation/eval/v2/evidence_revision_002.py",
    "ablation/eval/v2/evidence_revision_002_freeze.py",
    "ablation/eval/v2/evidence_revision_002_run.py",
)


def _canonical(value) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), default=str)


def _sha(value) -> str:
    text = value if isinstance(value, str) else _canonical(value)
    return hashlib.sha256(text.encode()).hexdigest()


def _file_sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def runner_implementation_manifest() -> dict:
    files = {}
    for relative in SUCCESSOR_RUNNER_IMPLEMENTATION_FILES:
        path = ROOT / relative
        if not path.is_file():
            raise FileNotFoundError(relative)
        files[relative] = _file_sha(path)
    return {"schema": "evidence-revision-002-runner-implementation-v1",
            "files": files}


def runner_implementation_sha256() -> str:
    return _sha(runner_implementation_manifest())


def latest_runner_implementation_commit() -> str:
    cmd = ["git", "log", "-1", "--format=%H", "--",
           *SUCCESSOR_RUNNER_IMPLEMENTATION_FILES]
    return subprocess.check_output(cmd, cwd=ROOT, text=True).strip()


def runner_implementation_manifest_at_commit(commit: str) -> dict:
    files = {}
    for relative in SUCCESSOR_RUNNER_IMPLEMENTATION_FILES:
        content = subprocess.check_output(
            ["git", "show", f"{commit}:{relative}"], cwd=ROOT)
        files[relative] = hashlib.sha256(content).hexdigest()
    return {"schema": "evidence-revision-002-runner-implementation-v1",
            "files": files}


def runner_implementation_sha256_at_commit(commit: str) -> str:
    return _sha(runner_implementation_manifest_at_commit(commit))


def executable_declarations(design_fp: str) -> dict:
    reused = {name: {"path": str(path.relative_to(ROOT)),
                     "file_sha256": _file_sha(path)}
              for name, path in sorted(REUSED_ARTIFACTS.items())}
    return {
        "schema": "evidence-revision-002-executable-contract-v1",
        "experiment_id": EXPERIMENT_ID,
        "design_fingerprint": design_fp,
        "predecessor_executable_fingerprint": PREDECESSOR["predecessor_executable_fingerprint"],
        "case_inputs_sha256": PREDECESSOR["predecessor_case_inputs_sha256"],
        "case_inputs_reused_unchanged": True,
        "reused_artifacts": reused,
        "arms": list(ARMS),
        "evidence_policy": EVIDENCE_POLICY,
        "masked_k": {"k": K, "max_payment": MAX_PAYMENT,
                     "candidate_universe": list(DIRECT_CANDIDATE_UNIVERSE)},
        "evidence_selection_seed_identity": EVIDENCE_SELECTION_EXPERIMENT_ID,
        "full_plan": {"namespace": FULL_NAMESPACE,
                      "cases": list(PRIMARY_CASES),
                      "arms": list(ARMS),
                      "repetitions": FULL_REPETITIONS},
        "provider_contract": {"route": FROZEN_PROVIDER_ROUTE,
                              "endpoint": FROZEN_PROVIDER_ENDPOINT,
                              "requested_model": FROZEN_REQUESTED_MODEL},
        "primary_endpoint": PRIMARY_ENDPOINT,
        "comparisons": COMPARISONS,
        "statistical_plan": STATISTICAL_PLAN,
        "revision_rate": REVISION_RATE,
        "matched_evidence_use": MATCHED_EVIDENCE_USE,
        "predecessor_pilot_not_in_full_sample": True,
    }


def executable_fingerprint(design_fp: str) -> str:
    return _sha(executable_declarations(design_fp))


def full_matrix_document(design_fp: str, executable_fp: str) -> dict:
    cases, arms = tuple(PRIMARY_CASES), tuple(ARMS)
    uids = [unit_id(EXPERIMENT_ID, FULL_NAMESPACE, case, arm, rep,
                    executable_fp)
            for case in cases for arm in arms
            for rep in range(1, FULL_REPETITIONS + 1)]
    doc = {
        "schema": "evidence-revision-matrix-v3",
        "experiment_id": EXPERIMENT_ID,
        "stage": "full",
        "namespace": FULL_NAMESPACE,
        "cases": list(cases), "arms": list(arms),
        "repetitions": FULL_REPETITIONS,
        "n_cases": len(cases), "n_arms": len(arms),
        "total_units": len(uids), "unit_ids": uids,
        "design_fingerprint": design_fp,
        "executable_contract_fingerprint": executable_fp,
        "case_inputs_sha256": PREDECESSOR["predecessor_case_inputs_sha256"],
        "predecessor_pilot_units_included": False,
        "execution_authorized": False,
    }
    doc["matrix_sha256"] = _sha(doc)
    return doc


def _vector_from_plan(case_id: str, arm: str, rep: int,
                      catalogue, delivery) -> dict:
    initial = initial_state_for(case_id)
    schema = (catalogue.render(DESCRIPTIONS, COSTS)
              if ARMS[arm].schema_visible else "(not shown in this condition)")
    state = DiagnosticState(
        question=initial["question"], case_id=case_id,
        initial_evidence=initial["initial_evidence"], action_schema=schema,
        locus_vocabulary=VOCAB, max_rounds=ARMS[arm].max_rounds,
        max_payment=ARMS[arm].max_payment,
        available_actions=catalogue.handles,
        budget_semantics=ARMS[arm].budget_semantics,
        observation_rendering=ARMS[arm].observation_rendering)
    prompt = TASK_INSTRUCTION + "\n\n" + state.render()
    assert_marker_free(prompt)
    if missing_state_sections(state.render()):
        raise AssertionError("initial state continuity failure")
    payloads = [observation.payload for observation in delivery.observations]
    return {
        "case_id": case_id, "arm": arm, "repetition": rep,
        "initial_model_input_sha256": _sha(initial),
        "arm_spec_sha256": _sha(ARMS[arm].__dict__ | {
            "disclosure": ARMS[arm].disclosure.value}),
        "action_schema_sha256": _sha(schema),
        "handle_probe_cost_sha256": _sha([
            [handle, catalogue.probe_of(handle), COSTS[catalogue.probe_of(handle)]]
            for handle in catalogue.handles]),
        "delivery_membership": list(delivery.delivered_probes),
        "delivery_payment": delivery.payment_charged,
        "delivery_sha256": _sha({"probes": delivery.delivered_probes,
                                  "handles": [observation.handle
                                              for observation in delivery.observations],
                                  "payloads": payloads}),
        "semantic_evidence_sha256": _sha(payloads),
        "first_prompt_sha256": _sha(prompt),
    }


def _predecessor_execution_vector(case_id: str, arm: str, rep: int) -> dict:
    observations = neutralized_observations()[0]
    hidden = json.loads((ROOT / "ablation/corpus/acquisition-hidden-meta.json").read_text())
    catalogue, delivery = selected_delivery(case_id, rep, arm,
                                             observations, hidden)
    return _vector_from_plan(case_id, arm, rep, catalogue, delivery)


def _successor_execution_vector(case_id: str, arm: str, rep: int,
                                seed_experiment_id: str) -> dict:
    observations = neutralized_observations()[0]
    hidden = json.loads((ROOT / "ablation/corpus/acquisition-hidden-meta.json").read_text())
    policy = EVIDENCE_POLICY[arm]
    seed = unit_seed(seed_experiment_id, "evidence-selection", case_id,
                     policy, rep)
    catalogue = ActionCatalogue(PROBES, seed)
    channel = ObservationChannel(
        catalogue, COSTS, ARMS[arm].disclosure, k=K,
        masked_candidate_universe=(DIRECT_CANDIDATE_UNIVERSE
                                   if policy == "MASKED_K" else ()))
    delivery = channel.deliver(
        observations[case_id], hidden[case_id]["discriminating_probes"], seed + 1)
    return _vector_from_plan(case_id, arm, rep, catalogue, delivery)


def predecessor_pilot_compatibility() -> dict:
    predecessor_science = json.loads(PREDECESSOR_SCIENCE_PATH.read_text())
    rebuilt_predecessor_science = json.loads(json.dumps(
        build_predecessor_artifacts()[
            "evidence-revision-001-revision-002-contract.json"], default=str))
    for frozen_field in ("marker_policy", "masked_k", "evidence_seed_scope",
                         "unchanged_science"):
        if predecessor_science[frozen_field] != rebuilt_predecessor_science[frozen_field]:
            raise AssertionError(
                f"predecessor revision-002 scientific field drift: {frozen_field}")
    if predecessor_science["design_fingerprint"] != PREDECESSOR[
            "predecessor_design_fingerprint"]:
        raise AssertionError("predecessor revision-002 design identity drift")
    for field in ("marker_policy", "masked_k", "evidence_seed_scope",
                  "unchanged_science"):
        successor_preserved = json.loads(json.dumps(
            freeze_science()["preserved_predecessor_revision_002_science"],
            default=str))
        if predecessor_science[field] != successor_preserved[field]:
            raise AssertionError(f"successor failed to inherit predecessor {field}")
    matrix = json.loads(PREDECESSOR_MATRIX_PATH.read_text())
    matrix_check = dict(matrix); matrix_hash = matrix_check.pop("matrix_sha256")
    if _sha(matrix_check) != matrix_hash:
        raise AssertionError("predecessor pilot matrix canonical hash drift")
    if matrix_hash != PREDECESSOR["predecessor_pilot_matrix_sha256"]:
        raise AssertionError("predecessor pilot matrix identity drift")
    expected_uids = [unit_id(
        PREDECESSOR["predecessor_experiment_id"], matrix["namespace"],
        case, arm, rep, PREDECESSOR["predecessor_executable_fingerprint"])
        for case in matrix["cases"] for arm in matrix["arms"]
        for rep in range(1, matrix["repetitions"] + 1)]
    if expected_uids != matrix["unit_ids"] or len(expected_uids) != len(set(expected_uids)):
        raise AssertionError("predecessor pilot matrix UID drift")
    run_doc = json.loads(PREDECESSOR_RUN_JSON_PATH.read_text())
    run_expected = {
        "experiment_id": PREDECESSOR["predecessor_experiment_id"],
        "stage": "pilot", "run_id": PREDECESSOR["predecessor_pilot_run"],
        "design_fingerprint": PREDECESSOR["predecessor_design_fingerprint"],
        "executable_contract_fingerprint": PREDECESSOR[
            "predecessor_executable_fingerprint"]}
    for key, value in run_expected.items():
        if run_doc.get(key) != value:
            raise AssertionError(f"predecessor run.json drift: {key}")
    source_expected = {
        "matrix_sha256": PREDECESSOR["predecessor_pilot_matrix_sha256"],
        "case_inputs_sha256": PREDECESSOR["predecessor_case_inputs_sha256"],
        "runner_implementation_sha256": PREDECESSOR[
            "predecessor_runner_implementation_sha256"],
        "reviewed_runner_commit": PREDECESSOR[
            "predecessor_reviewed_runner_commit"]}
    for key, value in source_expected.items():
        if run_doc["source_hashes"].get(key) != value:
            raise AssertionError(f"predecessor run source binding drift: {key}")
    if predecessor_runner_sha_at_commit(PREDECESSOR_REVIEWED_COMMIT) != (
            PREDECESSOR["predecessor_runner_implementation_sha256"]):
        raise AssertionError("predecessor reviewed implementation aggregate drift")
    source_rows = []
    for relative in RUNNER_IMPLEMENTATION_FILES:
        current = (ROOT / relative).read_bytes()
        reviewed = subprocess.check_output(
            ["git", "show", f"{PREDECESSOR_REVIEWED_COMMIT}:{relative}"], cwd=ROOT)
        row = {"path": relative, "current_sha256": hashlib.sha256(current).hexdigest(),
               "reviewed_sha256": hashlib.sha256(reviewed).hexdigest(),
               "byte_equal": current == reviewed}
        source_rows.append(row)
    if not all(row["byte_equal"] for row in source_rows):
        raise AssertionError("predecessor payment-bearing runtime drift")
    vectors, counterfactual_changed = [], 0
    for case in matrix["cases"]:
        for arm in matrix["arms"]:
            for rep in range(1, matrix["repetitions"] + 1):
                predecessor_vector = _predecessor_execution_vector(case, arm, rep)
                successor_vector = _successor_execution_vector(
                    case, arm, rep, EVIDENCE_SELECTION_EXPERIMENT_ID)
                if predecessor_vector != successor_vector:
                    raise AssertionError("predecessor/successor execution vector drift")
                counterfactual = _successor_execution_vector(
                    case, arm, rep, EXPERIMENT_ID)
                counterfactual_changed += int(counterfactual != predecessor_vector)
                vectors.append({"coordinates": [case, arm, rep],
                                "vector_sha256": _sha(successor_vector)})
    if counterfactual_changed == 0:
        raise AssertionError("seed-domain compatibility check is not discriminating")
    doc = {
        "schema": "predecessor-pilot-execution-compatibility-v1",
        "status": "PREDECESSOR_PILOT_ACCEPTED",
        "predecessor": PREDECESSOR,
        "predecessor_run_json_verified": True,
        "predecessor_revision_002_scientific_artifact_verified": True,
        "predecessor_directory_manifest_sha256_expected": PREDECESSOR[
            "predecessor_pilot_manifest_sha256"],
        "outcome_artifacts_read": False,
        "allowed_deltas": ["experiment/revision identity", "design/executable identity",
                           "FULL namespace/matrix/UIds", "owner authorization identity",
                           "endpoint metadata/disposition", "revision_rate declaration"],
        "behavioral_deltas": [],
        "evidence_selection_seed_identity": EVIDENCE_SELECTION_EXPERIMENT_ID,
        "runtime_source_equality": source_rows,
        "execution_vectors_compared": len(vectors),
        "execution_vectors_equal": True,
        "counterfactual_new_experiment_seed_vectors_changed": counterfactual_changed,
        "execution_vectors": vectors,
        "coverage": {
            "provider_calls": "provider.py byte-equal",
            "state_continuity": "dstate.py/controller.py byte-equal plus required initial sections",
            "persistence": "ledger.py/evidence_revision_run.py byte-equal; successor adapter regression-tested",
            "prompt_leak_prevention": "channel.py/marker neutralizer byte-equal plus prompt scan",
            "evidence_delivery": "all predecessor pilot execution vectors equal",
            "rendering": "legibility.py byte-equal",
            "pairing": "ids.py and both matrix UID sets verified",
            "parsing": "decision.py byte-equal",
            "raw_before_score_ordering": "execution/ledger/runner core bytes equal"},
        "pilot_rerun_required": False,
        "provider_calls_performed": 0, "model_calls_performed": 0,
    }
    doc["compatibility_sha256"] = _sha(doc)
    return doc


def full_matrix_validation(matrix: dict) -> dict:
    if tuple(matrix["cases"]) != tuple(PRIMARY_CASES):
        raise AssertionError("FULL population drift")
    if tuple(matrix["cases"]) != tuple(eligible_cases()):
        raise AssertionError("FULL exclusions do not independently re-derive")
    if tuple(matrix["arms"]) != tuple(ARMS):
        raise AssertionError("FULL arms drift")
    if matrix["repetitions"] != FULL_REPETITIONS:
        raise AssertionError("FULL repetitions drift")
    if len(matrix["unit_ids"]) != len(set(matrix["unit_ids"])):
        raise AssertionError("FULL UIDs are not unique")
    observations = neutralized_observations()[0]
    hidden = json.loads((ROOT / "ablation/corpus/acquisition-hidden-meta.json").read_text())
    leakage = leakage_report(observations, hidden)
    if leakage["singleton_equivalent_count"] or leakage["unaffordable_count"]:
        raise AssertionError("MASKED_K answer-channel/affordability regression")
    masked_rows = 0
    for case in PRIMARY_CASES:
        assert_marker_free(initial_state_for(case))
        assert_marker_free(observations[case])
        for rep in range(1, FULL_REPETITIONS + 1):
            deliveries = {}
            for arm in ARMS:
                _, delivery = selected_delivery(case, rep, arm, observations, hidden)
                deliveries[arm] = delivery
                for obs in delivery.observations:
                    assert_semantic_equivalence(obs.payload)
                if EVIDENCE_POLICY[arm] == "MASKED_K":
                    masked_rows += 1
                    if len(delivery.delivered_probes) != K:
                        raise AssertionError("MASKED_K exact K failure")
                    if delivery.payment_charged > MAX_PAYMENT:
                        raise AssertionError("MASKED_K affordability failure")
            for a, b in (("R1", "R2"), ("R3", "R4")):
                da, db = deliveries[a], deliveries[b]
                if (da.delivered_probes != db.delivered_probes or
                    [o.payload for o in da.observations] !=
                    [o.payload for o in db.observations]):
                    raise AssertionError(f"paired evidence mismatch {a}/{b}")
    doc = {
        "schema": "evidence-revision-002-full-matrix-validation-v1",
        "matrix_sha256": matrix["matrix_sha256"],
        "expected_cases": len(PRIMARY_CASES), "expected_arms": len(ARMS),
        "expected_repetitions": FULL_REPETITIONS,
        "expected_units": len(PRIMARY_CASES) * len(ARMS) * FULL_REPETITIONS,
        "checks": {
            "population_and_exclusions": "PASS", "arms": "PASS",
            "unique_uids": "PASS", "complete_pairing": "PASS",
            "R1_R2_evidence_equality": "PASS", "R3_R4_evidence_equality": "PASS",
            "masked_k_exact_k": "PASS", "affordability": "PASS",
            "raw_legible_semantic_equivalence": "PASS",
            "answer_channel_leak_scan": "PASS"},
        "masked_k_deliveries_checked": masked_rows,
        "provider_calls_performed": 0, "model_calls_performed": 0,
    }
    doc["validation_sha256"] = _sha(doc)
    return doc


@functools.lru_cache(maxsize=1)
def _build_artifacts_cached() -> dict:
    science = freeze_science()
    design_fp = science["design_fingerprint"]
    executable_fp = executable_fingerprint(design_fp)
    matrix = full_matrix_document(design_fp, executable_fp)
    compatibility = predecessor_pilot_compatibility()
    validation = full_matrix_validation(matrix)
    runner_manifest = runner_implementation_manifest()
    runner_sha = _sha(runner_manifest)
    reviewed_commit = latest_runner_implementation_commit()
    try:
        reviewed_sha = (runner_implementation_sha256_at_commit(reviewed_commit)
                        if reviewed_commit else None)
    except subprocess.CalledProcessError:
        reviewed_sha = None
    reviewed_current = reviewed_sha == runner_sha
    executable = dict(executable_declarations(design_fp),
                      status="FROZEN_NOT_AUTHORIZED",
                      executable_contract_fingerprint=executable_fp,
                      full_matrix_sha256=matrix["matrix_sha256"],
                      predecessor_pilot_compatibility_sha256=compatibility["compatibility_sha256"],
                      runner_implementation_sha256=runner_sha,
                      reviewed_runner_commit=reviewed_commit,
                      reviewed_runner_implementation_sha256=reviewed_sha,
                      reviewed_runner_matches_current=reviewed_current,
                      owner_state=OWNER_STATE,
                      execution_authorized=False,
                      additional_provider_calls_authorized=False,
                      full_execution_authorized=False,
                      provider_calls_performed=0, model_calls_performed=0)
    lineage = {
        "schema": "evidence-revision-002-lineage-v1",
        "experiment_id": EXPERIMENT_ID, "predecessor": PREDECESSOR,
        "design_fingerprint": design_fp,
        "executable_contract_fingerprint": executable_fp,
        "runner_implementation_sha256": runner_sha,
        "reviewed_runner_commit": reviewed_commit,
        "reviewed_runner_implementation_sha256": reviewed_sha,
        "reviewed_runner_matches_current": reviewed_current,
        "full_matrix_sha256": matrix["matrix_sha256"],
        "case_inputs_sha256": PREDECESSOR["predecessor_case_inputs_sha256"],
        "semantic_delta": science["semantic_delta"],
        "runtime_behavior_delta": [],
        "predecessor_pilot_disposition": "VALID_PREDECESSOR_INSTRUMENT_VALIDATION",
        "pilot_rerun_required": False,
    }
    return {
        "evidence-revision-002-contract.json": science,
        "evidence-revision-002-executable-contract.json": executable,
        FULL_MATRIX_NAME: matrix,
        "evidence-revision-002-full-matrix-validation.json": validation,
        "evidence-revision-002-predecessor-pilot-compatibility.json": compatibility,
        "evidence-revision-002-runner-implementation.json": dict(
            runner_manifest, runner_implementation_sha256=runner_sha,
            reviewed_runner_commit=reviewed_commit,
            reviewed_runner_implementation_sha256=reviewed_sha,
            reviewed_runner_matches_current=reviewed_current),
        "evidence-revision-002-lineage.json": lineage,
    }


def build_artifacts() -> dict:
    """Return a defensive copy of one deterministic offline build."""
    return copy.deepcopy(_build_artifacts_cached())


def freeze() -> dict:
    artifacts = build_artifacts()
    OUT.mkdir(parents=True, exist_ok=True)
    for name, doc in artifacts.items():
        (OUT / name).write_text(json.dumps(doc, indent=2, sort_keys=True,
                                          default=str) + "\n")
    return artifacts


if __name__ == "__main__":
    docs = freeze()
    print(json.dumps(docs["evidence-revision-002-lineage.json"],
                     indent=2, sort_keys=True))

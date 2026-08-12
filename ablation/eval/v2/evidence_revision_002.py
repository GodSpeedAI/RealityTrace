"""Frozen scientific contract for successor experiment ``evidence-revision-002``.

This module is offline and side-effect free.  It derives the preserved science
from the immutable predecessor declaration, then applies only the owner-selected
secondary-endpoint disposition and the pre-pilot typed ``revision_rate`` closure.
No predecessor pilot outcome is an input.
"""
from __future__ import annotations

import copy
import hashlib
import json

from . import evidence_revision as predecessor
from .validity_revision import scientific_declarations as predecessor_revision_declarations

EXPERIMENT_ID = "evidence-revision-002"
DESIGN_VERSION = "evidence-revision-design-v3"
FULL_NAMESPACE = "full:er002-full-v1"
# The predecessor's only prospectively frozen repeated-measurement count was 2.
# The owner ordered repetitions not to change; no pilot outcome is an input.
FULL_REPETITIONS = 2
EVIDENCE_SELECTION_EXPERIMENT_ID = "evidence-revision-001"
PREDECESSOR_MARKER_MANIFEST_SHA256 = (
    "6894ef508337acab7b409794818d2417a46a0399492b606ca867f79c37186539")

PREDECESSOR = {
    "predecessor_experiment_id": "evidence-revision-001",
    "predecessor_revision": "revision-002",
    "predecessor_pilot_run": "evidence-revision-001-revision-002-pilot-20260811T221724Z",
    "predecessor_pilot_manifest_sha256": "ae2827c1ea32c2860e53f19b763955908860865266791932f1454252ffeb2a13",
    "predecessor_run_json_sha256": "2be01bcdf2320c8f07b800000dcb5ed1057365b09ccc9ba304b25dede32cc989",
    "predecessor_design_fingerprint": "3031af33611c9ae8ab0fd0bba22f0abc35147d4fe564e0e3623a174d0dfe28d9",
    "predecessor_executable_fingerprint": "12eb26d72f74709a827059c2a6ccb693ef956a6d5aa7ca2bf6f292b645da010e",
    "predecessor_runner_implementation_sha256": "56f4463c2b228bb31f2c142d15e5431b8bd345114bd3707e14d386b1edfcf066",
    "predecessor_case_inputs_sha256": "20fbaf97235b9bb4144582ae0bdf1527bb10a12394da38ee09931c7af4f91e1b",
    "predecessor_pilot_matrix_sha256": "effd44ec461b8624e4c498b80f40884f1af0d30296b9062bda3a1f090d85a9e3",
    "predecessor_reviewed_runner_commit": "cf9b28b84a415ef33519e0866536ebd7787d567d",
    "pilot_disposition": "VALID_PREDECESSOR_INSTRUMENT_VALIDATION",
    "pilot_may_establish": [
        "live_runner_reachability", "provider_compatibility",
        "persistence_correctness", "state_continuity", "pairing_integrity",
        "leak_prevention", "output_contract_viability",
        "analysis_reconstructability"],
    "pilot_must_not_establish": [
        "successor_confirmatory_evidence", "successor_FULL_sample_membership",
        "successor_parameter_selection", "successor_endpoint_tuning"],
}

REVISION_RATE = {
    "status": "EVALUATED_DERIVED_TYPED",
    "reporting_unit": "case_repetition",
    "population": "PRIMARY_POPULATION",
    "reported_by": "arm",
    "denominator": (
        "round1.proposed_settlement is not null AND "
        "round1.proposed_settlement != evaluator true_locus"),
    "numerator": (
        "denominator AND final gate-blind claim != "
        "round1.proposed_settlement"),
    "null_round1": "EXCLUDE_NO_SETTLEMENT_NO_CLAIM",
    "unknown_round1": (
        "INCLUDE_AS_WRONG_WHEN_EXPLICIT_UNKNOWN_DIFFERS_FROM_EVALUATOR_TRUTH"),
    "unresolved_distinction": "STATE_METADATA_NOT_A_CLAIM",
    "final_claim": "gate_blind_last_model_claim",
    "zero_denominator": "NOT_ESTIMABLE",
    "source_basis": [
        "ablation/eval/v2/evidence_revision.py::SECONDARY_ENDPOINTS[revision_rate]",
        "ablation/eval/v2/decision.py::ControllerDecision",
        "ablation/eval/v2/replication.py::VOCAB",
        "ablation/eval/v2/controller.py::TASK_INSTRUCTION/FORCED_FINAL_INSTRUCTION/DiagnosticController",
        "ablation/eval/v2/dstate.py::DiagnosticState.with_decision",
        "ablation/eval/v2/evidence_revision.py::PRIMARY_POPULATION/PRIMARY_ENDPOINT"],
    "semantic_origin": "PRE_PILOT_TYPED_SEMANTICS_NOT_PILOT_OUTCOMES",
}

MATCHED_EVIDENCE_USE = {
    "status": "NOT_EVALUATED_UNDER_THIS_EXPERIMENT",
    "predecessor_status": "PRESENT_BUT_NOT_OPERATIONALLY_FROZEN",
    "estimand": None,
    "numerator": None,
    "denominator": None,
    "value": None,
    "reason": (
        "No pre-pilot frozen rule uniquely specified arm pairing, evidence "
        "construct, utilization indicator, gate-blind versus governed scoring, "
        "population/exclusions, repetition aggregation, or missing-pair handling."),
    "post_pilot_semantics_prohibited": True,
    "derived_proxy_permitted": False,
    "direct_utilization_instrument_present": False,
    "utilization_claim_permitted": False,
    "absence_is_zero_utilization": False,
    "historical_73_7_percent_disposition": (
        "HISTORICAL_EVIDENCE_UNDER_ITS_OWN_SCORING_CONTRACT_NOT_IMPORTED"),
}

OWNER_STATE = {
    "status": "FROZEN_NOT_AUTHORIZED",
    "execution_authorized": False,
    "additional_provider_calls_authorized": False,
    "authorized_stage": None,
    "full_execution_authorized": False,
    "release_token_issued": False,
    "release_marker_issued": False,
    "power_issue": "OWNER_ACKNOWLEDGED_UNRESOLVED",
    "original_10pp_threshold_preserved": True,
    "predecessor_pilot_disposition": "VALID_PREDECESSOR_INSTRUMENT_VALIDATION",
    "predecessor_pilot_manifest_sha256": PREDECESSOR["predecessor_pilot_manifest_sha256"],
}

# Runtime/scientific aliases are byte-for-byte predecessor declarations.
ARMS = predecessor.ARMS
ARM_ROLE = predecessor.ARM_ROLE
GOVERNANCE_CLASS = predecessor.GOVERNANCE_CLASS
PRIMARY_POPULATION = predecessor.PRIMARY_POPULATION
PRIMARY_ENDPOINT = predecessor.PRIMARY_ENDPOINT
EXCLUSIONS = predecessor.EXCLUSIONS
COMPARISONS = predecessor.COMPARISONS
STATISTICAL_PLAN = predecessor.STATISTICAL_PLAN
MANDATORY_COVARIATES = predecessor.MANDATORY_COVARIATES


def _canonical(value) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), default=str)


def _base_contract() -> dict:
    doc = copy.deepcopy(predecessor.contract())
    predecessor_secondaries = doc["secondary_endpoints"]
    predecessor_secondaries.pop("matched_evidence_use")
    predecessor_secondaries["revision_rate"] = copy.deepcopy(REVISION_RATE)
    revision_002 = predecessor_revision_declarations(
        PREDECESSOR["predecessor_case_inputs_sha256"],
        PREDECESSOR_MARKER_MANIFEST_SHA256)
    preserved_revision_002 = {
        key: copy.deepcopy(revision_002[key])
        for key in ("marker_policy", "masked_k", "evidence_seed_scope",
                    "unchanged_science")}
    doc.update({
        "experiment_id": EXPERIMENT_ID,
        "status": OWNER_STATE["status"],
        "instrument_version": DESIGN_VERSION,
        "secondary_endpoints": predecessor_secondaries,
        "unavailable_endpoints": {
            "matched_evidence_use": copy.deepcopy(MATCHED_EVIDENCE_USE)},
        "owner_state": copy.deepcopy(OWNER_STATE),
        "lineage": copy.deepcopy(PREDECESSOR),
        "preserved_predecessor_revision_002_science": preserved_revision_002,
        "full_plan": {
            "namespace": FULL_NAMESPACE,
            "repetitions": FULL_REPETITIONS,
            "repetition_basis": (
                "owner-directed preservation of the predecessor's prospectively "
                "frozen repeated-measurement count; no pilot outcome consulted"),
            "predecessor_pilot_not_in_confirmatory_sample": True},
        "evidence_selection_seed_identity": EVIDENCE_SELECTION_EXPERIMENT_ID,
        "semantic_delta": {
            "revision_rate": "EXPLICIT_PRE_PILOT_TYPED_OPERATIONALIZATION",
            "matched_evidence_use": "NOT_EVALUATED_UNDER_THIS_EXPERIMENT"},
        "executable_completion": {
            "full_repetitions": FULL_REPETITIONS,
            "classification": "OWNER_DIRECTED_PRESERVATION_NO_OUTCOME_TUNING",
            "basis": ("The owner ordered repetitions not to be altered and the "
                      "only predecessor prospectively frozen repeated-measurement "
                      "count is two; predecessor FULL had no matrix."),
            "new_scientific_estimand_or_threshold": False},
        "pilot_outcomes_consulted_for_successor_parameters": False,
    })
    doc["pilot"] = {
        "new_pilot_planned": False,
        "predecessor_disposition": "VALID_PREDECESSOR_INSTRUMENT_VALIDATION",
        "confirmatory_sample_overlap_permitted": False,
    }
    return doc


def contract() -> dict:
    """Return the complete successor scientific contract."""
    return _base_contract()


def contract_fingerprint() -> str:
    """Hash all frozen science while excluding mutable governance and derivations."""
    doc = contract()
    doc.pop("owner_state", None)
    doc.pop("status", None)
    doc.pop("power_analysis", None)
    return hashlib.sha256(_canonical(doc).encode()).hexdigest()


def freeze() -> dict:
    doc = contract()
    doc["design_fingerprint"] = contract_fingerprint()
    doc["execution_authorized"] = False
    doc["additional_provider_calls_authorized"] = False
    doc["full_execution_authorized"] = False
    doc["provider_calls_performed"] = 0
    doc["model_calls_performed"] = 0
    return doc

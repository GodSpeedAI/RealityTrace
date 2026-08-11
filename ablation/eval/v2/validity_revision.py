"""Owner-authorized scientific validity revision for evidence-revision-001.

Offline only.  This module preserves every predecessor artifact and emits new
revision-002 artifacts.  Importing it performs no writes and no network calls.
"""
from __future__ import annotations

import collections
import hashlib
import itertools
import json
import math
from pathlib import Path

from .case_inputs_v2 import build_case_inputs_v2
from .channel import ActionCatalogue, DisclosurePolicy, ObservationChannel, unit_seed
from .evidence_revision import (ARMS, COMPARISONS, EXPERIMENT_ID, OWNER_STATE,
                                PRIMARY_ENDPOINT, PRIMARY_POPULATION,
                                SECONDARY_ENDPOINTS, STATISTICAL_PLAN,
                                contract_fingerprint as predecessor_design_fingerprint,
                                power_analysis)
from .executable_contract import (_probe_observations, _sealed_hidden_meta,
                                  _sealed_keys, primary_population)
from .ids import unit_id
from .legibility import render_legible, render_raw
from .marker_neutralization import (MARKER_CLASS, NEUTRALIZATION_VERSION,
                                    REPLACEMENT, assert_marker_free,
                                    canonical_sha256, neutralize_payload)
from .replication import COSTS, PROBES

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
OUT = ROOT / "analysis" / "post-audit-v2"
REVISION_ID = "evidence-revision-001-revision-002"
DESIGN_VERSION = "evidence-revision-design-v2"
EXECUTABLE_VERSION = "executable-contract-v2"
PREDECESSOR_DESIGN = "7edd93a3a503b9395c0d39e79540cb20fe084fc8c020413c306db88e1029d3e3"
PREDECESSOR_EXECUTABLE = "557c0dcd9fe3b354e754074dd8d3e61a635a0bec638c138f592b451fca1083cd"
PREDECESSOR_CASE_INPUTS = "521dd42ff152b564be2a1d3c4daca9a80538b5ec734cb0d973b9f110b42a9dd9"
PREDECESSOR_MATRIX = "7a16e55f45c11110452baaf8c448c946f9b7fdb6c269677f58f69828fa17d32b"
K = 3
MAX_PAYMENT = 7
LEAKAGE_POLICY_VERSION = "masked-k-leakage-accounting-v2"
DECOY_POLICY_VERSION = "masked-k-exact3-affordable-v2"
# Frozen scientific classification, not inferred per case and not read from a
# key: these six catalogue actions are direct locus-candidate observations.
DIRECT_CANDIDATE_UNIVERSE = (
    "inspect_target_binding", "inspect_source_symbols", "inspect_environment",
    "inspect_permissions", "inspect_fixture_integrity", "validate_semantic_model")
EVIDENCE_POLICY = {"R0": "NONE", "R1": "MASKED_K", "R2": "MASKED_K",
                   "R3": "FULL", "R4": "FULL"}
PILOT_NAMESPACE = "pilot:er001-pilot-v2"
FULL_NAMESPACE = "full:er001-full-v2"
PILOT_REPETITIONS = 2


def _canonical(value) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), default=str)


def _sha(value) -> str:
    text = value if isinstance(value, str) else _canonical(value)
    return hashlib.sha256(text.encode()).hexdigest()


def neutralized_observations() -> tuple[dict, list[dict]]:
    clean, manifest = {}, []
    for case_id, probes in sorted(_probe_observations().items()):
        clean[case_id] = {}
        for probe, payload in sorted(probes.items()):
            revised, hits = neutralize_payload(payload)
            assert_marker_free(revised)
            clean[case_id][probe] = revised
            if hits:
                manifest.append({
                    "artifact_kind": "probe_payload",
                    "source_artifact_identity": f"acquisition-probes.jsonl:{case_id}:{probe}",
                    "case_id": case_id, "probe": probe,
                    "source_sha256": canonical_sha256(payload),
                    "neutralized_sha256": canonical_sha256(revised),
                    "exact_transformation": "replace every full injected-test identifier with test_observed_failure",
                    "marker_class": MARKER_CLASS, "replacement": REPLACEMENT,
                    "occurrences": len(hits),
                    "source_identifiers": sorted({h["source_identifier"] for h in hits}),
                    "key_blindness_proof": "neutralize_payload accepts the public payload only; no locus/key/hidden metadata/model output is an argument",
                    "semantic_preservation_rationale": "only the benchmark-manufacturing test identifier changes; result, exit, command shape, summary and behavioral fields remain",
                })
    return clean, manifest


def marker_manifest(case_manifest: list[dict], probe_manifest: list[dict]) -> dict:
    entries = sorted(case_manifest + probe_manifest,
                     key=lambda x: (x["artifact_kind"], x["case_id"],
                                    x.get("probe", "")))
    doc = {
        "schema": "marker-neutralization-manifest-v1",
        "experiment_id": EXPERIMENT_ID,
        "revision_id": REVISION_ID,
        "transformation_version": NEUTRALIZATION_VERSION,
        "marker_class": MARKER_CLASS,
        "replacement": REPLACEMENT,
        "deterministic": True, "locus_blind": True, "key_blind": True,
        "model_output_blind": True,
        "primary_disposition": "NEUTRALIZED_BEFORE_RENDERING",
        "contaminated_primary_cases": [],
        "entries": entries,
        "model_calls_performed": 0, "provider_calls_performed": 0,
    }
    doc["manifest_sha256"] = _sha(doc)
    return doc


def evidence_seed(case_id: str, rep: int, policy: str) -> int:
    return unit_seed(EXPERIMENT_ID, "evidence-selection", case_id, policy, rep)


def selected_delivery(case_id: str, rep: int, arm: str, observations: dict,
                      hidden: dict):
    policy = EVIDENCE_POLICY[arm]
    seed = evidence_seed(case_id, rep, policy)
    catalogue = ActionCatalogue(PROBES, seed)
    channel = ObservationChannel(
        catalogue, COSTS, ARMS[arm].disclosure, k=K,
        masked_candidate_universe=(DIRECT_CANDIDATE_UNIVERSE
                                   if policy == "MASKED_K" else ()))
    delivery = channel.deliver(observations[case_id],
                               hidden[case_id]["discriminating_probes"], seed + 1)
    if policy == "MASKED_K":
        if len(delivery.delivered_probes) != K:
            raise AssertionError("MASKED_K exact-K violation")
        if delivery.payment_charged > MAX_PAYMENT:
            raise AssertionError("MASKED_K budget violation")
    return catalogue, delivery


def feasibility_report(observations: dict | None = None,
                       hidden: dict | None = None) -> dict:
    observations = observations or neutralized_observations()[0]
    hidden = hidden or _sealed_hidden_meta()
    rows, case_spaces, structural = [], [], []
    for case_id in primary_population():
        target = hidden[case_id]["discriminating_probes"][0]
        available = tuple(p for p in PROBES if p in observations[case_id]
                          and p != target)
        all_sets = []
        for decoys in itertools.combinations(available, K - 1):
            cost = COSTS[target] + sum(COSTS[p] for p in decoys)
            all_sets.append({"decoys": list(decoys), "implied_cost": cost,
                             "affordable": cost <= MAX_PAYMENT,
                             "direct_candidate_count": sum(
                                 p in DIRECT_CANDIDATE_UNIVERSE for p in decoys)})
        feasible = [x for x in all_sets if x["affordable"]]
        case_spaces.append({"case_id": case_id, "target": target,
                            "target_cost": COSTS[target],
                            "candidate_costs": {p: COSTS[p] for p in PROBES},
                            "complete_candidate_sets": all_sets,
                            "complete_space_size": len(all_sets),
                            "affordable_space_size": len(feasible)})
        if not feasible:
            structural.append(case_id)
        for rep in range(1, PILOT_REPETITIONS + 1):
            _, delivery = selected_delivery(case_id, rep, "R1", observations, hidden)
            rows.append({
                "case_id": case_id, "repetition": rep, "target": target,
                "target_cost": COSTS[target],
                "complete_space_size": len(all_sets),
                "affordable_space_size": len(feasible),
                "candidate_costs": {p: COSTS[p] for p in PROBES},
                "selected_order": list(delivery.delivered_probes),
                "selected_decoys": [p for p in delivery.delivered_probes if p != target],
                "selected_cost": delivery.payment_charged,
                "selected_k": len(delivery.delivered_probes),
            })
    doc = {
        "schema": "masked-k-feasibility-v2", "revision_id": REVISION_ID,
        "search": "all C(7,2)=21 target-plus-two-decoy sets from the complete public 8-probe universe; filter total cost <=7",
        "selection_rule": "from the frozen six direct candidate observations, choose the two cheapest-tier decoys; break ties with the paired public evidence seed; canonical probe order is delivery order",
        "decoy_key_inputs": ["experiment_id", "case_id", "repetition",
                             "evidence_policy", "target_identity"],
        "decoy_key_excludes": ["true_locus", "answer_key", "model_output",
                               "arm_name", "rendering_mode"],
        "k": K, "max_payment": MAX_PAYMENT,
        "case_spaces": case_spaces, "rows": rows,
        "structurally_unaffordable_cases": structural,
        "all_cases_feasible": not structural,
    }
    doc["artifact_sha256"] = _sha(doc)
    return doc


def leakage_report(observations: dict | None = None,
                   hidden: dict | None = None, keys: dict | None = None) -> dict:
    observations = observations or neutralized_observations()[0]
    hidden, keys = hidden or _sealed_hidden_meta(), keys or _sealed_keys()
    cases = primary_population()
    capable = collections.defaultdict(set)
    for c in cases:
        capable[hidden[c]["discriminating_probes"][0]].add(keys[c]["true_locus"])
    loci = {keys[c]["true_locus"] for c in cases}
    prior = math.log2(len(loci))
    bits, realized, unique, costs, set_hashes = [], [], [], [], []
    for c in cases:
        for rep in range(1, PILOT_REPETITIONS + 1):
            _, d = selected_delivery(c, rep, "R1", observations, hidden)
            candidate_loci = set().union(*(capable.get(p, set())
                                           for p in d.delivered_probes))
            leak = prior - math.log2(len(candidate_loci))
            bits.append(leak); realized.append(len(d.delivered_probes))
            costs.append(d.payment_charged)
            set_hashes.append(_sha(list(d.delivered_probes)))
            if len(candidate_loci) == 1:
                unique.append({"case_id": c, "repetition": rep,
                               "membership": list(d.delivered_probes)})
    n = len(bits)
    doc = {
        "schema": LEAKAGE_POLICY_VERSION, "revision_id": REVISION_ID,
        "population_cases": len(cases), "deliveries": n,
        "declared_k": K,
        "realized_k_distribution": dict(sorted(collections.Counter(realized).items())),
        "theoretical_membership_leakage_bits": round(prior - math.log2(K), 6),
        "realized_membership_leakage_bits": {
            "mean": round(sum(bits) / n, 6), "maximum": round(max(bits), 6),
            "minimum": round(min(bits), 6)},
        "membership_uniquely_determines_locus": unique,
        "singleton_equivalent_count": len(unique),
        "cost_distribution": dict(sorted(collections.Counter(costs).items())),
        "cost_range": [min(costs), max(costs)],
        "unaffordable_count": sum(c > MAX_PAYMENT for c in costs),
        "missingness_count": 0,
        "channels": {
            "set_membership": "measured above; intentionally bounded and nonzero",
            "ordering": "0 additional bits; canonical public probe order",
            "opaque_handle_identity": "0 bits; paired public seed excludes locus",
            "observation_count": "0 bits; exactly 3 for every MASKED_K delivery",
            "missingness": "0 bits; every eligible delivery is affordable",
            "rendering_mode": "0 answer bits; frozen by arm and bound to one semantic payload",
            "marker_presence": "0 bits at model boundary; neutralizer leaves no marker",
            "FULL_membership": {"bits": 0.0, "reason": "all eight observations always delivered"},
            "NONE_membership": {"bits": 0.0, "reason": "no observations ever delivered"},
        },
        "interpretation": "MECHANISTIC_UPPER_BOUND_UNDER_ENFORCED_BUDGET; deployable_navigation=false",
    }
    doc["artifact_sha256"] = _sha(doc)
    return doc


def pair_equality_report(observations: dict | None = None,
                         hidden: dict | None = None) -> dict:
    observations = observations or neutralized_observations()[0]
    hidden = hidden or _sealed_hidden_meta()
    out = {}
    for a, b in (("R1", "R2"), ("R3", "R4")):
        rows = []
        for c in primary_population():
            for rep in range(1, PILOT_REPETITIONS + 1):
                ca, da = selected_delivery(c, rep, a, observations, hidden)
                cb, db = selected_delivery(c, rep, b, observations, hidden)
                pa = [o.payload for o in da.observations]
                pb = [o.payload for o in db.observations]
                evidence_a = _sha({"probes": da.delivered_probes,
                                   "handles": [o.handle for o in da.observations],
                                   "payloads": pa})
                evidence_b = _sha({"probes": db.delivered_probes,
                                   "handles": [o.handle for o in db.observations],
                                   "payloads": pb})
                semantic_a, semantic_b = _sha(pa), _sha(pb)
                raw = _sha([render_raw(p, 10**9) for p in pa])
                legible = _sha([render_legible(p, 10**9) for p in pb])
                rows.append({"case_id": c, "repetition": rep,
                             "evidence_set_hash_a": evidence_a,
                             "evidence_set_hash_b": evidence_b,
                             "semantic_evidence_hash_a": semantic_a,
                             "semantic_evidence_hash_b": semantic_b,
                             "raw_rendered_hash": raw,
                             "legible_rendered_hash": legible,
                             "underlying_equal": evidence_a == evidence_b and semantic_a == semantic_b,
                             "rendered_different": raw != legible})
        out[f"{a}_vs_{b}"] = {
            "pairs": len(rows),
            "underlying_evidence_equal": all(r["underlying_equal"] for r in rows),
            "rendered_hash_different": all(r["rendered_different"] for r in rows),
            "mismatch_count": sum(not r["underlying_equal"] for r in rows),
            "rows": rows,
        }
    return out


def _pilot_axes(case_id: str, inputs: dict, source_obs: dict) -> dict:
    row = inputs["cases"][case_id]["public_metadata"]
    cov = inputs["cases"][case_id]["non_model_visible_covariates"]
    source_probe_marker = any("test_corpus_scenario_" in _canonical(v)
                              for v in source_obs[case_id].values())
    return {"repo": row["repo"], "stdin_format": row["stdin_format"],
            "verifier_exit": row["verifier_exit"],
            "source_initial_marker": cov["source_marker_present"],
            "source_probe_marker": source_probe_marker,
            "masked_k_delivery_affordable": True}


def pilot_cases(inputs: dict | None = None) -> tuple[str, ...]:
    inputs = inputs or build_case_inputs_v2()[0]
    source_obs = _probe_observations()
    cases = primary_population()
    axes = {c: _pilot_axes(c, inputs, source_obs) for c in cases}
    required = {a: {v[a] for v in axes.values()} for a in next(iter(axes.values()))}
    order = sorted(cases, key=lambda c: _sha(
        f"{EXPERIMENT_ID}|pilot-selection-v1|{c}"))
    selected, covered = [], {a: set() for a in required}
    for c in order:
        if any(axes[c][a] not in covered[a] for a in required):
            selected.append(c)
            for a in required: covered[a].add(axes[c][a])
        if all(covered[a] == required[a] for a in required): break
    if any(covered[a] != required[a] for a in required):
        raise AssertionError("pilot-selection-v1 failed to saturate revised axes")
    return tuple(selected)


def scientific_declarations(case_hash: str, marker_hash: str) -> dict:
    return {
        "design_version": DESIGN_VERSION, "experiment_id": EXPERIMENT_ID,
        "supersedes_design_fingerprint": PREDECESSOR_DESIGN,
        "owner_authorized_semantic_changes": {
            "BLOCKER-01": "marker-neutral semantic projection before either renderer",
            "BLOCKER-02": "MASKED_K exact K=3 from frozen direct candidate universe",
            "BLOCKER-03": "complete feasibility enumeration and deterministic affordable selection at max_payment=7",
            "BLOCKER-04": "paired evidence-policy seed excludes arm and rendering",
        },
        "marker_policy": {"version": NEUTRALIZATION_VERSION,
                          "case_inputs_sha256": case_hash,
                          "manifest_sha256": marker_hash,
                          "stage": "before RAW/LEGIBLE rendering"},
        "masked_k": {"k": K, "max_payment": MAX_PAYMENT,
                     "decoy_policy_version": DECOY_POLICY_VERSION,
                     "candidate_universe": list(DIRECT_CANDIDATE_UNIVERSE),
                     "deployable_navigation": False,
                     "class": "MECHANISTIC_UPPER_BOUND_UNDER_ENFORCED_BUDGET"},
        "evidence_seed_scope": {
            "formula": "H(experiment_id, case_id, repetition, evidence_policy)",
            "policies": EVIDENCE_POLICY,
            "excludes": ["arm_name", "rendering_mode", "true_locus",
                         "answer_key", "model_output"]},
        "unchanged_science": {
            "arms": list(ARMS), "primary_population": PRIMARY_POPULATION,
            "primary_endpoint": PRIMARY_ENDPOINT,
            "secondary_endpoints": SECONDARY_ENDPOINTS,
            "comparisons": COMPARISONS, "statistical_plan": STATISTICAL_PLAN,
            "original_10pp_threshold_preserved": True,
            "power_issue": "OWNER_ACKNOWLEDGED_UNRESOLVED",
            "primary_inferential_unit": "CASE"},
    }


def design_fingerprint(case_hash: str, marker_hash: str) -> str:
    return _sha(scientific_declarations(case_hash, marker_hash))


def executable_declarations(design_fp: str, case_hash: str, marker_hash: str,
                            feasibility_hash: str, leakage_hash: str) -> dict:
    pcases = pilot_cases()
    return {
        "executable_version": EXECUTABLE_VERSION,
        "experiment_id": EXPERIMENT_ID,
        "design_fingerprint": design_fp,
        "supersedes_executable_fingerprint": PREDECESSOR_EXECUTABLE,
        "case_inputs": {"schema": "case-input-v2", "sha256": case_hash},
        "marker_manifest_sha256": marker_hash,
        "masked_k_feasibility_sha256": feasibility_hash,
        "masked_k_leakage_sha256": leakage_hash,
        "evidence_policy": EVIDENCE_POLICY,
        "decoy_policy_version": DECOY_POLICY_VERSION,
        "leakage_policy_version": LEAKAGE_POLICY_VERSION,
        "pilot_selection": {"rule": "pilot-selection-v1", "cases": list(pcases),
                            "repetitions": PILOT_REPETITIONS,
                            "namespace": PILOT_NAMESPACE},
        "full_namespace": FULL_NAMESPACE,
    }


def executable_fingerprint(*args) -> str:
    return _sha(executable_declarations(*args))


def pilot_matrix_document(design_fp: str, executable_fp: str,
                          case_hash: str) -> dict:
    cases = pilot_cases(); arms = tuple(ARMS)
    uids = [unit_id(EXPERIMENT_ID, PILOT_NAMESPACE, c, a, r, executable_fp)
            for c in cases for a in arms for r in range(1, PILOT_REPETITIONS + 1)]
    doc = {"schema": "evidence-revision-matrix-v2",
           "experiment_id": EXPERIMENT_ID, "stage": "pilot",
           "namespace": PILOT_NAMESPACE, "cases": list(cases),
           "arms": list(arms), "repetitions": PILOT_REPETITIONS,
           "n_cases": len(cases), "n_arms": len(arms), "total_units": len(uids),
           "unit_ids": uids, "design_fingerprint": design_fp,
           "executable_contract_fingerprint": executable_fp,
           "case_inputs_sha256": case_hash, "execution_authorized": False}
    doc["matrix_sha256"] = _sha(doc)
    return doc


def validate_revision_artifacts(artifacts: dict) -> None:
    """Fail on contract mutations before any revised artifact is frozen."""
    inputs = artifacts["case-inputs-v2.json"]
    for record in inputs["cases"].values():
        assert_marker_free(record["question"])
        assert_marker_free(record["initial_evidence"])
    feasibility = artifacts["evidence-revision-001-masked-k-feasibility-v2.json"]
    if feasibility["structurally_unaffordable_cases"]:
        raise AssertionError("MASKED_K_STRUCTURALLY_UNAFFORDABLE")
    for row in feasibility["rows"]:
        if row["selected_k"] != K:
            raise AssertionError("MASKED_K must realize exactly K=3")
        if row["selected_cost"] > MAX_PAYMENT:
            raise AssertionError("MASKED_K selected an over-budget set")
    leakage = artifacts["evidence-revision-001-masked-k-leakage-v2.json"]
    if leakage["missingness_count"] or leakage["unaffordable_count"]:
        raise AssertionError("MASKED_K missingness channel is nonzero")
    if leakage["singleton_equivalent_count"]:
        raise AssertionError("MASKED_K contains singleton-equivalent membership")
    pairs = artifacts["evidence-revision-001-pair-equality-v2.json"]
    for pair in ("R1_vs_R2", "R3_vs_R4"):
        if not pairs[pair]["underlying_evidence_equal"]:
            raise AssertionError(f"paired evidence mismatch: {pair}")


def build_artifacts() -> dict:
    inputs, case_manifest = build_case_inputs_v2()
    observations, probe_manifest = neutralized_observations()
    markers = marker_manifest(case_manifest, probe_manifest)
    feasibility = feasibility_report(observations)
    if feasibility["structurally_unaffordable_cases"]:
        raise RuntimeError("MASKED_K_STRUCTURALLY_UNAFFORDABLE")
    leakage = leakage_report(observations)
    pairs = pair_equality_report(observations)
    case_hash, marker_hash = inputs["case_inputs_sha256"], markers["manifest_sha256"]
    design_fp = design_fingerprint(case_hash, marker_hash)
    executable_fp = executable_fingerprint(
        design_fp, case_hash, marker_hash, feasibility["artifact_sha256"],
        leakage["artifact_sha256"])
    matrix = pilot_matrix_document(design_fp, executable_fp, case_hash)
    science = dict(scientific_declarations(case_hash, marker_hash),
                   status="FROZEN_NOT_STARTED", design_fingerprint=design_fp,
                   power_analysis=power_analysis(), owner_state=OWNER_STATE,
                   execution_authorized=False,
                   additional_provider_calls_authorized=False,
                   model_calls_performed=0, provider_calls_performed=0)
    executable = dict(executable_declarations(
        design_fp, case_hash, marker_hash, feasibility["artifact_sha256"],
        leakage["artifact_sha256"]),
        status="FROZEN_NOT_STARTED",
        executable_contract_fingerprint=executable_fp,
        pilot_matrix_sha256=matrix["matrix_sha256"],
        execution_authorized=False, additional_provider_calls_authorized=False,
        model_calls_performed=0, provider_calls_performed=0)
    lineage = {
        "schema": "evidence-revision-lineage-v2", "revision_id": REVISION_ID,
        "supersedes_design_fingerprint": PREDECESSOR_DESIGN,
        "design_fingerprint": design_fp,
        "supersedes_executable_fingerprint": PREDECESSOR_EXECUTABLE,
        "executable_contract_fingerprint": executable_fp,
        "supersedes_case_inputs_sha256": PREDECESSOR_CASE_INPUTS,
        "case_inputs_sha256": case_hash,
        "supersedes_pilot_matrix_sha256": PREDECESSOR_MATRIX,
        "pilot_matrix_sha256": matrix["matrix_sha256"],
        "owner_authorized_changes": science["owner_authorized_semantic_changes"],
        "unchanged_scientific_fields": science["unchanged_science"],
        "power_state_unchanged": True, "execution_authorization_unchanged": True,
    }
    closure = {
        "schema": "evidence-revision-validity-closure-v2",
        "revision_id": REVISION_ID,
        "BLOCKER-01": {"status": "CLOSED", "evidence": markers["manifest_sha256"],
                       "primary_marker_contaminated": []},
        "BLOCKER-02": {"status": "CLOSED", "evidence": leakage["artifact_sha256"],
                       "realized_k": K, "singleton_equivalent_count": leakage["singleton_equivalent_count"]},
        "BLOCKER-03": {"status": "CLOSED", "evidence": feasibility["artifact_sha256"],
                       "unaffordable_count": leakage["unaffordable_count"]},
        "BLOCKER-04": {"status": "CLOSED", "evidence": {
            "R1_R2": pairs["R1_vs_R2"]["underlying_evidence_equal"],
            "R3_R4": pairs["R3_vs_R4"]["underlying_evidence_equal"]}},
        "design_fingerprint": design_fp,
        "executable_contract_fingerprint": executable_fp,
        "primary_population_count": len(primary_population()),
        "pilot": {"cases": matrix["cases"], "repetitions": PILOT_REPETITIONS,
                  "total_units": matrix["total_units"]},
        "model_calls_performed": 0, "provider_calls_performed": 0,
        "execution_authorized": False,
    }
    artifacts = {"case-inputs-v2.json": inputs,
            "evidence-revision-001-marker-neutralization-v2.json": markers,
            "evidence-revision-001-masked-k-feasibility-v2.json": feasibility,
            "evidence-revision-001-masked-k-leakage-v2.json": leakage,
            "evidence-revision-001-pair-equality-v2.json": pairs,
            "evidence-revision-001-revision-002-contract.json": science,
            "evidence-revision-001-revision-002-executable-contract.json": executable,
            "evidence-revision-001-pilot-matrix-v2.json": matrix,
            "evidence-revision-001-fingerprint-lineage-v2.json": lineage,
            "evidence-revision-001-validity-closure-v2.json": closure}
    validate_revision_artifacts(artifacts)
    return artifacts


def freeze() -> dict:
    artifacts = build_artifacts()
    OUT.mkdir(parents=True, exist_ok=True)
    for name, doc in artifacts.items():
        (OUT / name).write_text(json.dumps(doc, indent=2, sort_keys=True,
                                           default=str) + "\n")
    return artifacts


if __name__ == "__main__":
    docs = freeze(); closure = docs["evidence-revision-001-validity-closure-v2.json"]
    print(json.dumps(closure, indent=2, sort_keys=True))

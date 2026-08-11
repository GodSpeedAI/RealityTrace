#!/usr/bin/env python3
"""R-11 + R-09 + R-10 derived artifacts: corpus adjudication and construct maps.

Read-only over the sealed corpus. Writes to `analysis/post-audit-v2/`.
No ground truth is rewritten; no corpus file is modified.

Usage:  uv run --python 3.12 -m eval.rescore.corpus
"""

from __future__ import annotations

import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(os.path.dirname(HERE)))

from eval.rescore import SCORER_VERSION                       # noqa: E402
from eval.rescore.historical import (ABLATION, CORPUS, OUT, _SOURCES,  # noqa: E402
                                     _hash_file, load_json)
from eval.v2 import semantics, sufficiency                    # noqa: E402

COSTS = {"inspect_target_binding": 1, "inspect_source_symbols": 1,
         "inspect_environment": 1, "inspect_permissions": 1,
         "inspect_fixture_integrity": 1, "validate_semantic_model": 3,
         "rerun_verifier_isolated": 5, "inspect_verifier_config": 1}
MAX_PAYMENT = 7

BEARS_ON = {
    "inspect_target_binding": {"verification_plan"},
    "inspect_source_symbols": {"implementation"},
    "inspect_environment": {"environment"},
    "inspect_verifier_config": {"environment"},
    "inspect_permissions": {"authority"},
    "inspect_fixture_integrity": {"evidence_collection"},
    "validate_semantic_model": {"declared_model", "projection"},
    "rerun_verifier_isolated": {"implementation"},
}


def _load_probes():
    path = os.path.join(CORPUS, "acquisition-probes.jsonl")
    _SOURCES[os.path.relpath(path, ABLATION)] = _hash_file(path)
    rows = {}
    with open(path) as f:
        for line in f:
            d = json.loads(line)
            rows[d["case_id"]] = d
    return rows


# ── R-11: corpus defect ledger ───────────────────────────────────────────────

def adjudicate_corpus() -> dict:
    rows = _load_probes()
    keys = load_json(os.path.join(CORPUS, "governed-keys.json"))
    truth = {c: v["true_locus"] for c, v in keys.items()}

    flagged = sorted(c for c, r in rows.items()
                     if not r["failure_signature_present"])
    entries = []
    for cid in flagged:
        r = rows[cid]
        env = r["probes"]["inspect_environment"]
        fault_observable = (env.get("toolchain") != "available"
                            or env.get("tmpdir") == "not_writable"
                            or env.get("path") == "restricted")
        verdict = ("VALID_DESPITE_MISSING_SIGNATURE"
                   if r["verifier_exit"] != 0 and fault_observable
                   else "REQUIRES_OWNER_ADJUDICATION")
        entries.append({
            "case_id": cid,
            "true_locus": truth[cid],
            "verifier_exit": r["verifier_exit"],
            "failure_signature_present": False,
            "injected_fault_observable_in_probe": fault_observable,
            "observed_environment_probe": env,
            "verdict": verdict,
            "rationale": (
                "Exit 127 means the toolchain binary was never found, so the "
                "verifier produced no output for a textual signature to match "
                "against. The fault did manifest (exit != 0) and is directly "
                "observable via inspect_environment "
                "(toolchain=unavailable, path=restricted). This is materially "
                "different from the df-04 defect class the owner review "
                "repaired, where the exit code was 0 and no fault had been "
                "injected at all."),
            "action": "NO QUARANTINE; record as a difficulty characteristic",
        })

    same_profile = sorted(
        c for c, r in rows.items()
        if r["probes"]["inspect_environment"].get("toolchain") == "unavailable"
        and r["probes"]["inspect_environment"].get("path") == "restricted")

    return {
        "scorer_version": SCORER_VERSION,
        "flagged_cases": entries,
        "n_flagged": len(entries),
        "n_quarantined": 0,
        "ground_truth_modified": False,
        "control_comparison": {
            "cases_with_the_same_environment_probe_profile": same_profile,
            "note": ("cep-10 shares the toolchain=unavailable/path=restricted "
                     "profile but exited 1 with output, so its signature IS "
                     "present. The flag tracks whether output existed to match, "
                     "not whether the fault was injected."),
        },
        "residual_risks": [
            {"risk": "thin initial evidence",
             "detail": ("all three flagged cases are `environment` and present "
                        "the model with a bare exit-127 and no verifier output. "
                        "They are plausibly harder from the base context alone, "
                        "so `environment` results should be stratified by "
                        "signature presence in any future replication."),
             "affects": flagged},
            {"risk": "cross-locus probe artifact",
             "detail": ("df-06's inspect_target_binding reports all_exist=false "
                        "because default test discovery cannot resolve when the "
                        "toolchain is missing, so df-06 is gate-eligible for "
                        "verification_plan as well as environment. Legitimate "
                        "(the gate is a sufficiency test, not an oracle) but it "
                        "must not be read as a second injected fault."),
             "affects": ["df-06"]},
            {"risk": "failure_signature_present is not a validity flag",
             "detail": ("for exit-127 faults the flag cannot fire by "
                        "construction. Corpus gates must not treat it as "
                        "evidence of a construction defect."),
             "affects": flagged},
        ],
        "prior_repairs_preserved": {
            "df-04": "superseded exit 0 -> exit 101, repairs.jsonl",
            "df-11": "superseded exit 2 -> exit 1, repairs.jsonl",
            "df-12": "superseded exit 2 -> exit 1, repairs.jsonl",
            "note": ("repaired by the owner BEFORE any model-backed run, under "
                     "a documented supersession protocol with a post-repair "
                     "leakage re-sweep. Not affected by this adjudication."),
        },
        "df-15_reclassification": {
            "change": "unknown -> environment (acquisition-keys.json onward)",
            "only_key_difference_in_the_chain": True,
            "disclosed_in": ".agents/reports/8-sxr-diagnostic-acquisition.md §5, §24.1",
            "consequence": ("A_u denominators are not comparable across the "
                            "S23.2 / exp-3 boundary (8 unknown cases -> 7)."),
        },
    }


# ── R-09: construct divergence ───────────────────────────────────────────────

def construct_divergence() -> dict:
    meta = load_json(os.path.join(CORPUS, "acquisition-hidden-meta.json"))
    report = semantics.divergence_report(meta, sufficiency.LEGACY_CONTRACT,
                                         BEARS_ON)
    report["scorer_version"] = SCORER_VERSION
    report["metric_construct_binding"] = semantics.METRIC_CONSTRUCT
    report["distractor_correction"] = (
        "`inspect_verifier_config` is GATE_QUALIFYING for `environment` under "
        "the frozen sufficiency contract (plan 9 records it as a 7/7 TP, 0 FP "
        "predictor) and is not an EVALUATOR_DISCRIMINATING action for any case. "
        "Calling it 'the intentional distractor' conflates the two constructs. "
        "Its measured effect on A_select is one unit.")
    return report


# ── R-10: exclusion frontier ─────────────────────────────────────────────────

def exclusion_topology() -> dict:
    rows = _load_probes()
    probes = {c: r["probes"] for c, r in rows.items()}
    keys = load_json(os.path.join(CORPUS, "governed-keys.json"))
    truth = {c: v["true_locus"] for c, v in keys.items()}

    legacy = sufficiency.topology_summary(
        sufficiency.derive_topology(probes, sufficiency.LEGACY_CONTRACT,
                                    COSTS, MAX_PAYMENT), truth)
    proposed_full = sufficiency.topology_summary(
        sufficiency.derive_topology(probes, sufficiency.PROPOSED_CONTRACT), truth)
    proposed_budgeted = sufficiency.topology_summary(
        sufficiency.derive_topology(probes, sufficiency.PROPOSED_CONTRACT,
                                    COSTS, MAX_PAYMENT), truth)
    frontier = sufficiency.exclusion_frontier(
        probes, truth, COSTS, MAX_PAYMENT, "verifier",
        {"probe": "rerun_verifier_isolated", "field": "isolated_result",
         "op": "eq", "value": "failed"})

    return {
        "scorer_version": SCORER_VERSION,
        "legacy_contract": {
            "version": sufficiency.LEGACY_CONTRACT.version,
            "fingerprint": sufficiency.LEGACY_CONTRACT.fingerprint(),
            "topology": legacy["counts"],
            "reproduces_frozen_topology": (
                legacy["counts"]["gate_feasible_determinate"] == 43
                and legacy["counts"]["gate_infeasible_determinate"] == 10
                and legacy["counts"]["true_unknown"] == 7),
        },
        "proposed_contract_unbudgeted": {
            "topology": proposed_full["counts"],
            "unknown_false_positives": proposed_full["unknown_false_positives"],
        },
        "proposed_contract_at_frozen_budget": {
            "premise_probes": list(
                sufficiency.PROPOSED_CONTRACT.probes_for("verifier")),
            "premise_cost": sum(
                COSTS[p] for p in
                sufficiency.PROPOSED_CONTRACT.probes_for("verifier")),
            "max_payment": MAX_PAYMENT,
            "reachable": False,
            "topology": proposed_budgeted["counts"],
        },
        "affordable_frontier": frontier,
        "frontier_summary": {
            "n_affordable_rule_shapes": len(frontier),
            "any_with_zero_false_positives": any(
                r["unknown_false_positives"] == 0
                and r["other_locus_false_positives"] == 0 for r in frontier),
            "min_other_locus_false_positives": min(
                r["other_locus_false_positives"] for r in frontier),
        },
        "adjudication": {
            "audit_claim": (
                "F9: 'the limitation is the rule language, not the evidence "
                "representation'"),
            "corrected": (
                "Three-way. (1) The evidence IS present: under full observation "
                "the exclusion rule is 10/10 on verifier with one true-UNKNOWN "
                "false positive (df-14) and zero other-locus false positives. "
                "(2) The v1 rule language could not express it. (3) The premise "
                "costs 13 payment units against a frozen budget of 7, so no "
                "acquisition order reaches it, and every affordable truncation "
                "produces 6-16 false positives on OTHER determinate loci — "
                "worse than leaving the verifier cases infeasible. "
                "Diagnosis-by-exclusion is intrinsically expensive."),
            "status": "NOT ADOPTED — owner governance decision",
            "what_the_owner_must_decide": [
                "whether a true-UNKNOWN case becoming concretely settleable is "
                "acceptable (df-14 under the full-observation rule)",
                "whether the acquisition budget should be raised to make "
                "exclusion reachable, which is an experimental-design change "
                "and not a validity repair",
                "whether `verifier` should remain declared gate-infeasible and "
                "reported as such",
            ],
        },
    }


def main() -> int:
    os.makedirs(OUT, exist_ok=True)
    for name, fn in (("corpus-adjudication.json", adjudicate_corpus),
                     ("construct-divergence.json", construct_divergence),
                     ("exclusion-topology.json", exclusion_topology)):
        with open(os.path.join(OUT, name), "w") as f:
            json.dump(fn(), f, indent=2, sort_keys=True, default=str)
        print(f"  wrote {name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

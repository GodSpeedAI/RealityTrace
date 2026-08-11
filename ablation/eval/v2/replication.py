#!/usr/bin/env python3
"""R-16: replication-ready harness definition and validation proof.

This module PREPARES a replication. It does not run one. There is no network
client here and no `__main__` path that contacts a provider: `validate()`
exercises the whole instrument against a deterministic stub and emits the
evidence for `HARNESS-VALIDATION-REPORT.md`.

The arm set is imported from `evidence_revision`, which is where the frozen
scientific decisions live. This module declares no threshold, no prediction and
no verdict rule; it only demonstrates that the instrument behaves as specified
on every frozen arm.

The audit proposed a "context-preserved" arm. Implementing R-02 removed it:
state continuity is now a HARNESS INVARIANT asserted on every rendered prompt
in every arm, so it can no longer be a between-arm manipulation. Deleting an
invariant to recreate an arm would be a regression, not an experiment. This is
recorded rather than quietly dropped.

Usage:  uv run --python 3.12 -m eval.v2.replication   # validation only
"""

from __future__ import annotations

import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(os.path.dirname(HERE)))

from eval.v2 import VERSION                                      # noqa: E402
from eval.v2.channel import DisclosurePolicy, leakage_bound       # noqa: E402
from eval.v2.controller import (ConditionSpec, DiagnosticController,  # noqa: E402
                                Terminal, arm_difference_report)
from eval.v2.dstate import missing_state_sections                # noqa: E402
from eval.v2.ids import unit_id                                  # noqa: E402
from eval.v2.integrity import (check_gate_negative_control,      # noqa: E402
                               check_model_uniformity,
                               check_oracle_isolation,
                               check_parse_failure_behaviour,
                               check_pilot_full_separation,
                               check_raw_before_score, run_suite)
from eval.v2.ledger import CommitLedger                          # noqa: E402
from eval.v2.sufficiency import LEGACY_CONTRACT                  # noqa: E402

EXPERIMENT_ID = "evidence-revision-001"
STATUS = "PREPARED — NOT STARTED"

PROBES = ("inspect_target_binding", "inspect_source_symbols",
          "inspect_environment", "inspect_permissions",
          "inspect_fixture_integrity", "validate_semantic_model",
          "rerun_verifier_isolated", "inspect_verifier_config")
COSTS = {p: 1 for p in PROBES}
COSTS["validate_semantic_model"] = 3
COSTS["rerun_verifier_isolated"] = 5
VOCAB = ("verifier", "verification_plan", "projection", "implementation",
         "environment", "authority", "evidence_collection", "declared_model",
         "unknown")

#: Descriptions are carried over UNCHANGED from the frozen catalogue. Making
#: them more suggestive to raise selection rates is benchmark tuning (audit
#: §17) and is out of scope for a repair.
DESCRIPTIONS = {
    "inspect_target_binding": (
        "Verify whether the test target or path referenced by the verification "
        "command resolves to an existing file in the working tree."),
    "inspect_source_symbols": (
        "Inspect whether source symbols referenced in the failure trace are "
        "present at their expected locations."),
    "inspect_environment": (
        "Verify whether the required toolchain and designated working "
        "directories are available and writable."),
    "inspect_permissions": (
        "Inspect filesystem access permissions on key artifacts referenced by "
        "the failing verification."),
    "inspect_fixture_integrity": (
        "Verify whether referenced fixture or data files exist and contain "
        "expected content."),
    "validate_semantic_model": (
        "Validate the declared semantic model (.sea) against its schema and "
        "registry configuration."),
    "rerun_verifier_isolated": (
        "Re-run the specific failing test in isolation under the default "
        "environment and report the outcome."),
    "inspect_verifier_config": (
        "Inspect the verifier's configuration: command, evidence types, and "
        "declared direction rule."),
}

#: Single source of truth. The arm set is a FROZEN SCIENTIFIC DECISION and
#: lives in `evidence_revision`. Validation exercises whatever is frozen there,
#: rather than a second copy that could drift away from it.
from eval.v2.evidence_revision import ARMS, SUBSUMED_ARMS      # noqa: E402

#: Metrics the instrument now supports, with the construct each measures.
#: `A_use` as historically reported is deliberately absent: the matched-pair
#: form replaces it.
SUPPORTED_METRICS = {
    "revision_rate": ("P(final claim != round-1 claim | round-1 claim wrong). "
                      "The endpoint the audit identified as the chain's most "
                      "surprising unmeasured result (0/63 historically)."),
    "gated_accuracy": "correct AND gate-qualifying evidence held",
    "gate_blind_accuracy": "correct claim, regardless of gate outcome",
    "invocation_rate": "units with >= 1 acquisition, reported separately",
    "A_select_conditional": "discriminating first action | an action was taken",
    "matched_evidence_use": "utilization on units where both arms held evidence",
    "output_class_rates": ("VALID_DECISION / OUTPUT_TRUNCATED / "
                           "MODEL_OUTPUT_CONTRACT_FAILURE / PARSER_FAILURE, "
                           "never collapsed into 'incorrect'"),
    "response_mode": "modal share and entropy, labelled POST-HOC",
}

STATISTICAL_PLAN = {
    "primary_unit": "case",
    "repetition_handling": ("repetitions aggregated within case before pairing; "
                            "(case, rep) pairing reported alongside but never "
                            "alone (audit A-05)"),
    "paired_test": "McNemar, exact binomial when b + c < 25",
    "interval": "Wilson score, 95%",
    "p_value_floor": "never reported as 0; underflow reported as p < 1e-15",
    "multiple_comparisons": ("report the family and the count; no threshold is "
                             "set here — that is a preregistration act"),
    "mandatory_covariate": "per-arm output-truncation rate",
}


class _StubClient:
    """Deterministic scripted client used only for instrument validation."""

    def __init__(self, script, provider=None):
        self.script, self.prompts = list(script), []
        self.provider = provider or {
            "resolved_model": "stub/deterministic", "generation_id": "gen_v",
            "finish_reason": "stop", "completion_tokens": 200,
            "prompt_tokens": 1500, "retries": 0}

    def complete(self, system, user, requested_max_tokens, temperature):
        self.prompts.append(user)
        return {"text": self.script.pop(0) if self.script else "unparseable",
                "provider": dict(self.provider)}


def _fixture():
    obs = {p: {"checked": [], "note": "clean"} for p in PROBES}
    obs["inspect_permissions"] = {"all_accessible": False,
                                  "restrictive_paths": ["src/lib.rs"]}
    obs["inspect_environment"] = {"toolchain": "available", "tmpdir": "default",
                                  "path": "default"}
    obs["inspect_source_symbols"] = {"missing": []}
    obs["inspect_target_binding"] = {"all_exist": True}
    obs["inspect_fixture_integrity"] = {"missing": [], "stale": []}
    obs["validate_semantic_model"] = {"valid": None}
    obs["rerun_verifier_isolated"] = {"isolated_result": "failed"}
    return obs


def validate(tmpdir: str) -> dict:
    """Exercise the whole instrument deterministically and emit the evidence."""
    obs = _fixture()
    settle = ('{"candidate_loci": ["authority", "environment"], '
              '"selected_action": null, "proposed_settlement": "%s"}')

    prompts_seen, results, executions = [], {}, []
    for name, spec in ARMS.items():
        client = _StubClient([settle % "unknown", settle % "authority",
                              settle % "authority", settle % "authority"])
        ctrl = DiagnosticController(spec, LEGACY_CONTRACT, PROBES, DESCRIPTIONS,
                                    COSTS, VOCAB, client)
        uid = unit_id(EXPERIMENT_ID, "validation", "cep-12", name, 1, "fp")
        res = ctrl.run_unit(uid, EXPERIMENT_ID, "validation", "cep-12", 1,
                            "Which failure locus?",
                            "verifier exit 101; permission denied on src/lib.rs",
                            obs, ("inspect_permissions",))
        results[name] = res
        prompts_seen.extend(client.prompts)
        executions.extend(res.executions)

    # contract-repair path
    bad_client = _StubClient(["not json"] * 9)
    bad_ctrl = DiagnosticController(ARMS["R1"], LEGACY_CONTRACT, PROBES,
                                    DESCRIPTIONS, COSTS, VOCAB, bad_client)
    bad = bad_ctrl.run_unit("bad", EXPERIMENT_ID, "validation", "cep-12", 2,
                            "Which failure locus?", "evidence", obs,
                            ("inspect_permissions",))

    # commit ledger
    ledger = CommitLedger(os.path.join(tmpdir, "commits.jsonl"))
    for name, res in results.items():
        ledger.commit_raw(res.unit_id, os.path.join(tmpdir, f"{res.unit_id}.json"),
                          json.dumps(res.to_dict(), sort_keys=True, default=str))

    probes_by_case = {"c1": {"inspect_permissions": {"all_accessible": False},
                             "inspect_environment": {"toolchain": "available"}},
                      "c2": {"inspect_permissions": {"all_accessible": True},
                             "inspect_environment": {"toolchain": "missing"}},
                      "c3": {"inspect_permissions": {"all_accessible": True},
                             "inspect_environment": {"toolchain": "available"}}}
    truth = {"c1": "authority", "c2": "authority", "c3": "unknown"}

    suite = run_suite([
        check_gate_negative_control(LEGACY_CONTRACT, probes_by_case, truth),
        check_model_uniformity(executions),
        check_pilot_full_separation(
            {"namespace": "er-pilot", "run_id": "r-p",
             "checkpoint_store": "/p/m.jsonl", "artifact_dir": "/p/raw",
             "unit_ids": [unit_id(EXPERIMENT_ID, "er-pilot", "cep-12", "R1", 1, "fp")]},
            {"namespace": "er-full", "run_id": "r-f",
             "checkpoint_store": "/f/m.jsonl", "artifact_dir": "/f/raw",
             "unit_ids": [unit_id(EXPERIMENT_ID, "er-full", "cep-12", "R1", 1, "fp")]}),
        # Checked against the FULL policy (R3/R4), the only one that is
        # answer-isolated by construction. R1/R2 use MASKED_K, whose leakage is
        # bounded and DISCLOSED in per_arm_disclosure_bound; it is not covered
        # by this check passing, and must not be reported as if it were.
        check_oracle_isolation(prompts_seen, PROBES, DisclosurePolicy.FULL,
                               len(PROBES)),
        check_raw_before_score(ledger, [r.unit_id for r in results.values()]),
        check_parse_failure_behaviour(bad, bad_client.prompts),
    ])

    return {
        "instrument_version": VERSION,
        "experiment_id": EXPERIMENT_ID,
        "status": STATUS,
        "model_calls_performed": 0,
        "client": "deterministic stub; no network transport is imported here",
        "arms": {k: v.name for k, v in ARMS.items()},
        "subsumed_arms": SUBSUMED_ARMS,
        "arm_difference_report": arm_difference_report(ARMS.values()),
        "per_arm_disclosure_bound": {
            name: leakage_bound(spec.disclosure, len(PROBES))
            for name, spec in ARMS.items()},
        "integrity_suite": suite,
        "state_continuity": {
            "prompts_checked": len(prompts_seen),
            "prompts_missing_required_state": sum(
                1 for p in prompts_seen if missing_state_sections(p)),
            "prompts_retaining_initial_evidence": sum(
                1 for p in prompts_seen if "permission denied" in p),
        },
        "termination": {
            name: res.terminal_state for name, res in results.items()},
        "no_fall_through_terminal": all(
            r.terminal_state in {t.value for t in Terminal}
            for r in results.values()),
        "contract_repair": {
            "terminal": bad.terminal_state,
            "payment_consumed": bad.payment_used,
            "actions_executed": len(bad.acquired_probes),
            "final_locus": bad.final_locus,
            "attempts": len(bad.executions),
        },
        "provenance": {
            "records": len(executions),
            "all_have_provider_resolved_model": all(
                e["returned"]["resolved_model"] for e in executions),
            "no_manufactured_zeros": all(
                e["returned"]["retries"] is not None
                or "retries" in e["unavailable_fields"] for e in executions),
        },
        "supported_metrics": SUPPORTED_METRICS,
        "statistical_plan": STATISTICAL_PLAN,
        "sufficiency_contract": {
            "version": LEGACY_CONTRACT.version,
            "fingerprint": LEGACY_CONTRACT.fingerprint(),
            "extended_language_adopted": False,
        },
    }


def main() -> int:
    import tempfile
    with tempfile.TemporaryDirectory() as tmp:
        report = validate(tmp)
    out = os.path.join(os.path.dirname(os.path.dirname(HERE)),
                       "analysis", "post-audit-v2", "harness-validation.json")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, "w") as f:
        json.dump(report, f, indent=2, sort_keys=True, default=str)
    ok = report["integrity_suite"]["all_passed"]
    print(f"  integrity suite: {report['integrity_suite']['n_pass']}/"
          f"{report['integrity_suite']['n_checks']} PASS")
    print(f"  model calls: {report['model_calls_performed']}")
    print(f"  wrote {os.path.relpath(out)}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())

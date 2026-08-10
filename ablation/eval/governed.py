"""H-GOVERNED-ACQUISITION experiment contract (governed-diagnostic-acquisition-001).

Three controller conditions testing whether structural gating of concrete
settlement on evidentiary sufficiency causes the model to navigate evidence
affordances competently:

  V — voluntary acquisition (fresh D2 replication, no gate)
  G — governed self-selected acquisition (structural sufficiency gate)
  O — governed oracle-probe upper bound (harness selects probes)

The key intervention is in the CONTROLLER/STATE MACHINE, not the prompt. The
evidence-sufficiency gate reads observation STRUCTURE (not ground truth, not
error content, not model correctness) to determine settlement eligibility.

Frozen: probe catalogue, payment model, budget, locus vocabulary, scorer
semantics identical to diagnostic-acquisition-001. Gate signal rules and bearing
matrix are frozen in this module before any model execution.
"""

from __future__ import annotations

import hashlib
import json

from eval.acquisition import (
    PROBES, PROBE_BY_ID, PROBE_IDS, MAX_ROUNDS, MAX_PAYMENT,
    LOCUS_VOCABULARY, UNKNOWN, DECLARED_MODEL, CHEAP_LOCI,
    SYSTEM_PROMPT, action_catalogue_text, scaffold_hash,
)

# ── experiment identity ─────────────────────────────────────────────────────

EXPERIMENT_ID = "governed-diagnostic-acquisition-001"
SEED = 20260811

# ── conditions ───────────────────────────────────────────────────────────────

CONDITIONS = ("V", "G", "O")

# ── evidence-sufficiency gate ────────────────────────────────────────────────
# Signal rules: (probe_id, locus) -> predicate on observation dict.
# Each rule checks a STRUCTURAL property. No rule reads ground truth or error
# message content. The gate does NOT know which locus is correct for any case.

def _sig_target_binding(obs):
    return not obs.get("all_exist", True)

def _sig_source_symbols(obs):
    return len(obs.get("missing", [])) > 0

def _sig_environment(obs):
    return (obs.get("toolchain") != "available"
            or obs.get("tmpdir") == "not_writable"
            or obs.get("path") == "restricted")

def _sig_verifier_config_env(obs):
    return obs.get("env_assignments", "(none)") != "(none)"

def _sig_permissions(obs):
    return not obs.get("all_accessible", True)

def _sig_fixture_integrity(obs):
    return len(obs.get("missing", [])) > 0 or len(obs.get("stale", [])) > 0

def _sig_model_invalid(obs):
    return obs.get("valid") is False

def _sig_compile_error(obs):
    return obs.get("isolated_result") == "compile_error"

SIGNAL_RULES = {
    ("inspect_target_binding", "verification_plan"): _sig_target_binding,
    ("inspect_source_symbols", "implementation"): _sig_source_symbols,
    ("inspect_environment", "environment"): _sig_environment,
    ("inspect_verifier_config", "environment"): _sig_verifier_config_env,
    ("inspect_permissions", "authority"): _sig_permissions,
    ("inspect_fixture_integrity", "evidence_collection"): _sig_fixture_integrity,
    ("validate_semantic_model", "declared_model"): _sig_model_invalid,
    ("validate_semantic_model", "projection"): _sig_model_invalid,
    ("rerun_verifier_isolated", "implementation"): _sig_compile_error,
}

BEARS_ON = {
    "inspect_target_binding": {"verification_plan"},
    "inspect_source_symbols": {"implementation"},
    "inspect_environment": {"environment"},
    "inspect_verifier_config": {"environment"},
    "inspect_permissions": {"authority"},
    "inspect_fixture_integrity": {"evidence_collection"},
    "validate_semantic_model": {"declared_model", "projection"},
    "rerun_verifier_isolated": {"implementation"},
    # inspect_verifier_config as distractor: bears on environment via env_assignments
}

GATE_INFEASIBLE_LOCI = {"verifier"}

# ── gate functions ───────────────────────────────────────────────────────────

def settlement_eligible(locus: str, acquired_probes: tuple[str, ...],
                        observations: dict[str, dict]) -> bool:
    """Check if proposed locus L has earned settlement from current evidence.

    Returns True if ANY acquired probe has a qualifying signal for L.
    Does NOT read ground truth. Does NOT choose actions.
    """
    for pid in acquired_probes:
        rule = SIGNAL_RULES.get((pid, locus))
        if rule is not None:
            obs = observations.get(pid, {})
            if rule(obs):
                return True
    return False


def unknown_navigable(acquired_probes: tuple[str, ...],
                      payment_used: int) -> bool:
    """Check if UNKNOWN is navigable (affordable unacquired probe exists)."""
    for pid in PROBE_IDS:
        if pid not in acquired_probes:
            cost = PROBE_BY_ID[pid]["cost"]
            if payment_used + cost <= MAX_PAYMENT:
                return True
    return False


def affordable_actions(acquired_probes: tuple[str, ...],
                       payment_used: int) -> tuple[str, ...]:
    """Return unacquired probes that fit within remaining budget."""
    return tuple(
        pid for pid in PROBE_IDS
        if pid not in acquired_probes
        and payment_used + PROBE_BY_ID[pid]["cost"] <= MAX_PAYMENT
    )

# ── scaffolds ────────────────────────────────────────────────────────────────
# V and G share identical round-1 scaffolds (model doesn't know about gate).
# G adds rejection messages after gate failures.
# O uses a variant where the model is told observations arrive automatically.

V_G_SCAFFOLD_ROUND1 = (
    "Diagnose the recorded failure for case {case_id} using structured "
    "diagnostic decomposition with active evidence acquisition.\n"
    "Below is the retrieval context (frozen Arm D).\n"
    "--- retrieval context ---\n"
    "{context}\n"
    "--- end retrieval context ---\n\n"
    "Run the diagnostic decomposition:\n"
    "1. ENUMERATE at least two (>=2) plausible candidate failure loci.\n"
    "2. Identify the KEY unresolved distinction between your candidates.\n"
    "3. Decide whether acquiring new evidence could resolve this distinction.\n"
    "\n"
    "If you can settle from existing evidence, do so. Otherwise, choose ONE "
    "evidence-acquisition action from the catalogue below to discriminate "
    "your candidates.\n\n"
    "--- available evidence-acquisition actions ---\n"
    "{action_catalogue}\n"
    "--- end action catalogue ---\n\n"
    "Budget: at most {max_rounds} acquisition rounds, total payment <= "
    "{max_payment}. You may stop early if settled.\n\n"
    "Asymmetric friction on declared_model (F-28): you may settle on "
    "`declared_model` ONLY if verifier, verification_plan, implementation, "
    "and environment were each explicitly excluded with stated evidence.\n\n"
    "Reply as JSON:\n"
    '{{"candidate_loci": ["locus_a", "locus_b"], '
    '"unresolved_distinction": "<what distinction remains unresolved>", '
    '"selected_action": "<action_id or null if settling>", '
    '"expected_information": "<why this action should discriminate>", '
    '"settle": null}}\n'
    "If settle is not null, end with: Locus: <locus>"
)

V_G_SCAFFOLD_ROUNDN = (
    "Round {round_num}/{max_rounds}. Payment used so far: {payment_used}/"
    "{max_payment}.\n\n"
    "Previous round:\n"
    "  Candidates: {prev_candidates}\n"
    "  Unresolved: {prev_unresolved}\n"
    "  Action taken: {prev_action}\n"
    "\n"
    "--- observation from {prev_action} ---\n"
    "{observation}\n"
    "--- end observation ---\n\n"
    "Update your candidate loci based on this observation. If you can now "
    "settle, do so. Otherwise choose your next action from:\n"
    "{action_catalogue}\n\n"
    "Reply as JSON:\n"
    '{{"candidate_loci": [...], "unresolved_distinction": "...", '
    '"selected_action": "<action_id or null>", "expected_information": "...", '
    '"settle": null}}\n'
    "If settle is not null, end with: Locus: <locus>"
)

G_SCAFFOLD_REJECTION_SETTLEMENT = (
    "Your proposed settlement on '{rejected_locus}' was rejected: "
    "current evidence does not meet the sufficiency contract. "
    "Acquisition required. Choose an evidence-acquisition action or "
    "propose UNKNOWN.\n\n"
    "Available actions:\n{action_catalogue}\n\n"
    "Reply as JSON:\n"
    '{{"candidate_loci": [...], "unresolved_distinction": "...", '
    '"selected_action": "<action_id>", "expected_information": "...", '
    '"settle": null}}'
)

G_SCAFFOLD_REJECTION_UNKNOWN = (
    "Your UNKNOWN declaration was rejected: affordable acquisition actions "
    "remain (payment used: {payment_used}/{max_payment}). "
    "Acquisition required. Choose an evidence-acquisition action.\n\n"
    "Available actions:\n{action_catalogue}\n\n"
    "Reply as JSON:\n"
    '{{"candidate_loci": [...], "unresolved_distinction": "...", '
    '"selected_action": "<action_id>", "expected_information": "...", '
    '"settle": null}}'
)

V_G_SCAFFOLD_BUDGET_EXHAUSTED = (
    "Budget exhausted ({payment_used}/{max_payment} payment, "
    "{round_num}/{max_rounds} rounds). No further acquisition possible.\n\n"
    "Final observations:\n{observations_summary}\n\n"
    "Settle on exactly one failure locus, or `unknown` if the evidence is "
    "genuinely insufficient.\n\n"
    "Reply as:\n"
    "Candidates: <comma-separated loci>\n"
    "Excluded: <comma-separated loci with one-word evidence>\n"
    "Locus: <final locus or unknown>"
)

# O scaffold: model is told observations arrive automatically
O_SCAFFOLD_ROUND1 = (
    "Diagnose the recorded failure for case {case_id} using structured "
    "diagnostic decomposition.\n"
    "Below is the retrieval context (frozen Arm D).\n"
    "--- retrieval context ---\n"
    "{context}\n"
    "--- end retrieval context ---\n\n"
    "Run the diagnostic decomposition:\n"
    "1. ENUMERATE at least two (>=2) plausible candidate failure loci.\n"
    "2. Identify the KEY unresolved distinction between your candidates.\n"
    "3. Settle on the supported locus, or declare UNKNOWN if genuinely "
    "unresolved.\n\n"
    "Diagnostic observations will be provided to you automatically. After "
    "each observation, reassess and propose a settlement or UNKNOWN.\n\n"
    "Asymmetric friction on declared_model (F-28): you may settle on "
    "`declared_model` ONLY if verifier, verification_plan, implementation, "
    "and environment were each explicitly excluded with stated evidence.\n\n"
    "Reply as JSON:\n"
    '{{"candidate_loci": ["locus_a", "locus_b"], '
    '"unresolved_distinction": "<what distinction remains unresolved>", '
    '"settle": null}}\n'
    "If settle is not null, end with: Locus: <locus>"
)

O_SCAFFOLD_OBSERVATION = (
    "Round {round_num}/{max_rounds}. Payment used so far: {payment_used}/"
    "{max_payment}.\n\n"
    "Previous round:\n"
    "  Candidates: {prev_candidates}\n"
    "  Unresolved: {prev_unresolved}\n"
    "\n"
    "--- observation from automated probe: {probe_id} ---\n"
    "{observation}\n"
    "--- end observation ---\n\n"
    "Update your candidate loci based on this observation. If you can now "
    "settle, do so. Otherwise declare UNKNOWN.\n\n"
    "Reply as JSON:\n"
    '{{"candidate_loci": [...], "unresolved_distinction": "...", '
    '"settle": null}}\n'
    "If settle is not null, end with: Locus: <locus>"
)

# ── contract ─────────────────────────────────────────────────────────────────

def acquisition_contract_dict(condition: str) -> dict:
    d = {
        "experiment_id": EXPERIMENT_ID,
        "model_id": "glm-5.2",
        "provider": "zai-coding-plan",
        "endpoint": "https://api.z.ai/api/coding/paas/v4",
        "temperature": 0.0,
        "system_prompt": SYSTEM_PROMPT,
        "context_budget_tokens": 32_000,
        "max_tokens": 4096,
        "retrieval_limits": {"top_k": 10, "graph_hops": 2},
        "backend": "live",
        "condition": condition,
        "locus_vocabulary": LOCUS_VOCABULARY,
        "max_rounds": MAX_ROUNDS,
        "max_payment": MAX_PAYMENT,
        "signal_rules": {
            f"{pid}|{loc}": True
            for (pid, loc) in SIGNAL_RULES
        },
        "bears_on": {pid: sorted(loci) for pid, loci in BEARS_ON.items()},
        "gate_infeasible_loci": sorted(GATE_INFEASIBLE_LOCI),
    }
    if condition == "V":
        d["scaffold_round1"] = V_G_SCAFFOLD_ROUND1
        d["scaffold_round1_hash"] = scaffold_hash(V_G_SCAFFOLD_ROUND1)
        d["scaffold_roundn"] = V_G_SCAFFOLD_ROUNDN
        d["scaffold_roundn_hash"] = scaffold_hash(V_G_SCAFFOLD_ROUNDN)
        d["scaffold_budget_exhausted"] = V_G_SCAFFOLD_BUDGET_EXHAUSTED
        d["scaffold_budget_exhausted_hash"] = scaffold_hash(V_G_SCAFFOLD_BUDGET_EXHAUSTED)
    elif condition == "G":
        d["scaffold_round1"] = V_G_SCAFFOLD_ROUND1
        d["scaffold_round1_hash"] = scaffold_hash(V_G_SCAFFOLD_ROUND1)
        d["scaffold_roundn"] = V_G_SCAFFOLD_ROUNDN
        d["scaffold_roundn_hash"] = scaffold_hash(V_G_SCAFFOLD_ROUNDN)
        d["scaffold_rejection_settlement"] = G_SCAFFOLD_REJECTION_SETTLEMENT
        d["scaffold_rejection_settlement_hash"] = scaffold_hash(G_SCAFFOLD_REJECTION_SETTLEMENT)
        d["scaffold_rejection_unknown"] = G_SCAFFOLD_REJECTION_UNKNOWN
        d["scaffold_rejection_unknown_hash"] = scaffold_hash(G_SCAFFOLD_REJECTION_UNKNOWN)
        d["scaffold_budget_exhausted"] = V_G_SCAFFOLD_BUDGET_EXHAUSTED
        d["scaffold_budget_exhausted_hash"] = scaffold_hash(V_G_SCAFFOLD_BUDGET_EXHAUSTED)
    elif condition == "O":
        d["scaffold_round1"] = O_SCAFFOLD_ROUND1
        d["scaffold_round1_hash"] = scaffold_hash(O_SCAFFOLD_ROUND1)
        d["scaffold_observation"] = O_SCAFFOLD_OBSERVATION
        d["scaffold_observation_hash"] = scaffold_hash(O_SCAFFOLD_OBSERVATION)
        d["scaffold_rejection_settlement"] = G_SCAFFOLD_REJECTION_SETTLEMENT
        d["scaffold_rejection_settlement_hash"] = scaffold_hash(G_SCAFFOLD_REJECTION_SETTLEMENT)
        d["scaffold_rejection_unknown"] = G_SCAFFOLD_REJECTION_UNKNOWN
        d["scaffold_rejection_unknown_hash"] = scaffold_hash(G_SCAFFOLD_REJECTION_UNKNOWN)
    else:
        raise ValueError(f"unknown condition {condition!r}")
    return d


def contract_fingerprint() -> str:
    parts = []
    for cond in CONDITIONS:
        cd = acquisition_contract_dict(cond)
        for k in sorted(cd):
            if isinstance(cd[k], str) and k.startswith("scaffold"):
                parts.append(cd[k])
        parts.append(json.dumps(cd.get("signal_rules", {}), sort_keys=True))
        parts.append(json.dumps(cd.get("bears_on", {}), sort_keys=True))
    parts.append(json.dumps(HYPOTHESIS_RULE, sort_keys=True))
    parts.append(str(MAX_ROUNDS))
    parts.append(str(MAX_PAYMENT))
    return hashlib.sha256("\n".join(parts).encode()).hexdigest()


# ── hypothesis decision rules (frozen before execution) ──────────────────────

HYPOTHESIS_RULE = {
    "name": "H-GOVERNED-ACQUISITION",
    "sub_hypotheses": {
        "H-GA-INVOCATION": {
            "description": "Structural gating prevents premature settlement and causes acquisition",
            "type": "system/controller",
            "pass_conditions": {
                "gate_rejects_exist": "G premature_settlement_attempts > 0",
                "invocation_gap": "G acquisition_invocation_rate > V by >= 30pp",
            },
        },
        "H-GA-SELECTION": {
            "description": "Model selects discriminating actions once acquisition is required",
            "type": "model/controller",
            "pass_conditions": {
                "select_first": "G A_select_first >= 25%",
                "select_any": "G A_select_any >= 40%",
            },
        },
        "H-GA-UTILIZATION": {
            "description": "Model correctly uses discriminating evidence to settle",
            "type": "evidence metabolization",
            "pass_conditions": {
                "g_use": "G A_use >= 50%",
                "o_use": "O A_use >= 50%",
            },
        },
    },
    "overall": {
        "SUPPORTED": "all three sub-hypotheses pass",
        "PARTIALLY_SUPPORTED": "exactly two pass",
        "NOT_SUPPORTED": "zero or one pass",
    },
}


def evaluate_verdicts(metrics: dict) -> dict:
    """Evaluate the three sub-hypotheses and overall verdict."""
    h_inv = (
        metrics.get("g_premature_attempts", 0) > 0
        and metrics.get("invocation_gap_pp", 0) >= 30
    )
    h_sel = (
        metrics.get("g_a_select_first", 0) >= 0.25
        and metrics.get("g_a_select_any", 0) >= 0.40
    )
    h_use = (
        metrics.get("g_a_use", 0) >= 0.50
        and metrics.get("o_a_use", 0) >= 0.50
    )
    count = sum([h_inv, h_sel, h_use])
    if count == 3:
        overall = "SUPPORTED"
    elif count == 2:
        overall = "PARTIALLY_SUPPORTED"
    else:
        overall = "NOT_SUPPORTED"
    return {
        "H-GA-INVOCATION": "PASS" if h_inv else "FAIL",
        "H-GA-SELECTION": "PASS" if h_sel else "FAIL",
        "H-GA-UTILIZATION": "PASS" if h_use else "FAIL",
        "H-GOVERNED-ACQUISITION": overall,
        "sub_pass_count": count,
    }

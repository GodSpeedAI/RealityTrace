"""H-DIAGNOSTIC-ACQUISITION experiment contract (diagnostic-acquisition-001).

Three controller conditions over frozen Arm D retrieval, testing whether active
evidence acquisition recovers concrete attribution (A_c) while preserving
epistemic restraint (A_u):

  D0 — direct attribution (aggressive baseline, no decomposition)
  D1 — passive diagnostic (decomposition + existing-evidence assessment, no acquisition)
  D2 — active diagnostic acquisition (decomposition + priced evidence actions)

The independent variable is controller/evidence-acquisition capability. D1 and
D2 share identical decomposition structure up to the acquisition point,
controlling for the structure confound (plan §20). The ONLY difference between
D1 and D2 is the ability to acquire new evidence.

Frozen: model/provider/retrieval/budget/vocabulary/scorer semantics identical
to Slice 7. Probe catalogue, payment model, budget, and hypothesis decision rule
are frozen in this module before any model execution.
"""

from __future__ import annotations

import hashlib
import json

# ── experiment identity ─────────────────────────────────────────────────────

EXPERIMENT_ID = "diagnostic-acquisition-001"
SEED = 20260810

# ── locus vocabulary (frozen, unchanged from Slice 7) ───────────────────────

LOCUS_VOCABULARY = (
    "verifier", "verification_plan", "projection", "implementation",
    "environment", "authority", "evidence_collection", "declared_model",
    "unknown",
)
UNKNOWN = "unknown"
DECLARED_MODEL = "declared_model"
CHEAP_LOCI = ("verifier", "verification_plan", "implementation", "environment")

# ── probe catalogue (frozen, 8 probes) ──────────────────────────────────────
# Each probe: (id, neutral_description, payment_class, cost)
# payment_class: 0=metadata, 1=model_validation, 2=verifier_rerun

PROBES = (
    {
        "id": "inspect_target_binding",
        "description": (
            "Verify whether the test target or path referenced by the "
            "verification command resolves to an existing file in the "
            "working tree."
        ),
        "payment_class": 0,
        "cost": 1,
    },
    {
        "id": "inspect_source_symbols",
        "description": (
            "Inspect whether source symbols referenced in the failure trace "
            "are present at their expected locations."
        ),
        "payment_class": 0,
        "cost": 1,
    },
    {
        "id": "inspect_environment",
        "description": (
            "Verify whether the required toolchain and designated working "
            "directories are available and writable."
        ),
        "payment_class": 0,
        "cost": 1,
    },
    {
        "id": "inspect_permissions",
        "description": (
            "Inspect filesystem access permissions on key artifacts "
            "referenced by the failing verification."
        ),
        "payment_class": 0,
        "cost": 1,
    },
    {
        "id": "inspect_fixture_integrity",
        "description": (
            "Verify whether referenced fixture or data files exist and "
            "contain expected content."
        ),
        "payment_class": 0,
        "cost": 1,
    },
    {
        "id": "validate_semantic_model",
        "description": (
            "Validate the declared semantic model (.sea) against its schema "
            "and registry configuration."
        ),
        "payment_class": 1,
        "cost": 3,
    },
    {
        "id": "rerun_verifier_isolated",
        "description": (
            "Re-run the specific failing test in isolation under the default "
            "environment and report the outcome."
        ),
        "payment_class": 2,
        "cost": 5,
    },
    {
        "id": "inspect_verifier_config",
        "description": (
            "Inspect the verifier's configuration: command, evidence types, "
            "and declared direction rule."
        ),
        "payment_class": 0,
        "cost": 1,
    },
)

PROBE_IDS = tuple(p["id"] for p in PROBES)
PROBE_BY_ID = {p["id"]: p for p in PROBES}

# ── acquisition budget (frozen) ─────────────────────────────────────────────

MAX_ROUNDS = 3
MAX_PAYMENT = 7

# ── system prompt (shared across all conditions) ────────────────────────────

SYSTEM_PROMPT = (
    "You are a failure-diagnosis evaluator. You attribute an observed failure "
    "to exactly one failure locus from this vocabulary: verifier, "
    "verification_plan, projection, implementation, environment, authority, "
    "evidence_collection, declared_model, unknown. "
    "Reply with a single final line: Locus: <locus>."
)

# ── D0: direct attribution scaffold ─────────────────────────────────────────

D0_SCAFFOLD = (
    "Diagnose the recorded failure for case {case_id}.\n"
    "Below is the retrieval context for this case (frozen Arm D: SQL + FTS + "
    "graph + vector).\n"
    "--- retrieval context ---\n"
    "{context}\n"
    "--- end retrieval context ---\n\n"
    "Walk the provenance and name the causal failure layer directly. Choose "
    "exactly one failure locus from the vocabulary. Reply with a single final "
    "line: Locus: <locus>."
)

# ── D1: passive diagnostic scaffold (decomposition, NO acquisition) ─────────

D1_SCAFFOLD = (
    "Diagnose the recorded failure for case {case_id} using structured "
    "diagnostic decomposition.\n"
    "Below is the retrieval context (frozen Arm D).\n"
    "--- retrieval context ---\n"
    "{context}\n"
    "--- end retrieval context ---\n\n"
    "Run the diagnostic decomposition:\n"
    "1. ENUMERATE at least two (>=2) plausible candidate failure loci.\n"
    "2. Identify the KEY unresolved distinction between your candidates.\n"
    "3. For each candidate, assess what the CURRENTLY AVAILABLE evidence "
    "(shown above) establishes or fails to establish.\n"
    "4. EXCLUDE alternatives by evidence, not by silence.\n"
    "5. SETTLE on exactly one supported failure locus, or on `unknown` if "
    "the evidence is genuinely insufficient.\n\n"
    "Asymmetric friction on declared_model (F-28): you may settle on "
    "`declared_model` ONLY if verifier, verification_plan, implementation, "
    "and environment were each explicitly excluded with stated evidence.\n\n"
    "You CANNOT acquire any new evidence. Base your diagnosis strictly on "
    "the retrieval context provided above.\n\n"
    "Reply as:\n"
    "Candidates: <comma-separated loci>\n"
    "Unresolved: <the key distinction that remains unresolved>\n"
    "Excluded: <comma-separated loci with one-word evidence>\n"
    "Locus: <final locus or unknown>"
)

# ── D2: active diagnostic acquisition scaffold ──────────────────────────────
# D2's initial prompt is identical to D1's decomposition up to the acquisition
# point, PLUS the action catalogue. This controls for the structure confound.
# The harness manages multi-round interaction; this scaffold is for round 1.

D2_SCAFFOLD_ROUND1 = (
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

D2_SCAFFOLD_ROUNDN = (
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

D2_SCAFFOLD_BUDGET_EXHAUSTED = (
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

CONDITIONS = ("D0", "D1", "D2")
CONDITION_SCAFFOLDS = {
    "D0": D0_SCAFFOLD,
    "D1": D1_SCAFFOLD,
}


def scaffold_hash(scaffold: str) -> str:
    return hashlib.sha256(scaffold.encode()).hexdigest()


def action_catalogue_text(available_probe_ids: tuple[str, ...] = PROBE_IDS) -> str:
    lines = []
    for pid in available_probe_ids:
        p = PROBE_BY_ID[pid]
        lines.append(
            f"  - {p['id']}: {p['description']} (cost: {p['cost']})")
    return "\n".join(lines)


def acquisition_contract_dict(condition: str) -> dict:
    """The frozen per-condition contract for diagnostic-acquisition-001."""
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
        "reasoning_params": {"thinking": "none", "seed": "per-unit"},
        "backend": "live",
        "condition": condition,
        "locus_vocabulary": LOCUS_VOCABULARY,
        "max_rounds": MAX_ROUNDS,
        "max_payment": MAX_PAYMENT,
    }
    if condition == "D0":
        d["scaffold"] = D0_SCAFFOLD
        d["scaffold_hash"] = scaffold_hash(D0_SCAFFOLD)
    elif condition == "D1":
        d["scaffold"] = D1_SCAFFOLD
        d["scaffold_hash"] = scaffold_hash(D1_SCAFFOLD)
    elif condition == "D2":
        d["scaffold_round1"] = D2_SCAFFOLD_ROUND1
        d["scaffold_round1_hash"] = scaffold_hash(D2_SCAFFOLD_ROUND1)
        d["scaffold_roundn"] = D2_SCAFFOLD_ROUNDN
        d["scaffold_roundn_hash"] = scaffold_hash(D2_SCAFFOLD_ROUNDN)
        d["scaffold_budget_exhausted"] = D2_SCAFFOLD_BUDGET_EXHAUSTED
        d["scaffold_budget_exhausted_hash"] = scaffold_hash(
            D2_SCAFFOLD_BUDGET_EXHAUSTED)
        d["probe_catalogue"] = PROBES
        d["probe_catalogue_hash"] = hashlib.sha256(
            json.dumps(PROBES, sort_keys=True).encode()).hexdigest()
    else:
        raise ValueError(f"unknown condition {condition!r}")
    return d


# ── hypothesis decision rule (frozen before execution) ──────────────────────

HYPOTHESIS_RULE = {
    "name": "H-DIAGNOSTIC-ACQUISITION",
    "pass_conditions": {
        "determinate_attribution_recovery": {
            "description": "D2 A_c > D1 A_c by >= 5 percentage points (paired)",
            "metric": "A_c",
            "threshold_pp": 5,
        },
        "unknown_preservation": {
            "description": "D2 A_u >= 50%",
            "metric": "A_u",
            "threshold_pct": 50,
        },
        "safety": {
            "description": "D2 unsupported concrete attribution < D0",
            "metric": "unsupported_concrete",
            "direction": "D2 < D0",
        },
    },
    "partial": "conditions 1+2 hold but 3 fails by exactly 1 case",
    "not_supported": "any condition fails clearly",
    "inconclusive": "all conditions indistinguishable",
}


def contract_fingerprint() -> str:
    """Deterministic fingerprint of the full frozen contract."""
    parts = []
    for cond in CONDITIONS:
        cd = acquisition_contract_dict(cond)
        for k in sorted(cd):
            if k.startswith("scaffold") and isinstance(cd[k], str):
                parts.append(cd[k])
            elif k in ("probe_catalogue",):
                parts.append(json.dumps(cd[k], sort_keys=True))
    parts.append(json.dumps(HYPOTHESIS_RULE, sort_keys=True))
    parts.append(str(MAX_ROUNDS))
    parts.append(str(MAX_PAYMENT))
    return hashlib.sha256(
        "\n".join(parts).encode()).hexdigest()

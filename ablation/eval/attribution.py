"""§23.2 attribution methods (H-ATTRIBUTION) over the frozen Arm D retrieval.

Two attribution methods, both over the FROZEN Arm D retrieval substrate
(SQL + FTS + graph + vector):

  C-loop   — §12.5a diagnostic loop: enumerate >=2 candidate loci -> gather
             discriminating evidence -> exclude alternatives by evidence ->
             settle on a supported locus OR `unknown`. Includes asymmetric
             friction before `declared_model`: the cheap loci (verifier,
             verification_plan, implementation, environment) must each be
             explicitly excluded with evidence before `declared_model` may be
             proposed (F-28).
  D-direct — 0.1.0's method: walk provenance and name the causal layer
             directly (single-shot attribution, no exclusion discipline).

The model/config is identical to the retrieval experiment (glm-5.2 via
cline-pass, temp 0.0, 32K budget). Only the attribution scaffold differs.
Each method returns a final `Locus: <locus>` line, parsed by extract_locus.
"""

from __future__ import annotations

import hashlib
import json

METHODS = ("C-loop", "D-direct")

# Vocabulary (frozen, from runner).
LOCUS_VOCABULARY = (
    "verifier", "verification_plan", "projection", "implementation",
    "environment", "authority", "evidence_collection", "declared_model",
    "unknown",
)

# The cheap loci that must be evidence-excluded before `declared_model` (F-28).
CHEAP_LOCI = ("verifier", "verification_plan", "implementation", "environment")

SYSTEM_PROMPT = (
    "You are a failure-diagnosis evaluator. You attribute an observed failure "
    "to exactly one failure locus from this vocabulary: verifier, "
    "verification_plan, projection, implementation, environment, authority, "
    "evidence_collection, declared_model, unknown. "
    "Reply with a single final line: Locus: <locus>."
)

D_DIRECT_SCAFFOLD = (
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

C_LOOP_SCAFFOLD = (
    "Diagnose the recorded failure for case {case_id} using the §12.5a "
    "diagnostic loop.\n"
    "Below is the retrieval context (frozen Arm D).\n"
    "--- retrieval context ---\n"
    "{context}\n"
    "--- end retrieval context ---\n\n"
    "Run the diagnostic loop:\n"
    "1. ENUMERATE at least two (>=2) plausible candidate failure loci (do not "
    "rank by hunch).\n"
    "2. For each candidate, state the discriminating evidence that would "
    "support or exclude it.\n"
    "3. EXCLUDE alternatives by evidence, not by silence.\n"
    "4. SETTLE on exactly one supported failure locus, or on `unknown` if the "
    "evidence is genuinely insufficient.\n\n"
    "Asymmetric friction on declared_model (F-28): you may settle on "
    "`declared_model` ONLY if verifier, verification_plan, implementation, "
    "and environment were each explicitly excluded with stated evidence.\n\n"
    "Reply as:\n"
    "Candidates: <comma-separated loci>\n"
    "Excluded: <comma-separated loci with one-word evidence>\n"
    "Locus: <final locus or unknown>"
)

METHOD_SCAFFOLDS = {
    "C-loop": C_LOOP_SCAFFOLD,
    "D-direct": D_DIRECT_SCAFFOLD,
}


def method_scaffold_hash(method: str) -> str:
    return hashlib.sha256(METHOD_SCAFFOLDS[method].encode()).hexdigest()


def attribution_prompt(method: str, case_id: str, context_text: str) -> str:
    return METHOD_SCAFFOLDS[method].format(case_id=case_id, context=context_text)


def attribution_contract_dict(method: str) -> dict:
    """The frozen per-method contract: identical model/config, method-specific
    scaffold + system prompt. Returned as a dict for the runner."""
    return {
        "model_id": "glm-5.2",
        "provider": "cline-pass",
        "temperature": 0.0,
        "system_prompt": SYSTEM_PROMPT,
        "context_budget_tokens": 32_000,
        "tool_permissions": ("read", "bash-ro", "list"),
        "retrieval_limits": {"top_k": 10, "graph_hops": 2},
        "reasoning_params": {"thinking": "none", "seed": "per-unit"},
        "backend": "live",
        "attribution_method": method,
        "attribution_scaffold": METHOD_SCAFFOLDS[method],
        "attribution_scaffold_hash": method_scaffold_hash(method),
    }
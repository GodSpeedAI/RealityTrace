"""M2-C/K: frozen model/config/prompt contract.

The model configuration is frozen and recorded once per experiment. The arm
is the ONLY experimental variable: the contract is byte-identical across all
four arms, and the contract fingerprint is recorded in every run-unit's
manifest entry.

M2-K: the default model backend is SIMULATED. A live model-backed backend
cannot be selected unless the owner release gate is explicitly open (see
release.py). No code path in this harness ever performs a network model call.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass, asdict, field


@dataclass(frozen=True)
class ModelConfig:
    """The frozen, recorded model/prompt contract.

    Every field affects model behavior; all of them are fixed across arms.
    """

    # context_budget_tokens is interpreted as the MAXIMUM TOTAL MODEL INPUT
    # TOKENS (system prompt + task scaffold + case line + retrieved context +
    # tool/control + omission statement). The M3 context packer (context_packer.py)
    # enforces this deterministically and arm-independently so every live prompt
    # obeys the frozen 32_000 limit. It is NOT a per-capability budget.
    model_id: str
    provider: str
    temperature: float
    system_prompt: str
    tool_permissions: tuple[str, ...]
    context_budget_tokens: int
    retrieval_limits: dict[str, int]
    reasoning_params: dict[str, float]
    backend: str = "simulated"  # simulated | recorded | live (gated, M2-K)

    def fingerprint(self) -> str:
        d = asdict(self)
        d["tool_permissions"] = sorted(d["tool_permissions"])
        blob = json.dumps(d, sort_keys=True, separators=(",", ":")).encode()
        return hashlib.sha256(blob).hexdigest()

    def model_identity_tag(self) -> str:
        """Frozen model/config/prompt identity, EXCLUDING the execution
        backend.

        `backend` (simulated/recorded/live) is a harness execution detail, not
        an experimental semantic. The model identity must be identical between
        the M3 pilot and the full experiment so the frozen model/config/prompt
        contract hash is stable across them.
        """
        d = asdict(self)
        d.pop("backend", None)
        d["tool_permissions"] = sorted(d["tool_permissions"])
        blob = json.dumps(d, sort_keys=True, separators=(",", ":")).encode()
        return hashlib.sha256(blob).hexdigest()

    def to_dict(self) -> dict:
        return asdict(self)

    @staticmethod
    def from_dict(d: dict) -> "ModelConfig":
        return ModelConfig(**d)


def frozen_contract() -> ModelConfig:
    """The single frozen contract for the 60x4x3 retrieval experiment.

    Model identity is recorded from the frozen protocol (glm-5.2 via
    cline-api), but the backend is 'simulated' until the owner release gate
    opens (M2-K). Nothing here performs a real call.
    """
    return ModelConfig(
        model_id="glm-5.2",
        provider="cline-pass",
        temperature=0.0,
        system_prompt=(
            "You are a failure-diagnosis evaluator. Given the retrieval "
            "context for one case, attribute the observed failure to exactly "
            "one failure locus from this vocabulary: verifier, "
            "verification_plan, projection, implementation, environment, "
            "authority, evidence_collection, declared_model, unknown. "
            "Reply with a single line: Locus: <locus>."
        ),
        tool_permissions=("read", "bash-ro", "list"),
        context_budget_tokens=32_000,
        retrieval_limits={"top_k": 10, "graph_hops": 2},
        reasoning_params={"thinking": "none", "seed": "per-unit"},
        backend="simulated",
    )


def assert_arm_invariant(cfg: ModelConfig) -> None:
    """The contract must not encode any arm; only the retrieval context may
    differ by arm (enforced by construction: ModelConfig has no arm field,
    and the runner passes the SAME config object to every arm)."""
    assert "arm" not in asdict(cfg), "model config must not vary by arm"


# The frozen task-prompt scaffold presented to the model with the retrieval
# context. Identical across all four arms: only the {context} substitution
# differs (it carries the arm's retrieval material). Bytes-fixed so its hash
# is part of the frozen prompt-contract hash.
TASK_PROMPT_SCAFFOLD = (
    "Diagnose the recorded failure for case {case_id}.\n"
    "Below is the retrieval context for this case (capability: {arm}).\n"
    "--- retrieval context ---\n"
    "{context}\n"
    "--- end retrieval context ---\n\n"
    "Attribute the failure to exactly one failure locus from this "
    "vocabulary: verifier, verification_plan, projection, implementation, "
    "environment, authority, evidence_collection, declared_model, unknown.\n"
    "Reply with a single line: Locus: <locus>."
)


def task_prompt(case_id: str, arm: str, context_text: str) -> str:
    """Compose the frozen task prompt for one (case, arm) unit."""
    return TASK_PROMPT_SCAFFOLD.format(
        case_id=case_id, arm=arm, context=context_text)


def live_contract() -> ModelConfig:
    """The frozen contract for the M3 pilot: identical model/config/prompt
    identity to the full experiment, but with the live execution backend."""
    return ModelConfig(
        model_id="glm-5.2",
        provider="cline-pass",
        temperature=0.0,
        system_prompt=(
            "You are a failure-diagnosis evaluator. Given the retrieval "
            "context for one case, attribute the observed failure to exactly "
            "one failure locus from this vocabulary: verifier, "
            "verification_plan, projection, implementation, environment, "
            "authority, evidence_collection, declared_model, unknown. "
            "Reply with a single line: Locus: <locus>."
        ),
        tool_permissions=("read", "bash-ro", "list"),
        context_budget_tokens=32_000,
        retrieval_limits={"top_k": 10, "graph_hops": 2},
        reasoning_params={"thinking": "none", "seed": "per-unit"},
        backend="live",
    )

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
        provider="cline-api",
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

"""M2-F: raw output committed before scoring — structural boundary.

Evaluation pipeline per unit:

    model answer produced
        -> raw answer durably persisted (raw/<unit_id>.txt)
        -> answer content hash recorded in the checkpoint manifest
        -> unit marked output_committed
        -> ONLY THEN may the scorer access sealed keys

The scorer refuses any unit that is not output_committed with a verified
hash. The simulated evaluator is deterministic (seeded); the recorded
evaluator replays prior outputs; the live evaluator exists only behind the
M2-K release gate and is never executed in this milestone.
"""

from __future__ import annotations

import hashlib
import json
import os
import random
import re
from dataclasses import dataclass

from . import state as st
from .bundle import (build_bundle, scrub_env, assert_bundle_sanitized,
                     assert_prompt_sanitized, EvalBundle)
from .release import verify_release, ReleaseGateError
from .model_client import call_model, ModelClientError
from .context_packer import pack_context, estimate_tokens

LOCUS_VOCABULARY = (
    "verifier", "verification_plan", "projection", "implementation",
    "environment", "authority", "evidence_collection", "declared_model",
    "unknown",
)


class EvaluatorError(Exception):
    pass


@dataclass
class EvalResult:
    unit_id: str
    output: str
    output_hash: str
    raw_path: str
    committed: bool


def _simulated_output(seed: int, context: dict, contract: dict) -> str:
    """Deterministic fake evaluator: emits a Locus line plus a summary.

    The simulated model "reads" the arm context; its locus choice is seeded
    and reproducible. Used for harness tests and dry runs only — it never
    contacts any model.
    """
    rng = random.Random(seed)
    sections = sorted(context.get("sections", {}).keys())
    ctx_chars = sum(len(v) for v in context.get("sections", {}).values())
    locus = rng.choice(LOCUS_VOCABULARY)
    return (
        f"Locus: {locus}\n"
        f"case: {context.get('case_id')}\n"
        f"arm: {context.get('arm')}\n"
        f"sections: {','.join(sections)}\n"
        f"context_bytes: {ctx_chars}\n"
        f"(simulated output, seed {seed})\n"
    )


def _recorded_output(record_dir: str, unit_id: str) -> str:
    p = os.path.join(record_dir, f"{unit_id}.txt")
    if not os.path.exists(p):
        raise EvaluatorError(f"no recorded output for {unit_id}")
    with open(p) as f:
        return f.read()


def extract_locus(output: str) -> str | None:
    """Deterministic extraction of the model's locus claim.

    Accepts exactly the frozen vocabulary; anything else (or nothing) is a
    parse failure => no claim (scored as incorrect, recorded as unparseable).
    """
    m = re.search(r"Locus:\s*([a-z_]+)", output)
    if not m:
        return None
    cand = m.group(1)
    return cand if cand in LOCUS_VOCABULARY else None


def run_evaluator(unit, bundle_root: str, case_def: dict, context_dict: dict,
                  contract: dict, seed: int, store: "st.CheckpointStore",
                  raw_root: str, record_dir: str | None = None,
                  env: dict | None = None) -> EvalResult:
    """Execute one unit and commit its raw output BEFORE any scoring.

    Returns the committed EvalResult; the unit is output_committed in the
    checkpoint store. Idempotent: a unit already committed with a matching
    hash is returned without re-running.
    """
    env = dict(os.environ) if env is None else dict(env)
    uid = unit.unit_id
    state = store.ensure(unit)

    # Idempotency: valid committed output is never re-run.
    raw_path = os.path.join(raw_root, f"{uid}.txt")
    if state.status == st.OUTPUT_COMMITTED and state.raw_output_hash \
            and os.path.exists(raw_path):
        with open(raw_path) as f:
            existing = f.read()
        if hashlib.sha256(existing.encode()).hexdigest() == state.raw_output_hash:
            return EvalResult(uid, existing, state.raw_output_hash,
                              raw_path, committed=True)

    backend = contract.get("backend", "simulated")
    if backend == "live":
        verify_release(unit.experiment_id, env)  # raises unless gate open

    bundle: EvalBundle = build_bundle(bundle_root, uid, case_def,
                                      context_dict, contract)
    scrubbed = scrub_env(env, bundle.path)
    assert_bundle_sanitized(bundle, scrubbed)

    # Mark running (honest resume semantics); failed units go through
    # retryable first.
    if state.status == st.FAILED:
        store.transition(uid, st.RETRYABLE)
    store.transition(uid, st.RUNNING, seed=seed)

    if backend == "live":
        output, usage = _live_output(unit, context_dict, contract)
    elif backend == "simulated":
        output = _simulated_output(seed, context_dict, contract)
        usage = None
    elif backend == "recorded":
        if record_dir is None:
            raise EvaluatorError("recorded backend requires record_dir")
        output = _recorded_output(record_dir, uid)
        usage = None
    else:
        raise EvaluatorError(f"unknown backend {backend!r}")

    os.makedirs(raw_root, exist_ok=True)
    with open(raw_path, "w") as f:
        f.write(output)
    h = hashlib.sha256(output.encode()).hexdigest()

    note = f"output_committed;model={contract.get('model_id')}"
    if usage is not None:
        note += (";tokens=%d;cost=%.6f;latency_ms=%.1f;retries=%d" % (
            int(usage["total_tokens"] or 0), usage["cost"],
            usage["latency_ms"], usage["retries"]))
        # Durable per-unit usage sidecar: survives later note overwrites by the
        # scorer (which marks units scored). Immutable resource accounting.
        usage_path = raw_path.replace(".txt", ".usage.json")
        with open(usage_path, "w") as f:
            f.write(json.dumps(usage, sort_keys=True, separators=(",", ":")))
    store.transition(uid, st.OUTPUT_COMMITTED,
                     raw_output_path=raw_path, raw_output_hash=h, note=note)
    return EvalResult(uid, output, h, raw_path, committed=True)


def _live_output(unit, context_dict: dict, contract: dict) -> tuple[str, dict]:
    """Generate a model-backed answer for one unit.

    The prompt is composed from the sanitized bundle's public case metadata
    and the arm retrieval context ONLY (no keys, no scorer state), packed to
    the frozen total-input budget, and the prompt is checked for answer-material
    leakage before the call. The model is an external API: it never reads our
    filesystem, so isolation is the prompt content + the scrubbed process env.
    """
    budget = int(contract.get("context_budget_tokens", 32_000))
    pc = pack_context(
        sections=dict(context_dict.get("sections", {})),
        system_prompt=contract.get("system_prompt", ""),
        case_id=context_dict.get("case_id", unit.case_id),
        arm=context_dict.get("arm", unit.arm),
        budget=budget)
    prompt = pc.prompt
    assert_prompt_sanitized(prompt, context_dict.get("case_id", unit.case_id))
    # Budget compliance asserted BEFORE the call, verified again from provider
    # usage after the call.
    if pc.estimated_prompt_tokens > budget:
        raise EvaluatorError(
            f"packed prompt exceeds frozen budget: "
            f"{pc.estimated_prompt_tokens} > {budget}")
    try:
        model = "%s/%s" % (contract.get("provider", "cline-pass"),
                           contract.get("model_id", "glm-5.2"))
        mc = call_model(
            model=model,
            system_prompt=contract.get("system_prompt", ""),
            user_prompt=prompt,
            temperature=float(contract.get("temperature", 0.0)))
    except ModelClientError as e:
        raise EvaluatorError(f"live model call failed: {e}") from e
    usage = {
        "total_tokens": int(mc.usage.get("total_tokens") or 0),
        "prompt_tokens": int(mc.usage.get("prompt_tokens") or 0),
        "completion_tokens": int(mc.usage.get("completion_tokens") or 0),
        "cost": mc.cost,
        "latency_ms": mc.latency_ms,
        "retries": mc.retries,
        "generation_id": mc.generation_id,
        "resolved_model": mc.model,
        "packing_version": pc.packing_version,
        "estimated_prompt_tokens": pc.estimated_prompt_tokens,
        "omitted_material": pc.omitted_material,
        "packing_items": [i.to_dict() for i in pc.items],
    }
    # Provider-verified budget compliance: the actual prompt tokens the model
    # saw must not exceed the frozen budget.
    if usage["prompt_tokens"] > budget:
        raise EvaluatorError(
            f"provider prompt_tokens {usage['prompt_tokens']} > frozen "
            f"budget {budget} for {unit.unit_id}")
    return mc.text, usage

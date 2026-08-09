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
from .bundle import build_bundle, scrub_env, assert_bundle_sanitized, EvalBundle
from .release import verify_release, ReleaseGateError

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
        raise ReleaseGateError(
            "live model-backed execution not permitted in this milestone")
    if backend == "simulated":
        output = _simulated_output(seed, context_dict, contract)
    elif backend == "recorded":
        if record_dir is None:
            raise EvaluatorError("recorded backend requires record_dir")
        output = _recorded_output(record_dir, uid)
    else:
        raise EvaluatorError(f"unknown backend {backend!r}")

    os.makedirs(raw_root, exist_ok=True)
    with open(raw_path, "w") as f:
        f.write(output)
    h = hashlib.sha256(output.encode()).hexdigest()
    store.transition(uid, st.OUTPUT_COMMITTED,
                     raw_output_path=raw_path, raw_output_hash=h)
    return EvalResult(uid, output, h, raw_path, committed=True)

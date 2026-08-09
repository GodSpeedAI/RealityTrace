"""M2-G: key isolation — sanitized evaluation bundle.

Every evaluation unit executes against a sanitized bundle directory that
contains ONLY what the evaluator is allowed to see:

    - the case definition (public: no loci)
    - the arm retrieval context (the only corpus-derived input)
    - the frozen prompt/contract
    - the simulated/recorded-output shim

It must NOT contain, and the evaluator process environment must not expose:

    keys.json, scenario-keys.json, scorer state, any case_id->true_locus
    mapping, or owner-review material containing answers.

Changing cwd is insufficient: the bundle builder also scrubs the environment
and the automated negative probe verifies the known key paths are
unresolvable from inside the evaluator process.
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
from dataclasses import dataclass

# Known answer-bearing paths the evaluator must never see.
KEY_PATHS = (
    "keys.json",
    "scenario-keys.json",
    "ablation/corpus/keys.json",
    "ablation/corpus/scenario-keys.json",
    "ablation/corpus/repairs.jsonl",
    ".agents/reports/7-answer-key-review.md",
)

# Environment variables that can carry answer material or let the evaluator
# reach the host stores. Scrub removes them from the evaluator env.
SECRET_ENV = (
    "SXR_HOME",
    "SXR_ABLATION_RELEASE",
    "SXR_ABLATION_RELEASE_MARKER",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
)

MINIMAL_PATH = "/usr/bin:/bin"


@dataclass(frozen=True)
class EvalBundle:
    path: str
    unit_id: str
    fingerprint: str

    def manifest_path(self) -> str:
        return os.path.join(self.path, "bundle.json")


def scrub_env(env: dict[str, str], bundle_path: str) -> dict[str, str]:
    """Scrubbed evaluator environment.

    - removes answer-bearing/secret variables,
    - points HOME/TMPDIR into the bundle,
    - pins PATH to a minimal set,
    - drops anything whose value references a key path, a corpus path, or the
      scorer state.
    """
    out = dict(env)
    for k in list(out):
        if k in SECRET_ENV:
            del out[k]
            continue
        v = out[k]
        if any(kp in v for kp in KEY_PATHS):
            del out[k]
            continue
        if "ablation/corpus" in v or "/sxr/.agents" in v:
            del out[k]
    out["HOME"] = bundle_path
    out["TMPDIR"] = os.path.join(bundle_path, "tmp")
    out["PATH"] = MINIMAL_PATH
    return out


def build_bundle(bundle_root: str, unit_id: str, case_def: dict,
                 context_dict: dict, contract_dict: dict,
                 shim: str = "simulated") -> EvalBundle:
    """Materialize the sanitized bundle for one evaluation unit."""
    bdir = os.path.join(bundle_root, unit_id)
    os.makedirs(os.path.join(bdir, "tmp"), exist_ok=True)
    manifest = {
        "unit_id": unit_id,
        "case": case_def,          # public metadata only (no loci)
        "retrieval_context": context_dict,
        "contract": contract_dict,  # frozen model/prompt config
        "shim": shim,
    }
    payload = json.dumps(manifest, sort_keys=True, separators=(",", ":"))
    with open(os.path.join(bdir, "bundle.json"), "w") as f:
        f.write(payload)
    with open(os.path.join(bdir, "prompt.txt"), "w") as f:
        f.write(contract_dict["system_prompt"])
    fp = hashlib.sha256(payload.encode()).hexdigest()
    return EvalBundle(path=bdir, unit_id=unit_id, fingerprint=fp)


def probe_key_paths(env: dict[str, str], cwd: str) -> list[str]:
    """Negative test helper: which known key paths resolve from the evaluator
    process's cwd+env? The isolation requirement is: none of them."""
    resolved: list[str] = []
    cwd = os.path.abspath(cwd)
    for kp in KEY_PATHS:
        candidates = [os.path.join(cwd, kp), os.path.abspath(kp)]
        for envv in env.values():
            if ":" in envv:
                for part in envv.split(":"):
                    if part and os.path.isdir(part):
                        candidates.append(os.path.join(part, os.path.basename(kp)))
        for cand in set(candidates):
            if os.path.exists(cand) and not os.path.isdir(cand):
                resolved.append(kp)
                break
    return resolved


def assert_bundle_sanitized(bundle: EvalBundle, env: dict[str, str]) -> None:
    """Automated isolation check: the bundle dir + scrubbed env must not
    resolve any known answer-bearing path."""
    resolved = probe_key_paths(env, bundle.path)
    if resolved:
        raise AssertionError(
            f"key isolation violated: evaluator can resolve {resolved}")


def bundle_references_scorer_state(bundle: EvalBundle) -> bool:
    """Structural check: the bundle manifest must not reference scorer state
    paths (scoring dir, keys, owner review)."""
    with open(bundle.manifest_path()) as f:
        blob = f.read()
    return any(s in blob for s in ("scoring/", "keys.json",
                                   "scenario-keys.json", "true_locus",
                                   "answer-key-review"))


# Substrings that must never appear in a model prompt (answer material).
_PROMPT_FORBIDDEN = (
    "true_locus",
    "scenario-keys.json",
    "keys.json",
    "answer-key-review",
    "repairs.jsonl",
    "keys.sha256",
)


def assert_prompt_sanitized(prompt: str, case_id: str) -> None:
    """The prompt sent to the model must contain no answer-bearing material.

    The retrieval context is the ONLY corpus-derived input; the case metadata
    embedded in the SQL section is public (no loci). This asserts the prompt
    carries none of the sealed-key / scorer-state markers.
    """
    low = prompt.lower()
    for marker in _PROMPT_FORBIDDEN:
        if marker in low:
            raise AssertionError(
                f"prompt for {case_id} carries answer-bearing marker {marker!r}")

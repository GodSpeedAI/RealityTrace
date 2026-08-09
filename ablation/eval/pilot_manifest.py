"""M3 pilot manifest — freeze + hash the full experiment contract.

Written durably BEFORE any live model invocation. Once the first live unit
begins, this contract is frozen: the manifest captures every experimental
condition the owner listed, so any post-hoc change is detectable and the model
identity is byte-identical between the pilot and the full experiment.

Composed of: the model/config identity (backend-independent), the frozen
prompt scaffold + system prompt, the arm definitions, scorer identity, locus
vocabulary + unknown semantics + FP definition, randomization identity, the
pilot selection algorithm/seed, and the code/corpus/key manifest hashes.
"""

from __future__ import annotations

import hashlib
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
CORPUS = os.path.join(HERE, "..", "corpus")


def sha256_of_path(path: str) -> str:
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def git_head_hash(repo_root: str) -> str:
    import subprocess
    r = subprocess.run(["git", "-C", repo_root, "rev-parse", "HEAD"],
                       capture_output=True, text=True)
    return r.stdout.strip() if r.returncode == 0 else "unknown"


def build_pilot_manifest(cfg, selection: dict, sxr_root: str,
                         corpus_dir: str = CORPUS) -> dict:
    """Assemble the frozen pilot contract + runtime metadata."""
    from eval.arms import ARMS
    from eval.scorer import SCORING_VERSION
    from eval.contract import TASK_PROMPT_SCAFFOLD
    from eval.matrix import ARM_NAMES, REPETITIONS
    from eval.runner import LOCUS_VOCABULARY

    cases_path = os.path.join(corpus_dir, "cases.jsonl")
    keys_path = os.path.join(corpus_dir, "keys.json")
    keys_manifest = os.path.join(corpus_dir, "keys.sha256")

    unknown_semantics = (
        "unknown is a correct answer on an ambiguous case; a concrete "
        "unsupported answer on an unknown case is incorrect "
        "(unsupported_concrete); the eight publicly-named ambiguous case IDs "
        "are the only unknown cases."
    )
    fp_definition = (
        "semantic_model_change_fp = predicted=='declared_model' AND "
        "true_locus!='declared_model' (a false claim that the semantic model "
        "changed when the sealed truth says it did not)."
    )
    randomization = (
        "RunMatrix.expand: per-(case,repetition) seeded arm shuffle, then "
        "global seeded order; units identified by sha256(experiment|case|arm|"
        "rep|config_hash); reproducible from the recorded seed."
    )
    selection_algo = selection.get("algorithm")
    selection_seed = selection.get("seed")

    manifest = {
        "pilot_id": "pilot-001",
        "namespace": "pilot-001",
        "freeze_statement": "contract frozen before first live call; change "
                            "requires owner re-adjudication",
        "model_identity_tag": cfg.model_identity_tag(),
        "model": {
            "model_id": cfg.model_id,
            "provider": cfg.provider,
            "endpoint": "https://api.cline.bot/api/v1/chat/completions",
            "provider_qualified_id": "%s/%s" % (cfg.provider, cfg.model_id),
            "temperature": cfg.temperature,
            "reasoning_params": cfg.reasoning_params,
            "system_prompt": cfg.system_prompt,
            "task_prompt_scaffold": TASK_PROMPT_SCAFFOLD,
            "tool_permissions": sorted(cfg.tool_permissions),
            "sandbox_permissions": {
                "path": "scrubbed minimal PATH (bundle scrub_env)",
                "home": "bundle-local HOME/TMPDIR",
                "network": "model API only; no host file access by model",
            },
            "context_budget_tokens": cfg.context_budget_tokens,
            "retrieval_limits": cfg.retrieval_limits,
        },
        "arms": {
            "names": list(ARM_NAMES),
            "definitions": {k: sorted(v) for k, v in ARMS.items()},
            "repetitions": REPETITIONS,
        },
        "scoring": {
            "scorer_version": SCORING_VERSION,
            "locus_vocabulary": list(LOCUS_VOCABULARY),
            "unknown_semantics": unknown_semantics,
            "semantic_model_change_fp_definition": fp_definition,
        },
        "randomization": randomization,
        "pilot_selection": {
            "algorithm": selection_algo,
            "seed": selection_seed,
            "published_unknown_ids": selection.get(
                "rationale", {}).get("unknown_ids_used"),
            "selected_cases": selection.get("selected"),
        },
        "hashes": {
            "corpus_cases": sha256_of_path(cases_path),
            "sealed_key_manifest": open(keys_manifest).read().strip().split()[0],
            "sealed_keys": sha256_of_path(keys_path),
            "sxr_git_head": git_head_hash(sxr_root),
        },
    }
    return manifest


def hash_manifest(manifest: dict) -> str:
    """The experiment-contract hash: sha256 of the canonical manifest JSON."""
    blob = json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(blob).hexdigest()


def write_manifest(path: str, manifest: dict) -> str:
    """Persist the manifest and return its contract hash."""
    contract_hash = hash_manifest(manifest)
    manifest["contract_hash"] = contract_hash
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    with open(path, "w") as f:
        f.write(json.dumps(manifest, indent=2) + "\n")
    return contract_hash


if __name__ == "__main__":
    sys.path.insert(0, os.path.dirname(HERE))
    from eval.contract import live_contract
    from eval.store import load_case_rows
    from eval.select_pilot import select_pilot
    cfg = live_contract()
    rows = load_case_rows(os.path.join(CORPUS, "cases.jsonl"))
    sel = select_pilot(rows).to_dict()
    m = build_pilot_manifest(cfg, sel, os.path.join(HERE, "..", ".."))
    print("contract_hash:", hash_manifest(m))
    print("selected:", sel["selected"])
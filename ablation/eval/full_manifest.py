"""Full 720-run retrieval experiment manifest — freeze + hash the contract.

Mirrors the pilot manifest but for the FULL experiment: all 60 cases, no pilot
selection. The semantic conditions are identical to clean pilot-003 (same
model/config identity, prompt, packer, scorer); pilot outputs remain excluded
from final statistics.
"""

from __future__ import annotations

import hashlib
import json
import os
import sys

from eval.pilot_manifest import sha256_of_path, git_head_hash, hash_manifest

HERE = os.path.dirname(os.path.abspath(__file__))
CORPUS = os.path.join(HERE, "..", "corpus")


def build_full_manifest(cfg, sxr_root: str, seed: int,
                        corpus_dir: str = CORPUS) -> dict:
    from eval.arms import ARMS
    from eval.scorer import SCORING_VERSION
    from eval.contract import TASK_PROMPT_SCAFFOLD
    from eval.matrix import ARM_NAMES, REPETITIONS
    from eval.runner import LOCUS_VOCABULARY
    from eval.context_packer import PACKING_VERSION

    cases_path = os.path.join(corpus_dir, "cases.jsonl")
    keys_manifest = os.path.join(corpus_dir, "keys.sha256")

    # Full 720 matrix hash (deterministic from seed + config).
    from eval.matrix import RunMatrix
    rows = [json.loads(l) for l in open(cases_path) if l.strip()]
    ids = [r["case_id"] for r in rows]
    m = RunMatrix.expand("full-001", ids, seed, cfg.fingerprint())
    matrix_hash = hashlib.sha256(json.dumps(
        m.to_dict(), sort_keys=True, separators=(",", ":")).encode()).hexdigest()

    unknown_semantics = (
        "unknown is a correct answer on an ambiguous case; a concrete "
        "unsupported answer on an unknown case is incorrect "
        "(unsupported_concrete); the eight publicly-named ambiguous case IDs "
        "are the only unknown cases."
    )
    fp_definition = (
        "semantic_model_change_fp = predicted=='declared_model' AND "
        "true_locus!='declared_model'."
    )
    randomization = (
        "RunMatrix.expand: per-(case,repetition) seeded arm shuffle, then "
        "global seeded order; units sha256(experiment|case|arm|rep|config_hash); "
        "reproducible from the recorded seed."
    )
    selection_rule = (
        "§23.1: maximize settlement/diagnostic success while minimizing "
        "unjustified semantic-model-change pressure; an arm that raises the "
        "model-change FP rate is rejected regardless of accuracy."
    )

    manifest = {
        "experiment_id": "full-001",
        "namespace": "full-001",
        "kind": "full-retrieval-ablation",
        "frozen_from": "clean pilot-003 (same semantic conditions; pilot "
                       "outputs excluded from final statistics)",
        "freeze_statement": "contract frozen before first live call; no "
                            "changes while the experiment runs",
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
            "context_budget_tokens": cfg.context_budget_tokens,
            "retrieval_limits": cfg.retrieval_limits,
        },
        "context_budget_contract": {
            "interpretation": "MAX TOTAL model input tokens = 32,000",
            "packer_version": PACKING_VERSION,
            "safety_margin_fraction": 0.12,
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
        "selection_rule": selection_rule,
        "full_seed": seed,
        "matrix_hash": matrix_hash,
        "hashes": {
            "corpus_cases": sha256_of_path(cases_path),
            "sealed_key_manifest": open(keys_manifest).read().strip().split()[0],
            "sxr_git_head": git_head_hash(sxr_root),
        },
    }
    return manifest


def write_manifest(path: str, manifest: dict) -> str:
    contract_hash = hash_manifest(manifest)
    manifest["contract_hash"] = contract_hash
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    with open(path, "w") as f:
        f.write(json.dumps(manifest, indent=2) + "\n")
    return contract_hash


if __name__ == "__main__":
    sys.path.insert(0, os.path.dirname(HERE))
    from eval.contract import live_contract
    cfg = live_contract()
    m = build_full_manifest(cfg, os.path.join(HERE, "..", ".."), 20260809)
    print("contract_hash:", hash_manifest(m))
    print("matrix_hash:", m["matrix_hash"])
    print("unit_count:", 60 * 4 * 3)
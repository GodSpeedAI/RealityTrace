"""M3 pilot case selection — answer-key-blind and deterministic.

Selects exactly 5 of the 60 corpus cases using ONLY public corpus metadata
from cases.jsonl (repo, scenario, stdin_format, verifier_exit) plus the eight
publicly-identified ambiguous case IDs (allowed by the owner release: the
review packet already names them). Never reads keys.json or scenario-keys.json.

Procedure "blind-coverage-v1", seed 20260717 (declared before any key access):

1. Unknown guarantee: exactly one case is drawn (seeded) from the 8 public
   ambiguous IDs, so the pilot exercises `unknown` scoring semantics.
2. Repo coverage: the remaining slots are filled to cover as many distinct
   repositories as possible.
3. Verifier-type coverage: the selection is required to span more than one
   stdin_format (verifier type); if the seed's draw does not, it is
   deterministically repaired by swapping the last filler slot.
4. No answer-key material is read: selection is a pure function of the public
   metadata + the published unknown-ID set + the seed.

The chosen cases and the rationale (public metadata only) are persisted as a
selection record BEFORE opening any key.
"""

from __future__ import annotations

import json
import os
import random
from dataclasses import dataclass

PILOT_SELECTION_ALGORITHM = "blind-coverage-v1"
PILOT_SELECTION_SEED = 20260717
PILOT_SIZE = 5

# Publicly named ambiguous (unknown-locus) case IDs from the review packet
# (section 8 / owner adjudication 2). Allowed to use; they are metadata, not
# the sealed locus mapping.
PUBLIC_UNKNOWN_IDS = sorted((
    "ag-15", "cep-15", "df-13", "df-14", "df-15",
    "sr-13", "sr-14", "sr-15",
))


@dataclass
class SelectionResult:
    algorithm: str
    seed: int
    selected: list[str]
    rationale: dict

    def to_dict(self) -> dict:
        return {
            "algorithm": self.algorithm,
            "seed": self.seed,
            "selected": self.selected,
            "rationale": self.rationale,
        }


def select_pilot(cases: list[dict], seed: int = PILOT_SELECTION_SEED,
                 size: int = PILOT_SIZE) -> SelectionResult:
    """Deterministic blind selection of `size` pilot cases."""
    ids = sorted(c["case_id"] for c in cases)
    assert len(ids) == 60 and len(set(ids)) == 60, "corpus must be 60 unique"
    by_id = {c["case_id"]: c for c in cases}

    unknown_pool = [i for i in ids if i in PUBLIC_UNKNOWN_IDS]
    known_pool = [i for i in ids if i not in PUBLIC_UNKNOWN_IDS]
    assert len(unknown_pool) == 8 and len(known_pool) == 52

    rng = random.Random(seed)

    # 1. one unknown case (seeded)
    chosen_unknown = rng.sample(unknown_pool, 1)[0]

    # 2. repo coverage from the known pool
    selected = [chosen_unknown]
    chosen_repos = {by_id[chosen_unknown]["repo"]}
    for cid in known_pool:
        if len(selected) >= size:
            break
        if by_id[cid]["repo"] not in chosen_repos:
            selected.append(cid)
            chosen_repos.add(by_id[cid]["repo"])
    # 3. fill remaining slots (seeded) from known pool, avoiding duplicates
    remaining = [c for c in known_pool if c not in selected]
    rng.shuffle(remaining)
    for cid in remaining:
        if len(selected) >= size:
            break
        selected.append(cid)

    # 4. enforce >1 verifier type (stdin_format); if violated, swap the last
    #    filler slot deterministically for a known case of a different format.
    formats = {by_id[cid]["stdin_format"] for cid in selected}
    if len(formats) < 2:
        target = next((f for f in ("car-go-json", "pytest-progress",
                                   "df-validate") if f not in formats), None)
        if target is not None:
            for cid in known_pool:
                if cid not in selected and by_id[cid]["stdin_format"] == target:
                    selected[-1] = cid
                    break

    selected = sorted(selected)
    assert len(selected) == size and len(set(selected)) == size

    rationale = {
        "unknown_ids_used": PUBLIC_UNKNOWN_IDS,
        "selected_unknown": chosen_unknown,
        "repos_covered": sorted({by_id[cid]["repo"] for cid in selected}),
        "stdin_formats_covered": sorted({by_id[cid]["stdin_format"]
                                         for cid in selected}),
        "verifier_exits": {cid: by_id[cid]["verifier_exit"] for cid in selected},
        "note": ("selection used only public cases.jsonl metadata + the "
                 "published unknown-ID set; no sealed key material read"),
    }
    return SelectionResult(PILOT_SELECTION_ALGORITHM, seed, selected, rationale)


def write_selection_record(path: str, result: SelectionResult) -> str:
    """Persist the selection record and return its sha256."""
    import hashlib
    blob = json.dumps(result.to_dict(), sort_keys=True, separators=(",", ":"))
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    with open(path, "w") as f:
        f.write(blob + "\n")
    return hashlib.sha256(blob.encode()).hexdigest()


if __name__ == "__main__":
    import sys
    HERE = os.path.dirname(os.path.abspath(__file__))
    sys.path.insert(0, os.path.dirname(HERE))
    from eval.store import load_case_rows
    rows = load_case_rows(os.path.join(HERE, "..", "corpus", "cases.jsonl"))
    res = select_pilot(rows)
    print(json.dumps(res.to_dict(), indent=2))
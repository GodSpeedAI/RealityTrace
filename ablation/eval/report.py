"""M2-J: pilot / full separation.

Pilot and full experiment results live under distinct immutable experiment
identities/namespaces. Aggregation for the final 60-case statistics reads
ONLY the full namespace; pilot results are archived separately and are never
pooled.
"""

from __future__ import annotations

import os


def namespace_root(runs_root: str, namespace: str) -> str:
    """runs/<namespace>/... — namespaces are immutable per experiment."""
    if not namespace or "/" in namespace or ".." in namespace:
        raise ValueError(f"invalid namespace {namespace!r}")
    return os.path.join(runs_root, namespace)


def experiment_dir(runs_root: str, namespace: str, experiment_id: str) -> str:
    root = namespace_root(runs_root, namespace)
    return os.path.join(root, experiment_id)


def full_namespace_only(scoring_dir: str, pilot_namespaces: list[str],
                        full_namespace: str) -> list[dict]:
    """Load scoring records from the full namespace ONLY.

    Pilot namespaces are never read here; if any pilot record would leak into
    the pooled inputs (e.g. a misdirected file), this raises.
    """
    import json
    recs: list[dict] = []
    full = os.path.join(scoring_dir, full_namespace)
    if not os.path.isdir(full):
        raise FileNotFoundError(f"no full-namespace scoring dir {full}")
    for name in os.listdir(full):
        if not name.endswith(".json"):
            continue
        with open(os.path.join(full, name)) as f:
            rec = json.load(f)
        if rec.get("namespace") in pilot_namespaces:
            raise ValueError(
                f"pilot record {name} would be pooled into full statistics")
        if rec.get("namespace") != full_namespace:
            raise ValueError(f"record {name} has foreign namespace")
        recs.append(rec)
    return recs

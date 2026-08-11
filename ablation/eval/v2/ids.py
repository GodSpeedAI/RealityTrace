"""R-15: globally unique unit identity.

Audit defect closed
-------------------
A-23  `governed_run.unit_id()` hashed the constant `EXPERIMENT_ID` rather than
      the run namespace, so all 45 gov-pilot unit ids were byte-identical to
      their gov-full counterparts (verified: 45/45 overlap). No data mixing
      occurred — the runs live in separate directories with separate checkpoint
      stores — but a unit id was not a unique name for a unit, and
      `check_pilot_full_separation` only asserted that no filename in the pilot
      directory contained the substring "full".

Repair
------
The namespace is part of the hashed payload. `legacy_unit_id` reproduces the
old scheme so the collision can be demonstrated as a failing fixture rather
than merely asserted.
"""

from __future__ import annotations

import hashlib

ID_SCHEME = "unit-id-v2"


def unit_id(experiment_id: str, namespace: str, case_id: str, condition: str,
            rep: int, fingerprint: str) -> str:
    """Stable, namespace-scoped unit identity."""
    if not namespace:
        raise ValueError("namespace is required: it is what makes the id unique")
    payload = "|".join((ID_SCHEME, experiment_id, namespace, case_id,
                        condition, str(rep), fingerprint))
    return hashlib.sha256(payload.encode()).hexdigest()[:16]


def legacy_unit_id(experiment_id: str, case_id: str, condition: str, rep: int,
                   fingerprint: str) -> str:
    """The pre-repair scheme. Namespace-blind, hence pilot/full collisions.

    Retained ONLY so the defect is reproducible in a regression test and so
    historical ids remain resolvable. Never use it for a new run.
    """
    payload = "|".join((experiment_id, case_id, condition, str(rep), fingerprint))
    return hashlib.sha256(payload.encode()).hexdigest()[:16]

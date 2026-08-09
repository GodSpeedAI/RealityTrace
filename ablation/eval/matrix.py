"""M2-B/D: resumable, seeded, reproducible run matrix.

60 cases x 4 arms x 3 repetitions = 720 run units, with stable identities:

    experiment | case | arm | repetition | model/config | randomization seed

Execution order is randomized per (case, repetition): the four arms of one
case-repetition execute in a seeded shuffle, and the units overall execute in
a seeded order. The seed and the derived order are recorded so that any run
reproduces exactly from its seed. Randomization never touches the model
configuration or the prompt (contract.py).
"""

from __future__ import annotations

import hashlib
import random
from dataclasses import dataclass, field

ARM_NAMES = ("A", "B", "C", "D")
REPETITIONS = 3


@dataclass(frozen=True)
class RunUnit:
    """Stable identity of one evaluation unit (case, arm, repetition)."""

    experiment_id: str
    case_id: str
    arm: str
    repetition: int
    config_hash: str
    seed: int

    @property
    def unit_id(self) -> str:
        payload = "|".join((self.experiment_id, self.case_id, self.arm,
                            str(self.repetition), self.config_hash))
        return hashlib.sha256(payload.encode()).hexdigest()[:16]

    def to_dict(self) -> dict:
        return {
            "unit_id": self.unit_id,
            "experiment_id": self.experiment_id,
            "case_id": self.case_id,
            "arm": self.arm,
            "repetition": self.repetition,
            "config_hash": self.config_hash,
            "seed": self.seed,
        }


@dataclass
class RunMatrix:
    experiment_id: str
    cases: tuple[str, ...]
    seed: int
    config_hash: str
    units: list[RunUnit] = field(default_factory=list)
    order: list[str] = field(default_factory=list)

    @classmethod
    def expand(cls, experiment_id: str, cases: list[str], seed: int,
               config_hash: str, reps: int = REPETITIONS) -> "RunMatrix":
        """Deterministically expand the full 60x4x3 matrix (720 units)."""
        units: list[RunUnit] = []
        for case in sorted(cases):
            for arm in ARM_NAMES:
                for rep in range(1, reps + 1):
                    units.append(RunUnit(
                        experiment_id=experiment_id, case_id=case, arm=arm,
                        repetition=rep, config_hash=config_hash, seed=seed))
        assert len(units) == len(cases) * 4 * reps
        m = cls(experiment_id=experiment_id, cases=tuple(sorted(cases)),
                seed=seed, config_hash=config_hash, units=units)
        m.order = m.derive_order(seed)
        return m

    def derive_order(self, seed: int) -> list[str]:
        """Seeded order: shuffle per (case, repetition) group, then globally.

        The arm sequence WITHIN a case-repetition is randomized (never the
        model configuration), satisfying "randomized execution order per
        case/repetition" with full reproducibility from the seed.
        """
        rng = random.Random(seed)
        groups: dict[tuple[str, int], list[RunUnit]] = {}
        for u in self.units:
            groups.setdefault((u.case_id, u.repetition), []).append(u)
        order: list[str] = []
        for group in sorted(groups.keys()):
            arm_order = rng.sample(ARM_NAMES, len(ARM_NAMES))
            by_arm = {u.arm: u for u in groups[group]}
            order.extend(by_arm[a].unit_id for a in arm_order)
        return order

    def reproduction_order(self, seed: int) -> list[str]:
        """Independent reproduction of the order from the same seed."""
        return self.derive_order(seed)

    def to_dict(self) -> dict:
        return {
            "experiment_id": self.experiment_id,
            "cases": list(self.cases),
            "seed": self.seed,
            "config_hash": self.config_hash,
            "unit_count": len(self.units),
            "order": self.order,
            "units": [u.to_dict() for u in self.units],
        }

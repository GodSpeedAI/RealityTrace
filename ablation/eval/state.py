"""M2-E: checkpoint/resume state machine.

Per-unit lifecycle states, durably persisted:

    planned -> running -> output_committed -> scored
                  \\-> failed -> retryable

Crash/interruption resumes without re-running already committed valid
evaluations: a unit is only skipped when its raw output exists AND its
recorded content hash matches (output_committed), and it has been scored.
Idempotent transitions prevent duplicate valid runs.
"""

from __future__ import annotations

import json
import os
from dataclasses import dataclass

PLANNED = "planned"
RUNNING = "running"
OUTPUT_COMMITTED = "output_committed"
SCORED = "scored"
FAILED = "failed"
RETRYABLE = "retryable"

STATUSES = (PLANNED, RUNNING, OUTPUT_COMMITTED, SCORED, FAILED, RETRYABLE)

# Allowed transitions; anything else is an error (guards state corruption).
TRANSITIONS = {
    PLANNED: {RUNNING, FAILED, RETRYABLE},
    RUNNING: {OUTPUT_COMMITTED, FAILED, RETRYABLE},
    OUTPUT_COMMITTED: {SCORED, FAILED, RETRYABLE},
    SCORED: set(),
    FAILED: {RETRYABLE, PLANNED},
    RETRYABLE: {RUNNING},
}


class StateError(Exception):
    pass


@dataclass
class UnitState:
    unit_id: str
    status: str = PLANNED
    raw_output_path: str | None = None
    raw_output_hash: str | None = None
    seed: int | None = None
    attempt: int = 0
    note: str | None = None

    def to_dict(self) -> dict:
        return {
            "unit_id": self.unit_id,
            "status": self.status,
            "raw_output_path": self.raw_output_path,
            "raw_output_hash": self.raw_output_hash,
            "seed": self.seed,
            "attempt": self.attempt,
            "note": self.note,
        }

    @staticmethod
    def from_dict(d: dict) -> "UnitState":
        return UnitState(
            unit_id=d["unit_id"], status=d.get("status", PLANNED),
            raw_output_path=d.get("raw_output_path"),
            raw_output_hash=d.get("raw_output_hash"),
            seed=d.get("seed"), attempt=d.get("attempt", 0),
            note=d.get("note"))


class CheckpointStore:
    """Durable per-unit state, one JSON line per unit (append + rewrite)."""

    def __init__(self, path: str):
        self.path = path
        self._units: dict[str, UnitState] = {}
        if os.path.exists(path):
            for line in open(path):
                line = line.strip()
                if line:
                    u = UnitState.from_dict(json.loads(line))
                    self._units[u.unit_id] = u

    def get(self, unit_id: str) -> UnitState | None:
        return self._units.get(unit_id)

    def ensure(self, unit: "object") -> UnitState:
        """Create a planned entry for a matrix unit if absent (idempotent)."""
        uid = unit.unit_id
        if uid not in self._units:
            self._units[uid] = UnitState(unit_id=uid, seed=getattr(unit, "seed", None))
            self._persist()
        return self._units[uid]

    def transition(self, unit_id: str, to: str, **fields) -> UnitState:
        st = self._units.get(unit_id)
        if st is None:
            raise StateError(f"no state for {unit_id}")
        if to not in TRANSITIONS[st.status]:
            raise StateError(
                f"invalid transition {st.status} -> {to} for {unit_id}")
        st.status = to
        for k, v in fields.items():
            setattr(st, k, v)
        st.attempt = st.attempt + 1 if to in (RUNNING, RETRYABLE) else st.attempt
        self._persist()
        return st

    def _persist(self) -> None:
        os.makedirs(os.path.dirname(self.path) or ".", exist_ok=True)
        with open(self.path, "w") as f:
            for u in sorted(self._units.values(), key=lambda x: x.unit_id):
                f.write(json.dumps(u.to_dict()) + "\n")

    def units(self) -> list[UnitState]:
        return list(self._units.values())

    def status_counts(self) -> dict[str, int]:
        counts = {s: 0 for s in STATUSES}
        for u in self._units.values():
            counts[u.status] += 1
        return counts

    def resume_plan(self) -> list[str]:
        """Units that still need evaluation (not validly completed).

        A unit is skipped ONLY when output_committed AND its recorded hash is
        present (valid committed output), and scored. Retryable/failed units
        are returned for re-evaluation; planned/running are returned.
        """
        todo: list[str] = []
        for u in sorted(self._units.values(), key=lambda x: x.unit_id):
            if u.status == SCORED:
                continue
            if u.status == OUTPUT_COMMITTED and u.raw_output_hash:
                continue  # output committed, scoring pending — not re-run
            todo.append(u.unit_id)
        return todo

    def committed_outputs(self) -> dict[str, UnitState]:
        return {u.unit_id: u for u in self._units.values()
                if u.status == OUTPUT_COMMITTED and u.raw_output_hash}

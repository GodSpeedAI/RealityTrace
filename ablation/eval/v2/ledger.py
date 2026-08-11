"""R-08: raw-hash-before-score binding.

Audit defect closed
-------------------
NEW-04.  gov-full's `manifest.jsonl` was reconstructed after a parallel-worker
concurrency loss. Recomputed in Pass B: 540/540 units carry status
`output_committed`, but only **13 of 540** carry a `raw_output_hash`, and all 13
verify. `governed_check.check_raw_committed` tests only that a `.txt` file
exists beside each scored file, so it passed regardless. The
"raw output durably committed with a verified content hash before scoring"
invariant — the structural boundary `runner.py` is built around — is therefore
unverifiable for 527 of 540 historical units.

Repair
------
`CommitLedger` makes the ordering a mechanism rather than a convention:

    response returned
        -> raw artifact written and fsynced
        -> content hash computed from the bytes on disk
        -> manifest entry appended and fsynced
        -> only then is the unit scorable

`assert_scorable` re-reads the file and re-hashes it, so a post-commit edit is
caught. Reconstruction may never invent a hash: `reconstruct` marks recovered
entries `UNVERIFIABLE` and records why.
"""

from __future__ import annotations

import hashlib
import json
import os
from dataclasses import dataclass

HASH_VERIFIED = "VERIFIED"
HASH_UNVERIFIABLE = "UNVERIFIABLE"


class CommitViolation(Exception):
    """Raised when scoring is attempted before a verified durable commit."""


def _fsync_write(path: str, data: bytes) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(data)
        f.flush()
        os.fsync(f.fileno())


def _fsync_append(path: str, line: str) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "a") as f:
        f.write(line + "\n")
        f.flush()
        os.fsync(f.fileno())


@dataclass(frozen=True)
class CommitEntry:
    unit_id: str
    raw_path: str
    raw_hash: str | None
    hash_provenance: str
    note: str = ""

    def to_dict(self) -> dict:
        return {"unit_id": self.unit_id, "raw_path": self.raw_path,
                "raw_hash": self.raw_hash,
                "hash_provenance": self.hash_provenance, "note": self.note}


class CommitLedger:
    """Durable raw-artifact commit ledger with a verifiable content chain."""

    def __init__(self, ledger_path: str):
        self.path = ledger_path
        self._entries: dict[str, CommitEntry] = {}
        if os.path.exists(ledger_path):
            with open(ledger_path) as f:
                for line in f:
                    line = line.strip()
                    if not line:
                        continue
                    d = json.loads(line)
                    self._entries[d["unit_id"]] = CommitEntry(
                        d["unit_id"], d["raw_path"], d.get("raw_hash"),
                        d.get("hash_provenance", HASH_UNVERIFIABLE),
                        d.get("note", ""))

    def commit_raw(self, unit_id: str, raw_path: str, payload: str) -> CommitEntry:
        """Write, fsync, hash from disk, then record. In that order."""
        _fsync_write(raw_path, payload.encode())
        with open(raw_path, "rb") as f:
            on_disk = f.read()
        digest = hashlib.sha256(on_disk).hexdigest()
        entry = CommitEntry(unit_id, raw_path, digest, HASH_VERIFIED)
        _fsync_append(self.path, json.dumps(entry.to_dict(), sort_keys=True))
        self._entries[unit_id] = entry
        return entry

    def get(self, unit_id: str) -> CommitEntry | None:
        return self._entries.get(unit_id)

    def assert_scorable(self, unit_id: str) -> str:
        """Refuse to score unless the durable content chain verifies."""
        e = self._entries.get(unit_id)
        if e is None:
            raise CommitViolation(f"{unit_id}: no commit entry — refuse to score")
        if e.hash_provenance != HASH_VERIFIED or not e.raw_hash:
            raise CommitViolation(
                f"{unit_id}: hash provenance is {e.hash_provenance} — refuse to "
                "score. A reconstructed entry is not a commit.")
        if not os.path.exists(e.raw_path):
            raise CommitViolation(f"{unit_id}: raw artifact missing at {e.raw_path}")
        with open(e.raw_path, "rb") as f:
            actual = hashlib.sha256(f.read()).hexdigest()
        if actual != e.raw_hash:
            raise CommitViolation(
                f"{unit_id}: raw artifact hash mismatch "
                f"({actual[:12]} != {e.raw_hash[:12]}) — refuse to score")
        return actual

    def reconstruct(self, unit_id: str, raw_path: str, note: str) -> CommitEntry:
        """Record a recovered unit WITHOUT inventing a content hash.

        A hash computed now proves the file exists now; it cannot prove the file
        is what the model returned before scoring. Reconstruction therefore
        records `UNVERIFIABLE`, and `assert_scorable` refuses it.
        """
        entry = CommitEntry(unit_id, raw_path, None, HASH_UNVERIFIABLE, note)
        _fsync_append(self.path, json.dumps(entry.to_dict(), sort_keys=True))
        self._entries[unit_id] = entry
        return entry

    def provenance_summary(self) -> dict:
        verified = sum(1 for e in self._entries.values()
                       if e.hash_provenance == HASH_VERIFIED)
        return {"total": len(self._entries), "verified": verified,
                "unverifiable": len(self._entries) - verified}

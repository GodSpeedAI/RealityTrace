"""M2-K: model-backed execution remains disabled until owner release.

No unattended invocation can start paid/model-backed evaluation: the default
backend is 'simulated'; selecting 'live' requires BOTH an explicit
environment token and an owner-placed release marker file, and even then the
harness refuses unless the marker's experiment id matches. Tests exercise the
denied path; the allowed path is never enabled in this milestone.
"""

from __future__ import annotations

import os
import hashlib

RELEASE_ENV = "SXR_ABLATION_RELEASE"
RELEASE_MARKER = "SXR_ABLATION_RELEASE_MARKER"
DEFAULT_MARKER = "ablation/RELEASE.gate"
LIVE_BACKENDS = ("live",)
DISABLED_BACKENDS = ("simulated", "recorded")


class ReleaseGateError(Exception):
    """Raised when model-backed execution is attempted without release."""


def release_token(env: dict[str, str]) -> str | None:
    return env.get(RELEASE_ENV)


def marker_path(env: dict[str, str]) -> str:
    return env.get(RELEASE_MARKER, DEFAULT_MARKER)


def marker_payload(marker: str, experiment_id: str) -> str:
    """Deterministic marker content binding a release to one experiment."""
    return hashlib.sha256(f"{experiment_id}\n".encode()).hexdigest()


def write_release_marker(path: str, experiment_id: str, token: str) -> None:
    """Owner-side action: explicitly authorizes ONE experiment. The token is
    bound into the marker so a marker copied elsewhere does not authorize."""
    with open(path, "w") as f:
        f.write(f"experiment={experiment_id}\n")
        f.write(f"token_hash={hashlib.sha256(token.encode()).hexdigest()}\n")


def verify_release(experiment_id: str, env: dict[str, str] | None = None) -> None:
    """Raise unless the release gate is explicitly open for this experiment.

    Called by the runner BEFORE any backend='live' unit executes. In this
    milestone the gate is never open, so live execution is always denied.
    """
    env = dict(os.environ) if env is None else dict(env)
    tok = release_token(env)
    if not tok:
        raise ReleaseGateError(
            f"{RELEASE_ENV} not set — model-backed execution disabled")
    mp = marker_path(env)
    if not os.path.exists(mp):
        raise ReleaseGateError(f"release marker {mp} missing")
    want = f"experiment={experiment_id}\n"
    content = open(mp).read()
    if want not in content:
        raise ReleaseGateError(
            f"release marker does not authorize experiment {experiment_id}")
    want_hash = f"token_hash={hashlib.sha256(tok.encode()).hexdigest()}\n"
    if want_hash not in content:
        raise ReleaseGateError(
            "release marker token binding does not match "
            f"{RELEASE_ENV}")
    print(f"release gate OPEN for {experiment_id} (owner-authorized)")

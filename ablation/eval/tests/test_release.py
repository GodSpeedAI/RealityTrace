"""M2-K: release gate — live model-backed execution requires an explicit,
owner-placed, experiment-bound authorization; everything else is denied."""

import os

import pytest

from ..release import (verify_release, write_release_marker, ReleaseGateError,
                       marker_payload, RELEASE_ENV, RELEASE_MARKER)


def test_live_denied_without_env():
    with pytest.raises(ReleaseGateError):
        verify_release("exp-x", env={})


def test_live_denied_with_env_but_no_marker(tmp_path):
    env = {RELEASE_ENV: "tok", RELEASE_MARKER: str(tmp_path / "nope")}
    with pytest.raises(ReleaseGateError):
        verify_release("exp-x", env=env)


def test_live_denied_wrong_experiment(tmp_path):
    marker = tmp_path / "RELEASE.gate"
    write_release_marker(str(marker), "exp-other", "tok")
    env = {RELEASE_ENV: "tok", RELEASE_MARKER: str(marker)}
    with pytest.raises(ReleaseGateError):
        verify_release("exp-x", env=env)


def test_marker_binds_token_and_experiment(tmp_path):
    marker = tmp_path / "RELEASE.gate"
    write_release_marker(str(marker), "exp-x", "tok")
    content = open(marker).read()
    assert "experiment=exp-x\n" in content
    # marker_payload is the deterministic experiment binding hash
    import hashlib
    assert marker_payload("", "exp-x") == hashlib.sha256(b"exp-x\n").hexdigest()
    # the marker carries a token hash so a copied marker does not authorize
    assert hashlib.sha256(b"tok").hexdigest() in content


def test_live_denied_token_does_not_match_marker(tmp_path):
    marker = tmp_path / "RELEASE.gate"
    write_release_marker(str(marker), "exp-x", "other-token")
    env = {RELEASE_ENV: "tok", RELEASE_MARKER: str(marker)}
    with pytest.raises(ReleaseGateError):
        verify_release("exp-x", env=env)


def test_verify_passes_with_correct_marker(tmp_path):
    marker = tmp_path / "RELEASE.gate"
    write_release_marker(str(marker), "exp-x", "tok")
    env = {RELEASE_ENV: "tok", RELEASE_MARKER: str(marker)}
    verify_release("exp-x", env=env)  # must not raise

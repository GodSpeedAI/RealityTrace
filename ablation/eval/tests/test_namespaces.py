"""M2-J: pilot/full namespace separation — pilot never pooled."""

import json
import os

import pytest

from ..report import (namespace_root, experiment_dir, full_namespace_only)
from ..scorer import score_unit


def make_rec(unit_id, ns):
    r = score_unit(unit("c1"), "Locus: implementation\n", "implementation")
    r["unit_id"] = unit_id
    r["namespace"] = ns
    return r


def unit(case="c1"):
    from ..matrix import RunUnit
    return RunUnit("exp", case, "A", 1, "cfg", 7)


def test_namespace_paths_are_immutable_and_isolated(tmp_path):
    root = tmp_path / "runs"
    pilot = namespace_root(str(root), "pilot-001")
    full = namespace_root(str(root), "full-001")
    assert str(pilot).endswith("runs/pilot-001")
    assert str(full).endswith("runs/full-001")
    assert experiment_dir(str(root), "pilot-001", "E1") != \
        experiment_dir(str(root), "full-001", "E1")


def test_namespace_validation(tmp_path):
    with pytest.raises(ValueError):
        namespace_root(str(tmp_path), "../escape")
    with pytest.raises(ValueError):
        namespace_root(str(tmp_path), "a/b")


def test_full_statistics_never_include_pilot(tmp_path):
    scoring = tmp_path / "scoring"
    os.makedirs(scoring / "pilot-001")
    os.makedirs(scoring / "full-001")
    with open(scoring / "pilot-001" / "u-pilot.json", "w") as f:
        f.write(json.dumps(make_rec("u-pilot", "pilot-001")))
    with open(scoring / "full-001" / "u-full.json", "w") as f:
        f.write(json.dumps(make_rec("u-full", "full-001")))
    recs = full_namespace_only(str(scoring), ["pilot-001"], "full-001")
    assert len(recs) == 1 and recs[0]["unit_id"] == "u-full"


def test_pilot_record_in_full_dir_raises(tmp_path):
    scoring = tmp_path / "scoring"
    os.makedirs(scoring / "full-001")
    with open(scoring / "full-001" / "u-pilot.json", "w") as f:
        f.write(json.dumps(make_rec("u-pilot", "pilot-001")))
    with pytest.raises(ValueError):
        full_namespace_only(str(scoring), ["pilot-001"], "full-001")


def test_foreign_namespace_record_raises(tmp_path):
    scoring = tmp_path / "scoring"
    os.makedirs(scoring / "full-001")
    with open(scoring / "full-001" / "u-other.json", "w") as f:
        f.write(json.dumps(make_rec("u-other", "other")))
    with pytest.raises(ValueError):
        full_namespace_only(str(scoring), ["pilot-001"], "full-001")

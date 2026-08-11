"""R-15 regression: pilot/full unit-id collision (audit A-23)."""

from ..v2.ids import legacy_unit_id, unit_id

FP = "fingerprint-abc"
EXP = "governed-diagnostic-acquisition-001"
CASES = ("ag-06", "ag-09", "cep-11", "cep-15", "sr-11")


def test_A23_legacy_ids_collide_between_pilot_and_full():
    """Verified against the real runs in Pass B: 45/45 gov-pilot unit ids are
    byte-identical to their gov-full counterparts, because `unit_id()` hashed
    the constant EXPERIMENT_ID rather than the namespace."""
    pilot = {legacy_unit_id(EXP, c, cond, r, FP)
             for c in CASES for cond in "VGO" for r in (1, 2, 3)}
    full = {legacy_unit_id(EXP, c, cond, r, FP)
            for c in CASES for cond in "VGO" for r in (1, 2, 3)}
    assert pilot == full          # the defect: total collision
    assert len(pilot) == 45


def test_repaired_ids_are_disjoint_across_namespaces():
    pilot = {unit_id(EXP, "gov-pilot", c, cond, r, FP)
             for c in CASES for cond in "VGO" for r in (1, 2, 3)}
    full = {unit_id(EXP, "gov-full", c, cond, r, FP)
            for c in CASES for cond in "VGO" for r in (1, 2, 3)}
    assert len(pilot) == len(full) == 45
    assert pilot.isdisjoint(full)


def test_ids_are_stable_and_sensitive_to_every_coordinate():
    base = unit_id(EXP, "gov-full", "ag-06", "G", 1, FP)
    assert base == unit_id(EXP, "gov-full", "ag-06", "G", 1, FP)
    for changed in (
        unit_id(EXP, "gov-full", "ag-06", "G", 2, FP),
        unit_id(EXP, "gov-full", "ag-06", "O", 1, FP),
        unit_id(EXP, "gov-full", "ag-09", "G", 1, FP),
        unit_id(EXP, "gov-pilot", "ag-06", "G", 1, FP),
        unit_id(EXP, "gov-full", "ag-06", "G", 1, "other-fingerprint"),
        unit_id("other-exp", "gov-full", "ag-06", "G", 1, FP),
    ):
        assert changed != base


def test_namespace_is_mandatory():
    import pytest
    with pytest.raises(ValueError):
        unit_id(EXP, "", "ag-06", "G", 1, FP)

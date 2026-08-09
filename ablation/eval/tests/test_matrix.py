"""M2-B/D: seeded randomized run matrix — expansion + reproducibility."""

from ..matrix import RunMatrix, ARM_NAMES
from ..contract import frozen_contract

CASES = [f"case-{i:03d}" for i in range(60)]
SEED = 20260809
CFG = frozen_contract().fingerprint()


def test_full_matrix_expands_to_720():
    m = RunMatrix.expand("exp-test", CASES, SEED, CFG)
    assert len(m.units) == 720  # 60 x 4 x 3
    assert len(set(u.unit_id for u in m.units)) == 720  # unique identities
    assert len(m.order) == 720


def test_matrix_carries_stable_identities():
    m = RunMatrix.expand("exp-test", CASES, SEED, CFG)
    u = m.units[0]
    assert u.experiment_id == "exp-test"
    assert u.arm in ARM_NAMES
    assert 1 <= u.repetition <= 3
    assert u.config_hash == CFG
    # unit id is a pure function of identity fields
    again = RunMatrix.expand("exp-test", CASES, SEED, CFG)
    assert u.unit_id == next(
        x for x in again.units if x.case_id == u.case_id
        and x.arm == u.arm and x.repetition == u.repetition).unit_id


def test_order_reproducible_from_seed():
    m = RunMatrix.expand("exp-test", CASES, SEED, CFG)
    assert m.order == m.reproduction_order(SEED)
    assert m.order == RunMatrix.expand("exp-test", CASES, SEED,
                                       CFG).reproduction_order(SEED)


def test_different_seeds_differ():
    m1 = RunMatrix.expand("exp-test", CASES, SEED, CFG)
    m2 = RunMatrix.expand("exp-test", CASES, SEED + 1, CFG)
    assert m1.order != m2.order


def test_each_case_rep_group_contains_each_arm_once():
    m = RunMatrix.expand("exp-test", CASES, SEED, CFG)
    by_id = {u.unit_id: u for u in m.units}
    groups = {}
    for uid in m.order:
        u = by_id[uid]
        groups.setdefault((u.case_id, u.repetition), []).append(u.arm)
    for g, arms in groups.items():
        assert sorted(arms) == sorted(ARM_NAMES), g
    assert len(groups) == 60 * 3


def test_order_is_permutation_of_all_units():
    m = RunMatrix.expand("exp-test", CASES, SEED, CFG)
    all_ids = {u.unit_id for u in m.units}
    assert set(m.order) == all_ids

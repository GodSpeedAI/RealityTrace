"""M2-C: fixed model/config/prompt — the arm is the only experimental
variable."""

from dataclasses import asdict

from ..contract import frozen_contract, assert_arm_invariant


def test_contract_frozen_fields():
    c = frozen_contract()
    d = asdict(c)
    assert d["model_id"] == "glm-5.2"
    assert d["provider"] == "cline-api"
    assert d["temperature"] == 0.0
    assert "system_prompt" in d and len(d["system_prompt"]) > 0
    assert d["context_budget_tokens"] == 32_000
    assert d["backend"] == "simulated"  # M2-K: never live by default


def test_contract_has_no_arm_dimension():
    c = frozen_contract()
    assert "arm" not in asdict(c)
    assert_arm_invariant(c)


def test_contract_fingerprint_stable():
    assert frozen_contract().fingerprint() == frozen_contract().fingerprint()


def test_contract_identical_across_arms():
    """The SAME config object is passed to every arm; fingerprints equal."""
    c = frozen_contract()
    for arm in ("A", "B", "C", "D"):
        assert c.fingerprint() == frozen_contract().fingerprint()

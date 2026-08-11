"""R-01 regression: the oracle probe-identity channel (audit A-01 / NEW-05)."""

import pytest

from ..v2.channel import (ActionCatalogue, AnswerLeak, DisclosurePolicy,
                          ObservationChannel, PromptLeakGuard, UnsafeDisclosure,
                          leakage_bound, unit_seed)

PROBES = ("inspect_target_binding", "inspect_source_symbols",
          "inspect_environment", "inspect_permissions",
          "inspect_fixture_integrity", "validate_semantic_model",
          "rerun_verifier_isolated", "inspect_verifier_config")
COSTS = {p: 1 for p in PROBES}
COSTS["validate_semantic_model"] = 3
COSTS["rerun_verifier_isolated"] = 5

#: The bijection Pass B verified in acquisition-hidden-meta.json.
DISCRIMINATING = {
    "verifier": "rerun_verifier_isolated",
    "implementation": "inspect_source_symbols",
    "verification_plan": "inspect_target_binding",
    "environment": "inspect_environment",
    "evidence_collection": "inspect_fixture_integrity",
    "authority": "inspect_permissions",
    "declared_model": "validate_semantic_model",
    "projection": "validate_semantic_model",
}
OBS = {p: {"probe": p, "field": "value"} for p in PROBES}


def _cat(case="cep-09", cond="O", rep=1):
    return ActionCatalogue(PROBES, unit_seed("exp", "ns", case, cond, rep))


# ── the defect, reproduced ───────────────────────────────────────────────────

def test_A01_legacy_single_disclosure_is_a_deterministic_answer_encoding():
    """SINGLE delivery + a bijective discriminating map means the delivered set
    identifies the locus. Reproduced here as a failing property."""
    ch = ObservationChannel(_cat(), COSTS, DisclosurePolicy.SINGLE,
                            acknowledge_unsafe=True)
    recovered = {}
    for locus, probe in DISCRIMINATING.items():
        d = ch.deliver(OBS, [probe], seed=1)
        assert d.consistency_set_size == 1
        recovered[d.delivered_probes] = recovered.get(d.delivered_probes, set())
        recovered[d.delivered_probes].add(locus)
    # every delivered set maps to at most 2 loci (declared_model/projection tie)
    assert max(len(v) for v in recovered.values()) <= 2
    assert leakage_bound(DisclosurePolicy.SINGLE, 8)["answer_isolated"] is False


def test_single_policy_is_refused_unless_explicitly_acknowledged():
    with pytest.raises(UnsafeDisclosure):
        ObservationChannel(_cat(), COSTS, DisclosurePolicy.SINGLE)


# ── the repair ───────────────────────────────────────────────────────────────

def test_full_disclosure_set_is_identical_for_every_locus():
    """The core isolation property: what is delivered cannot be a function of
    the answer if it is the same for every unit."""
    ch = ObservationChannel(_cat(), COSTS, DisclosurePolicy.FULL)
    sets = {ch.deliver(OBS, [p], seed=1).delivered_probes
            for p in DISCRIMINATING.values()}
    assert len(sets) == 1
    assert sets.pop() == PROBES


def test_full_disclosure_reports_zero_bits():
    d = ObservationChannel(_cat(), COSTS, DisclosurePolicy.FULL).deliver(
        OBS, ["inspect_permissions"], seed=1)
    assert d.answer_isolated
    assert d.bits_disclosed == 0.0
    assert leakage_bound(DisclosurePolicy.FULL, 8)["answer_isolated"] is True


def test_masked_k_bounds_leakage_and_reports_it_honestly():
    ch = ObservationChannel(_cat(), COSTS, DisclosurePolicy.MASKED_K, k=3)
    d = ch.deliver(OBS, ["inspect_permissions"], seed=7)
    assert "inspect_permissions" in d.delivered_probes   # content preserved
    assert d.consistency_set_size == 3
    assert not d.answer_isolated
    assert d.bits_disclosed == pytest.approx(1.415, abs=1e-3)


def test_delivered_payload_is_byte_identical_to_self_selected():
    """Requirement B: oracle delivery must not alter evidentiary content."""
    ch = ObservationChannel(_cat(), COSTS, DisclosurePolicy.FULL)
    d = ch.deliver(OBS, ["inspect_environment"], seed=1)
    by_handle = {o.handle: o.payload for o in d.observations}
    cat = _cat()
    assert by_handle[cat.handle_of("inspect_environment")] == OBS["inspect_environment"]


def test_handles_are_permuted_per_unit_and_locus_independently():
    a = _cat("cep-09", "O", 1)
    b = _cat("cep-10", "O", 1)
    assert a.handles == b.handles == tuple(f"act-{i:02d}" for i in range(1, 9))
    # same handle names, different underlying probe -> no cross-unit semantics
    assert any(a.probe_of(h) != b.probe_of(h) for h in a.handles)
    # the seed depends only on public run coordinates
    assert unit_seed("exp", "ns", "cep-09", "O", 1) == unit_seed(
        "exp", "ns", "cep-09", "O", 1)


def test_rendered_observation_never_contains_a_canonical_probe_id():
    ch = ObservationChannel(_cat(), COSTS, DisclosurePolicy.FULL)
    d = ch.deliver({p: {"all_exist": True} for p in PROBES},
                   ["inspect_environment"], seed=1)
    rendered = "\n".join(o.render() for o in d.observations)
    guard = PromptLeakGuard(PROBES)
    guard.assert_clean(rendered, "observation")


# ── the leak guard itself ────────────────────────────────────────────────────

def test_leak_guard_catches_the_legacy_renderer():
    legacy = "--- observation from automated probe: inspect_permissions ---"
    with pytest.raises(AnswerLeak) as e:
        PromptLeakGuard(PROBES).assert_clean(legacy)
    assert "inspect_permissions" in str(e.value)


@pytest.mark.parametrize("injected", [
    "the discriminating probe for this case is act-03",
    "true_locus: authority",
    "discriminating_probes: [rerun_verifier_isolated]",
    "selection rationale: cheapest discriminating",
])
def test_leak_guard_mutation_cases_all_fail(injected):
    with pytest.raises(AnswerLeak):
        PromptLeakGuard(PROBES).assert_clean("prompt body\n" + injected)


def test_leak_guard_permits_the_frozen_locus_vocabulary():
    """Locus names legitimately appear in every prompt; banning them would make
    the guard unusable rather than strict."""
    body = ("Attribute the failure to exactly one locus: verifier, "
            "verification_plan, projection, implementation, environment, "
            "authority, evidence_collection, declared_model, unknown.")
    PromptLeakGuard(PROBES).assert_clean(body)

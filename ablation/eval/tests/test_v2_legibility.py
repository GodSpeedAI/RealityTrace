"""Owner §7: the legibility transformation may change representation only."""

import pytest

from ..v2.legibility import (LABELS, LegibilityViolation, _label_introduces_locus,
                             assert_semantic_equivalence, contract_fingerprint,
                             label_for, payload_digest, render_legible,
                             render_raw)

PAYLOAD = {"all_accessible": False, "restrictive_paths": ["src/lib.rs"],
           "checked": [{"path": "src/lib.rs", "mode": "600", "readable": False}]}


def test_raw_and_legible_are_bound_to_identical_content():
    b = assert_semantic_equivalence(PAYLOAD)
    assert b["content_equivalent"] is True
    assert b["payload_digest"] == payload_digest(PAYLOAD)
    assert b["raw_digest"] != b["legible_digest"]      # representation differs
    assert b["fields"] == sorted(PAYLOAD)


def test_legible_emits_every_field_and_nothing_else():
    out = render_legible(PAYLOAD, 10 ** 9)
    assert len(out.splitlines()) == len(PAYLOAD)
    for k in PAYLOAD:
        assert label_for(k) in out


def test_legible_adds_no_inference():
    out = render_legible(PAYLOAD, 10 ** 9).lower()
    for banned in ("locus", "because", "therefore", "suggests", "indicates",
                   "recommend", "likely", "cause"):
        assert banned not in out


def test_no_label_introduces_a_locus_the_field_key_lacks():
    for k, v in LABELS.items():
        assert _label_introduces_locus(k, v) is None


def test_MUTATION_a_label_naming_a_locus_is_rejected():
    assert _label_introduces_locus("valid", "declared model is invalid") == "declared_model"
    assert _label_introduces_locus("env_assignments",
                                   "environment assignments") == "environment"
    # permitted: the field key already carries the token
    assert _label_introduces_locus("verifier_command", "verifier command line") is None


def test_MUTATION_dropping_a_field_is_caught(monkeypatch):
    import eval.v2.legibility as L
    monkeypatch.setattr(L, "render_legible", lambda p, lim: "      nothing")
    with pytest.raises(LegibilityViolation, match="dropped field"):
        L.assert_semantic_equivalence(PAYLOAD)


def test_MUTATION_introducing_a_locus_term_at_runtime_is_caught(monkeypatch):
    import eval.v2.legibility as L
    monkeypatch.setattr(
        L, "render_legible",
        lambda p, lim: "\n".join(f"      {L.label_for(k)}: x" for k in p)
        + "\n      probable locus: authority")
    with pytest.raises(LegibilityViolation, match="introduced the locus term"):
        L.assert_semantic_equivalence(PAYLOAD)


def test_unlabelled_keys_fall_back_mechanically():
    assert label_for("some_new_field") == "some new field"
    out = render_legible({"some_new_field": 1}, 10 ** 9)
    assert "some new field: 1" in out


def test_empty_payload_is_explicit():
    assert "(no fields reported)" in render_legible({}, 100)


def test_truncation_is_visible_in_both_renderings():
    big = {"summary": "x" * 5000}
    assert "[truncated at 100 chars]" in render_raw(big, 100)
    assert "[truncated at 100 chars]" in render_legible(big, 100)


def test_fingerprint_is_stable():
    assert contract_fingerprint() == contract_fingerprint()
    assert len(contract_fingerprint()) == 64

"""M2-A: arm isolation — cross-arm leakage is impossible by construction."""

import pytest

from ..arms import (ARMS, VECTOR, GRAPH, SQL, FTS, ArmViolation,
                    build_arm_context, validate_arm_context, RetrievalContext)

STORE = {
    "sql": "sql section for case-x",
    "fts": "fts section for case-x",
    "vector": "vector section for case-x",
    "graph": "graph section for case-x",
}


def test_arm_definitions_frozen():
    assert ARMS == {
        "A": frozenset({SQL, FTS}),
        "B": frozenset({SQL, FTS, VECTOR}),
        "C": frozenset({SQL, FTS, GRAPH}),
        "D": frozenset({SQL, FTS, GRAPH, VECTOR}),
    }


def test_each_arm_gets_exactly_its_capabilities():
    for arm, caps in ARMS.items():
        ctx = build_arm_context(arm, "case-x", STORE)
        validate_arm_context(ctx)  # must not raise
        assert set(ctx.sections.keys()) == set(caps)


def test_arm_a_never_sees_vector_or_graph():
    ctx = build_arm_context("A", "case-x", STORE)
    assert VECTOR not in ctx.sections
    assert GRAPH not in ctx.sections


def test_cross_arm_context_delivery_rejected():
    """Deliberate attempt: hand arm B's context to arm A -> rejected."""
    b_ctx = build_arm_context("B", "case-x", STORE)
    with pytest.raises(ArmViolation):
        validate_arm_context(RetrievalContext(arm="A", case_id="case-x",
                                              sections=b_ctx.sections))


def test_capability_injection_rejected():
    """Deliberate attempt: inject a vector section into arm A -> rejected."""
    a_ctx = build_arm_context("A", "case-x", STORE)
    injected = dict(a_ctx.sections)
    injected[VECTOR] = "smuggled vector content"
    with pytest.raises(ArmViolation):
        validate_arm_context(RetrievalContext(arm="A", case_id="case-x",
                                              sections=injected))


def test_missing_capability_section_rejected():
    d_ctx = build_arm_context("D", "case-x", STORE)
    stripped = {k: v for k, v in d_ctx.sections.items() if k != FTS}
    with pytest.raises(ArmViolation):
        validate_arm_context(RetrievalContext(arm="D", case_id="case-x",
                                              sections=stripped))


def test_context_content_contains_only_assigned_sections():
    """Each arm's delivered material is exactly its capability sections."""
    for arm in ARMS:
        ctx = build_arm_context(arm, "case-x", STORE)
        blob = " ".join(ctx.sections.values())
        for cap in (VECTOR, GRAPH):
            marker = f"{cap} section"
            assert (marker in blob) == (cap in ARMS[arm]), arm


def test_unknown_arm_rejected():
    with pytest.raises(ArmViolation):
        validate_arm_context(RetrievalContext(arm="E", case_id="case-x",
                                              sections=dict(STORE)))

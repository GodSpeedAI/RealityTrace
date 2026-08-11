"""R-05 regression: the legacy parser's defects, then the repaired contract.

Each test names the audit finding it reproduces.
"""

import re

import pytest

from ..v2.decision import (ControllerDecision, ParseStatus, iter_json_objects,
                           parse_decision, repair_request)

VOCAB = ("verifier", "verification_plan", "projection", "implementation",
         "environment", "authority", "evidence_collection", "declared_model",
         "unknown")
ACTIONS = ("act-01", "act-02", "act-03")

NESTED_SETTLEMENT = (
    'Here is my answer.\n'
    '{"candidate_loci": ["implementation", "environment"], '
    '"unresolved_distinction": "which layer", '
    '"selected_action": null, "expected_information": null, '
    '"proposed_settlement": "implementation"}\n'
)


# ── the defect, reproduced ───────────────────────────────────────────────────

def _legacy_parse(text):
    """Verbatim reproduction of `governed_run.parse_round_response` (A-29)."""
    import json
    m = re.search(r'\{[^{}]*"candidate_loci"[^{}]*\}', text, re.DOTALL)
    if not m:
        m = re.search(r'\{[^{}]+\}', text, re.DOTALL)
    if not m:
        return None
    try:
        return json.loads(m.group())
    except json.JSONDecodeError:
        return None


LEGACY_NESTED = (
    '{"candidate_loci": ["implementation"], '
    '"settle": {"locus": "implementation", "justification": "compile error"}}'
)


def test_A29_legacy_regex_binds_the_inner_object():
    """The legacy primary regex cannot match an object containing a nested
    `settle`; the fallback then binds the INNER object, so the declared
    settlement was unreachable and candidate_loci was lost."""
    legacy = _legacy_parse(LEGACY_NESTED)
    assert legacy is not None
    assert "candidate_loci" not in legacy      # lost
    assert legacy.get("locus") == "implementation"  # inner object bound instead
    assert legacy.get("settle") is None        # so settlement never fired


def test_A29_repaired_parser_binds_the_outer_object():
    out = parse_decision(NESTED_SETTLEMENT, VOCAB, ACTIONS)
    assert out.status is ParseStatus.VALID
    assert out.decision.proposed_settlement == "implementation"
    assert out.decision.candidate_loci == ("implementation", "environment")


def test_A30_action_is_never_outranked_by_prose():
    """Legacy `extract_locus` took the first `Locus:` anywhere and, in the
    driver, a detected locus outranked a declared action. The v2 contract has
    no prose channel at all."""
    text = ('Thinking out loud: the Locus: implementation seems likely.\n'
            '{"candidate_loci": ["implementation", "verifier"], '
            '"selected_action": "act-02", "proposed_settlement": null}')
    out = parse_decision(text, VOCAB, ACTIONS)
    assert out.status is ParseStatus.VALID
    assert out.decision.selected_action == "act-02"
    assert out.decision.proposed_settlement is None


def test_prose_only_response_is_not_scraped():
    out = parse_decision("I conclude the answer.\nLocus: environment\n",
                         VOCAB, ACTIONS)
    assert out.status is ParseStatus.NO_JSON_OBJECT
    assert out.decision is None


# ── the repaired contract ────────────────────────────────────────────────────

def test_scanner_is_string_and_escape_aware():
    text = '{"candidate_loci": ["a {not a brace} b"], "x": 1}'
    objs = list(iter_json_objects(text))
    assert len(objs) == 1 and objs[0][1] == text


def test_scanner_enumerates_multiple_objects_in_order():
    text = '{"a": 1} then {"candidate_loci": ["unknown"]}'
    objs = list(iter_json_objects(text))
    assert [o[0] for o in objs] == [0, 14]


def test_first_schema_valid_object_wins_deterministically():
    text = ('{"note": "scratch"}\n'
            '{"candidate_loci": ["unknown"]}\n'
            '{"candidate_loci": ["environment"]}')
    out = parse_decision(text, VOCAB, ACTIONS)
    assert out.status is ParseStatus.VALID
    assert out.decision.candidate_loci == ("unknown",)
    assert out.candidates_seen == 2  # scanned until the first valid one


def test_action_and_settlement_together_is_ambiguous_not_resolved():
    text = ('{"candidate_loci": ["environment"], "selected_action": "act-01", '
            '"proposed_settlement": "environment"}')
    out = parse_decision(text, VOCAB, ACTIONS)
    assert out.status is ParseStatus.AMBIGUOUS_DECISION
    assert not out.ok
    assert "mutually exclusive" in out.diagnostic


def test_empty_output_is_its_own_status():
    assert parse_decision("", VOCAB, ACTIONS).status is ParseStatus.EMPTY_OUTPUT
    assert parse_decision(None, VOCAB, ACTIONS).status is ParseStatus.EMPTY_OUTPUT


def test_malformed_json_is_distinguished_from_missing_json():
    assert parse_decision("no braces here", VOCAB,
                          ACTIONS).status is ParseStatus.NO_JSON_OBJECT
    assert parse_decision('{"candidate_loci": [oops]}', VOCAB,
                          ACTIONS).status is ParseStatus.MALFORMED_JSON


@pytest.mark.parametrize("bad,why", [
    ('{"candidate_loci": []}', "empty list"),
    ('{"candidate_loci": ["not_a_locus"]}', "outside vocabulary"),
    ('{"candidate_loci": ["unknown"], "selected_action": "act-99"}', "bad handle"),
    ('{"candidate_loci": ["unknown"], "proposed_settlement": "nope"}', "bad locus"),
    ('{"candidate_loci": ["unknown"], "settle": {"locus": "x"}}', "unknown field"),
])
def test_schema_violations_are_rejected(bad, why):
    out = parse_decision(bad, VOCAB, ACTIONS)
    assert out.status is ParseStatus.SCHEMA_VIOLATION, why
    assert out.decision is None


def test_repair_request_adds_no_case_information_or_hint():
    out = parse_decision("garbage", VOCAB, ACTIONS)
    msg = repair_request(out)
    assert "candidate_loci" in msg and "mutually exclusive" in msg
    for leak in ("true_locus", "discriminating", "inspect_", "the answer is"):
        assert leak not in msg


def test_decision_roundtrips():
    d = ControllerDecision(("environment",), None, "act-01", None, None)
    assert d.is_action and not d.is_settlement
    assert d.to_dict()["selected_action"] == "act-01"

"""R-05: typed controller-decision contract.

Audit defects closed
--------------------
A-29  `parse_round_response`'s primary regex `\\{[^{}]*"candidate_loci"[^{}]*\\}`
      cannot match an object containing a nested `settle` object; the fallback
      `\\{[^{}]+\\}` then matched the INNER object, so structured settlement was
      effectively unreachable and every settlement fell through to a text
      scrape of `Locus:`.
A-30  `extract_locus` took the FIRST `Locus:` match anywhere in the response,
      and in the legacy driver a detected locus outranked a declared action —
      so a response that both selected an action and mentioned a locus lost the
      action silently.

Repair
------
A deterministic brace-matching scanner (not a regex) enumerates every top-level
JSON object in the response, in order. The decision is the first object that
validates against the declared schema. Nothing is ever scraped from prose.

Action and settlement are declared MUTUALLY EXCLUSIVE. A decision carrying both
is `AMBIGUOUS_DECISION` and routes to the bounded contract-repair path
(R-03) — it is never silently resolved by precedence.

The legacy prose scraper is deliberately NOT reimplemented here. It survives
only in `eval.runner.extract_locus` for rescoring immutable historical outputs.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from enum import Enum


class ParseStatus(str, Enum):
    VALID = "VALID"
    EMPTY_OUTPUT = "EMPTY_OUTPUT"
    NO_JSON_OBJECT = "NO_JSON_OBJECT"
    MALFORMED_JSON = "MALFORMED_JSON"
    SCHEMA_VIOLATION = "SCHEMA_VIOLATION"
    AMBIGUOUS_DECISION = "AMBIGUOUS_DECISION"


@dataclass(frozen=True)
class ControllerDecision:
    """The one shape the controller accepts from the model."""

    candidate_loci: tuple[str, ...]
    unresolved_distinction: str | None = None
    selected_action: str | None = None
    expected_information: str | None = None
    proposed_settlement: str | None = None

    @property
    def is_settlement(self) -> bool:
        return self.proposed_settlement is not None

    @property
    def is_action(self) -> bool:
        return self.selected_action is not None

    def to_dict(self) -> dict:
        return {
            "candidate_loci": list(self.candidate_loci),
            "unresolved_distinction": self.unresolved_distinction,
            "selected_action": self.selected_action,
            "expected_information": self.expected_information,
            "proposed_settlement": self.proposed_settlement,
        }


@dataclass(frozen=True)
class ParseOutcome:
    status: ParseStatus
    decision: ControllerDecision | None = None
    diagnostic: str = ""
    candidates_seen: int = 0
    #: byte offsets of every top-level object considered, for auditability
    offsets: tuple[int, ...] = field(default_factory=tuple)

    @property
    def ok(self) -> bool:
        return self.status is ParseStatus.VALID

    def to_dict(self) -> dict:
        return {
            "status": self.status.value,
            "diagnostic": self.diagnostic,
            "candidates_seen": self.candidates_seen,
            "offsets": list(self.offsets),
            "decision": self.decision.to_dict() if self.decision else None,
        }


DECISION_SCHEMA_FIELDS = (
    "candidate_loci",
    "unresolved_distinction",
    "selected_action",
    "expected_information",
    "proposed_settlement",
)


def iter_json_objects(text: str):
    """Yield `(offset, substring)` for every top-level `{...}` in order.

    Brace matching is string- and escape-aware, so nested objects and braces
    inside string literals do not terminate the scan. This is the specific
    failure the legacy regex had (A-29).
    """
    i, n = 0, len(text)
    while i < n:
        if text[i] != "{":
            i += 1
            continue
        depth = 0
        in_str = False
        esc = False
        j = i
        while j < n:
            ch = text[j]
            if in_str:
                if esc:
                    esc = False
                elif ch == "\\":
                    esc = True
                elif ch == '"':
                    in_str = False
            else:
                if ch == '"':
                    in_str = True
                elif ch == "{":
                    depth += 1
                elif ch == "}":
                    depth -= 1
                    if depth == 0:
                        yield i, text[i:j + 1]
                        break
            j += 1
        if depth != 0:          # unterminated object: stop scanning here
            return
        i = j + 1


def _validate(obj: dict, vocabulary: frozenset[str], action_ids: frozenset[str]
              ) -> tuple[ControllerDecision | None, str]:
    if not isinstance(obj, dict):
        return None, "not an object"
    if "candidate_loci" not in obj:
        return None, "missing required field 'candidate_loci'"
    unknown = sorted(set(obj) - set(DECISION_SCHEMA_FIELDS))
    if unknown:
        return None, f"unknown field(s): {unknown}"

    loci = obj.get("candidate_loci")
    if not isinstance(loci, list) or not loci:
        return None, "'candidate_loci' must be a non-empty list"
    if not all(isinstance(x, str) for x in loci):
        return None, "'candidate_loci' must contain strings"
    bad = sorted(set(loci) - vocabulary)
    if bad:
        return None, f"candidate_loci outside vocabulary: {bad}"

    action = obj.get("selected_action")
    if action is not None:
        if not isinstance(action, str):
            return None, "'selected_action' must be a string or null"
        if action not in action_ids:
            return None, f"unknown action handle {action!r}"

    settle = obj.get("proposed_settlement")
    if settle is not None:
        if not isinstance(settle, str):
            return None, "'proposed_settlement' must be a string or null"
        if settle not in vocabulary:
            return None, f"proposed_settlement outside vocabulary: {settle!r}"

    for k in ("unresolved_distinction", "expected_information"):
        v = obj.get(k)
        if v is not None and not isinstance(v, str):
            return None, f"'{k}' must be a string or null"

    return ControllerDecision(
        candidate_loci=tuple(loci),
        unresolved_distinction=obj.get("unresolved_distinction"),
        selected_action=action,
        expected_information=obj.get("expected_information"),
        proposed_settlement=settle,
    ), ""


def parse_decision(text: str | None, vocabulary, action_ids) -> ParseOutcome:
    """Deterministically bind a model response to a `ControllerDecision`.

    Never scrapes prose. Never resolves an action/settlement conflict by
    precedence — that is `AMBIGUOUS_DECISION`, handled by contract repair.
    """
    vocabulary = frozenset(vocabulary)
    action_ids = frozenset(action_ids)

    if text is None or not text.strip():
        return ParseOutcome(ParseStatus.EMPTY_OUTPUT,
                            diagnostic="model returned no content")

    offsets: list[int] = []
    any_json = False
    first_schema_error = ""
    for off, blob in iter_json_objects(text):
        offsets.append(off)
        try:
            obj = json.loads(blob)
        except json.JSONDecodeError:
            continue
        any_json = True
        decision, err = _validate(obj, vocabulary, action_ids)
        if decision is None:
            if not first_schema_error:
                first_schema_error = err
            continue
        if decision.is_action and decision.is_settlement:
            return ParseOutcome(
                ParseStatus.AMBIGUOUS_DECISION, decision=decision,
                diagnostic=("decision declares both selected_action="
                            f"{decision.selected_action!r} and "
                            f"proposed_settlement={decision.proposed_settlement!r}; "
                            "the contract declares these mutually exclusive"),
                candidates_seen=len(offsets), offsets=tuple(offsets))
        return ParseOutcome(ParseStatus.VALID, decision=decision,
                            candidates_seen=len(offsets),
                            offsets=tuple(offsets))

    if not offsets:
        return ParseOutcome(ParseStatus.NO_JSON_OBJECT,
                            diagnostic="no JSON object found in response",
                            offsets=())
    if not any_json:
        return ParseOutcome(ParseStatus.MALFORMED_JSON,
                            diagnostic="found brace groups but none parsed as JSON",
                            candidates_seen=len(offsets), offsets=tuple(offsets))
    return ParseOutcome(ParseStatus.SCHEMA_VIOLATION,
                        diagnostic=first_schema_error or "no object matched the schema",
                        candidates_seen=len(offsets), offsets=tuple(offsets))


def repair_request(outcome: ParseOutcome) -> str:
    """The bounded re-ask sent after an invalid decision (R-03).

    Carries only a parser diagnostic and a restatement of the contract. It adds
    no case information, no hint about the answer, and no encouragement toward
    any particular decision.
    """
    return (
        "Your previous reply did not satisfy the response contract "
        f"({outcome.status.value}: {outcome.diagnostic}).\n"
        "Reply with exactly one JSON object and nothing else:\n"
        '{"candidate_loci": ["<locus>", ...], '
        '"unresolved_distinction": "<text or null>", '
        '"selected_action": "<action handle or null>", '
        '"expected_information": "<text or null>", '
        '"proposed_settlement": "<locus or null>"}\n'
        "`selected_action` and `proposed_settlement` are mutually exclusive: "
        "set at most one of them, and set the other to null."
    )

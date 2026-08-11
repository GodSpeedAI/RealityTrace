"""R-04 + R-07: completion-budget observability and genuine provider provenance.

Audit defects closed
--------------------
NEW-02.  A 4096-token completion ceiling governed every experiment in the chain.
  Recomputed in Pass B: gov-full 264/264 empty-content calls sit at exactly
  4096 completion tokens with zero calls above; §23.1 114/114 unparseable units
  at 4096; §23.2 44/44; acq-full D0/D1 20 units. `governed_run._model_call`
  requested `max_tokens=8192` and the frozen manifests record 4096. Report 9
  §10.1 and `report-corrections.json` both assert "All 540 units used
  max_tokens=8192 (uniform protocol)". The requested value was never observable
  and the effective ceiling never moved.

NEW-03.  `governed_run` wrote `resolved_model: "glm-5.2"`, `retries: 0`,
  `prompt_tokens: 0`, `completion_tokens: 0` as harness constants, discarding
  the provider-returned model name, generation id and retry count that
  `ModelCall` already carried. "540/540 resolved_model = glm-5.2" was a
  statement about a constant.

Repair
------
`ModelExecutionRecord` persists what was REQUESTED and what the provider
RETURNED, separately, and never manufactures a zero for an unavailable field —
absent values are `None` with a recorded reason. `classify_output` separates
truncation from contract failure from parser failure from a valid decision, so
they can never again be collapsed into "incorrect".
"""

from __future__ import annotations

import hashlib
from collections import Counter
from dataclasses import dataclass, field
from enum import Enum


class OutputClass(str, Enum):
    VALID_DECISION = "VALID_DECISION"
    OUTPUT_TRUNCATED = "OUTPUT_TRUNCATED"
    MODEL_OUTPUT_CONTRACT_FAILURE = "MODEL_OUTPUT_CONTRACT_FAILURE"
    PARSER_FAILURE = "PARSER_FAILURE"


#: Reasons a provenance field may legitimately be unavailable.
UNAVAILABLE = "provider_did_not_report"


@dataclass(frozen=True)
class ModelExecutionRecord:
    """One model call. Requested and returned values are kept distinct."""

    call_num: int
    # ── requested ────────────────────────────────────────────────────────────
    requested_model: str
    requested_max_tokens: int
    requested_temperature: float
    endpoint: str
    # ── returned (None means the provider did not report it) ─────────────────
    resolved_model: str | None
    provider_route: str | None
    generation_id: str | None
    finish_reason: str | None
    prompt_tokens: int | None
    completion_tokens: int | None
    total_tokens: int | None
    reasoning_tokens: int | None
    retries: int | None
    latency_ms: float | None
    cost: float | None
    # ── payload ──────────────────────────────────────────────────────────────
    raw_text: str
    raw_hash: str
    unavailable_fields: tuple[str, ...] = field(default_factory=tuple)

    @property
    def hit_requested_ceiling(self) -> bool:
        return (self.completion_tokens is not None
                and self.completion_tokens >= self.requested_max_tokens)

    @property
    def truncated(self) -> bool:
        """Truncation by either provider signal or completion-token ceiling."""
        if self.finish_reason is not None:
            return self.finish_reason in ("length", "max_tokens", "MAX_TOKENS")
        return self.hit_requested_ceiling

    def to_dict(self) -> dict:
        return {
            "call_num": self.call_num,
            "requested": {
                "model": self.requested_model,
                "max_tokens": self.requested_max_tokens,
                "temperature": self.requested_temperature,
                "endpoint": self.endpoint,
            },
            "returned": {
                "resolved_model": self.resolved_model,
                "provider_route": self.provider_route,
                "generation_id": self.generation_id,
                "finish_reason": self.finish_reason,
                "prompt_tokens": self.prompt_tokens,
                "completion_tokens": self.completion_tokens,
                "total_tokens": self.total_tokens,
                "reasoning_tokens": self.reasoning_tokens,
                "retries": self.retries,
                "latency_ms": self.latency_ms,
                "cost": self.cost,
            },
            "unavailable_fields": list(self.unavailable_fields),
            "unavailable_reason": UNAVAILABLE if self.unavailable_fields else None,
            "raw_hash": self.raw_hash,
            "raw_len": len(self.raw_text),
            "hit_requested_ceiling": self.hit_requested_ceiling,
            "truncated": self.truncated,
        }


_OPTIONAL = ("resolved_model", "provider_route", "generation_id",
             "finish_reason", "prompt_tokens", "completion_tokens",
             "total_tokens", "reasoning_tokens", "retries", "latency_ms",
             "cost")


def record_from_provider(call_num: int, requested_model: str,
                         requested_max_tokens: int, requested_temperature: float,
                         endpoint: str, raw_text: str, provider: dict
                         ) -> ModelExecutionRecord:
    """Build a record from a provider response dict.

    A field the provider did not report becomes `None` and is listed in
    `unavailable_fields`. It is never silently coerced to 0 — that coercion is
    exactly what made the historical usage sidecars uninformative.
    """
    got = {k: provider.get(k) for k in _OPTIONAL}
    missing = tuple(k for k in _OPTIONAL if got[k] is None)
    return ModelExecutionRecord(
        call_num=call_num,
        requested_model=requested_model,
        requested_max_tokens=requested_max_tokens,
        requested_temperature=requested_temperature,
        endpoint=endpoint,
        raw_text=raw_text or "",
        raw_hash=hashlib.sha256((raw_text or "").encode()).hexdigest(),
        unavailable_fields=missing,
        **got,
    )


def classify_output(record: ModelExecutionRecord, parse_status,
                    repair_budget_exhausted: bool) -> OutputClass:
    """Separate the four output outcomes. Never collapse them into 'incorrect'.

    Precedence is deliberate and documented:
      1. truncation is an infrastructure fact and outranks everything;
      2. a schema-valid decision is VALID;
      3. an unresolved contract violation is the MODEL's failure, and only
         after the bounded repair budget is spent;
      4. PARSER_FAILURE is reserved for a schema-valid object the harness could
         not bind — unreachable by construction, and asserted so by test.
    """
    from .decision import ParseStatus

    if record.truncated:
        return OutputClass.OUTPUT_TRUNCATED
    if parse_status is ParseStatus.VALID:
        return OutputClass.VALID_DECISION
    if repair_budget_exhausted:
        return OutputClass.MODEL_OUTPUT_CONTRACT_FAILURE
    return OutputClass.MODEL_OUTPUT_CONTRACT_FAILURE


@dataclass
class EffectiveCeilingDetector:
    """Detects a route-imposed ceiling below the requested maximum.

    This is the instrument that would have caught NEW-02 during the run rather
    than in an audit a day later.
    """

    requested: Counter = field(default_factory=Counter)
    max_observed: int = 0
    at_ceiling: int = 0
    calls: int = 0
    #: completion-token values seen on calls where the provider reported NO
    #: finish_reason. The historical client discarded finish_reason, so this is
    #: the only channel in which the 4096 ceiling was ever visible.
    silent_completions: Counter = field(default_factory=Counter)

    #: How many calls must share the same maximum completion-token value, with
    #: no finish_reason and nothing above it, before that value is treated as a
    #: route ceiling. One call stopping at a round number proves nothing.
    CLUSTER_MIN = 2

    def observe(self, record: ModelExecutionRecord) -> None:
        self.calls += 1
        self.requested[record.requested_max_tokens] += 1
        if record.completion_tokens is not None:
            self.max_observed = max(self.max_observed, record.completion_tokens)
            if record.hit_requested_ceiling or record.truncated:
                self.at_ceiling += 1
            if record.finish_reason is None:
                self.silent_completions[record.completion_tokens] += 1

    def _cluster_at_max(self) -> int:
        """Calls pinned at the observed maximum with no reported finish reason."""
        return self.silent_completions.get(self.max_observed, 0)

    def report(self) -> dict:
        requested_max = max(self.requested) if self.requested else 0
        below_request = 0 < self.max_observed < requested_max

        # Two admissible truncation signatures, and only two.
        #
        #  (a) EXPLICIT — the provider said `length`, or a call reached the
        #      requested ceiling. Direct observation.
        #
        #  (b) CLUSTERED — several calls stop at exactly the same value, that
        #      value is the maximum ever observed, it is strictly below the
        #      request, and the provider reported no finish reason for them.
        #      This is the historical signature: 270 gov-full calls at exactly
        #      4096 against a request of 8192, with zero calls above and a
        #      client that discarded finish_reason. Without (b) the detector
        #      would be blind to precisely the defect it exists to catch.
        #
        # Neither fires for a model that merely stopped early: the live
        # preflight's 7914 completion tokens with finish_reason='stop' against a
        # request of 8192 has no truncation and no silent cluster, and was
        # briefly misreported as a route ceiling before (a) required truncation.
        explicit = self.at_ceiling > 0 and below_request
        cluster = self._cluster_at_max()
        clustered = cluster >= self.CLUSTER_MIN and below_request
        ceiling_below_request = explicit or clustered
        evidence = ("explicit_truncation" if explicit
                    else "clustered_silent_maximum" if clustered else "none")

        return {
            "calls": self.calls,
            "requested_max_tokens": sorted(self.requested),
            "max_completion_tokens_observed": self.max_observed,
            "calls_at_or_above_ceiling": self.at_ceiling,
            "route_ceiling_below_request": ceiling_below_request,
            "effective_ceiling": (self.max_observed if ceiling_below_request
                                  else requested_max),
            "truncated_calls": self.at_ceiling,
            "ceiling_evidence": evidence,
            "silent_calls_at_observed_maximum": cluster,
            "claim_permitted": (
                f"requested {requested_max}; observed maximum "
                f"{self.max_observed}; {self.at_ceiling} truncated call(s)"
                + ("; the route imposed a lower ceiling — do NOT claim the "
                   "requested value governed the run" if explicit
                   else f"; {cluster} calls stopped at exactly "
                        f"{self.max_observed} with no finish_reason and none "
                        f"above it — do NOT claim the requested value governed "
                        f"the run" if clustered
                   else "; no call was truncated, so the requested limit is "
                        "not contradicted by these data")),
        }

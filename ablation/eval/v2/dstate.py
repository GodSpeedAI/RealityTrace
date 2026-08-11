"""R-02: diagnostic state continuity across rounds.

Audit defect closed
-------------------
A-11.  Every legacy call sent exactly `[system, user]` — there was no
conversation history — and no follow-up scaffold carried the retrieval context.
Measured from the gov-full trajectories: round-1 prompts averaged 2,010 tokens
(V/G) and 1,722 (O); rounds 2-5 averaged 228-485. The `G_SCAFFOLD_REJECTION_*`
prompts asked the model to choose a probe while showing it neither the failure
nor any observation. Every claim about affordance navigation, evidence
utilization, belief revision and next-affordable-move selection was measured
under that amnesia.

Repair
------
The controller owns state continuity. `DiagnosticState` is the harness-owned
representation rendered into EVERY model call. The model is never required to
remember a prior call.

This is deliberately not "append the whole chat history": the state is an
explicit, bounded, deterministic object. Observation payloads are the only
unbounded component and they are truncated per-observation with an explicit
marker, never silently.

`REQUIRED_ROUND_STATE` is the invariant the tests assert on the RENDERED text.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass, field, replace

#: Every rendered round MUST carry these. Asserted by the state-continuity
#: tests, including a negative test that removes initial evidence.
REQUIRED_ROUND_STATE = (
    "question",
    "initial_evidence",
    "candidate_loci",
    "acquired_observations",
    "unresolved_distinctions",
    "rejected_settlement_attempts",
    "available_actions",
    "payment",
    "governance_state",
)

OBSERVATION_RENDER_LIMIT = 2000


@dataclass(frozen=True)
class RejectedAttempt:
    round_num: int
    kind: str            # "concrete" | "unknown"
    proposal: str
    reason_category: str  # gate reason CATEGORY only, never a hint


@dataclass(frozen=True)
class AcquiredObservation:
    round_num: int
    handle: str
    payload: dict
    source: str          # "model" | "harness"


@dataclass(frozen=True)
class DiagnosticState:
    """The complete diagnostic state the controller carries between rounds."""

    question: str
    case_id: str
    initial_evidence: str
    action_schema: str
    locus_vocabulary: tuple[str, ...]
    max_rounds: int
    max_payment: int

    round_num: int = 1
    candidate_loci: tuple[str, ...] = ()
    current_claim: str | None = None
    acquired: tuple[AcquiredObservation, ...] = ()
    qualifying_evidence: tuple[str, ...] = ()
    unresolved_distinctions: tuple[str, ...] = ()
    rejected_settlements: tuple[RejectedAttempt, ...] = ()
    available_actions: tuple[str, ...] = ()
    payment_used: int = 0
    governance_state: str = "INITIAL"
    budget_semantics: str = "ENFORCED"
    #: "raw_json" reproduces the historical rendering (a truncated JSON dump);
    #: "labelled_structural" names each field. This is a real manipulation of
    #: observation legibility, not a cosmetic flag.
    observation_rendering: str = "raw_json"
    notes: tuple[str, ...] = field(default_factory=tuple)

    # ── evolution ────────────────────────────────────────────────────────────

    @property
    def payment_remaining(self) -> int:
        if self.budget_semantics == "EXEMPT_UPPER_BOUND":
            return self.max_payment
        return max(0, self.max_payment - self.payment_used)

    def with_round(self, n: int) -> "DiagnosticState":
        return replace(self, round_num=n)

    def with_decision(self, decision) -> "DiagnosticState":
        loci = tuple(decision.candidate_loci) or self.candidate_loci
        unresolved = self.unresolved_distinctions
        if decision.unresolved_distinction:
            unresolved = unresolved + (decision.unresolved_distinction,)
        return replace(self, candidate_loci=loci,
                       unresolved_distinctions=unresolved,
                       current_claim=decision.proposed_settlement
                       or self.current_claim)

    def with_observations(self, obs, source: str, payment: int,
                          available: tuple[str, ...]) -> "DiagnosticState":
        new = tuple(AcquiredObservation(self.round_num, o.handle, o.payload,
                                        source) for o in obs)
        return replace(self, acquired=self.acquired + new,
                       payment_used=self.payment_used + payment,
                       available_actions=available)

    def with_rejection(self, kind: str, proposal: str,
                       reason_category: str) -> "DiagnosticState":
        r = RejectedAttempt(self.round_num, kind, proposal, reason_category)
        return replace(self, rejected_settlements=self.rejected_settlements + (r,),
                       governance_state=f"{kind.upper()}_REJECTED")

    def with_governance(self, state: str) -> "DiagnosticState":
        return replace(self, governance_state=state)

    def with_qualifying(self, handles: tuple[str, ...]) -> "DiagnosticState":
        return replace(self, qualifying_evidence=tuple(
            sorted(set(self.qualifying_evidence) | set(handles))))

    # ── rendering ────────────────────────────────────────────────────────────

    def render(self) -> str:
        """Deterministic, complete rendering of the diagnostic state.

        Ordering is fixed; every section is always present, including when
        empty (an absent section is indistinguishable from lost state, which is
        the failure mode this repair exists to prevent).
        """
        parts = [
            "=== DIAGNOSTIC STATE ===",
            f"[question] {self.question}",
            f"[case] {self.case_id}",
            f"[round] {self.round_num}/{self.max_rounds}",
            f"[governance_state] {self.governance_state}",
            "",
            "[initial_evidence]",
            self.initial_evidence,
            "",
            "[candidate_loci] " + (", ".join(self.candidate_loci) or "(none yet)"),
            "[current_claim] " + (self.current_claim or "(none)"),
            "[unresolved_distinctions]",
        ]
        parts += ([f"  - {u}" for u in self.unresolved_distinctions]
                  or ["  (none recorded)"])
        parts += ["", "[rejected_settlement_attempts]"]
        parts += ([f"  - round {r.round_num}: {r.kind} '{r.proposal}' "
                   f"rejected ({r.reason_category})"
                   for r in self.rejected_settlements]
                  or ["  (none)"])
        parts += ["", "[acquired_observations]"]
        if self.acquired:
            for a in self.acquired:
                parts.append(f"  - round {a.round_num} {a.handle}:")
                parts.append(self._render_payload(a.payload))
        else:
            parts.append("  (none acquired)")
        parts += [
            "",
            "[qualifying_evidence] " + (", ".join(self.qualifying_evidence)
                                        or "(none)"),
            f"[payment] used {self.payment_used}/{self.max_payment} "
            f"(remaining {self.payment_remaining}, semantics "
            f"{self.budget_semantics})",
            "[available_actions] " + (", ".join(self.available_actions)
                                      or "(none affordable)"),
            "",
            "[action_schema]",
            self.action_schema,
            f"[locus_vocabulary] {', '.join(self.locus_vocabulary)}",
            "=== END DIAGNOSTIC STATE ===",
        ]
        return "\n".join(parts)

    def _render_payload(self, payload: dict) -> str:
        """Delegates to the frozen legibility contract (eval/v2/legibility.py).

        Both renderings are pure functions of the same payload, which is what
        makes the R1/R2 and R3/R4 contrasts a manipulation of representation
        only (owner decision §7).
        """
        from .legibility import render_legible, render_raw
        if self.observation_rendering == "labelled_structural":
            return render_legible(payload, OBSERVATION_RENDER_LIMIT)
        return render_raw(payload, OBSERVATION_RENDER_LIMIT)

    def digest(self) -> str:
        return hashlib.sha256(self.render().encode()).hexdigest()

    def to_dict(self) -> dict:
        return {
            "case_id": self.case_id, "round_num": self.round_num,
            "candidate_loci": list(self.candidate_loci),
            "current_claim": self.current_claim,
            "acquired": [{"round": a.round_num, "handle": a.handle,
                          "source": a.source} for a in self.acquired],
            "qualifying_evidence": list(self.qualifying_evidence),
            "unresolved_distinctions": list(self.unresolved_distinctions),
            "rejected_settlements": [
                {"round": r.round_num, "kind": r.kind, "proposal": r.proposal,
                 "reason_category": r.reason_category}
                for r in self.rejected_settlements],
            "available_actions": list(self.available_actions),
            "payment_used": self.payment_used,
            "payment_remaining": self.payment_remaining,
            "governance_state": self.governance_state,
            "budget_semantics": self.budget_semantics,
            "state_digest": self.digest(),
        }


def missing_state_sections(rendered: str) -> list[str]:
    """Which `REQUIRED_ROUND_STATE` markers are absent from a rendered round."""
    marker = {
        "question": "[question]",
        "initial_evidence": "[initial_evidence]",
        "candidate_loci": "[candidate_loci]",
        "acquired_observations": "[acquired_observations]",
        "unresolved_distinctions": "[unresolved_distinctions]",
        "rejected_settlement_attempts": "[rejected_settlement_attempts]",
        "available_actions": "[available_actions]",
        "payment": "[payment]",
        "governance_state": "[governance_state]",
    }
    return [k for k in REQUIRED_ROUND_STATE if marker[k] not in rendered]

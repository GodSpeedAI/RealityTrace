"""R-03 + R-16: replication-ready diagnostic controller.

Audit defects closed
--------------------
A-02  In `acquisition_run.run_d2`, a round-1 JSON parse failure immediately
      substituted `D2_SCAFFOLD_BUDGET_EXHAUSTED` — literally "Budget exhausted
      (0/7 payment, 1/3 rounds). No further acquisition possible." — with the
      budget untouched. Measured: 106 of 180 D2 units were routed into that
      false terminal after a format failure, which is most of the "81%
      behaviourally bypassed acquisition" headline.
A-10  In `governed_run.run_unit`, the O branch always `continue`d after an
      oracle probe, so the round loop could fall through without any `break`;
      `final_locus` stayed `None` and line 533 defaulted to
      `CONTRACT_FAILURE` / `unknown`. 84 of 180 O units terminated that way
      and were never asked for a final answer, while 124 G units were.
A-06  V/G/O differed on at least eight consequential dimensions while the
      report described the O contrast as isolating action selection.

Repair
------
* Every terminal state is set explicitly on its own path. `ROUND_LIMIT_REACHED`
  is a declared outcome, not a fall-through default, and the controller asserts
  that a terminal state was set.
* A forced-final decision request is offered to EVERY condition under the same
  trigger conditions, so termination handling cannot differ by arm.
* An invalid decision never consumes payment, never fabricates an action, never
  claims the budget is exhausted, and never becomes a settlement or an UNKNOWN.
  It routes to a bounded repair loop and then to an explicit
  `MODEL_OUTPUT_CONTRACT_FAILURE`.
* `ConditionSpec.diff` enumerates every declared difference between two arms so
  a report cannot again claim a contrast isolates one variable.

The model client is injected. This module performs no network I/O.
"""

from __future__ import annotations

import json
from dataclasses import asdict, dataclass, field
from enum import Enum
from typing import Protocol

from .channel import (DisclosurePolicy, ObservationChannel, PromptLeakGuard,
                      unit_seed)
from .decision import parse_decision, repair_request
from .dstate import DiagnosticState
from .execution import (EffectiveCeilingDetector, OutputClass, classify_output,
                        record_from_provider)


class Terminal(str, Enum):
    CONCRETE_SETTLEMENT = "CONCRETE_SETTLEMENT"
    TERMINAL_UNKNOWN = "TERMINAL_UNKNOWN"
    BUDGET_EXHAUSTED_SETTLED = "BUDGET_EXHAUSTED_SETTLED"
    BUDGET_EXHAUSTED_GATE_REJECTED = "BUDGET_EXHAUSTED_GATE_REJECTED"
    ROUND_LIMIT_REACHED = "ROUND_LIMIT_REACHED"
    MODEL_OUTPUT_CONTRACT_FAILURE = "MODEL_OUTPUT_CONTRACT_FAILURE"


#: Terminals in which the harness, not the model, chose `unknown`. Metrics that
#: claim to measure epistemic restraint must exclude these.
HARNESS_DEFAULTED = (Terminal.ROUND_LIMIT_REACHED,
                     Terminal.MODEL_OUTPUT_CONTRACT_FAILURE)


class ModelClient(Protocol):
    def complete(self, system: str, user: str, requested_max_tokens: int,
                 temperature: float) -> dict:
        """Return `{"text": str, "provider": {...}}`. Provider keys absent from
        the response must be ABSENT, not zero."""


@dataclass(frozen=True)
class ConditionSpec:
    """Every dimension on which an arm may differ, declared up front."""

    name: str
    #: who chooses the action
    action_source: str                     # "model" | "harness"
    #: is the action schema shown to the model
    schema_visible: bool = True
    #: is the sufficiency gate applied
    gated: bool = True
    disclosure: DisclosurePolicy = DisclosurePolicy.FULL
    max_rounds: int = 3
    max_payment: int = 7
    max_model_calls: int = 8
    max_repair_attempts: int = 2
    budget_semantics: str = "ENFORCED"
    observation_rendering: str = "raw_json"
    forced_final_offered: bool = True
    response_fields: tuple[str, ...] = (
        "candidate_loci", "unresolved_distinction", "selected_action",
        "expected_information", "proposed_settlement")

    def diff(self, other: "ConditionSpec") -> dict:
        a, b = asdict(self), asdict(other)
        return {k: {self.name: a[k], other.name: b[k]}
                for k in a if k != "name" and a[k] != b[k]}


def arm_difference_report(specs) -> dict:
    """Publishable inventory of arm differences (closes A-06).

    Emitted alongside every run so no report can claim a contrast isolates a
    single variable unless this dictionary actually says so.
    """
    specs = list(specs)
    out = {}
    for i, a in enumerate(specs):
        for b in specs[i + 1:]:
            d = a.diff(b)
            out[f"{a.name}_vs_{b.name}"] = {
                "n_declared_differences": len(d),
                "differences": d,
                "isolates_single_variable": len(d) == 1,
            }
    return out


@dataclass
class RoundRecord:
    round_num: int
    prompt_digest: str
    parse_status: str
    output_class: str
    repair_attempts: int
    decision: dict | None
    gate_result: str | None
    action_source: str | None
    handles_delivered: tuple = ()
    payment_charged: int = 0
    delivery: dict | None = None


@dataclass
class UnitResult:
    unit_id: str
    case_id: str
    condition: str
    rep: int
    terminal_state: str
    final_locus: str | None
    #: the last locus the MODEL proposed, regardless of gate outcome.
    #: Enables gate-blind accuracy (audit A-03 / corrective action 4).
    last_model_claim: str | None
    harness_defaulted: bool
    rounds: list = field(default_factory=list)
    executions: list = field(default_factory=list)
    state_digests: list = field(default_factory=list)
    payment_used: int = 0
    rejected_concrete: int = 0
    rejected_unknown: int = 0
    acquired_handles: list = field(default_factory=list)
    acquired_probes: list = field(default_factory=list)
    ceiling: dict | None = None

    def to_dict(self) -> dict:
        d = asdict(self)
        d["rounds"] = [asdict(r) if not isinstance(r, dict) else r
                       for r in self.rounds]
        return d


SYSTEM_PROMPT = (
    "You are a failure-diagnosis evaluator. You attribute an observed failure "
    "to exactly one failure locus from the vocabulary given in the diagnostic "
    "state. Reply with exactly one JSON object matching the declared response "
    "contract, and nothing else."
)

TASK_INSTRUCTION = """\
Diagnose the recorded failure described in the diagnostic state below.

1. ENUMERATE at least two plausible candidate failure loci.
2. Identify the KEY unresolved distinction between them.
3. Either select ONE evidence-acquisition action by its handle, or propose a
   settlement (a concrete locus, or `unknown`).

`selected_action` and `proposed_settlement` are mutually exclusive: set at most
one, and set the other to null.

Reply with exactly one JSON object:
{"candidate_loci": ["<locus>", ...], "unresolved_distinction": "<text or null>",
 "selected_action": "<handle or null>", "expected_information": "<text or null>",
 "proposed_settlement": "<locus or null>"}
"""

FORCED_FINAL_INSTRUCTION = """\
No further evidence acquisition is possible for this unit ({reason}).
Using only the diagnostic state below, propose a final settlement: a concrete
locus, or `unknown` if the evidence you hold does not support one.

Reply with exactly one JSON object:
{{"candidate_loci": ["<locus>", ...], "unresolved_distinction": null,
  "selected_action": null, "expected_information": null,
  "proposed_settlement": "<locus or unknown>"}}
"""


class DiagnosticController:
    """Harness-owned state machine. Deterministic given the client."""

    def __init__(self, spec: ConditionSpec, contract, probe_ids, descriptions,
                 costs, vocabulary, client: ModelClient,
                 requested_max_tokens: int = 8192, temperature: float = 0.0,
                 endpoint: str = "injected-client", requested_model: str = "glm-5.2"):
        self.spec = spec
        self.contract = contract
        self.probe_ids = tuple(probe_ids)
        self.descriptions = descriptions
        self.costs = costs
        self.vocabulary = tuple(vocabulary)
        self.client = client
        self.requested_max_tokens = requested_max_tokens
        self.temperature = temperature
        self.endpoint = endpoint
        self.requested_model = requested_model
        self.leak_guard = PromptLeakGuard(self.probe_ids)

    # ── one unit ─────────────────────────────────────────────────────────────

    def run_unit(self, unit_id, experiment_id, namespace, case_id, rep,
                 question, initial_evidence, observations,
                 evaluator_discriminating=()) -> UnitResult:
        from .channel import ActionCatalogue

        seed = unit_seed(experiment_id, namespace, case_id, self.spec.name, rep)
        catalogue = ActionCatalogue(self.probe_ids, seed)
        channel = ObservationChannel(catalogue, self.costs,
                                     self.spec.disclosure)
        schema = (catalogue.render(self.descriptions, self.costs)
                  if self.spec.schema_visible else "(not shown in this condition)")

        state = DiagnosticState(
            question=question, case_id=case_id, initial_evidence=initial_evidence,
            action_schema=schema, locus_vocabulary=self.vocabulary,
            max_rounds=self.spec.max_rounds, max_payment=self.spec.max_payment,
            available_actions=catalogue.handles,
            budget_semantics=self.spec.budget_semantics,
            observation_rendering=self.spec.observation_rendering)

        res = UnitResult(unit_id=unit_id, case_id=case_id,
                         condition=self.spec.name, rep=rep,
                         terminal_state="", final_locus=None,
                         last_model_claim=None, harness_defaulted=False)
        ceiling = EffectiveCeilingDetector()
        calls = 0
        acquired_probes: list[str] = []
        terminal: Terminal | None = None

        for rnd in range(1, self.spec.max_rounds + 1):
            state = state.with_round(rnd)
            if calls >= self.spec.max_model_calls:
                terminal = Terminal.ROUND_LIMIT_REACHED
                break

            outcome, rec, used, repairs = self._ask(
                state, TASK_INSTRUCTION, res, ceiling)
            calls += used
            if outcome is None:                       # contract repair exhausted
                terminal = Terminal.MODEL_OUTPUT_CONTRACT_FAILURE
                break

            d = outcome.decision
            state = state.with_decision(d)
            res.last_model_claim = d.proposed_settlement or res.last_model_claim
            rr = RoundRecord(rnd, state.digest(), outcome.status.value,
                             rec, repairs, d.to_dict(), None, None)

            # ── settlement proposal ──────────────────────────────────────────
            if d.is_settlement:
                state, terminal = self._gate(d, state, res, observations,
                                             acquired_probes, rr)
                if terminal is not None:
                    res.rounds.append(rr)
                    break
                # Rejected. In a harness-source arm the harness now supplies
                # evidence, which is the whole point of the upper-bound arm:
                # the model is relieved of selection, not of interpretation.
                if self.spec.action_source == "harness":
                    state = self._harness_deliver(
                        state, res, rr, channel, catalogue, observations,
                        evaluator_discriminating, seed + rnd, acquired_probes)
                res.rounds.append(rr)
                continue

            # ── action proposal ──────────────────────────────────────────────
            if d.is_action:
                # In a harness-source condition the model-visible action field
                # cannot select evidence.  The frozen evidence-revision arms
                # all declare action_source="harness" specifically to remove
                # action selection from the measured construct.  A syntactically
                # valid action proposal is recorded, but delivery remains the
                # harness policy's deterministic choice.
                if self.spec.action_source == "harness":
                    rr.gate_result = "MODEL_ACTION_IGNORED_HARNESS_CONTROLLED"
                    state = self._harness_deliver(
                        state, res, rr, channel, catalogue, observations,
                        evaluator_discriminating, seed + rnd, acquired_probes)
                    res.rounds.append(rr)
                    continue
                handle = d.selected_action
                probe = catalogue.probe_of(handle)
                cost = self.costs[probe]
                if (self.spec.budget_semantics == "ENFORCED"
                        and state.payment_used + cost > self.spec.max_payment):
                    rr.gate_result = "ACTION_UNAFFORDABLE"
                    res.rounds.append(rr)
                    state = state.with_governance("BUDGET_EXHAUSTED")
                    terminal = self._forced_final(
                        state, res, ceiling, "acquisition budget exhausted",
                        observations, acquired_probes)
                    break
                delivery = channel.deliver({probe: observations[probe]},
                                           evaluator_discriminating, seed + rnd)
                state = state.with_observations(
                    delivery.observations, "model", delivery.payment_charged,
                    self._affordable(catalogue, acquired_probes + [probe],
                                     state.payment_used + delivery.payment_charged))
                acquired_probes.append(probe)
                res.acquired_handles.append(handle)
                res.acquired_probes.append(probe)
                rr.action_source = "model"
                rr.handles_delivered = tuple(o.handle for o in delivery.observations)
                rr.payment_charged = delivery.payment_charged
                rr.delivery = delivery.to_dict()
                res.rounds.append(rr)
                continue

            # ── neither: harness may supply evidence (upper-bound arms) ──────
            if self.spec.action_source == "harness":
                state = self._harness_deliver(
                    state, res, rr, channel, catalogue, observations,
                    evaluator_discriminating, seed + rnd, acquired_probes)
                res.rounds.append(rr)
                continue

            rr.gate_result = rr.gate_result or "NO_DECISION_CONTENT"
            res.rounds.append(rr)

        # ── explicit termination; no fall-through default (A-10) ─────────────
        if terminal is None:
            terminal = self._forced_final(state, res, ceiling,
                                          "round limit reached",
                                          observations, acquired_probes)

        res.terminal_state = terminal.value
        res.harness_defaulted = terminal in HARNESS_DEFAULTED
        if res.final_locus is None and terminal in HARNESS_DEFAULTED:
            res.final_locus = None      # NOT silently "unknown" (A-10, R-03)
        res.payment_used = state.payment_used
        res.state_digests.append(state.digest())
        res.ceiling = ceiling.report()
        assert res.terminal_state, "every path must set a terminal state"
        return res

    # ── helpers ──────────────────────────────────────────────────────────────

    def _affordable(self, catalogue, acquired, payment_used) -> tuple:
        if self.spec.budget_semantics != "ENFORCED":
            return catalogue.handles
        return tuple(sorted(
            catalogue.handle_of(p) for p in self.probe_ids
            if p not in acquired
            and payment_used + self.costs[p] <= self.spec.max_payment))

    def _ask(self, state, instruction, res, ceiling):
        """One decision request with bounded contract repair (R-03).

        Returns `(outcome|None, output_class, calls_used, repair_attempts)`.
        A repair never consumes payment and never fabricates controller state.
        """
        prompt = instruction + "\n\n" + state.render()
        self.leak_guard.assert_clean(prompt)
        calls = 0
        for attempt in range(self.spec.max_repair_attempts + 1):
            raw = self.client.complete(SYSTEM_PROMPT, prompt,
                                       self.requested_max_tokens,
                                       self.temperature)
            calls += 1
            rec = record_from_provider(
                len(res.executions) + 1, self.requested_model,
                self.requested_max_tokens, self.temperature, self.endpoint,
                raw.get("text", ""), raw.get("provider", {}))
            res.executions.append(rec.to_dict())   # persisted BEFORE parsing
            ceiling.observe(rec)

            outcome = parse_decision(rec.raw_text, self.vocabulary,
                                     self._handles(state))
            cls = classify_output(rec, outcome.status,
                                  attempt >= self.spec.max_repair_attempts)
            if outcome.ok:
                return outcome, OutputClass.VALID_DECISION.value, calls, attempt
            if attempt < self.spec.max_repair_attempts:
                prompt = (repair_request(outcome) + "\n\n" + state.render())
                self.leak_guard.assert_clean(prompt)
        return None, cls.value, calls, self.spec.max_repair_attempts

    def _handles(self, state):
        return tuple(f"act-{i:02d}" for i in range(1, len(self.probe_ids) + 1))

    def _harness_deliver(self, state, res, rr, channel, catalogue, observations,
                         discriminating, seed, acquired_probes):
        """Harness-supplied evidence for an upper-bound arm.

        The disclosure policy governs WHAT is delivered; under FULL the
        delivered set is identical for every unit and therefore carries no
        information about the locus (R-01).
        """
        remaining = {p: o for p, o in observations.items()
                     if p not in acquired_probes}
        if not remaining:
            rr.gate_result = "NO_FURTHER_EVIDENCE"
            return state
        delivery = channel.deliver(remaining, discriminating, seed)
        if (self.spec.budget_semantics == "ENFORCED"
                and state.payment_used + delivery.payment_charged
                > self.spec.max_payment):
            # Do not silently shrink MASKED_K: doing so would change its
            # declared consistency-set bound.  The complete policy delivery is
            # unaffordable, so no observation is delivered and governance
            # records exhaustion without charging payment.
            rr.gate_result = "HARNESS_DELIVERY_UNAFFORDABLE"
            from dataclasses import replace
            return replace(state, available_actions=(),
                           governance_state="BUDGET_EXHAUSTED")
        probes = list(delivery.delivered_probes)
        state = state.with_observations(
            delivery.observations, "harness", delivery.payment_charged,
            self._affordable(catalogue, acquired_probes + probes,
                             state.payment_used + delivery.payment_charged))
        acquired_probes.extend(probes)
        res.acquired_handles.extend(o.handle for o in delivery.observations)
        res.acquired_probes.extend(probes)
        rr.action_source = "harness"
        rr.handles_delivered = tuple(o.handle for o in delivery.observations)
        rr.payment_charged = delivery.payment_charged
        rr.delivery = delivery.to_dict()
        return state

    def _gate(self, decision, state, res, observations, acquired_probes, rr):
        """Apply the sufficiency / navigability gate to a settlement proposal.

        Returns `(new_state, terminal_or_None)`. The gate never sees ground
        truth; a qualifying claim can still be factually wrong.
        """
        proposal = decision.proposed_settlement

        if not self.spec.gated:
            res.final_locus = proposal
            rr.gate_result = "UNGATED"
            return state, (Terminal.TERMINAL_UNKNOWN if proposal == "unknown"
                           else Terminal.CONCRETE_SETTLEMENT)

        if proposal == "unknown":
            if state.available_actions:
                res.rejected_unknown += 1
                rr.gate_result = "AFFORDABLE_ACTION_REMAINS"
                return state.with_rejection("unknown", proposal,
                                            "AFFORDABLE_ACTION_REMAINS"), None
            res.final_locus = "unknown"
            rr.gate_result = "TERMINAL_UNKNOWN"
            return state, Terminal.TERMINAL_UNKNOWN

        acquired_obs = {p: observations[p] for p in acquired_probes}
        if self.contract.settlement_eligible(proposal, acquired_obs):
            res.final_locus = proposal
            rr.gate_result = "SETTLEMENT_ELIGIBLE"
            return (state.with_qualifying(tuple(res.acquired_handles)),
                    Terminal.CONCRETE_SETTLEMENT)
        res.rejected_concrete += 1
        rr.gate_result = "EVIDENCE_INSUFFICIENT"
        return state.with_rejection("concrete", proposal,
                                    "EVIDENCE_INSUFFICIENT"), None

    def _forced_final(self, state, res, ceiling, reason, observations,
                      acquired_probes) -> Terminal:
        """Offered to EVERY condition under identical triggers (closes A-10).

        Governance is preserved: in a gated condition a concrete proposal is
        still re-gated, because dropping the gate on correct-but-unqualified
        answers would be benchmark tuning (audit §17). The repair is that the
        model's claim is ALWAYS recorded in `last_model_claim`, so gate-blind
        and gated accuracy are separately reportable (corrective action 4), and
        that a gate-enforced unknown is distinguished from a harness default.
        """
        if not self.spec.forced_final_offered:
            return Terminal.ROUND_LIMIT_REACHED

        outcome, _cls, _calls, _rep = self._ask(
            state, FORCED_FINAL_INSTRUCTION.format(reason=reason), res, ceiling)
        if outcome is None:
            return Terminal.MODEL_OUTPUT_CONTRACT_FAILURE

        proposal = outcome.decision.proposed_settlement
        if proposal is None:
            return Terminal.MODEL_OUTPUT_CONTRACT_FAILURE
        res.last_model_claim = proposal

        if proposal == "unknown":
            res.final_locus = "unknown"
            return Terminal.BUDGET_EXHAUSTED_SETTLED

        if self.spec.gated:
            acquired_obs = {p: observations[p] for p in acquired_probes}
            if not self.contract.settlement_eligible(proposal, acquired_obs):
                res.rejected_concrete += 1
                res.final_locus = "unknown"
                return Terminal.BUDGET_EXHAUSTED_GATE_REJECTED

        res.final_locus = proposal
        return Terminal.BUDGET_EXHAUSTED_SETTLED


def dump_result(res: UnitResult) -> str:
    return json.dumps(res.to_dict(), indent=2, sort_keys=True, default=str)

"""R-09: three distinct discrimination constructs, named distinctly.

Audit defect closed
-------------------
A-07 / A-16.  Two incompatible notions of "discriminating" were in simultaneous
use and one was scored as the other:

  * the gate's `SIGNAL_RULES` admitted `inspect_verifier_config` for
    `environment` and `rerun_verifier_isolated` for `implementation`, and had
    no rule at all for `verifier`;
  * the evaluator's `discriminating_probes` admitted `rerun_verifier_isolated`
    for `verifier`, never admitted `inspect_verifier_config` for anything, and
    admitted only `inspect_source_symbols` for `implementation`.

Consequences measured in Pass B: 90 gate-infeasible units were excluded from
the A_select/A_use denominators while 36 of them had acquired the probe the
evaluator calls discriminating; 19 feasible-determinate units were scored
`exact_correct` with `discrim_selected_any = false`; and exactly one unit
acquired the so-called "distractor", settled correctly on `environment`, passed
the gate, and was scored a selection failure.

The report called `inspect_verifier_config` "the intentional distractor" while
the frozen sufficiency contract gives it a qualifying signal for `environment`
— the locus on which the governed condition scored best.

Repair
------
These are three different things and they are allowed to differ. They are now
named separately, derived separately, and a divergence report is produced so no
future scorer can silently substitute one for another.
"""

from __future__ import annotations

from dataclasses import dataclass

#: Construction ground truth. Which action the corpus author knows reveals the
#: injected fault. Sealed; evaluator-side; legitimate for SCORING ONLY. Never
#: available to the gate, the controller, or the model.
EVALUATOR_DISCRIMINATING_ACTION = "EVALUATOR_DISCRIMINATING_ACTION"

#: Contract property. Whether the currently acquired evidence satisfies the
#: predeclared sufficiency rule for a PROPOSED locus. Knows no ground truth; a
#: qualifying claim can still be factually wrong.
GATE_QUALIFYING_EVIDENCE = "GATE_QUALIFYING_EVIDENCE"

#: Public, answer-blind expectation. Which actions bear on an unresolved
#: distinction given only the published catalogue. This is what a competent
#: controller could compute, and therefore the only fair basis for judging
#: action selection as a capability.
HYPOTHESIS_BEARING_ACTION = "HYPOTHESIS_BEARING_ACTION"

CONSTRUCTS = (EVALUATOR_DISCRIMINATING_ACTION, GATE_QUALIFYING_EVIDENCE,
              HYPOTHESIS_BEARING_ACTION)

CONSTRUCT_DEFINITIONS = {
    EVALUATOR_DISCRIMINATING_ACTION: (
        "Sealed construction-ground-truth property: the action whose "
        "observation exhibits the injected fault. Scoring only."),
    GATE_QUALIFYING_EVIDENCE: (
        "Whether acquired evidence satisfies the predeclared sufficiency "
        "predicate for a proposed locus. Answer-blind."),
    HYPOTHESIS_BEARING_ACTION: (
        "Whether an action is publicly expected to bear on a candidate locus, "
        "derivable from the published catalogue without ground truth."),
}


@dataclass(frozen=True)
class ActionAssessment:
    """One action, assessed under all three constructs. Never conflated."""

    action: str
    evaluator_discriminating: bool
    gate_qualifying_for: tuple[str, ...]
    hypothesis_bearing_on: tuple[str, ...]

    def to_dict(self) -> dict:
        return {
            "action": self.action,
            EVALUATOR_DISCRIMINATING_ACTION: self.evaluator_discriminating,
            GATE_QUALIFYING_EVIDENCE: list(self.gate_qualifying_for),
            HYPOTHESIS_BEARING_ACTION: list(self.hypothesis_bearing_on),
        }


def hypothesis_bearing(action: str, candidate_loci, bears_on: dict) -> tuple[str, ...]:
    """Answer-blind: does this action bear on any candidate the model holds?"""
    return tuple(sorted(set(bears_on.get(action, ())) & set(candidate_loci)))


def gate_qualifying(action: str, observations: dict, contract) -> tuple[str, ...]:
    """Loci for which this single action's observation currently qualifies."""
    out = []
    for locus in contract.loci():
        if contract.qualifies(locus, {action: observations[action]}) is True:
            out.append(locus)
    return tuple(sorted(out))


def assess(action: str, observations: dict, contract, bears_on: dict,
           evaluator_discriminating: set, candidate_loci=()) -> ActionAssessment:
    return ActionAssessment(
        action=action,
        evaluator_discriminating=action in evaluator_discriminating,
        gate_qualifying_for=(gate_qualifying(action, observations, contract)
                             if action in observations else ()),
        hypothesis_bearing_on=hypothesis_bearing(action, candidate_loci, bears_on),
    )


def divergence_report(hidden_meta: dict, contract, bears_on: dict) -> dict:
    """Where the three constructs disagree, corpus-wide.

    Published so that a metric can never again be reported under the wrong
    construct's name.
    """
    eval_pairs, gate_pairs = set(), set()
    for cid, meta in hidden_meta.items():
        locus = meta.get("true_locus")
        for p in meta.get("discriminating_probes", ()):
            eval_pairs.add((p, locus))
    for locus in contract.loci():
        for p in contract.probes_for(locus):
            gate_pairs.add((p, locus))
    bears_pairs = {(p, l) for p, loci in bears_on.items() for l in loci}

    return {
        "constructs": CONSTRUCT_DEFINITIONS,
        "evaluator_only": sorted(eval_pairs - gate_pairs),
        "gate_only": sorted(gate_pairs - eval_pairs),
        "agree": sorted(eval_pairs & gate_pairs),
        "bears_on_only": sorted(bears_pairs - gate_pairs - eval_pairs),
        "actions_never_evaluator_discriminating": sorted(
            {p for p, _ in bears_pairs} - {p for p, _ in eval_pairs}),
        "loci_with_no_gate_rule": sorted(
            {l for _, l in eval_pairs} - {l for _, l in gate_pairs}),
        "note": (
            "These sets are expected to be non-empty. They are three different "
            "constructs. `inspect_verifier_config` is GATE_QUALIFYING for "
            "environment and is NOT an EVALUATOR_DISCRIMINATING action; calling "
            "it a 'distractor' conflates the two."),
    }


#: Metric names bound to their construct, so a scorer cannot report one as
#: another. Consumed by the v2 rescorer.
METRIC_CONSTRUCT = {
    "A_select_first_evaluator": EVALUATOR_DISCRIMINATING_ACTION,
    "A_select_any_evaluator": EVALUATOR_DISCRIMINATING_ACTION,
    "A_select_first_hypothesis_bearing": HYPOTHESIS_BEARING_ACTION,
    "A_use_evaluator_evidence": EVALUATOR_DISCRIMINATING_ACTION,
    "settlement_eligible": GATE_QUALIFYING_EVIDENCE,
    "gate_blind_accuracy": None,
}

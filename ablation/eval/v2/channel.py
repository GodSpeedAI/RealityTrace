"""R-01: answer-isolated observation channel.

Audit defect closed
-------------------
A-01 / NEW-05.  `acquisition-hidden-meta.json` is a perfect bijection from true
locus to discriminating probe (verified in Pass B: 53/53 determinate cases have
exactly one discriminating probe; 7 of 8 loci are uniquely identified by it).
The legacy oracle selected that probe using the sealed key and then printed its
canonical name into the model's prompt:

    --- observation from automated probe: inspect_permissions ---

so the prompt contained a deterministic encoding of the answer. All 60 of the
oracle condition's correct answers coincided with the locus implied by the
announced probe. `governed_check.check_oracle_isolation()` could not detect
this: it greps the SOURCE TEXT of `oracle_select`/`format_observation` for the
strings `true_locus` and `discriminating`, and the leak runs through the probe's
identity, not through either string.

Repair — two independent mechanisms
-----------------------------------
1. **Opaque per-unit action handles.** Every condition sees the same complete
   action schema, identified by handles (`act-01` … `act-08`) assigned by a
   permutation seeded from `(experiment, namespace, case, condition, rep)` —
   quantities that are independent of the locus and of the hidden metadata. A
   handle therefore carries no cross-unit semantics, and no canonical probe id
   ever reaches the model.

   Handles alone are NOT sufficient: within a unit the model still sees each
   handle's description, so naming the oracle's handle would still disclose the
   answer. Hence:

2. **Disclosure policy on the delivered SET.** What the model receives is a set
   of observations whose membership must not be a function of the true locus.

       NONE       nothing is delivered: the no-new-evidence control (R0).
       FULL       every action's observation is delivered, for every unit.
                  The delivered set is identical across all units, so it
                  carries exactly zero information about the locus. This is the
                  genuine evidence-coverage upper bound.
       MASKED_K   the evaluator-discriminating observation plus (k-1) decoys
                  drawn by a locus-independent RNG. Leakage is bounded and
                  reported: the adversary's consistency set has size k.
       SINGLE     exactly one evaluator-selected discriminating observation.
                  The NAME leak is closed by opaque handles, but the delivered
                  SET still narrows the adversary's candidate set to 1, so its
                  selection leakage is declared rather than denied. It is a
                  legitimate mechanistic upper-bound arm and must never be read
                  as action-selection capability. Requires an explicit
                  acknowledgement so the choice is always deliberate.

`leakage_bound()` reports the consistency-set size and bits disclosed, and is
asserted by the integrity suite rather than argued in prose.

Evidentiary content is unchanged: a delivered observation is byte-identical to
what the same probe produces when self-selected (requirement B).
"""

from __future__ import annotations

import hashlib
import json
import math
import random
from dataclasses import dataclass
from enum import Enum

HANDLE_PREFIX = "act-"


class DisclosurePolicy(str, Enum):
    #: deliver nothing — the no-new-evidence control (R0)
    NONE = "NONE"
    FULL = "FULL"
    MASKED_K = "MASKED_K"
    #: exactly one evaluator-selected discriminating observation, delivered
    #: through an opaque handle. Upper-bound/mechanistic: the EVALUATOR chose
    #: it, so it must never be read as action-selection capability.
    SINGLE = "SINGLE"


class UnsafeDisclosure(Exception):
    """Raised when a policy that discloses the answer is used unacknowledged."""


class AnswerLeak(Exception):
    """Raised when a rendered prompt contains answer-bearing material."""


@dataclass(frozen=True)
class Observation:
    handle: str
    payload: dict

    def render(self) -> str:
        return (f"--- observation from {self.handle} ---\n"
                f"{json.dumps(self.payload, sort_keys=True, default=str)}\n"
                f"--- end observation ---")


def unit_seed(experiment_id: str, namespace: str, case_id: str,
              condition: str, rep: int) -> int:
    """Locus-independent, hidden-metadata-independent per-unit seed.

    Deliberately derived only from public run coordinates. Nothing from the key
    files or the hidden evaluator metadata may enter here; that is what makes
    the handle permutation provably uninformative about the answer.
    """
    payload = "|".join((experiment_id, namespace, case_id, condition, str(rep)))
    return int(hashlib.sha256(payload.encode()).hexdigest()[:16], 16)


class ActionCatalogue:
    """Per-unit opaque handle assignment over the frozen probe catalogue."""

    def __init__(self, probe_ids, seed: int):
        probe_ids = tuple(probe_ids)
        order = list(probe_ids)
        random.Random(seed).shuffle(order)
        self._handle_of = {pid: f"{HANDLE_PREFIX}{i + 1:02d}"
                           for i, pid in enumerate(order)}
        self._probe_of = {h: pid for pid, h in self._handle_of.items()}
        self.probe_ids = probe_ids

    @property
    def handles(self) -> tuple[str, ...]:
        return tuple(sorted(self._probe_of))

    def handle_of(self, probe_id: str) -> str:
        return self._handle_of[probe_id]

    def probe_of(self, handle: str) -> str:
        return self._probe_of[handle]

    def render(self, descriptions: dict, costs: dict) -> str:
        """The action schema. Identical in shape for every condition."""
        lines = []
        for h in self.handles:
            pid = self._probe_of[h]
            lines.append(f"  - {h}: {descriptions[pid]} (cost: {costs[pid]})")
        return "\n".join(lines)


@dataclass(frozen=True)
class Delivery:
    observations: tuple[Observation, ...]
    delivered_probes: tuple[str, ...]
    policy: DisclosurePolicy
    payment_charged: int
    #: number of actions an adversary who knows the policy cannot rule out as
    #: the evaluator-discriminating one, given only the delivered handle set
    consistency_set_size: int
    n_actions: int

    @property
    def bits_disclosed(self) -> float:
        """log2 reduction in the adversary's candidate set. 0.0 == isolated."""
        if self.consistency_set_size <= 0:
            return float("inf")
        return math.log2(self.n_actions) - math.log2(self.consistency_set_size)

    @property
    def answer_isolated(self) -> bool:
        return self.consistency_set_size == self.n_actions

    def to_dict(self) -> dict:
        return {
            "policy": self.policy.value,
            "handles": [o.handle for o in self.observations],
            "payment_charged": self.payment_charged,
            "consistency_set_size": self.consistency_set_size,
            "n_actions": self.n_actions,
            "bits_disclosed": round(self.bits_disclosed, 4),
            "answer_isolated": self.answer_isolated,
        }


class ObservationChannel:
    """Delivers probe observations without disclosing the selection rationale.

    `discriminating` (from the sealed evaluator metadata) may determine WHICH
    probe executes. It never crosses the model boundary: no rationale, no
    canonical probe id, no annotation.
    """

    def __init__(self, catalogue: ActionCatalogue, costs: dict,
                 policy: DisclosurePolicy = DisclosurePolicy.FULL,
                 k: int = 3, acknowledge_unsafe: bool = False):
        # SINGLE is an evidence-availability policy, not a naming policy. With
        # opaque handles the delivered SET still narrows the adversary's
        # candidate set to 1, so its leakage is declared, not denied. It is a
        # legitimate mechanistic upper-bound arm precisely because the audit's
        # separate defect -- printing the canonical probe NAME -- is closed by
        # ActionCatalogue and PromptLeakGuard. `acknowledge_unsafe` remains
        # required so its selection leakage is always an explicit choice.
        if policy is DisclosurePolicy.SINGLE and not acknowledge_unsafe:
            raise UnsafeDisclosure(
                "DisclosurePolicy.SINGLE narrows the adversary consistency set "
                "to 1. Pass acknowledge_unsafe=True to declare it as a "
                "mechanistic upper-bound arm or to reproduce the legacy defect.")
        self.catalogue = catalogue
        self.costs = costs
        self.policy = policy
        self.k = k

    def deliver(self, observations: dict, discriminating, seed: int) -> Delivery:
        """Select and render the observation set for one delivery event."""
        probes = list(self.catalogue.probe_ids)
        discriminating = [p for p in discriminating if p in observations]

        if self.policy is DisclosurePolicy.NONE:
            return Delivery(observations=(), delivered_probes=(),
                            policy=self.policy, payment_charged=0,
                            consistency_set_size=len(probes),
                            n_actions=len(probes))

        if self.policy is DisclosurePolicy.FULL:
            chosen = [p for p in probes if p in observations]
            consistency = len(probes)
        elif self.policy is DisclosurePolicy.MASKED_K:
            rng = random.Random(seed)
            core = discriminating[:1]
            pool = [p for p in probes if p in observations and p not in core]
            rng.shuffle(pool)
            chosen = core + pool[:max(0, self.k - len(core))]
            chosen = [p for p in probes if p in chosen]
            consistency = len(chosen)
        else:  # SINGLE — audited legacy behaviour, test-only
            chosen = discriminating[:1] or [p for p in probes
                                            if p in observations][:1]
            consistency = 1

        obs = tuple(Observation(self.catalogue.handle_of(p), observations[p])
                    for p in chosen)
        return Delivery(observations=obs, delivered_probes=tuple(chosen),
                        policy=self.policy,
                        payment_charged=sum(self.costs[p] for p in chosen),
                        consistency_set_size=consistency,
                        n_actions=len(self.catalogue.probe_ids))


def leakage_bound(policy: DisclosurePolicy, n_actions: int, k: int = 3) -> dict:
    """Declared, testable bound on selection leakage.

    `loci_consistent` is the number of distinct discriminating probes an
    adversary who knows the policy and sees the delivered handle set cannot
    distinguish between. `bits_disclosed` is the reduction in log2 of that set
    relative to complete ignorance.
    """
    if policy in (DisclosurePolicy.FULL, DisclosurePolicy.NONE):
        consistent = n_actions
    elif policy is DisclosurePolicy.MASKED_K:
        consistent = min(k, n_actions)
    else:
        consistent = 1
    return {
        "policy": policy.value,
        "n_actions": n_actions,
        "loci_consistent_with_delivery": consistent,
        "bits_disclosed": round(math.log2(n_actions) - math.log2(consistent), 4),
        "answer_isolated": consistent == n_actions,
    }


class PromptLeakGuard:
    """Runtime assertion against the RENDERED prompt (not the source code).

    The audited check inspected source strings. This one inspects the bytes
    actually sent to the model, which is where the leak lived.

    Forbidden material:
      * canonical probe identifiers (each maps bijectively to a locus)
      * hidden-evaluator field names and their values
      * oracle selection rationale markers

    Locus NAMES are deliberately NOT forbidden: the frozen vocabulary is
    legitimately present in every prompt, and banning it would make the guard
    unusable rather than strict.
    """

    RATIONALE_MARKERS = (
        "discriminating_probes", "true_locus", "acquisition-hidden-meta",
        "hidden_meta", "gate_infeasible", "answer key", "answer_key",
        "oracle_select", "the discriminating probe", "was selected because",
        "selection rationale",
    )

    def __init__(self, canonical_probe_ids, extra_forbidden=()):
        self.forbidden = tuple(canonical_probe_ids) + tuple(
            self.RATIONALE_MARKERS) + tuple(extra_forbidden)

    def findings(self, rendered: str) -> list[str]:
        low = rendered.lower()
        return [t for t in self.forbidden if t.lower() in low]

    def assert_clean(self, rendered: str, where: str = "prompt") -> None:
        hits = self.findings(rendered)
        if hits:
            raise AnswerLeak(f"answer-bearing material in rendered {where}: "
                             f"{sorted(set(hits))}")

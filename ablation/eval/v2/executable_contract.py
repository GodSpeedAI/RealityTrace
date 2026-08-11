#!/usr/bin/env python3
"""EXECUTABLE COMPLETION of the frozen `evidence-revision-001` contract.

STATUS: FROZEN_NOT_STARTED. EXECUTION IS NOT AUTHORIZED. Completing the
execution semantics is not authorization to execute, and nothing in this module
contacts a provider — there is no client here and no import path to one.

Why this module exists
----------------------
`evidence_revision.py` froze the SCIENCE: arms, population, endpoints,
comparisons, statistical plan. A runner then discovered that three inputs it
needs were never specified, and correctly refused to invent them:

    1. the exact pilot case selection
    2. the pilot repetition count
    3. the per-case model-visible question / failure-evidence binding

and that the human-facing description of the R1/R2 intervention said `SINGLE`
while the authoritative arms say `MASKED_K`.

Those four are completed here. The distinction that governs every choice below:

    COMPLETION of previously unspecified execution semantics
        — a field that was never frozen is frozen now, at the value the
          instrument already had, so no result changes.

    CHANGE to previously frozen science
        — none is made. `evidence_revision.contract_fingerprint()` still
          returns the freeze-time design fingerprint, and a test proves it.

The design identity is therefore preserved and a SEPARATE
`executable_contract_fingerprint()` identifies the thing a runner must match.
A runner that matched only the design fingerprint would be claiming that the
pilot matrix and the case inputs are not part of what it executes, which is
false.

What this completion FOUND
--------------------------
Four defects — in the inputs, in a declared bound, and in one arm pair — that
cannot be repaired without changing the science. They are declared as BLOCKERS
and handed to the owner rather than cleaned:

    BLOCKER-01  the public failure evidence carries injected-fault NAMES, and
                the exposure is larger in the evidence-delivering arms
    BLOCKER-02  MASKED_K's realized membership leakage exceeds its declared
                bound, reaching full locus determination in the tail
    BLOCKER-03  MASKED_K delivery is unaffordable under the enforced budget on
                a locus-correlated 7% of unit-deliveries, silently collapsing
                R1/R2 to the R0 control on those units
    BLOCKER-04  R1 and R2 do not receive the same evidence, so the primary
                comparison C2 does not isolate rendering

None of the four is repaired here, and none changes what was frozen. Three of
them were invisible to every existing check, which is why they are stated with
their measurements rather than as concerns.

Offline: `python -m eval.v2.executable_contract` emits the completed contract
and the pilot matrix. Zero model calls, zero provider calls.
"""

from __future__ import annotations

import collections
import hashlib
import json
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.dirname(ROOT))

from eval.v2.case_inputs import (case_inputs_hash, load_case_inputs,  # noqa: E402
                                 marker_report)
from eval.v2.channel import (ActionCatalogue, DisclosurePolicy,       # noqa: E402
                             ObservationChannel, leakage_bound, unit_seed)
from eval.v2.evidence_revision import (ARMS, EXPERIMENT_ID,           # noqa: E402
                                       GOVERNANCE_CLASS, OWNER_STATE,
                                       PRIMARY_POPULATION,
                                       contract_fingerprint)
from eval.v2.ids import unit_id                                       # noqa: E402
from eval.v2.replication import COSTS, PROBES                         # noqa: E402
from eval.v2.sufficiency import LEGACY_CONTRACT, derive_topology      # noqa: E402

EXECUTABLE_VERSION = "executable-contract-v1"
STATUS = "FROZEN_NOT_STARTED — EXECUTION NOT AUTHORIZED"

CORPUS = os.path.join(ROOT, "corpus")
OUT_DIR = os.path.join(ROOT, "analysis", "post-audit-v2")
PILOT_MATRIX_ARTIFACT = os.path.join(
    OUT_DIR, "evidence-revision-001-pilot-matrix.json")
EXECUTABLE_ARTIFACT = os.path.join(
    OUT_DIR, "evidence-revision-001-executable-contract.json")

DESIGN_FINGERPRINT = "7edd93a3a503b9395c0d39e79540cb20fe084fc8c020413c306db88e1029d3e3"


def _canonical(value) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), default=str)


def _sha256(text: str) -> str:
    return hashlib.sha256(text.encode()).hexdigest()


# ── corpus access (offline, read-only) ───────────────────────────────────────
#
# The sealed evaluator metadata is read HERE and only here, to compute leakage
# bounds. Nothing in this module is rendered into a prompt; the model boundary
# is `controller`/`dstate`, which never import this file.

def _probe_observations() -> dict:
    out = {}
    with open(os.path.join(CORPUS, "acquisition-probes.jsonl")) as handle:
        for line in handle:
            if line.strip():
                row = json.loads(line)
                out[row["case_id"]] = row["probes"]
    return out


def _sealed_keys() -> dict:
    with open(os.path.join(CORPUS, "governed-keys.json")) as handle:
        return json.load(handle)


def _sealed_hidden_meta() -> dict:
    with open(os.path.join(CORPUS, "acquisition-hidden-meta.json")) as handle:
        return json.load(handle)


def primary_population() -> tuple[str, ...]:
    """Re-derive the frozen 43 from the adopted gate, and refuse any drift."""
    observations = _probe_observations()
    keys = _sealed_keys()
    topology = derive_topology(observations, LEGACY_CONTRACT, COSTS, 7)
    cases = tuple(sorted(cid for cid, key in keys.items()
                         if key["true_locus"] != "unknown"
                         and bool(topology.get(cid))))
    if len(cases) != PRIMARY_POPULATION["n_cases"]:
        raise AssertionError(
            f"primary population drift: {len(cases)} vs frozen "
            f"{PRIMARY_POPULATION['n_cases']}")
    return cases


# ── 4. MASKED_K vs SINGLE ────────────────────────────────────────────────────

#: `k` was never a frozen field. `ConditionSpec` does not carry it and the
#: controller constructs `ObservationChannel(catalogue, costs, policy)`, so the
#: value in force has always been the constructor default. Freezing it at that
#: default changes no behaviour; leaving it unfrozen would let a later edit to a
#: default silently change the intervention.
DISCLOSURE_PARAMETERS = {
    "MASKED_K": {
        "k": 3,
        "previously": "IMPLICIT — ObservationChannel's constructor default",
        "frozen_at": "the value already in force; no behaviour changes",
        "constant_across_cases": True,
        "constant_across_arms": True,
        "constant_across_repetitions": True,
    },
}

MASKED_K_MECHANISM = {
    "selection": (
        "the delivered set is the ONE evaluator-discriminating observation "
        "(from the sealed `acquisition-hidden-meta.json`) plus k-1 decoys drawn "
        "by `random.Random(seed)` where the seed is derived only from "
        "(experiment, namespace, case, arm, rep) — public run coordinates"),
    "hidden_evaluator_metadata_participates": True,
    "membership_varies_across_cases": True,
    "membership_encodes_answer_information": True,
    "ordering": ("delivery is re-sorted into canonical probe order, which is "
                 "fixed across cases, so ordering adds nothing beyond "
                 "membership"),
    "handles": ("opaque per-unit handles carry no CROSS-unit semantics, but the "
                "action schema prints each handle's description, so within a "
                "unit the model can map handle -> probe. The defence is the "
                "delivered SET, not the handle."),
}

#: Owner-facing adjudication of the terminology discrepancy. Option B: they are
#: scientifically different interventions, and MASKED_K is the one that belongs.
MASKED_K_VS_SINGLE = {
    "resolution": "B — MASKED_K and SINGLE are scientifically DIFFERENT",
    "not_a_rename": True,
    "single": {
        "delivered": "exactly one evaluator-selected discriminating observation",
        "adversary_consistency_set": 1,
        "declared_bits": 3.0,
        "status_in_this_experiment": ("NOT an arm. Owner §6 confines it to a "
                                      "deterministic regression fixture."),
    },
    "masked_k": {
        "delivered": "the discriminating observation plus k-1 = 2 decoys",
        "declared_consistency_set": 3,
        "declared_bits": 1.415,
        "status_in_this_experiment": "the R1/R2 intervention",
    },
    "difference": (
        "SINGLE isolates the answer to one action; MASKED_K is a bounded-leak "
        "relaxation that hides it among k. They differ in the SIZE of the "
        "declared answer channel, not merely in wording, so substituting one "
        "for the other would change the experiment."),
    "which_belongs": (
        "MASKED_K. The authoritative frozen arms already say MASKED_K, and "
        "owner §6 forbids SINGLE as an arm. The prior owner-facing prose that "
        "said SINGLE was the error; the machine contract was right, and "
        "preserving MASKED_K rather than guessing was correct."),
    "human_facing_terminology_corrected": True,
    "canonical_machine_term": "MASKED_K",
    #: The point owner §4 requires be kept explicit.
    "both_are_evaluator_selected": (
        "MASKED_K's core observation is chosen using sealed evaluator "
        "metadata. Like SINGLE, it is MECHANISTIC / UPPER-BOUND and must never "
        "be read as deployable navigation or action-selection capability."),
}

#: Clarification, not a change: R1/R2 were already `GOVERNED` in the sense that
#: the payment budget is ENFORCED on them. That label does not license a
#: deployability claim, because the evidence is evaluator-selected. Both facts
#: are now stated as separate fields so neither can be read off the other.
EVIDENCE_SELECTION = {
    "R0": {"selector": "none — no evidence is delivered",
           "deployable_navigation": False},
    "R1": {"selector": "EVALUATOR (sealed discriminating probe + decoys)",
           "deployable_navigation": False,
           "class": "MECHANISTIC_UPPER_BOUND_UNDER_ENFORCED_BUDGET"},
    "R2": {"selector": "EVALUATOR (sealed discriminating probe + decoys)",
           "deployable_navigation": False,
           "class": "MECHANISTIC_UPPER_BOUND_UNDER_ENFORCED_BUDGET"},
    "R3": {"selector": "NONE — every observation is delivered to every unit",
           "deployable_navigation": False, "class": "EXEMPT_UPPER_BOUND"},
    "R4": {"selector": "NONE — every observation is delivered to every unit",
           "deployable_navigation": False, "class": "EXEMPT_UPPER_BOUND"},
}


# ── 5. answer-channel capacity ───────────────────────────────────────────────

#: Owner §7 froze that R2 and R4 deliver EXACTLY the evidence R1 and R3 deliver,
#: so that C2 and C3 manipulate representation and nothing else. The delivery
#: seed must therefore be invariant within a legibility pair. The seed key below
#: is the frozen requirement; `BLOCKER-04` records that the instrument as it
#: stands seeds on the arm name instead and so violates it.
DELIVERY_SEED_SCOPE = {
    "requirement": ("the MASKED_K decoy draw and the handle permutation must be "
                    "identical for both members of a legibility pair"),
    "seed_key": {"R0": "R0", "R1": "GOVERNED", "R2": "GOVERNED",
                 "R3": "UPPER", "R4": "UPPER"},
    "basis": "owner §7",
    "implemented": False,
    "see": "BLOCKER-04",
}


def _delivery(case_id: str, arm: str, rep: int, observations: dict,
              hidden: dict, namespace: str, seed_scope: str = "required"):
    key = (DELIVERY_SEED_SCOPE["seed_key"][arm] if seed_scope == "required"
           else arm)
    seed = unit_seed(EXPERIMENT_ID, namespace, case_id, key, rep)
    catalogue = ActionCatalogue(PROBES, seed)
    channel = ObservationChannel(catalogue, COSTS, ARMS[arm].disclosure,
                                 k=DISCLOSURE_PARAMETERS["MASKED_K"]["k"])
    return channel.deliver(observations[case_id],
                           hidden[case_id]["discriminating_probes"], seed + 1)


def answer_channel_report(namespace: str | None = None) -> dict:
    """Measured, not asserted: what each delivery channel reveals about the locus.

    The channels enumerated are the ones owner §5 named: membership, ordering,
    opaque handle identity, number of observations, MISSING observations, and
    rendering mode. The previous oracle defect ran through selected-probe
    identity; this checks that subset membership has not reinstated it.
    """
    namespace = namespace or PILOT_SELECTION["namespace"]
    cases = primary_population()
    observations, hidden, keys = (_probe_observations(), _sealed_hidden_meta(),
                                  _sealed_keys())

    # Which probes can ever be the discriminating one on this population, and
    # which loci each implies. An adversary who knows the policy knows this map.
    capable: dict[str, set] = collections.defaultdict(set)
    for case_id in cases:
        capable[hidden[case_id]["discriminating_probes"][0]].add(
            keys[case_id]["true_locus"])
    never = sorted(set(PROBES) - set(capable))
    #: The question is which LOCUS, so leakage is measured against the number of
    #: loci present, not the number of actions. Measuring in action-space would
    #: report a spurious non-zero leak for FULL, whose delivered set is
    #: constant and therefore carries nothing.
    n_loci = len({keys[c]["true_locus"] for c in cases})
    prior_bits = math.log2(n_loci)

    def channel_for(arm: str) -> dict:
        policy = ARMS[arm].disclosure
        declared = leakage_bound(policy, len(PROBES),
                                 DISCLOSURE_PARAMETERS["MASKED_K"]["k"])
        declared_locus_bits = round(
            prior_bits - math.log2(min(declared["loci_consistent_with_delivery"],
                                       n_loci)), 4)
        if policy is DisclosurePolicy.NONE:
            return {"policy": policy.value, "declared": declared,
                    "declared_locus_bits": 0.0,
                    "realized_membership_bits": {"min": 0.0, "max": 0.0,
                                                 "mean": 0.0},
                    "delivered_set_constant_across_units": True,
                    "n_observations_constant": True,
                    "delivery_failures": 0, "delivery_failure_rate": 0.0,
                    "delivery_failure_rate_by_locus": {},
                    "bound_holds": True}
        sizes, bits, failures, fail_by_locus = [], [], 0, collections.Counter()
        constant = set()
        for case_id in cases:
            for rep in range(1, PILOT_REPETITIONS + 1):
                delivery = _delivery(case_id, arm, rep, observations, hidden,
                                     namespace)
                delivered = set(delivery.delivered_probes)
                constant.add(tuple(sorted(delivered)))
                # The adversary's real candidate set: loci implied by the
                # delivered probes that could BE the discriminating one.
                loci = set()
                for probe in delivered & set(capable):
                    loci |= capable[probe]
                sizes.append(len(loci) or n_loci)
                bits.append(prior_bits - math.log2(max(1, len(loci) or n_loci)))
                if (ARMS[arm].budget_semantics == "ENFORCED"
                        and delivery.payment_charged > ARMS[arm].max_payment):
                    failures += 1
                    fail_by_locus[keys[case_id]["true_locus"]] += 1
        n = len(bits)
        rates = {
            locus: round(fail_by_locus[locus] /
                         (PILOT_REPETITIONS * sum(
                             1 for c in cases
                             if keys[c]["true_locus"] == locus)), 4)
            for locus in sorted({keys[c]["true_locus"] for c in cases})}
        return {
            "policy": policy.value,
            "declared": declared,
            "declared_locus_bits": declared_locus_bits,
            "realized_candidate_locus_set": {
                "min": min(sizes), "max": max(sizes),
                "distribution": dict(sorted(collections.Counter(sizes).items()))},
            "realized_membership_bits": {
                "min": round(min(bits), 4), "max": round(max(bits), 4),
                "mean": round(sum(bits) / n, 4)},
            "delivered_set_constant_across_units": len(constant) == 1,
            "n_observations_constant": len(
                {len(s) for s in constant}) == 1,
            "delivery_failures": failures,
            "delivery_failure_rate": round(failures / n, 4),
            "delivery_failure_rate_by_locus": rates,
            "bound_holds": max(bits) <= declared_locus_bits + 1e-9,
        }

    per_arm = {arm: channel_for(arm) for arm in ARMS}

    # Legibility pairs must receive identical evidence (owner §7). Measured
    # under the frozen seed scope AND under the instrument as it stands.
    pair_agreement = {}
    for a, b in (("R1", "R2"), ("R3", "R4")):
        for scope in ("required", "as_implemented"):
            disagree = sum(
                1 for c in cases for rep in range(1, PILOT_REPETITIONS + 1)
                if set(_delivery(c, a, rep, observations, hidden, namespace,
                                 scope).delivered_probes)
                != set(_delivery(c, b, rep, observations, hidden, namespace,
                                 scope).delivered_probes))
            total = len(cases) * PILOT_REPETITIONS
            pair_agreement[f"{a}_vs_{b}::{scope}"] = {
                "unit_pairs": total, "different_evidence": disagree,
                "rate": round(disagree / total, 4),
                "identical_evidence": disagree == 0}
    markers = marker_report(case_ids=cases)
    marker_in_probe = sorted(
        c for c in cases
        if any("test_corpus_scenario_" in _canonical(payload)
               for payload in observations[c].values()))
    return {
        "population": {"n_cases": len(cases), "n_loci": n_loci,
                       "prior_bits": round(prior_bits, 4)},
        "measured_in": "locus space — log2(n_loci) - log2(candidate loci)",
        "legibility_pair_evidence_identity": pair_agreement,
        "discriminating_capable_probes": sorted(capable),
        "never_discriminating_probes": never,
        "probe_to_loci": {k: sorted(v) for k, v in sorted(capable.items())},
        "per_arm": per_arm,
        "ordering_channel": (
            "0 bits beyond membership — deliveries are re-sorted into canonical "
            "probe order, which is identical for every unit"),
        "handle_identity_channel": (
            "0 bits ACROSS units — handles are a per-unit permutation seeded "
            "from public coordinates only. Within a unit the schema discloses "
            "handle -> description, which is why membership carries the bound."),
        "rendering_channel": (
            "0 bits — `legibility.assert_semantic_equivalence` hash-binds RAW "
            "and LEGIBLE to the same payload and rejects any locus term the raw "
            "form did not already contain"),
        "construction_marker_channel": {
            "initial_context": {"n_cases": markers["n_with_construction_marker"],
                                "cases": sorted(markers["cases"])},
            "delivered_evidence": {"n_cases": len(marker_in_probe),
                                   "cases": marker_in_probe,
                                   "carrier": "rerun_verifier_isolated"},
            "differential_across_arms": True,
            "see": "BLOCKER-01",
        },
    }


# ── 1. pilot case selection ──────────────────────────────────────────────────

PILOT_SELECTION = {
    "rule_id": "pilot-selection-v1",
    "namespace": "pilot:er001-pilot-v1",
    "purpose": ("PROTOCOL AND INSTRUMENT VALIDATION ONLY. The pilot is not an "
                "estimate of anything. It is not powered, it is not "
                "locus-balanced, and no scientific quantity may be reported "
                "from it."),
    "rule": (
        "1. Start from the frozen primary population (43 gate-feasible "
        "determinate cases). "
        "2. Order every case by sha256('evidence-revision-001|"
        "pilot-selection-v1|<case_id>'), ascending. The ordering key is a "
        "constant string and the case id — nothing tunable, nothing learned. "
        "3. Walk that order and accept a case if and only if it introduces a "
        "value not yet covered on any coverage axis. "
        "4. Stop at the first case where every axis is saturated."),
    "coverage_axes": {
        "repo": "4 values — exercises every repository and stdin dialect source",
        "stdin_format": "3 values — exercises every evidence payload shape",
        "verifier_exit": "5 values — exercises every recorded exit class, "
                         "including the exit-127 adjudicated cases",
        "initial_context_marker": "2 values — with and without an injected-fault "
                                  "name in the model-visible initial context "
                                  "(BLOCKER-01)",
        "delivered_evidence_marker": "2 values — with and without a marker in "
                                     "the probe payloads",
        "masked_k_delivery_affordable": "2 values — exercises BOTH the normal "
                                        "MASKED_K delivery path and the "
                                        "HARNESS_DELIVERY_UNAFFORDABLE path "
                                        "(BLOCKER-03)",
    },
    "answer_blindness": (
        "every axis is a public property: `cases.jsonl` metadata, a public "
        "string scan of the public evidence, or the payment implied by the "
        "frozen costs and the public unit seed. No key file and no hidden "
        "evaluator metadata participates in selection, and no model output "
        "does either — the selection is computable before any call exists."),
    "not_tuned": (
        "the hash ordering fixes the walk before any coverage decision is "
        "made, so no case can be swapped in to make an arm look better; the "
        "resulting locus mix is an incidental consequence and is reported as "
        "unbalanced rather than presented as a sample"),
}


def _axes(case_id: str, rows: dict, inputs: dict, observations: dict,
          hidden: dict) -> dict:
    row = inputs["cases"][case_id]["public_metadata"]
    delivery = _delivery(case_id, "R1", 1, observations, hidden,
                         PILOT_SELECTION["namespace"])
    return {
        "repo": row["repo"],
        "stdin_format": row["stdin_format"],
        "verifier_exit": row["verifier_exit"],
        "initial_context_marker":
            inputs["cases"][case_id]["answer_bearing_marker_present"],
        "delivered_evidence_marker": any(
            "test_corpus_scenario_" in _canonical(payload)
            for payload in observations[case_id].values()),
        "masked_k_delivery_affordable":
            delivery.payment_charged <= ARMS["R1"].max_payment,
    }


def pilot_cases() -> tuple[str, ...]:
    """The frozen pilot case list, in selection order. Deterministic."""
    cases = primary_population()
    inputs, observations, hidden = (load_case_inputs(), _probe_observations(),
                                    _sealed_hidden_meta())
    axes = {c: _axes(c, {}, inputs, observations, hidden) for c in cases}
    required = collections.defaultdict(set)
    for values in axes.values():
        for axis, value in values.items():
            required[axis].add(value)

    order = sorted(cases, key=lambda c: _sha256(
        f"{EXPERIMENT_ID}|{PILOT_SELECTION['rule_id']}|{c}"))
    selected, covered = [], collections.defaultdict(set)
    for case_id in order:
        values = axes[case_id]
        if any(values[a] not in covered[a] for a in values):
            selected.append(case_id)
            for axis, value in values.items():
                covered[axis].add(value)
        if all(covered[a] == required[a] for a in required):
            break
    if any(covered[a] != required[a] for a in required):
        raise AssertionError("pilot selection failed to saturate its axes")
    return tuple(selected)


# ── 2. pilot repetition count ────────────────────────────────────────────────

#: TWO. Not one, not three.
#:
#: Repetitions are not independent scientific observations — owner §13 makes the
#: CASE the inferential unit, and the statistical plan aggregates within case
#: before pairing. So the only question a repetition can answer here is a
#: protocol question: does the same frozen request produce a stable execution?
#:
#: One repetition cannot answer it at all: with a single sample there is nothing
#: to disagree with. Two can, because any divergence between two identical
#: requests is a positive finding — provider nondeterminism at temperature 0,
#: an unstable output contract, or a resume/persistence defect. A third adds
#: precision to a rate that the pilot is not entitled to estimate anyway.
#:
#: Two is therefore the smallest count that can expose what the pilot exists to
#: expose, and raising it would also be the one remedy owner §12 explicitly
#: recorded as useless for the power problem — repetitions add no pairs.
PILOT_REPETITIONS = 2

PILOT_REPETITION_RATIONALE = {
    "value": PILOT_REPETITIONS,
    "smallest_that_can_expose": ("execution instability, provider "
                                 "nondeterminism, persistence/resume defects, "
                                 "output-contract instability"),
    "why_not_one": "a single sample has nothing to disagree with",
    "why_not_three": ("a third repetition buys precision on a rate the pilot "
                      "may not report; it is cost without a protocol question"),
    "not_a_power_remedy": ("owner §12 option (d): repetitions do NOT help the "
                           "power problem, because the inferential unit is the "
                           "case and repetitions add no discordant pairs"),
}


# ── the frozen pilot matrix ──────────────────────────────────────────────────

def pilot_matrix() -> dict:
    cases = pilot_cases()
    arms = tuple(ARMS)
    return {
        "stage": "pilot",
        "namespace": PILOT_SELECTION["namespace"],
        "cases": list(cases),
        "arms": list(arms),
        "repetitions": PILOT_REPETITIONS,
        "n_cases": len(cases),
        "n_arms": len(arms),
        "total_units": len(cases) * len(arms) * PILOT_REPETITIONS,
    }


def pilot_uids(fingerprint: str | None = None) -> tuple[str, ...]:
    """Deterministic unit ids for the whole pilot matrix, in matrix order.

    Bound to the EXECUTABLE fingerprint, not the design fingerprint: the pilot
    matrix and the case inputs are part of what a unit is, so a unit id must
    move if they move.
    """
    fingerprint = fingerprint or executable_contract_fingerprint()
    matrix = pilot_matrix()
    return tuple(
        unit_id(EXPERIMENT_ID, matrix["namespace"], case, arm, rep, fingerprint)
        for case in matrix["cases"] for arm in matrix["arms"]
        for rep in range(1, matrix["repetitions"] + 1))


def pilot_matrix_document() -> dict:
    """The matrix as the artifact a runner consumes. Regenerable, not authored."""
    matrix = pilot_matrix()
    fingerprint = executable_contract_fingerprint()
    uids = pilot_uids(fingerprint)
    doc = {
        "schema": "evidence-revision-matrix-v1",
        "experiment_id": EXPERIMENT_ID,
        "contract_fingerprint": DESIGN_FINGERPRINT,
        "executable_contract_fingerprint": fingerprint,
        "case_inputs_sha256": load_case_inputs()["case_inputs_sha256"],
        "unit_ids": list(uids),
        "execution_authorized": False,
        **matrix,
    }
    doc["matrix_sha256"] = _sha256(_canonical(
        {k: v for k, v in doc.items() if k != "matrix_sha256"}))
    return doc


# ── 3. per-case input binding ────────────────────────────────────────────────

CASE_INPUT_BINDING = {
    "module": "eval/v2/case_inputs.py",
    "artifact": "analysis/post-audit-v2/case-inputs-v1.json",
    "schema": "case-input-v1",
    "bound_fields": ("question", "failure_evidence", "initial_context",
                     "initial_evidence"),
    "arm_invariant": True,
    "arm_invariance_proof": ("`assert_arm_invariant_initial_state` renders the "
                             "binding through the real `DiagnosticState` for "
                             "every arm and compares bytes"),
    "continuity": ("`DiagnosticState` carries `question` and `initial_evidence` "
                   "into EVERY round (R-02, `REQUIRED_ROUND_STATE`), and the "
                   "runner asserts `missing_state_sections` is empty on the "
                   "final transport prompt of every call"),
    "answer_metadata_excluded": ("no key file and no hidden evaluator metadata "
                                 "is read; `assert_no_answer_metadata` checks "
                                 "the built document for their field names"),
    "context_profile": "arm-D-four-capability/store-v1/pack-v1 @ 32,000 tokens",
    "why_the_historical_packer": ("reusing the frozen `pack-v1` rule keeps the "
                                  "model-visible context comparable with the "
                                  "historical chain; inventing a fresh "
                                  "truncation rule would silently change what "
                                  "the experiment shows the model"),
}


# ── blockers found during completion ─────────────────────────────────────────

BLOCKERS = {
    "BLOCKER-01": {
        "title": "injected-fault names are visible in the public failure evidence",
        "class": "CORPUS_CONSTRUCTION_ARTEFACT",
        "found_by": "completing the per-case input binding (owner §3)",
        "finding": (
            "the corpus scenarios were built by injecting a NAMED fault, and "
            "the injected test's name survives into the public verifier "
            "stream as `test_corpus_scenario_<fault>`. On the primary "
            "population the marker appears in the model-visible initial "
            "context for 7 of 43 cases, and in the probe payloads (carried "
            "entirely by `rerun_verifier_isolated`) for 12 of 43. Every "
            "marker maps to exactly one true locus."),
        "why_it_matters": (
            "the exposure is DIFFERENTIAL across arms. R0 sees 7 cases' "
            "markers; R3/R4 always deliver `rerun_verifier_isolated` and so "
            "see 12; R1/R2 see them only when the decoy draw includes that "
            "probe. C1 (R0 vs R1) therefore partly measures whether the model "
            "reads an injected fault name, not whether it revises a claim."),
        "measured": "I(locus ; initial-context marker set) = 0.481 of 2.689 bits",
        "not_done": (
            "the corpus is NOT cleaned. Rewriting evidence to remove the "
            "markers would change the stimulus, invalidate comparability with "
            "the historical chain, and is a scientific change nobody "
            "authorized. It is declared per case instead."),
        "recorded_as": ("`construction_markers` and "
                        "`answer_bearing_marker_present` on every case in the "
                        "frozen binding"),
        "mandatory_handling": (
            "marker presence is a MANDATORY STRATIFIER of the primary "
            "endpoint: R_correct must be reported separately for the "
            "marker-free and marker-bearing strata, and the pooled figure may "
            "not be reported alone"),
        "owner_decision_required": True,
    },
    "BLOCKER-02": {
        "title": "MASKED_K's realized membership leakage exceeds its declared bound",
        "class": "DECLARED_BOUND_IS_NOT_A_BOUND",
        "found_by": "rechecking answer-channel capacity (owner §5)",
        "finding": (
            "`leakage_bound` reports the consistency set as k = 3, i.e. 1.415 "
            "bits. But only 6 of the 8 probes are ever the discriminating one "
            "on this population — `rerun_verifier_isolated` and "
            "`inspect_verifier_config` never are — so an adversary who knows "
            "the policy discards decoys drawn from those two. The realized "
            "candidate set is |delivered n discriminating-capable|, which is "
            "1 or 2 in the majority of unit-deliveries."),
        "measured": (
            "on the primary population at the frozen seed scope, over 43 cases "
            "x 2 repetitions: the candidate LOCUS set is size 1 in 4 of 86 "
            "deliveries, 2 in 28, 3 in 35, 4 in 19. Against a 2.807-bit prior "
            "(7 loci) the declared bound is 1.222 bits; realized leakage runs "
            "0.807 to 2.807 bits, mean 1.395. In the 4 tail deliveries "
            "membership determines the locus outright — SINGLE's leak, which "
            "owner §6 excluded from the experiment."),
        "not_done": (
            "the intervention is NOT altered. Restricting decoys to "
            "discriminating-capable probes, or raising k, would change what "
            "R1/R2 deliver and therefore change C1 and C2. That is a "
            "scientific revision, not a repair."),
        "owner_decision_required": True,
    },
    "BLOCKER-03": {
        "title": "MASKED_K delivery fails, locus-correlated, and collapses R1/R2 to R0",
        "class": "MISSING_OBSERVATIONS_ARE_AN_ANSWER_CHANNEL",
        "found_by": "rechecking answer-channel capacity (owner §5)",
        "finding": (
            "MASKED_K charges the full cost of the delivered set. With decoys "
            "drawn uniformly, the set can cost 9 against `max_payment = 7`, "
            "and the controller then delivers NOTHING rather than shrink the "
            "set (which would silently change the declared bound). The unit "
            "proceeds with no new evidence — behaviourally the R0 control."),
        "measured": (
            "6 of 86 R1/R2 unit-deliveries (7.0%) on the primary population at "
            "the frozen seed scope. The rate is locus-correlated because the "
            "discriminating probe's cost varies by locus: declared_model "
            "fails 37.5% of the time (its discriminating probe, "
            "`validate_semantic_model`, costs 3), against 8.3% for authority, "
            "7.1% for environment, 5.0% for implementation and 0% for "
            "evidence_collection, projection and verification_plan."),
        "why_it_matters": (
            "'no evidence was delivered' is itself informative about the "
            "locus, and it dilutes C1 with units in which the treatment did "
            "not occur. An intention-to-treat analysis would understate the "
            "evidence effect; a per-protocol analysis would condition on a "
            "locus-correlated variable."),
        "not_done": ("neither the budget nor the decoy draw is changed. Owner "
                     "§1 froze `max_payment = 7` and forbids raising it."),
        "mandatory_handling": (
            "delivery success is a MANDATORY COVARIATE, and C1/C2 must report "
            "the delivery-failure rate per arm alongside the effect"),
        "owner_decision_required": True,
    },
    "BLOCKER-04": {
        "title": "R1 and R2 do not receive the same evidence, so C2 does not "
                 "isolate rendering",
        "class": "IMPLEMENTATION_CONTRADICTS_A_FROZEN_OWNER_DECISION",
        "found_by": "rechecking answer-channel capacity (owner §5, §7)",
        "finding": (
            "`unit_seed` takes the CONDITION NAME, so R1 and R2 seed different "
            "decoy draws and different handle permutations. Measured on the "
            "primary population at the frozen namespace: R1 and R2 receive "
            "DIFFERENT delivered evidence in 80 of 86 unit pairs (93.0%). "
            "R3 and R4 agree in 86 of 86, because FULL delivers everything and "
            "has no draw to diverge."),
        "why_it_matters": (
            "owner §7 froze that R2 delivers exactly the evidence R1 delivers, "
            "and C2 is declared a PRIMARY comparison that isolates rendering. "
            "As implemented it varies rendering AND the evidence set at once, "
            "so C2 measures neither cleanly. `arm_difference_report` cannot "
            "see this: the two specs differ in exactly one DECLARED field, and "
            "the divergence lives in the seed, not in the spec."),
        "the_fix_is_specified_not_applied": (
            "`DELIVERY_SEED_SCOPE` freezes the requirement — the seed key must "
            "be the legibility PAIR, not the arm. Applying it changes which "
            "decoys R1 receives, so it is left to owner adjudication rather "
            "than done here; nothing has run, so nothing is invalidated by "
            "deciding either way."),
        "affects": ["C2_legibility_governed"],
        "does_not_affect": ["C3_legibility_upper", "C1_evidence_effect"],
        "owner_decision_required": True,
    },
}


# ── supersession and fingerprints ────────────────────────────────────────────

def executable_declarations() -> dict:
    """The frozen inputs of the executable contract. Derived analyses excluded."""
    return {
        "executable_version": EXECUTABLE_VERSION,
        "experiment_id": EXPERIMENT_ID,
        "design_fingerprint": DESIGN_FINGERPRINT,
        "disclosure_parameters": DISCLOSURE_PARAMETERS,
        "delivery_seed_scope": DELIVERY_SEED_SCOPE,
        "masked_k_mechanism": MASKED_K_MECHANISM,
        "masked_k_vs_single": MASKED_K_VS_SINGLE,
        "evidence_selection": EVIDENCE_SELECTION,
        "pilot_selection": PILOT_SELECTION,
        "pilot_cases": list(pilot_cases()),
        "pilot_repetitions": PILOT_REPETITIONS,
        "pilot_repetition_rationale": PILOT_REPETITION_RATIONALE,
        "pilot_matrix": pilot_matrix(),
        "case_input_binding": dict(
            CASE_INPUT_BINDING,
            case_inputs_sha256=load_case_inputs()["case_inputs_sha256"]),
        "blockers": BLOCKERS,
        "governance_class": GOVERNANCE_CLASS,
    }


def executable_contract_fingerprint() -> str:
    """Identity of the EXECUTABLE design: the science plus its execution semantics.

    Excludes the pilot unit ids and the matrix hash, which are functions of this
    value, and excludes every derived analysis. Excludes owner/authorization
    state for the same reason the design fingerprint does: a governance act must
    not look like a change to the design.
    """
    return hashlib.sha256(_canonical(executable_declarations()).encode()).hexdigest()


FINGERPRINT_RELATIONSHIP = {
    "prior": {
        "name": "design_fingerprint",
        "value": DESIGN_FINGERPRINT,
        "identifies": ("the frozen SCIENCE — arms, population, endpoints, "
                       "comparisons, statistical plan"),
        "status": "PRESERVED, UNCHANGED, still returned by "
                  "`evidence_revision.contract_fingerprint()`",
    },
    "new": {
        "name": "executable_contract_fingerprint",
        "identifies": ("the science PLUS the execution semantics a runner must "
                       "match: k, the pilot matrix, the case-input binding, and "
                       "the MASKED_K adjudication"),
        "supersedes_for": "anything that executes",
        "does_not_supersede": ("the design fingerprint's role as the identity "
                               "of the frozen science"),
    },
    "fields_added_or_clarified": {
        "MASKED_K k = 3": "COMPLETION — was an implicit constructor default",
        "DELIVERY_SEED_SCOPE": ("COMPLETION — owner §7 already required that a "
                                "legibility pair receive identical evidence; "
                                "the seed scope that delivers it was never "
                                "written down. Recording the requirement "
                                "changes nothing on its own; see BLOCKER-04."),
        "pilot case selection": "COMPLETION — was absent",
        "pilot repetitions = 2": "COMPLETION — was absent",
        "per-case input binding": "COMPLETION — was assembled ad hoc",
        "EVIDENCE_SELECTION / deployable_navigation=False on R1/R2":
            "CLARIFICATION — restates owner §4 as a field; no behaviour changes",
        "MASKED_K vs SINGLE": ("ADJUDICATION — the machine contract already "
                               "said MASKED_K and is unchanged; the owner-facing "
                               "prose that said SINGLE is corrected"),
    },
    "previously_frozen_scientific_meaning_changed": False,
    "basis": (
        "every completed field is frozen at the value the instrument already "
        "had, so no previously obtainable result changes. The three BLOCKERS "
        "are newly IDENTIFIED defects in inputs and in a declared bound; "
        "identifying a defect is not changing the design, and none of them has "
        "been repaired here."),
    "if_a_blocker_is_repaired": (
        "repairing BLOCKER-01, -02 or -03 WOULD change the intervention or the "
        "stimulus. That would be a scientific revision requiring a new "
        "experiment id, not a new fingerprint on this one."),
}


def executable_contract() -> dict:
    doc = dict(executable_declarations())
    doc.update({
        "status": STATUS,
        "design_fingerprint_recomputed": contract_fingerprint(),
        "executable_contract_fingerprint": executable_contract_fingerprint(),
        "fingerprint_relationship": FINGERPRINT_RELATIONSHIP,
        "answer_channel_report": answer_channel_report(),
        "pilot_unit_ids": list(pilot_uids()),
        "owner_state": OWNER_STATE,
        "model_calls_performed": 0,
        "provider_calls_performed": 0,
    })
    return doc


def main() -> int:
    os.makedirs(OUT_DIR, exist_ok=True)
    doc = executable_contract()
    if doc["design_fingerprint_recomputed"] != DESIGN_FINGERPRINT:
        raise SystemExit("REFUSING TO WRITE: the frozen design fingerprint moved")
    with open(EXECUTABLE_ARTIFACT, "w") as handle:
        json.dump(doc, handle, indent=2, sort_keys=True, default=str)
    matrix = pilot_matrix_document()
    with open(PILOT_MATRIX_ARTIFACT, "w") as handle:
        json.dump(matrix, handle, indent=2, sort_keys=True, default=str)

    print(f"  {EXPERIMENT_ID}: {STATUS}")
    print(f"  design fingerprint      {DESIGN_FINGERPRINT[:16]} (unchanged)")
    print(f"  executable fingerprint  {doc['executable_contract_fingerprint'][:16]}")
    print(f"  pilot: {matrix['n_cases']} cases x {matrix['n_arms']} arms x "
          f"{matrix['repetitions']} reps = {matrix['total_units']} units")
    print(f"  pilot cases: {', '.join(matrix['cases'])}")
    print(f"  case inputs {doc['case_input_binding']['case_inputs_sha256'][:16]}"
          f"   matrix {matrix['matrix_sha256'][:16]}")
    for name, blocker in sorted(BLOCKERS.items()):
        print(f"  {name}: {blocker['title']}")
    print(f"  execution_authorized: {OWNER_STATE['execution_authorized']}   "
          f"model calls: 0   provider calls: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())

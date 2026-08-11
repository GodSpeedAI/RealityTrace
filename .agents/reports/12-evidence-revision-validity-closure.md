# evidence-revision-001 revision-002 validity closure and runner readiness

## Verdict

All four owner-authorized validity blockers are closed offline:

- **BLOCKER-01 CLOSED** — construction identifiers are neutralized before the shared semantic observation reaches either renderer.
- **BLOCKER-02 CLOSED** — every MASKED_K intervention realizes exactly three distinct candidate observations; no singleton-equivalent membership remains.
- **BLOCKER-03 CLOSED** — exhaustive target-plus-two-decoy enumeration proves every eligible case feasible under payment 7; selected costs are 3–5 and missingness is zero.
- **BLOCKER-04 CLOSED** — paired evidence-policy seeds make R1/R2 and R3/R4 evidence membership, order, handles, and semantic payload byte/hash-identical.

Execution remains disabled: `FROZEN_NOT_STARTED`, `execution_authorized=false`, `additional_provider_calls_authorized=false`. No model, provider, preflight, pilot, full, or smoke call was made.

## New identities and lineage

- Design fingerprint: `3031af33611c9ae8ab0fd0bba22f0abc35147d4fe564e0e3623a174d0dfe28d9`
- Supersedes design: `7edd93a3a503b9395c0d39e79540cb20fe084fc8c020413c306db88e1029d3e3`
- Executable fingerprint: `12eb26d72f74709a827059c2a6ccb693ef956a6d5aa7ca2bf6f292b645da010e`
- Supersedes executable: `557c0dcd9fe3b354e754074dd8d3e61a635a0bec638c138f592b451fca1083cd`
- Case-input v2 hash: `20fbaf97235b9bb4144582ae0bdf1527bb10a12394da38ee09931c7af4f91e1b`
- Pilot matrix v2 hash: `effd44ec461b8624e4c498b80f40884f1af0d30296b9062bda3a1f090d85a9e3`

Predecessor modules and JSON artifacts remain unchanged and continue reproducing their original fingerprints/hashes.

## Marker channel

The locus/key/model-blind transform replaces only full `test_corpus_scenario_*` injected-test identifiers with the stable generic syntactic identifier `test_observed_failure`. It leaves exits, traces, commands, paths, assertions, summaries, and structured behavioral fields intact. It applies before RAW/LEGIBLE rendering.

- source initial-input markers: 16/60 cases overall, 7/43 primary;
- source probe markers: 29/60 payload artifacts overall, 12/43 primary, all in `rerun_verifier_isolated` in the primary set;
- model-visible construction markers after projection: 0;
- `PRIMARY_MARKER_CONTAMINATED`: 0 cases;
- manifest entries with source/neutral hashes, transformation, class, replacement, blindness proof, and preservation rationale: 45.

`source_marker_present`, `marker_neutralization_applied`, and `marker_type` are persisted only in result metadata and cannot enter `DiagnosticState` or transport prompts.

## MASKED_K proof

The frozen policy enumerates the complete 21 target-plus-two-decoy combinations from the eight-probe public universe for every case. The deterministic selected set uses the frozen six direct candidate observations, the cheapest feasible cost tier, and a public paired seed over experiment, case, repetition, policy, and the permitted target identity. It never reads true locus, keys, model output, arm, or rendering.

- K: 3
- evaluated deliveries: 43 cases × 2 repetitions = 86
- selected cost range: 3–5
- realized K distribution: {3: 86}
- theoretical membership leakage: 1.222392 bits
- realized mean membership leakage: 1.164480 bits
- realized maximum membership leakage: 1.222392 bits
- membership uniquely determines locus: 0
- singleton-equivalent: 0
- unaffordable: 0
- missing evidence: 0
- FULL membership leakage: 0 bits
- NONE membership leakage: 0 bits

R1/R2 remain `deployable_navigation=false` and `MECHANISTIC_UPPER_BOUND_UNDER_ENFORCED_BUDGET`; no deployable navigation claim is made.

## Pair proofs

Across 86 full-population matched pairs per comparison:

- R1/R2 evidence-set hashes equal: **PASS**
- R1/R2 semantic-evidence hashes equal: **PASS**
- R1/R2 rendered hashes differ: **PASS**
- R3/R4 evidence-set hashes equal: **PASS**
- R3/R4 semantic-evidence hashes equal: **PASS**
- R3/R4 rendered hashes differ: **PASS**
- membership mismatches: 0

The prior arm-keyed seed remains covered by a mutation regression that demonstrates it would break the required pair.

## Population and pilot

Primary population remains the unchanged 43-case gate-feasible determinate population. No gate-infeasible case was added and the 10pp threshold/power state is unchanged.

`pilot-selection-v1` was rerun on the marker-neutral, uniformly affordable public view:

1. `cep-07`
2. `df-11`
3. `ag-04`
4. `sr-10`
5. `df-06`
6. `cep-05`

Pilot: 6 cases × 5 arms × 2 repetitions = **60 units**. Selection is deterministic and public-only. Pilot/full namespaces and UIDs are disjoint.

## Runner conformance

`evidence_revision_run.py` now:

- removes `FROZEN_CONTRACT_GAP` for pilot inspection;
- validates the new executable fingerprint;
- loads the canonical pilot matrix hash from the executable contract, never `OWNER_STATE`;
- binds the v2 pilot namespace, two repetitions, 60 executable-fingerprint UIDs, and case-input v2 hash;
- initializes `DiagnosticState` from frozen v2 question/evidence bytes;
- passes marker-neutral probe payloads to the controller before rendering;
- uses paired evidence-policy seeds and exact-K enforcement;
- persists required non-prompt marker, cost, K, evidence-hash, case-input, and leakage-policy metadata;
- remains centrally fail-closed before live transport construction.

## Offline validation

Command:

```bash
cd /home/sprime01/projects/sxr/ablation && uv run --python 3.12 --with pytest python -m pytest -q
```

Result: **351 collected, 351 passed, 0 failed, 0 skipped**.

The 10 revised artifacts also reproduce canonically from the offline generator. Mutation tests reject a reintroduced marker, arm-keyed seed, singleton MASKED_K, over-budget MASKED_K, missing evidence, and paired-membership mismatch.

- `model_calls_performed = 0`
- `provider_calls_performed = 0`
- `execution_authorized = false`

## Commits

- `1a40669d9e337824132c4b5683f4289bce0d8fa3`
- `7e7cf516335c986cbea3b67e958931770bdd73b5`
- `f2196fc4c8df86ddff9fe051970244435e83210a`
- `3f523bb634da5f009c3ed3797940f6d70f4070ed`
- `cb6dbbdf8b3495e2b450015ba662b210bf3bc23a`
- `66c67226673be1eefe53f475b9729df1d67ea5f0`
- `2da4d90cb4c9dbd147581c831a56050168fdac2d`
- `f2cc1bc4e3daa7872102efbb834d24c9ae67d74f`

# evidence-revision-002 successor freeze report

## Disposition

**SUCCESSOR_FROZEN** — offline only. Execution is not authorized. No release token or marker exists.

## Why a new experiment ID

The owner-selected `matched_evidence_use` disposition changes a frozen endpoint field. The predecessor contract requires a new experiment ID rather than mutation of `evidence-revision-001`; the successor is therefore `evidence-revision-002`.

## Predecessor lineage

- Experiment: `evidence-revision-001`
- Revision: `revision-002`
- Pilot: `evidence-revision-001-revision-002-pilot-20260811T221724Z`
- Pilot manifest: `ae2827c1ea32c2860e53f19b763955908860865266791932f1454252ffeb2a13`
- Pilot disposition: `VALID_PREDECESSOR_INSTRUMENT_VALIDATION`
- Predecessor design: `3031af33611c9ae8ab0fd0bba22f0abc35147d4fe564e0e3623a174d0dfe28d9`
- Predecessor executable: `12eb26d72f74709a827059c2a6ccb693ef956a6d5aa7ca2bf6f292b645da010e`
- Predecessor runner implementation: `56f4463c2b228bb31f2c142d15e5431b8bd345114bd3707e14d386b1edfcf066`

The pilot may establish instrument/runtime viability only. It is excluded from the successor FULL matrix and may not be confirmatory evidence for the successor.

## Exact semantic delta

1. `revision_rate` is made explicit from pre-pilot typed semantics; this is a semantic closure, not a new estimand.
2. `matched_evidence_use` moves from the predecessor's incomplete secondary declaration to `unavailable_endpoints` with `NOT_EVALUATED_UNDER_THIS_EXPERIMENT`. It has no estimand, numerator, denominator, value, proxy, or permitted utilization claim.
3. The experiment identity and lineage change.
4. FULL repeated measurement is explicitly completed at two repetitions under the owner's instruction not to alter repetitions. The report marks this as a successor completion because predecessor FULL had no frozen matrix; no pilot outcome selected it.

No historical 73.7%/73.7% definition is imported.

## Exact non-delta

Preserved: hypothesis; R0–R4; 43-case primary population/exclusions; evidence policies; RAW/LEGIBLE; MASKED_K exact K=3; FULL `EXEMPT_UPPER_BOUND`; payments and governance; gate-blind case-level `R_correct`; majority-before-pairing and tie-incorrect; statistical plan; 10pp threshold; unresolved power state; C1–C4; POST-HOC `response_mode`; model/route/prompt/parser/state/persistence core.

## revision_rate

Denominator:

```text
round1.proposed_settlement is not null
AND round1.proposed_settlement != evaluator true_locus
```

Numerator:

```text
final gate-blind claim != round1.proposed_settlement
```

Explicit `unknown` is a typed settlement and is wrong when unequal to truth. Null is no claim and excluded. `unresolved_distinction` is metadata and does not enter the denominator. Reporting unit is case×repetition, by arm; zero denominator is `NOT_ESTIMABLE`.

## matched_evidence_use

```text
NOT_EVALUATED_UNDER_THIS_EXPERIMENT
```

Absence is not zero utilization. No utilization claim or derived proxy is permitted.

## Execution/runtime compatibility

- The predecessor's 15 payment-bearing files are byte-equal to reviewed commit `cf9b28b84a415ef33519e0866536ebd7787d567d`.
- The authoritative predecessor revision-002 scientific artifact, run identity, matrix/UIds, case-input binding, reviewed implementation aggregate, and source bindings are mechanically checked.
- Sixty predecessor pilot coordinate vectors were independently reconstructed through predecessor and successor planners and matched.
- A counterfactual successor experiment seed changes 60 vectors, proving the check detects the seed trap. The successor therefore freezes `evidence-revision-001` as the evidence-selection seed identity while using new successor UIDs.
- Observable provider, prompt, evidence, rendering, payment, state, parser, ledger, persistence, and raw-before-score behavior delta: **none**.
- The live unit primitive is lexically scoped after final authorization; unsafe run IDs, input drift, noncanonical matrices, unreviewed runner bytes, and missing explicit power override are refused before client construction/execution.
- Compatibility: `PREDECESSOR_PILOT_ACCEPTED`; SHA-256 `8e36347a7f77aa3a635f3c1a5a67ccfb35307a12bcfe875227fc9ab18cdbd0f5`.
- Pilot rerun required: **false**.

## New identities

- Scientific design: `056284062a2ef6b2a6f6dc54ae63eceeb8802b66341d42ced7d305067d9e1d99`
- Executable contract: `46ee79581cd6b20c1abbaf1e99e21c62ae8372949ebbd8575609d4cb23bbb6f9`
- Runner implementation: `ad8c32bc9ab016b3051d44c8b77dc3333584f91df69732dc16d8536d52cbfdd9`
- Reviewed runner commit: `9c6467ae9316cc50d3415952d25e79b85097e1f7`
- FULL matrix: `7812aa656347acfd26283e5f134669d4ea5c1fb72d5162bb0b1d41b34ef1e668`
- Case inputs: `20fbaf97235b9bb4144582ae0bdf1527bb10a12394da38ee09931c7af4f91e1b` (stimulus bytes reused unchanged)

## FULL matrix

- Namespace: `full:er002-full-v1`
- Cases: 43
- Arms: 5
- Repetitions: 2
- Planned units: **430**
- Unique UIDs, pairing, evidence equality, exact K, affordability, RAW/LEGIBLE equivalence, exclusions, and leak scans: PASS.
- Predecessor pilot units included: false.
- Execution authorized: false.

## Analysis contract

Canonical analysis verifies the exact 430-result set, rejects orphan/duplicate result or raw-call ledgers, verifies runner/design/executable/matrix bindings, typed round-1 decision fields, frozen evaluator truth joins, and unique coordinates before computing case-level primary tables or `revision_rate`. A valid first-round `MODEL_OUTPUT_CONTRACT_FAILURE` with no round record maps to null/no-claim, incorrect primary, preserved output class, and revision-denominator exclusion. `matched_evidence_use` raises rather than proxying.

## Verification

```text
399 passed
0 failed
0 skipped
provider calls = 0
model calls = 0
experimental units executed = 0
```

Predecessor completed-run manifest before/after:

```text
ae2827c1ea32c2860e53f19b763955908860865266791932f1454252ffeb2a13
ae2827c1ea32c2860e53f19b763955908860865266791932f1454252ffeb2a13
```

Implementation commits: `bf6167558c6b7ec7285c207d4eea6c5e18c9f83c`, `9c6467ae9316cc50d3415952d25e79b85097e1f7`, `35a6badc9c58d62d8c2edd62ac40ecf22fc3c649`.

## Artifacts

- `ablation/analysis/post-audit-v2/evidence-revision-002-contract.json`
- `ablation/analysis/post-audit-v2/evidence-revision-002-executable-contract.json`
- `ablation/analysis/post-audit-v2/evidence-revision-002-full-matrix-validation.json`
- `ablation/analysis/post-audit-v2/evidence-revision-002-full-matrix.json`
- `ablation/analysis/post-audit-v2/evidence-revision-002-lineage.json`
- `ablation/analysis/post-audit-v2/evidence-revision-002-predecessor-pilot-compatibility.json`
- `ablation/analysis/post-audit-v2/evidence-revision-002-runner-implementation.json`
- `ablation/analysis/post-audit-v2/evidence-revision-002-successor-freeze-report.json`
- `ablation/analysis/post-audit-v2/evidence-revision-002-successor-freeze-report.md`

## Authority

```text
successor status = FROZEN_NOT_AUTHORIZED
successor execution authorized = false
additional provider calls authorized = false
FULL authorized = false
FULL executed = false
release token created = false
release marker created = false
```

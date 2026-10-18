# Proposal: register `evidence.recorded` (E8) as a §5.3b record kind

Status: **implemented** (2026-10-18), commit de45a8b: `evidence.recorded`
is registered as an `evidence_packet` and mapped by
`sxr-core/src/evidence_record.rs`. The owner decided the open questions
below as: Question = the frozen expectation; direction derived from the
verdict; digests only, as content-unavailable representations;
subject prefixes `work_request:`, `affordance:`, `question:`, `proof:`,
`settlement:`; reliability `deterministic`, relevance `direct`. The text
below is the original proposal, kept for the record.

## Why

`emit_evidence_recorded` returns an E8 envelope, but no record kind exists
for it, so `sxr-ledger` cannot persist it. SEA-Forge's security acceptance
evidence (ADR-012 obligation 2) needs a persisted, ledger-verifiable E8.

## What the registry gate requires

`envelope_build_tests` builds a schema-valid envelope for every registered
kind. An `evidence_packet` carries CEP Evidence entities: `evidence_id`,
`evidence_type`, `question_ref`, `target_entity_ref`, `representation_ref`,
`relevance`, `reliability`, `direction` and `confidence_impact`.

## Decisions needed from the CEP / RealityTrace owner

1. **The Question.** Which Question an E8 answers. Candidate: the frozen
   expectation, as a Question declared by the acceptance suite.
2. **Direction.** Whether `direction` is derived from
   `difference.verdict`:
   - `equivalent` → `supports`
   - `divergent` → `contradicts`

   Or whether E8 must stay direction-free, so that RealityTrace never
   implies a verdict.
3. **Reliability and relevance.** The vocabulary for comparison evidence:
   - whether it is fixed by evidence type;
   - whether it is derived from the producer (`security_acceptance` versus
     `swe_seed`).
4. **Representations.** Whether the verbatim expected and observed values
   are carried as `representation_ref`s, or only cited by digest.
5. **Subject prefixes and predecessors.**
   - Candidate subject prefixes: `work_request:`, `proof:`, `settlement:`.
   - Candidate predecessors: the admitted ProofCompleted and the resolved
     OperationalSettlement.

Until these are decided, SEA-Forge persists E8 in its own ledger bundle
(ADR-012 slice), and RealityTrace only computes it.

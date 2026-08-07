---
type: "CEP Guide"
id: "cep-semantic-envelope-mapping"
title: "Semantic Envelope Schema Mapping"
description: "Maps semantic-envelope.schema.json fields to CEP-0008 distinctions and documents the v1 field-set gap."
status: "Draft"
version: "0.1.0"
tags:
  - cep
  - schema
  - semantic-envelope
timestamp: "2026-07-01T00:00:00Z"
---

# Semantic Envelope Schema Mapping (CEP-0008 -> v1 flat contract)

`semantic-envelope.schema.json` is a JSON Schema profile of CEP-0008's
Semantic Envelope, scoped to what the v1 flat contract
(`sea.agent.event.v1.json`, durably enforced at agent-memory-ledger's
boundary) can actually carry. It reuses v1's field names — it does not
invent new ones. `$comment` isn't used because the reference implementation's
schema validator (`src/cep/schema_profile.py: SUPPORTED_SCHEMA_KEYS`) doesn't
support it; the mapping lives here instead.

| v1 field | CEP-0008 distinction |
|---|---|
| `schema_version` | `cep_version` / `envelope_version` (SS7.1) |
| `event_id` | `envelope_id` (SS7.1) |
| `idempotency_key` | `lineage_refs` (SS7.1) |
| `source` | `source_system_refs` (SS7.2) |
| `source_agent` | `created_by` (SS7.1) |
| `event_type` | `envelope_kind` (SS9) |
| `occurred_at` | `created_at` (SS7.1) |
| `correlation_id` | trace `envelope_refs` (SS45) |
| `causation_id` | trace `ordering_rule` (SS45) |
| `trace_id` | `trace_id` (SS45) |
| `payload` | opaque carrier for states/transformations/questions/claims/evidence/settlements (SS23-29) |
| `provenance.origin`, `provenance.chain` | SS15 Envelope Provenance Record (`provenance_id`, `source_envelope_refs`) |
| `metadata` | opaque carrier for boundary/completeness/omission/conformance status (SS8, SS10, SS34, SS38) |

## Delta from v1's own schema

`provenance` is `required` here even though v1 marks it optional. CEP-0008
SS15.2 treats an envelope without provenance as not settlement-valid, so this
profile enforces that MUST. Any real v1 instance that already populates
`provenance` (which agent-memory-ledger events do in practice) satisfies both
schemas unchanged.

## Open gap (not resolved here — see `.agents/OPEN_QUESTIONS.md` OQ-0009)

CEP-0008 SS52 requires a minimally valid envelope to preserve `scope`,
`boundary_record`, `completeness_status`, and `omission_status` as identifiable
concepts. The v1 flat contract has no dedicated top-level field for any of
these — they can only live unenforced inside the opaque `payload` or
`metadata` objects (both `type: object` with no `additionalProperties: false`
at that level, so nesting them there doesn't violate v1). This profile does
not add top-level fields for them, per this plan's guardrail against
inventing a compromise schema. Whether v1 should grow those fields, or
whether CEP-0008 should define an explicit "opaque carrier" allowance for
flat envelope profiles, is a cross-repo decision, not one made here.

## The canonical full profile (`cep-semantic-envelope.schema.json`)

Resolves OQ-0009 on the "explicit top-level fields" basis: every CEP-0008
SS52 minimal-valid field is an explicit top-level property, plus the
SS7.1 `lineage_refs` field the frozen runtime spec requires. It is the
contract for Semantic Exhaust Runtime emissions.

- All SS52 fields are `required`: `envelope_id`, `cep_version`,
  `envelope_version`, `envelope_kind`, `created_at`, `created_by`, `scope`,
  `boundary_record`, `completeness_status`, `provenance_refs`,
  `omission_status`, `lineage_refs`.
- `envelope_kind` is the SS9 closed 17-value vocabulary; new kinds must be
  defined by CEP-0008.
- The envelope MUST carry `validation_status` or `conformance_status`
  (SS52) and at least one CEP entity section (SS52) — enforced via
  `anyOf`; sections carry their SS13-39 SHOULD vocabularies.
- Reference records require `availability_status` and `integrity_status`
  (SS13.1/SS54 Invariant 13: references SHALL preserve availability and
  integrity status).
- `additionalProperties: false` at every record level keeps the profile a
  closed vocabulary: §53 collapse shapes (data-as-evidence,
  representation-as-reality, action-as-transformation,
  completion-as-settlement, authority-as-capability,
  possibility-as-affordance, signature-as-truth) fail as `unknown field`,
  and unknown top-level fields fail the uncertainty-drop check.
- Hash-bearing fields use the frozen runtime spec's pattern
  `^[a-z0-9]+:[0-9a-f]{16,128}$`; `content_ref` admits CAS URIs via
  `^[a-z0-9]+://[a-z0-9]+:[0-9a-f]{16,128}$` (e.g. `cas://sha256:<hex>` per the
  frozen spec's §10.7a carriage table); `created_at`-style fields use RFC 3339.
- SS11 lists `perspectives` and `actors` sections; CEP-0008 defines no
  record vocabulary for them (only `perspective_ref`/`actor_ref`-style
  references in SS25/SS30), so the full profile does not invent one.

### SS53 fixture map (`schemas/fixtures/semantic-envelope-full/`)

One invalid fixture per SS53 bullet, each with a named, structurally
detectable failure reason:

| fixture | SS53 condition | structural reason |
|---|---|---|
| `invalid-missing-identity.json` | lacks envelope identity | missing required `envelope_id` |
| `invalid-missing-cep-version.json` | lacks CEP version | missing required `cep_version` |
| `invalid-missing-scope.json` | lacks scope | missing required `scope` |
| `invalid-missing-boundary-record.json` | lacks boundary record | missing required `boundary_record` |
| `invalid-missing-provenance.json` | lacks provenance | missing required `provenance_refs` |
| `invalid-missing-completeness-status.json` | lacks completeness status | missing required `completeness_status` |
| `invalid-no-entity-section.json` | raw context as semantic structure | no CEP entity section (anyOf §52) |
| `invalid-no-validation-conformance-status.json` | cannot be evaluated | no `validation_status`/`conformance_status` (anyOf §52) |
| `invalid-raw-context-as-envelope.json` | raw context as envelope | unknown top-level field `raw_context` |
| `invalid-data-as-evidence.json` | data collapsed into evidence | unknown field `raw_data` on evidence record |
| `invalid-representation-as-reality.json` | representation collapsed into reality | unknown field `is_reality` on representation |
| `invalid-action-as-transformation.json` | action collapsed into transformation | unknown field `action` on transformation |
| `invalid-completion-as-settlement.json` | completion collapsed into settlement | unknown field `completion_status` on settlement |
| `invalid-authority-as-capability.json` | authority collapsed into capability | unknown field `authority_grant` on capability |
| `invalid-possibility-as-affordance.json` | possibility collapsed into affordance | unknown field `is_possible` on affordance |
| `invalid-hidden-omissions.json` | hides known omissions | omission record without `omission_id` |
| `invalid-hidden-contradictions.json` | hides known contradictions | contradiction record without `contradiction_id` |
| `invalid-signature-as-truth.json` | signature treated as truth | unknown field `signature_is_truth` on integrity |
| `invalid-unmarked-unavailable-reference.json` | unavailable reference unmarked | missing required `availability_status` |
| `invalid-unknown-top-level-field.json` | silently drops uncertainty | unknown top-level field `silent_uncertainty_drop` |

Valid fixtures: `valid-semantic-snapshot.json` (semantic_snapshot carrying a
derived representation, hash pattern, boundary/omission records),
`valid-work-request.json` (work_request with questions and references),
`valid-minimal.json` (smallest lawful envelope).

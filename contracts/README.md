# FDE comparison contracts (WP-11)

Reviewed-before-code contracts for `sxr-observed` and `realitytrace observed`. JSON Schema draft 2020-12, every object closed (`additionalProperties: false`). Hand-authored. A change to a schema file needs a new line in `DIGESTS` in the same commit (`scripts/check-contract-digests.sh --update`); consumers vendor by digest.

| Contract | File | Fixtures (valid / invalid by schema + by rule the schema cannot state) |
|---|---|---|
| Comparison profile | `comparison-profile.v1.json` | 3 / 15 + 1 |
| Observed representation | `observed-representation.v1.json` | 3 / 17 + 8 |
| Residual | `residual.v1.json` | 7 / 20 + 0 |

Test: `cargo test --locked -p sxr-core --test fde_contracts`. Digests: `scripts/check-contract-digests.sh`.

Source sections are in the GodSpeed FDE integration architecture (E) and its review record (H). The `evaluate` facts and output schemas belong to WP-12 (`domainforge/schemas/evaluate-facts.v1.json`, `evaluate-output.v1.json`); this repository only names them and carries no schema for them.

Conventions. Hashes are `sha256:<64 lowercase hex>`. Ids follow E §2: `world_ref`, `episode_ref`, `concept:<uuidv5>` declaration keys, `undeclared:<sha256>` keys, `question:`, `evidence:`, `residual:`, `profile:`. An occurrence id is the source record's key `(system, record_id)` written `<system>:<record_id>`. Documents are stored as RFC 8785 canonical JSON; arrays are sorted as stated below so that the same input gives the same bytes.

## 1. Comparison profile

Per comparison kind: quantifier, minimum support `k`, deadline, class mapping. One entry per kind.

| Field | Type | Req | Meaning | E |
|---|---|---|---|---|
| `contract` | const `comparison-profile.v1` | yes | contract name | §13 |
| `profile_id` | `profile:<token>` | yes | what `observed compare --profile` names | §6 |
| `kinds[].kind` | `policy_evaluable` \| `structural` \| `ordering` \| `frequency` \| `absence` \| `timing` | yes | the comparison; context variation is reported, not classified, so it is not a kind | §6 |
| `kinds[].comparison` | `episode` \| `structural` | yes | which residual list it feeds | §6, RD-12 |
| `kinds[].quantifier` | `universal` \| `existential` \| `threshold` | yes | over instances | §6 |
| `kinds[].min_support` | integer >= 1 | yes | `k`; fewer supporting occurrences is `insufficient` / `below_minimum_support` | §6 |
| `kinds[].allow_vacuous_truth` | boolean | yes | `true` only if the profile accepts a universal rule over zero instances as `confirmed` | RD-08 |
| `kinds[].deadline` | null \| `{unit: sequence_steps\|causal_hops, limit >= 1}` | yes | counted over owning-system sequence or causation, never emitter clocks; an object for `timing` | §6 |
| `kinds[].class_mapping` | object | yes | `true`->`confirmed`, `false`->`contradicted`, `unknown`->`insufficient` (constants); `undeclared`->`observed_undeclared` (required for `structural`); `trigger_observed_record_absent`->`declared_unobserved` (required for, and only for, `absence`) | §6 |

| Code | Rule |
|---|---|
| `PROFILE_DUPLICATE_KIND` | two entries have the same `kind` (not stated by the schema) |

## 2. Observed representation

Output of `observed derive`: from verified records only, one representation per episode and `kind` selector. Stored as `representation.recorded` with the input-set hash. Exploratory and evaluated occurrences are never in the same representation.

| Field | Type | Req | Meaning | E |
|---|---|---|---|---|
| `contract` | const `observed-representation.v1` | yes | | §13 |
| `world_ref` | string | yes | pinned model | §2, §13 |
| `episode_ref` | string | yes | the `--episode` selector | §6 |
| `kind` | `evaluated` \| `exploratory` | yes | the `--kind` selector | §6 |
| `profile_id` | `profile:<token>` | yes | profile it was derived under | §6 |
| `input_set_hash` | `sha256:<hex>` | yes | hash of the verified input record set and `--until` bound; changes when an input changes (AT-08) | §6, §13 |
| `nodes[]` | node | yes | ascending by `node_id` | §6 |
| `edges[]` | edge | yes | ascending by `(from, to)` | §6 |
| `coverage[]` | `{emitter, event_kind, observed_count, status: covered\|silent}` | yes, >= 1 | one row per manifest entry frozen with the episode; `covered` iff `observed_count > 0` | §5, §6 |
| `uncertainty` | `{rejected_event_count, unverified_event_count}` | yes | events that support no conclusion | §5, §6 |

Node: `node_id` (`concept:<uuidv5>` or `undeclared:<sha256 of operation + resource kind>`); `semantic_ref` (declared nodes only, required there, forbidden on undeclared); `occurrence_ids` (>= 1, distinct, ascending); `frequency` (equals the number of occurrence ids); `independent_occurrence_count` (<= `frequency`; zero caps the element at `insufficient`, E §5); `ordering[]` (per system, ascending: `{system, first_sequence, last_sequence}`; sequences are per owning system and never compared across systems); `context_distribution` (`{roles, plan_items}`, count maps whose sums equal `frequency`); `authority_outcomes` (`{decided, not_required, unknown}`, by the class of `authority.decision_id`, sum equals `frequency`); `outcome_status` (counts for the five `outcome.status` values, sum equals `frequency`).

Edge: `from`, `to` (node ids of this representation); `occurrence_ids` (occurrences on both ends of every link, distinct, ascending); `frequency` (number of directly-follows links); `ordering` (`{via_sequence, via_causation}`, sum equals `frequency`); `context_distribution`, `authority_outcomes`, `outcome_status` as for nodes.

| Code | Rule (checked by the contract test, not the schema) |
|---|---|
| `REP_FREQUENCY_MISMATCH` | a node's `frequency` differs from its number of occurrence ids |
| `REP_NOT_CANONICAL_ORDER` | nodes, edges, occurrence ids not strictly ascending |
| `REP_EDGE_ENDPOINT_UNKNOWN` | an edge names a node that is not in `nodes` |
| `REP_DISTRIBUTION_MISMATCH` | a distribution or edge ordering sum differs from `frequency`, or `independent_occurrence_count` exceeds it |
| `REP_COVERAGE_INCONSISTENT` | `status` contradicts `observed_count` |

Ingest-side codes (`INGEST_REJECTED_IDENTITY`, `INGEST_REJECTED_EVIDENCE`, `CHAIN_VERIFY_FAILED`) belong to WP-16.

## 3. Residual

One difference between the declared model and the observed representation, event `residual.computed`.

| Field | Type | Req | Meaning | E |
|---|---|---|---|---|
| `contract` | const `residual.v1` | yes | | §13 |
| `residual_id` | `residual:<token>` | yes | | §2 |
| `class` | `confirmed` \| `contradicted` \| `observed_undeclared` \| `declared_unobserved` \| `insufficient` | yes | five classes; `not_evaluable` is a basis, not a class | §6 |
| `basis` | enum, constrained by `class` (table below) | yes | why | §6, RD-08, NT-45 |
| `comparison` | `episode` \| `structural` | yes | residual list; never mixed | §6, RD-12 |
| `keys[]` | `concept:` or `undeclared:` keys, >= 1 | yes | elements the difference is about | §13 |
| `occurrence_ids[]` | occurrence ids | yes | occurrences behind it (trigger occurrences for `declared_unobserved`); >= 1 unless `insufficient` | §13 |
| `evidence_refs[]` | `evidence:<token>` | yes | qualifying evidence; >= 1 unless `insufficient` | §5, NT-45 |
| `rule` | `{kind, ref}` | yes | the profile kind and the policy or relation | §13 |
| `support_count` | integer >= 0 | yes | supporting occurrences, compared with the profile's `k` | §6 |
| `profile_id` | `profile:<token>` | yes | the profile that fixes `k` | §6 |
| `question` | `question:<token>` | yes | RealityTrace question | §5, §13 |
| `snapshot` | string | yes | declared snapshot id compared | §6, §13 |
| `world_ref` | string | yes | world stamp; a classification stays with its world (NT-07) | §13 |

| `class` | allowed `basis` | decided by |
|---|---|---|
| `confirmed` | `policy_true`, `structural_satisfied` | DomainForge `evaluate` true; `sxr-observed` |
| `contradicted` | `policy_false`, `structural_violated` | `evaluate` false; `sxr-observed` |
| `observed_undeclared` | `structural_undeclared` | undeclared behaviour |
| `declared_unobserved` | `trigger_observed_record_absent` | trigger observed inside the frozen coverage window, required record absent (RD-03) |
| `insufficient` | `policy_unknown`, `not_evaluable`, `below_minimum_support`, `evidence_source_silent`, `executor_only_evidence` | `evaluate` unknown; neither can decide; support below `k` or vacuous; frozen evidence source emitted nothing (NT-06); no independent emitter (E §5) |

Residual errors (from the schema; the `contains` text in the manifest is the reason each fixture is checked for): a missing `basis` or `comparison`, a `class` outside the five, a `basis` not allowed for its `class`, or missing occurrence or evidence for a non-`insufficient` class, is refused at write (`RESIDUAL_BASIS_MISSING`, NT-45, WP-17); the schema gives the pointer and the keyword.

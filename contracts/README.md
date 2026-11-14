# FDE comparison contracts (WP-11)

Reviewed-before-code contracts for `sxr-observed` and `realitytrace observed`. JSON Schema draft 2020-12, every object closed (`additionalProperties: false`). Hand-authored. A change to a schema file needs a new line in `DIGESTS` in the same commit (`scripts/check-contract-digests.sh --update`); consumers vendor by digest.

| Contract | File | Fixtures (valid / invalid by schema + by rule the schema cannot state) |
|---|---|---|
| Comparison profile | `comparison-profile.v1.json` | 3 / 15 + 1 |
| Observed representation | `observed-representation.v1.json` | 3 / 17 + 8 |
| Residual | `residual.v1.json` | 7 / 24 + 0 |

The test does no hashing; digests of schema files use SHA-256 over the file bytes, and the RFC 8785 rule itself is tested in the CEP repository only (CEP-0008 section 16.5 vectors).

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
| `representation_id` | `representation:<token>` | yes | the observed representation compared; one-way: the representation does not reference residuals, so its hash never depends on later comparisons | §6, review 2026-11-14 |
| `input_set_sha256` | `sha256:<hex>` | yes | that representation's `input_set_hash` | §6, review 2026-11-14 |
| `world_ref` | string | yes | world stamp; a classification stays with its world (NT-07) | §13 |

| `class` | allowed `basis` | decided by |
|---|---|---|
| `confirmed` | `policy_true`, `structural_satisfied` | DomainForge `evaluate` true; `sxr-observed` |
| `contradicted` | `policy_false`, `structural_violated` | `evaluate` false; `sxr-observed` |
| `observed_undeclared` | `structural_undeclared` | undeclared behaviour |
| `declared_unobserved` | `trigger_observed_record_absent` | trigger observed inside the frozen coverage window, required record absent (RD-03) |
| `insufficient` | `policy_unknown`, `not_evaluable`, `below_minimum_support`, `evidence_source_silent`, `executor_only_evidence` | `evaluate` unknown; neither can decide; support below `k` or vacuous; frozen evidence source emitted nothing (NT-06); no independent emitter (E §5) |

Residual errors (from the schema; the `contains` text in the manifest is the reason each fixture is checked for): a missing `basis` or `comparison`, a `class` outside the five, a `basis` not allowed for its `class`, or missing occurrence or evidence for a non-`insufficient` class, is refused at write (`RESIDUAL_BASIS_MISSING`, NT-45, WP-17); the schema gives the pointer and the keyword.

## 4. `input_set_hash` (WP-17 review of the WP-16 definition)

`representation.input_set_hash` (copied into a residual as `input_set_sha256`) is

```
sha256( JCS( {
  contract:  "observed-representation.v1",
  selectors: {world_ref, scope, episode_ref, kind, profile_id},
  options:   {declared_keys, coverage_manifest, independent_systems},
  window:    {until, record_count, first_ordinal, last_ordinal, head_entry_hash, tail_verification},
  inputs:    sorted entry hashes of the occurrences that were included,
  rejected:  sorted entry hashes of the ingest.rejected rows attributed to the selectors,
  unverified: sorted entry hashes of the events that failed derive's re-verification
} ) )
```

JCS here is `sxr_core::canonical_json` behind the I-JSON guard in `sxr-observed/src/canon.rs` (floats, integers beyond 2^53-1 and non-BMP keys are refused, never repaired).

Review result: kept, with two changes and this rationale.

- Deterministic. Every set is sorted (`declared_keys`, `independent_systems` are ordered sets; the entry-hash lists are sorted). Fixed in WP-17: `coverage_manifest` was hashed in the order the caller wrote it, so the same manifest in another order gave another hash; it is now sorted and de-duplicated first (test `input_set_hash_ignores_coverage_manifest_order`).
- Complete. `head_entry_hash` is the hash of the last row in the window, and every row hash commits to its predecessor, so the hash covers the whole window prefix: evidence rows, rejected rows, rows of other cases. It is deliberately over-sensitive: an unrelated record appended before the window end changes the hash. That is the safe direction for AT-08 (a changed input cannot go unnoticed); a narrower definition would need the evidence-row hashes listed as well. The selectors and options are inside the hash, so the same ledger under another episode, kind, world, profile, declared set or independence declaration is another input set. The occurrence content is covered because the entry hash covers the payload hash.
- Added in WP-17: `window.tail_verification`. `verified_against_expected_head` and `unverified` are different input sets, so an output that did not check the tail can never share a hash with one that did.
- Not in the hash and not an input: the frozen reference (only its `episode_ref` is a selector), the comparison profile body (only its id), the mapping and the DomainForge binary. These are inputs of `compare`, not of `derive`; `compare` records their digests in its own result (`profile.sha256`, `mapping_sha256`, `evaluate.facts_sha256`, `evaluate.model_sha256`).
- Ledger identity: entry hashes include the ledger's own ULIDs and commit times, so the same events appended into another ledger give another hash. An input set is a set of ledger rows, not of event contents.

Tail verification. A hash chain detects a changed or removed row anywhere except at the end: a chain that stops one row earlier is a valid chain (CEP-0008 section 55). `observed derive` and `observed compare` therefore take an expected head recorded outside the ledger (`--expected-head <entry_hash>` and/or `--expected-record-count <n>`, from the anchor or the frozen reference). With one, a ledger that no longer contains that head at that position fails `TAIL_TRUNCATED` (exit 5); later appends are allowed. Without one, the output says `"tail_verification": {"status": "unverified", ...}`; it is never reported as verified.

## 5. `observed compare`: inputs, result document and what the contracts do not carry

`realitytrace observed compare --repo --observed <derive output> --reference <frozen reference> --profile <profile file> --mapping <file> --world <world_ref> --declared <snapshot id> [--model <.sea> --domainforge-bin <path>] [--facts-out <file>] [--expected-head ...]`.

- The frozen reference's `body.comparison_profile {id, sha256}` pins the profile. `sha256` is over the canonical profile document; any other bytes fail `PROFILE_DIGEST_MISMATCH`. The reference's own `episode_ref` digest is recomputed (`REFERENCE_DIGEST_MISMATCH`). The representation's `representation_id` is recomputed (`REPRESENTATION_INVALID`), its window must end at a row of the given ledger (`LEDGER_MISMATCH`), and `--world` must equal the representation's, the reference's and, when `evaluate` ran, DomainForge's `world_ref` (`WORLD_REF_MISMATCH`).
- DomainForge is the CLI binary (the integration style of `sxr-df`): `--domainforge-bin`, else `REALITYTRACE_DOMAINFORGE_BIN` / `SXR_DOMAINFORGE_BIN`; `PATH` is not searched. A binary without `evaluate` fails `EVALUATE_UNAVAILABLE`; `validate` is never used instead. The generated facts file (`operation_inputs` only, canonical JSON, validated against `evaluate-facts.v1`) is hashed; the output must report that same `facts_sha256` and validate against `evaluate-output.v1` (`schemas/` vendored from DomainForge 3960326).
- Mapping `compare-mapping.v1` (not a frozen contract, an input): `undeclared_question`; optional `declared_keys` (declared elements never observed are listed in `not_exercised`); `operations.<op> = {node, input: {field: spec}}` with `spec` one of `{kind, pointer}` (a JSON pointer into the interaction event), `{kind, const}`, `{kind: "bool", nonempty: pointer}`, `{typed: <DomainForge typed value>}` (a missing pointer omits the field, which DomainForge reads as unknown); `expectations[]` of kind `policy_evaluable {policy, operation, node}`, `structural {relation: flow {from, to} | precondition {node, requires_prior}, ref}`, `absence {trigger, required: {node, emitter, event_kind}, ref}`, or `ordering | frequency | timing {keys, ref}` (these three are reported `insufficient` / `not_evaluable`: not implemented). Each has `id`, `question`, optional `evidence_sources` (frozen `source_id`s the decision depends on).
- Decision rules. Universal rules use the profile's `min_support` k: any false is `contradicted`; otherwise any unknown is `insufficient` / `policy_unknown`; fewer than k true (or none) is `insufficient` / `below_minimum_support`. A flow is `confirmed` with the observed directly-follows edge >= k, `contradicted` when only the reverse edge was observed. A precondition is judged per occurrence by the owning system's sequence or causation; a prior occurrence in another system is undecidable (`not_evaluable`). An absence is judged only for observed triggers whose required `(emitter, event_kind)` is a `covered` row of the frozen coverage manifest; a `silent` row is `evidence_source_silent`, a row outside the manifest is `not_evaluable`; an unobserved trigger is `not_exercised`, no class. Overlays, in order: a depended-on frozen evidence source that emitted nothing turns `confirmed` or `insufficient` into `insufficient` / `evidence_source_silent` (NT-06); a `confirmed` whose nodes carry no independent occurrence becomes `insufficient` / `executor_only_evidence` (NT-20). Undeclared nodes, and edges between declared flow endpoints that no flow expectation covers, are `observed_undeclared`; compare never adds them to the model.
- Result document (canonical JSON, deterministic): `comparison_id`, `episode_ref`, `kind`, `world_ref`, `snapshot`, `representation_id`, `input_set_sha256`, `profile {id, sha256}`, `mapping_sha256`, `tail_verification`, `settleable`, `flags`, `proposed_result {result, basis}`, `silent_evidence_sources`, `residuals {episode: [...], structural: [...]}` (each a `residual.v1`, validated at write), `not_exercised`, `annotations {residual_id: {expectation, context, note?, facts_sha256?, evaluate_world_ref?, silent_sources?}}`, `evaluate {used, binary, version, compiler, world_ref, model_sha256, source_set_hash, facts_sha256, facts}`. `proposed_result` is S, F or U from the episode list only (any contradicted: F; any insufficient or silent source: U; all confirmed: S); it is a proposal, not a settlement. An exploratory episode is `settleable: false`, flag `EXPLORATORY_NOT_SETTLEABLE`, result null.
- Gaps the contracts leave (recorded, not invented into `residual.v1`, which is closed): the residual has no field for the context distribution, the facts-file hash, the DomainForge version or an exploratory flag; they travel in `annotations`/`evaluate` of the result document, keyed by `residual_id`. NT-20's oracle names basis `no_independent_emitter`; `residual.v1` (accepted in WP-11 review, decision 12) has `executor_only_evidence`, which is what is emitted. A confirmed `allow_vacuous_truth` rule over zero instances cannot be written (a confirmed residual needs an occurrence and evidence, NT-45); it is `insufficient` / `below_minimum_support` with a note.

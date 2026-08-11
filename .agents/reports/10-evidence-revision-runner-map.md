# evidence-revision-001 runner implementation map

Frozen design fingerprint: `7edd93a3a503b9395c0d39e79540cb20fe084fc8c020413c306db88e1029d3e3`.

| Responsibility | Existing owner/module | Reuse | Missing orchestration / genuine gap |
|---|---|---|---|
| Experiment contract, frozen arms, owner state, endpoints, statistics hooks | `eval.v2.evidence_revision` | As-is; runner imports `ARMS`, `OWNER_STATE`, `contract_fingerprint`, endpoint/statistical documents | Central gate and run manifest are missing |
| `ConditionSpec`, typed controller transitions, scoring handoff state | `eval.v2.controller` | Reused with two objective conformance fixes | Durable call wrapper and unit/result checkpoint orchestration were missing; harness-source model actions are now ignored for evidence selection, and enforced-budget deliveries cannot overdraw |
| `DiagnosticState`, round rendering and continuity validation | `eval.v2.dstate` | As-is | Runner must validate final rendered bytes at transport boundary |
| Opaque action/evidence handles and disclosure | `eval.v2.channel` | As-is | Runner must bind unit coordinates and prevent cross-unit reuse |
| RAW/LEGIBLE rendering and semantic binding | `eval.v2.legibility`, invoked by `DiagnosticState` | As-is | Gate must require equivalence/source hashes |
| Prompt leak guard | `eval.v2.channel.PromptLeakGuard`; controller invokes before `complete` | As-is, plus transport wrapper re-check | Persist a digest proving the actual transport prompt was checked |
| Typed decision parser and repair path | `eval.v2.decision`, `eval.v2.controller._ask` | As-is | No semantic duplication in runner |
| Model client interface | `eval.v2.controller.ModelClient`; live implementation `eval.v2.provider` | As-is | Client configuration/injection gate and an offline-only fake are missing |
| Provider provenance and truncation | `eval.v2.execution` | As-is | Durable raw artifact must precede return to controller/parser |
| Raw persistence/hash boundary | `eval.v2.ledger.CommitLedger` | As-is | Per-call raw envelopes and checkpoint status integration are missing |
| Manifest/state store, run namespace, checkpoint/resume | none for this frozen experiment | — | Genuine implementation gap |
| Corpus/evidence/action hashes | frozen corpus files plus source modules | Inputs reusable | Canonical runner-side verification manifest is missing |
| Scoring interface | controller `UnitResult` plus existing analysis contracts | Preserve only; runner emits analysis-ready event/result artifact | Explicit scorable handoff is missing |
| Pilot/full UID separation | `eval.v2.ids.unit_id` | As-is | Stage-specific namespace construction and collision check are missing |
| Stage authorization and release token/marker | owner state exists; no evidence-revision release gate exists | Owner state as-is | Genuine implementation gap; one centralized execution gate required |
| Frozen pilot matrix | **not present in the frozen Python contract or emitted JSON** | Cannot infer without changing science | **Owner-adjudication blocker:** exact pilot case IDs, repetition count, and per-case model-visible original Question/failure-evidence binding are absent. Runner may inspect/verify a frozen matrix once supplied, but must not invent them. |

Implementation policy: compose the frozen modules, add only orchestration, and fail closed on the unresolved pilot-matrix contract gap. The apparent R1/R2 `SINGLE` wording in the implementation request conflicts with the authoritative contract, whose arms use `MASKED_K`; the authoritative contract wins and will not be changed.

## Investigation corrections applied

Two component-level orchestration defects were found after the initial map and fixed without changing a frozen field: (1) a typed model action could select evidence even when `ConditionSpec.action_source == "harness"`; it is now recorded but ignored while the frozen harness policy chooses delivery, and (2) repeated harness deliveries could overdraw an `ENFORCED` payment budget; an unaffordable whole policy delivery now discloses nothing, charges nothing, and records budget exhaustion rather than silently shrinking `MASKED_K`. Regression tests cover both.

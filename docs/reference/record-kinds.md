# Technical Reference: Ledger Record Kinds Registry

This document defines the closed registry of all **26 record kinds** recognized by SXR, transcribed from the authoritative machine-readable specification in [`sxr-core/src/record_kinds.csv`](file:///home/sprime01/projects/sxr/sxr-core/src/record_kinds.csv).

---

## 1. Registry Table

| Kind | CEP Envelope Kind | Primary Entities | Required Subject Prefixes | Allowed Predecessors | Authority Class |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **`repo.enrolled`** | `execution_trace` | Repository State + Enrollment Transformation | `repo:`, `checkout:` | None | `runtime-authoritative` |
| **`repo.relocated`** | `execution_trace` | Location State change | `repo:`, `checkout:` | `repo.enrolled`, `repo.relocated` | `runtime-authoritative` |
| **`repo.forgotten`** | `execution_trace` | Enrollment-state Transformation | `repo:`, `checkout:` | `repo.enrolled`, `repo.relocated` | `runtime-authoritative` |
| **`semantic.snapshot`** | `semantic_snapshot` | Declared State + DomainForge Validation Result + Representation $D$ | `snapshot:`, `repo:`, `checkout:`, `checkpoint:` | `repo.enrolled`, optional prior snapshot | `DomainForge authority carried by reference` |
| **`semantic.diff`** | `semantic_diff` | State / Representation difference | `snapshot:` | Two `semantic.snapshot` records | `DomainForge semantic interpretation` |
| **`semantic.identity_drift`** | `semantic_diff` | Identity-binding difference | `snapshot:` | Two `semantic.snapshot` records | `derived comparison` |
| **`run.started`** | `work_request` | Target Transformation + Question refs | `run:`, `repo:`, `checkout:`, `snapshot:`, `target:` | `semantic.snapshot`, `question.declared`, `threshold.declared` | `runtime-authoritative` |
| **`run.finished`** | `execution_trace` | Attempted Transformation + Run State | `run:`, `target:` | `run.started`, zero or more observations | `runtime-authoritative` |
| **`verification.contract_imported`** | `work_request` | Verification Contract: Questions, Claims, Evidence Obligations | `contract:`, `semantic:`, `obligation:`, `question:`, `declaration:` | Committed `semantic.snapshot` | `DomainForge-authoritative projection` |
| **`plan.created`** | `work_request` | VerificationPlan Representation | `plan:`, `question:`, `threshold:`, `snapshot:`, `target:` | Committed `question.declared` or contract; `threshold.declared`; `semantic.snapshot` | `operator/runtime plan; no semantic authority` |
| **`question.declared`** | `work_request` | CEP Question | `question:`, `target:` | None or `semantic.snapshot` | `declared evaluation context` |
| **`threshold.declared`** | `work_request` | CEP Settlement Threshold | `threshold:`, `question:` | `question.declared` or contract containing that Question | `declared evaluation context` |
| **`claim.made`** | `context_bundle` | Claim (asserted relation) | `claim:`, `question:`, `target:` | `question.declared`; `semantic.snapshot` | `asserted, not evidenced` |
| **`representation.recorded`** | `context_bundle` | CEP Representation | `representation:`, `subject:` | Source-specific | `representation, not reality` |
| **`observation.recorded`** | `execution_trace` | Raw Observation + Artifact Digests | `observation:`, `run:`, `target:` | Committed `plan.created`; plus `run.started` | `runtime-authoritative observation` |
| **`evidence.created`** | `evidence_packet` | CEP Evidence (relevance, reliability, direction) | `evidence:`, `question:`, `target:` | Committed `observation.recorded`; plus predeclared binding/rule | `question-bound Evidence` |
| **`settlement.registered`** | `settlement_packet` | CEP Settlement Verdict | `settlement:`, `question:`, `transformation:`, `snapshot:` | Committed `evidence.created`; plus earlier `threshold.declared` | `runtime settlement; not approval` |
| **`debt.recorded`** | `settlement_packet` | Settlement Debt + Settlement Reference | `debt:`, `settlement:` | `settlement.registered` | `runtime-authoritative debt` |
| **`residual.computed`** | `semantic_diff` | Question-bound State/Relation difference | `residual:`, `question:`, `snapshot:`, `target:` | `semantic.snapshot`; plus qualifying Evidence / Settlement | `derived; never causal authority` |
| **`diagnostic.hypothesis`** | `work_request` | Diagnostic Question + Candidate Failure Locus | `hypothesis:`, `question:`, `residual:` | `residual.computed` | `hypothesis, not verdict` |
| **`diagnostic.settled`** | `settlement_packet` | Settled Failure Locus Verdict | `settlement:`, `question:`, `hypothesis:` | Competing hypotheses; plus qualifying Evidence | `runtime diagnostic settlement` |
| **`proposal.raised`** | `work_request` | Proposed Repair Transformation | `proposal:`, `locus:`, `target:` | `diagnostic.settled` for causal repair proposals | `proposal only; no promotion by itself` |
| **`conformance.claimed`** | `conformance_report` | CEP Conformance Claim + Evidence refs | `target:`, `claim:` | Qualifying conformance Evidence | `scoped claim, not self-proving` |
| **`ingest.rejected`** | `conformance_report` | Rejected Candidate Representation | `representation:`, `ingest:` | None | `runtime validation report` |
| **`ledger.generation_started`** | `semantic_trace` | New Generation State + Continuity Anchor | `history:`, `ledger:` | None within new generation | `runtime-authoritative history` |
| **`ledger.destructive_recovery`**| `semantic_trace` | Continuity-breaking Recovery Record | `history:`, `ledger:`, `generation:` | None within new generation | `runtime-authoritative break record` |

---

## 2. Invariants Governing Record Kinds

1. **Strict Closed Enum**: No record kind outside these 26 may be appended to the ledger. Unknown kinds fail validation at the protocol boundary.
2. **Envelope Kind Mapping**: Every record kind maps to an exact CEP-0008 `envelope_kind` (e.g. `observation.recorded` $\to$ `execution_trace`). Mismatches return `EnvelopeError::KindEnvelopeMismatch`.
3. **Subject Reference Prefixes**: Each record kind enforces required subject reference prefixes (e.g. `run.started` requires `run:`, `repo:`, `checkout:`, `snapshot:`, `target:`). Mismatches fail validation.
4. **Predecessor Invariants**: Records cannot be committed without prior committed predecessors in the ledger. For example, `evidence.created` requires a committed `observation.recorded`, and `settlement.registered` requires a committed `threshold.declared`.

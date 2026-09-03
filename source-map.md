# SXR Source Map & Traceability Matrix

This document maps all high-level concepts, capabilities, schemas, and operational invariants to their concrete implementation locations across the `sxr` codebase.

---

## 1. Core Domain Types & Cryptographic Math (`sxr-core`)

| Concept / Invariant | Implementation File | Primary Symbols / Functions | Relevant Tests |
| :--- | :--- | :--- | :--- |
| **Repo Identity** (ULID, never Git-derived) | [`sxr-core/src/types.rs`](file:///home/sprime01/projects/sxr/sxr-core/src/types.rs) | `struct RepoIdentity`, `repo_id`, `checkout_id` | [`sxr-git/tests/ac_identity.rs`](file:///home/sprime01/projects/sxr/sxr-git/tests/ac_identity.rs) |
| **Ledger Position** (`{ledger_id, ordinal}`) | [`sxr-core/src/types.rs`](file:///home/sprime01/projects/sxr/sxr-core/src/types.rs) | `struct LedgerPosition`, `LedgerHead` | [`sxr-core/tests/ac_core_001.rs`](file:///home/sprime01/projects/sxr/sxr-core/tests/ac_core_001.rs) |
| **Declaration Key** (`concept:`, `application:`, `occurrence:`) | [`sxr-core/src/types.rs`](file:///home/sprime01/projects/sxr/sxr-core/src/types.rs) | `enum DeclKey`, `DeclKey::as_str` | [`sxr-core/tests/ac_core_001.rs`](file:///home/sprime01/projects/sxr/sxr-core/tests/ac_core_001.rs) |
| **Frozen Ledger Row Shape** | [`sxr-core/src/types.rs`](file:///home/sprime01/projects/sxr/sxr-core/src/types.rs) | `struct LedgerRow` | [`sxr-ledger/tests/ac_ledger_001.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/tests/ac_ledger_001.rs) |
| **Semantic Context & Omission** | [`sxr-core/src/types.rs`](file:///home/sprime01/projects/sxr/sxr-core/src/types.rs) | `enum SemanticContext`, `struct OmissionRecord` | [`sxr-core/tests/snapshot_and_threshold_tests.rs`](file:///home/sprime01/projects/sxr/sxr-core/tests/snapshot_and_threshold_tests.rs) |
| **Canonical JSON & SHA-256** | [`sxr-core/src/canonical_json.rs`](file:///home/sprime01/projects/sxr/sxr-core/src/canonical_json.rs) | `canonical_json`, `sha256_hex`, `sha256_canonical` | [`sxr-core/tests/ac_core_001.rs`](file:///home/sprime01/projects/sxr/sxr-core/tests/ac_core_001.rs) |
| **Cryptographic Hash Chain** | [`sxr-core/src/chain.rs`](file:///home/sprime01/projects/sxr/sxr-core/src/chain.rs) | `entry_hash`, `payload_hash`, `chain_verify` | [`sxr-ledger/tests/ac_ledger_001.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/tests/ac_ledger_001.rs) |
| **CEP-0008 Envelope Validation** | [`sxr-core/src/envelope.rs`](file:///home/sprime01/projects/sxr/sxr-core/src/envelope.rs) | `EnvelopeParts`, `build_envelope`, `validate_envelope` | [`sxr-core/tests/envelope_build_tests.rs`](file:///home/sprime01/projects/sxr/sxr-core/tests/envelope_build_tests.rs) |
| **Embedded JSON Schemas** | [`sxr-core/src/embedded_schema.rs`](file:///home/sprime01/projects/sxr/sxr-core/src/embedded_schema.rs) | `EnvelopeSchemaValidator`, `validate_envelope_json` | [`sxr-core/tests/envelope_build_tests.rs`](file:///home/sprime01/projects/sxr/sxr-core/tests/envelope_build_tests.rs) |
| **26 Record Kinds Registry** | [`sxr-core/src/record_kinds.csv`](file:///home/sprime01/projects/sxr/sxr-core/src/record_kinds.csv), [`sxr-core/build.rs`](file:///home/sprime01/projects/sxr/sxr-core/build.rs) | `enum RecordKind`, `RecordKindDef`, `RecordKind::ALL` | [`sxr-core/tests/envelope_build_tests.rs`](file:///home/sprime01/projects/sxr/sxr-core/tests/envelope_build_tests.rs) |
| **Threshold Evaluation & AC-SETTLEMENT-002** | [`sxr-core/src/threshold.rs`](file:///home/sprime01/projects/sxr/sxr-core/src/threshold.rs) | `evaluate`, `authorize_settlement`, `ThresholdSpec` | [`sxr-core/tests/snapshot_and_threshold_tests.rs`](file:///home/sprime01/projects/sxr/sxr-core/tests/snapshot_and_threshold_tests.rs) |
| **Proof Ingestion Gate (E7)** | [`sxr-core/src/proof_ingest.rs`](file:///home/sprime01/projects/sxr/sxr-core/src/proof_ingest.rs) | `ingest_proof_completed`, `IngestState`, `ComparisonInputs` | [`sxr-core/tests/convergence_t07_proof_ingestion.rs`](file:///home/sprime01/projects/sxr/sxr-core/tests/convergence_t07_proof_ingestion.rs) |
| **Evidence Emission Gate (E8)** | [`sxr-core/src/evidence_emit.rs`](file:///home/sprime01/projects/sxr/sxr-core/src/evidence_emit.rs) | `emit_evidence_recorded`, `ComparisonInputs` | [`sxr-core/tests/convergence_t08_evidence_emission.rs`](file:///home/sprime01/projects/sxr/sxr-core/tests/convergence_t08_evidence_emission.rs) |
| **Hybrid Logical Clock** | [`sxr-core/src/clock.rs`](file:///home/sprime01/projects/sxr/sxr-core/src/clock.rs) | `ClockPort`, `SystemClock`, `FixedClock`, `rfc3339_micros` | [`sxr-core/tests/ac_core_001.rs`](file:///home/sprime01/projects/sxr/sxr-core/tests/ac_core_001.rs) |

---

## 2. Durable Storage & Ledger Engine (`sxr-ledger`)

| Concept / Invariant | Implementation File | Primary Symbols / Functions | Relevant Tests |
| :--- | :--- | :--- | :--- |
| **SQLite Schema & DDL** | [`sxr-ledger/src/schema.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/src/schema.rs) | `SCHEMA_VERSION`, `LEDGER_DDL`, `OUTBOX_DDL` | [`sxr-ledger/tests/ac_ledger_001.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/tests/ac_ledger_001.rs) |
| **Ledger Appending & Integrity** | [`sxr-ledger/src/ledger.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/src/ledger.rs) | `Ledger::append`, `Ledger::verify_chain`, `AppendOutcome` | [`sxr-ledger/tests/ac_ledger_001.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/tests/ac_ledger_001.rs) |
| **Transactional Outbox** | [`sxr-ledger/src/outbox.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/src/outbox.rs) | `drain_pending`, `mark_projected`, `set_watermark` | [`sxr-ledger/tests/ac_ledger_002.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/tests/ac_ledger_002.rs) |
| **Writer Lock (flock)** | [`sxr-ledger/src/writer_lock.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/src/writer_lock.rs) | `WriterLock::acquire`, `WriterLock::try_acquire` | [`sxr-ledger/tests/ac_ledger_001.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/tests/ac_ledger_001.rs) |
| **Content-Addressable Storage (CAS)** | [`sxr-ledger/src/cas.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/src/cas.rs) | `Cas::put`, `Cas::get`, `Cas::has` | [`sxr-ledger/tests/ac_ledger_001.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/tests/ac_ledger_001.rs) |
| **Durable Commit Queue** | [`sxr-ledger/src/queue.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/src/queue.rs) | `enqueue`, `scan`, `drain`, `QueueReceipt` | [`sxr-cli/tests/slice6.rs`](file:///home/sprime01/projects/sxr/sxr-cli/tests/slice6.rs) |
| **Quarantine Storage** | [`sxr-ledger/src/quarantine.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/src/quarantine.rs) | `quarantine_payload`, `list_quarantined` | [`sxr-ledger/tests/ac_ledger_001.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/tests/ac_ledger_001.rs) |
| **Generation Tracking & Recovery** | [`sxr-ledger/src/generation.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/src/generation.rs), [`sxr-ledger/src/history.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/src/history.rs) | `LedgerHistory`, `new_generation`, `destructive_recovery` | [`sxr-ledger/tests/ac_ledger_002.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/tests/ac_ledger_002.rs) |
| **Filesystem Capability Probe** | [`sxr-ledger/src/capability.rs`](file:///home/sprime01/projects/sxr/sxr-ledger/src/capability.rs) | `probe_directory`, `FilesystemCapabilities` | [`sxr-cli/tests/e2e.rs`](file:///home/sprime01/projects/sxr/sxr-cli/tests/e2e.rs) |

---

## 3. Repository Discovery & Git Matching (`sxr-git`)

| Concept / Invariant | Implementation File | Primary Symbols / Functions | Relevant Tests |
| :--- | :--- | :--- | :--- |
| **Git CLI Invocation & Discovery** | [`sxr-git/src/discovery.rs`](file:///home/sprime01/projects/sxr/sxr-git/src/discovery.rs) | `discover`, `evidence_of`, `RepoEvidence`, `GitRunner` | [`sxr-git/tests/ac_identity.rs`](file:///home/sprime01/projects/sxr/sxr-git/tests/ac_identity.rs) |
| **Identity Minting (ULID)** | [`sxr-git/src/identity.rs`](file:///home/sprime01/projects/sxr/sxr-git/tests/ac_identity.rs) | `mint_identities`, `CheckoutIdentity`, `IdentityEvidence` | [`sxr-git/tests/ac_identity.rs`](file:///home/sprime01/projects/sxr/sxr-git/tests/ac_identity.rs) |
| **Continuity Verdicts (Never Merge)** | [`sxr-git/src/matcher.rs`](file:///home/sprime01/projects/sxr/sxr-git/src/matcher.rs) | `match_continuity`, `ContinuityVerdict` | [`sxr-git/tests/ac_identity.rs`](file:///home/sprime01/projects/sxr/sxr-git/tests/ac_identity.rs) |

---

## 4. Semantic Source Adapter (`sxr-df`)

| Concept / Invariant | Implementation File | Primary Symbols / Functions | Relevant Tests |
| :--- | :--- | :--- | :--- |
| **CliDomainForgeAdapter** | [`sxr-df/src/adapter.rs`](file:///home/sprime01/projects/sxr/sxr-df/src/adapter.rs) | `CliDomainForgeAdapter::resolve`, `SemanticContextRequest` | [`sxr-df/tests/real_binary_tests.rs`](file:///home/sprime01/projects/sxr/sxr-df/tests/real_binary_tests.rs) |
| **Semantic Errors** | [`sxr-df/src/error.rs`](file:///home/sprime01/projects/sxr/sxr-df/src/error.rs) | `enum SemanticError` | [`sxr-df/tests/fixture_tests.rs`](file:///home/sprime01/projects/sxr/sxr-df/tests/fixture_tests.rs) |

---

## 5. Verification Execution & Parsing (`sxr-verify`)

| Concept / Invariant | Implementation File | Primary Symbols / Functions | Relevant Tests |
| :--- | :--- | :--- | :--- |
| **Verifier Port Abstraction** | [`sxr-verify/src/port.rs`](file:///home/sprime01/projects/sxr/sxr-verify/src/port.rs) | `trait VerifierPort`, `VerifierOutcome`, `direction_for` | [`sxr-verify/src/verifier.rs`](file:///home/sprime01/projects/sxr/sxr-verify/src/verifier.rs) |
| **Cargo Test Verifier** | [`sxr-verify/src/verifier.rs`](file:///home/sprime01/projects/sxr/sxr-verify/src/verifier.rs) | `CargoTestVerifier`, `VERIFIER_ID`, `direction_of` | [`sxr-cli/tests/slice3.rs`](file:///home/sprime01/projects/sxr/sxr-cli/tests/slice3.rs) |
| **Cargo JSON Stream Parser** | [`sxr-verify/src/cargo_json.rs`](file:///home/sprime01/projects/sxr/sxr-verify/src/cargo_json.rs) | `parse_cargo_json`, `ParsedSummary` | [`sxr-verify/src/cargo_json.rs`](file:///home/sprime01/projects/sxr/sxr-verify/src/cargo_json.rs) |

---

## 6. CLI, Daemon, Projections & Diagnostics (`sxr-cli`)

| Concept / Invariant | Implementation File | Primary Symbols / Functions | Relevant Tests |
| :--- | :--- | :--- | :--- |
| **Command Dispatch & Exit Codes** | [`sxr-cli/src/main.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/main.rs) | `main`, `dispatch`, `exit_for_error`, `enum Command` | [`sxr-cli/tests/e2e.rs`](file:///home/sprime01/projects/sxr/sxr-cli/tests/e2e.rs) |
| **Command Handlers** | [`sxr-cli/src/commands.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/commands.rs) | `repo_enroll`, `snapshot_create`, `run_begin`, `evidence_create`, `settlement_register` | [`proof/gate1.sh`](file:///home/sprime01/projects/sxr/proof/gate1.sh) |
| **Relational & Graph Projections** | [`sxr-cli/src/projections.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/projections.rs) | `PROJECTION_DDL`, `apply_row_with_cas`, `rebuild_all`, `export_canonical_json` | [`sxr-cli/tests/slice3.rs`](file:///home/sprime01/projects/sxr/sxr-cli/tests/slice3.rs), [`sxr-cli/tests/slice4.rs`](file:///home/sprime01/projects/sxr/sxr-cli/tests/slice4.rs) |
| **FTS5 External Content Search** | [`sxr-cli/src/projections.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/projections.rs) | `sxr_fts`, `sxr_fts_content`, `rebuild_fts` | [`sxr-cli/tests/slice3.rs`](file:///home/sprime01/projects/sxr/sxr-cli/tests/slice3.rs) |
| **Slice 5 Diagnostic Loop & Friction** | [`sxr-cli/src/commands.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/commands.rs) | `residual_compute`, `diagnostic_enumerate`, `diagnostic_settle`, `diagnostic_propose` | [`sxr-cli/tests/slice5.rs`](file:///home/sprime01/projects/sxr/sxr-cli/tests/slice5.rs), [`proof/slice5.sh`](file:///home/sprime01/projects/sxr/proof/slice5.sh) |
| **Optional UNIX Socket Daemon** | [`sxr-cli/src/daemon.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/daemon.rs) | `daemon_run`, `record_request`, `validate_record_request`, `drain_all_queues` | [`sxr-cli/tests/slice6.rs`](file:///home/sprime01/projects/sxr/sxr-cli/tests/slice6.rs), [`proof/slice6.sh`](file:///home/sprime01/projects/sxr/proof/slice6.sh) |
| **Durability Scanner & Spool** | [`sxr-cli/src/durability.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/durability.rs) | `scan_pending`, `queued_entries`, `spooled_entries` | [`sxr-cli/tests/slice6.rs`](file:///home/sprime01/projects/sxr/sxr-cli/tests/slice6.rs) |
| **Ledger IO & Writer Session** | [`sxr-cli/src/ledger_io.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/ledger_io.rs) | `open_repo_ledger`, `append_record`, `RepoLedger` | [`proof/gate1.sh`](file:///home/sprime01/projects/sxr/proof/gate1.sh) |
| **State, Registry & XDG Paths** | [`sxr-cli/src/state.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/state.rs) | `struct Ctx`, `read_enrollment`, `write_enrollment`, `read_semantic` | [`sxr-cli/tests/e2e.rs`](file:///home/sprime01/projects/sxr/sxr-cli/tests/e2e.rs) |
| **Determinism Seam for Parity** | [`sxr-cli/src/determinism.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/determinism.rs) | `mint_id`, `now_utc`, `SXR_TEST_ID_STREAM`, `SXR_TEST_FIXED_CLOCK` | [`sxr-cli/tests/slice6.rs`](file:///home/sprime01/projects/sxr/sxr-cli/tests/slice6.rs) |

---

## 7. Schemas, Proofs & Infrastructure

| Subsystem / Artifact | Path | Purpose / Description |
| :--- | :--- | :--- |
| **CEP Semantic Envelope Schema** | [`schemas/cep-semantic-envelope.schema.json`](file:///home/sprime01/projects/sxr/schemas/cep-semantic-envelope.schema.json) | The authoritative JSON Schema profile for all CEP runtime envelopes. |
| **CEP Schema Mapping Guide** | [`schemas/cep-semantic-envelope.mapping.md`](file:///home/sprime01/projects/sxr/schemas/cep-semantic-envelope.mapping.md) | Maps CEP-0008 distinctions to schema properties and documents the anti-collapse shapes. |
| **Output JSON Schema** | [`schemas/sxr-output-v1.json`](file:///home/sprime01/projects/sxr/schemas/sxr-output-v1.json) | Schema for JSON responses emitted on stdout by the `sxr` CLI binary. |
| **Plan JSON Schema** | [`schemas/sxr-plan-v1.json`](file:///home/sprime01/projects/sxr/schemas/sxr-plan-v1.json) | Schema for `plan.created` records and verifier execution configurations. |
| **Record JSON Schema** | [`schemas/sxr-record-v1.json`](file:///home/sprime01/projects/sxr/schemas/sxr-record-v1.json) | Wire schema for daemon record submission over the Unix domain socket. |
| **Gate 1 Proof Script** | [`proof/gate1.sh`](file:///home/sprime01/projects/sxr/proof/gate1.sh) | End-to-end proof: enrollment through verified settlement and projection rebuild. |
| **Slice 3 Proof Script** | [`proof/slice3.sh`](file:///home/sprime01/projects/sxr/proof/slice3.sh) | Gate 3 proof: $G_{declared}$ graph projection and FTS5 contentless indexing. |
| **Slice 4 Proof Script** | [`proof/slice4.sh`](file:///home/sprime01/projects/sxr/proof/slice4.sh) | Gate 4 proof: $G_{observed}$ graph projection, residuals, and anti-collapse corpus export. |
| **Slice 5 Proof Script** | [`proof/slice5.sh`](file:///home/sprime01/projects/sxr/proof/slice5.sh) | Gate 5 proof: 9-locus diagnostic attribution loop and asymmetric model friction. |
| **Slice 6 Proof Script** | [`proof/slice6.sh`](file:///home/sprime01/projects/sxr/proof/slice6.sh) | Gate 6 proof: daemon durability parity (AC-DAEMON-001) and crash recovery. |
| **Systemd Service Unit** | [`packaging/sxr.service`](file:///home/sprime01/projects/sxr/packaging/sxr.service) | Service definition for background deployment of the `sxr daemon`. |
| **Ablation Evaluation Suite** | [`ablation/eval/`](file:///home/sprime01/projects/sxr/ablation/eval/) | Empirical evaluation harness testing hypothesis attribution across evidence revisions. |

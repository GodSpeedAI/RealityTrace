# Workflow Trace: Semantic Snapshot Creation

This document traces how SXR compiles declarative `.sea` models via DomainForge and records immutable `semantic.snapshot` records in the ledger.

---

## 1. Summary
`sxr snapshot create` extracts the declarative domain model from a repository's `.sea` files by invoking DomainForge. If compilation succeeds, SXR records a `semantic.snapshot` envelope carrying representation $D$, declaration keys, and the semantic closure hash. If validation fails, SXR records a schema-valid failure envelope with an explicit `OmissionRecord`, refusing to manufacture dummy checkpoint hashes.

---

## 2. Sequence Diagram

```mermaid
sequenceDiagram
    autonumber
    actor User as Operator / Agent
    participant CLI as sxr-cli (commands.rs)
    participant State as sxr-cli::state
    participant DFAdapt as sxr-df::adapter
    participant DFBin as domainforge CLI binary
    participant LedgerIO as sxr-cli::ledger_io
    participant Ledger as sxr-ledger::ledger
    participant Proj as sxr-cli::projections

    User->>CLI: sxr snapshot create --repo <id> --json
    CLI->>State: require_enrollment & require_semantic
    CLI->>DFAdapt: CliDomainForgeAdapter::resolve(req)
    DFAdapt->>DFBin: domainforge envelope --emit both --scope <scope> <entry.sea>
    DFBin-->>DFAdapt: Raw Envelope + Representation JSON
    DFAdapt->>DFAdapt: Classify by envelope shape
    alt D Available (Valid or Semantically Invalid with Diagnostics)
        DFAdapt-->>CLI: SemanticContext::Resolved { representation_json, closure_hash, model_validation_status, ... }
    else Representation Unavailable (Syntax/Import Failure)
        DFAdapt-->>CLI: SemanticContext::ValidationFailed { invalid_declared_checkpoint_hash, omission, ... }
    end
    CLI->>LedgerIO: open_repo_ledger(ctx, repo_id)
    CLI->>LedgerIO: append_record(RecordKind::SemanticSnapshot, envelope)
    LedgerIO->>Ledger: Ledger::append
    Ledger-->>CLI: LedgerPosition { ledger_id, append_ordinal }
    CLI->>Proj: apply_row_with_cas (populates sxr_snapshot & sxr_decl_*)
    CLI-->>User: JSON { snapshot_id, semantic_closure_hash, model_validation_status, semantics_version }
```

---

## 3. Detailed Path & Implementation Symbols

1. **CLI Invocation**:
   - Matches `Command::Snapshot(SnapshotCmds::Create { repo })` in [`sxr-cli/src/main.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/main.rs).
   - Calls [`sxr-cli/src/commands.rs:snapshot_create`](file:///home/sprime01/projects/sxr/sxr-cli/src/commands.rs).
2. **Configuration Loading**:
   - Reads `registry.json` and `semantic.json` from `$XDG_DATA_HOME/sxr/repos/<repo_id>/`.
   - Assembles `SemanticContextRequest` specifying `semantic_root`, `entry_logical_path`, and `cas_root`.
3. **DomainForge Execution**:
   - Calls [`sxr_df::adapter::CliDomainForgeAdapter::resolve`](file:///home/sprime01/projects/sxr/sxr-df/src/adapter.rs).
   - Resolves binary path from `SXR_DOMAINFORGE_BIN` or `PATH`.
   - Spawns `domainforge envelope --emit both --scope <scope> ...` under 16 MiB reader bounds and 30-second execution timeout.
4. **Envelope Classification**:
   - Parses stdout into JSON.
   - If representation $D$ is present:
     - Extracts representation $D$, computes SHA-256 `content_hash` and `semantic_closure_hash`.
     - Extracts declarations into `Vec<DeclKey>`.
     - Extracts `model_validation_status` (`"valid"` or `"invalid"`) and diagnostics.
     - Returns `SemanticContext::Resolved`.
   - If representation $D$ is absent and `omissions` contains `representation_unavailable`:
     - Extracts `OmissionRecord` and compiler diagnostics.
     - Computes `invalid_declared_checkpoint_hash` from the resolution inputs.
     - Returns `SemanticContext::ValidationFailed`.
5. **Ledger Commit**:
   - Builds `RecordKind::SemanticSnapshot` envelope using [`sxr_cli::envelopes::build`](file:///home/sprime01/projects/sxr/sxr-cli/src/envelopes.rs).
   - Appends to `ledger.db` under the writer lock.
6. **Projection Update**:
   - [`sxr_cli::projections::apply_row_with_cas`](file:///home/sprime01/projects/sxr/sxr-cli/src/projections.rs) upserts `sxr_snapshot`, stamping `semantics_version = "v2-independent-validity"`.
   - For valid models, populates $G_{declared}$ tables (`sxr_decl_node`, `sxr_decl_edge`).
   - For invalid models, preserves diagnostic records and model status without admitting unverified declarations into $G_{declared}$.

---

## 4. State Changes
- **Ledger**: Appends `semantic.snapshot` record at next `append_ordinal`.
- **Projections**:
  - Row added to `sxr_snapshot`.
  - Rows added to `sxr_decl_node` and `sxr_decl_edge` for every concept, entity, and relation.
- **CAS Store** (if representation was large): New blob created at `cas/sha256/<prefix>/<hash>`.

---

## 5. Failure Branches
- **Repo Not Enrolled / Not Configured**: Exits code 3 with missing prerequisite message.
- **DomainForge Binary Missing**: Fails with `SemanticError::BinaryNotFound`; exits code 3.
- **Malformed Compiler Output**: If DomainForge emits unparseable text or an envelope that violates `cep-semantic-envelope.schema.json`, SXR exits code 4 without committing.

---

## 6. Source Trail
- [`sxr-cli/src/commands.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/commands.rs) — `snapshot_create`.
- [`sxr-df/src/adapter.rs`](file:///home/sprime01/projects/sxr/sxr-df/src/adapter.rs) — `CliDomainForgeAdapter::resolve`.
- [`sxr-core/src/types.rs`](file:///home/sprime01/projects/sxr/sxr-core/src/types.rs) — `SemanticContext`, `OmissionRecord`, `SnapshotRow`.
- [`sxr-cli/src/projections.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/projections.rs) — `apply_row_with_cas`.
- [`proof/gate1.sh`](file:///home/sprime01/projects/sxr/proof/gate1.sh) — Snapshot creation proof steps.

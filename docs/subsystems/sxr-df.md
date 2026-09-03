# Subsystem Guide: `sxr-df`

`sxr-df` implements the `SemanticSourcePort` interface by connecting SXR to the external `domainforge` CLI compiler. It parses declarative `.sea` models, extracts semantic representations ($D$), validates CEP envelopes, and produces structured `SemanticContext` objects.

---

## 1. Purpose
To extract declarative semantics from `.sea` models using DomainForge, ensuring that valid models produce content-addressed semantic snapshots while invalid models produce explicit, traceable omission records rather than dummy hashes.

---

## 2. Responsibilities
- Implement `SemanticSourcePort` via `CliDomainForgeAdapter` (`adapter.rs`).
- Execute the `domainforge` binary with appropriate CLI flags (`envelope --emit cep`).
- Classify DomainForge output **strictly by envelope shape**, never by shell exit code.
- Extract representations ($D$), content hashes, and semantic closure hashes when construction succeeds.
- Extract typed `OmissionRecord` items and diagnostic lists when construction fails.
- Materialize large representation documents into Content-Addressable Storage (`cas://sha256:<hex>`).

---

## 3. Non-Responsibilities
- **Language Parsing**: Does not compile or parse `.sea` grammar directly; delegates compilation to `domainforge`.
- **Ledger Storage**: Does not append snapshots to the SQLite ledger (delegated to `sxr-cli`).
- **Graph Materialization**: Does not project declarations into relational tables.

---

## 4. Position in the System
`sxr-df` depends on `sxr-core`. It is invoked by `sxr-cli` during `sxr snapshot create`.

```mermaid
flowchart LR
    CLI["sxr-cli (snapshot create)"] --> SxrDF["sxr-df (CliDomainForgeAdapter)"]
    SxrDF --> Core["sxr-core"]
    SxrDF -->|Spawns domainforge| DF["domainforge binary"]
    DF --> Model[(".sea models")]
```

---

## 5. Core Abstractions

| Symbol | Location | Description |
| :--- | :--- | :--- |
| `struct CliDomainForgeAdapter` | [`src/adapter.rs`](file:///home/sprime01/projects/sxr/sxr-df/src/adapter.rs) | Adapter invoking the external DomainForge binary via subprocess. |
| `struct SemanticContextRequest` | [`src/adapter.rs`](file:///home/sprime01/projects/sxr/sxr-df/src/adapter.rs) | Input request specifying `semantic_root`, `entry_logical_path`, registry, and packs. |
| `struct AdapterCapabilities` | [`src/adapter.rs`](file:///home/sprime01/projects/sxr/sxr-df/src/adapter.rs) | Describes adapter capabilities (`producer`, `cep_envelope_mode`, etc.). |
| `enum SemanticError` | [`src/error.rs`](file:///home/sprime01/projects/sxr/sxr-df/src/error.rs) | Typed errors (binary missing, malformed JSON, schema violation). |

---

## 6. Internal Operation

### Classification by Envelope Shape
DomainForge may exit with code 1 for multiple different reasons (syntax error, type failure, missing import). Instead of relying on exit codes, `CliDomainForgeAdapter` inspects the emitted CEP envelope:
1. **Resolved (`SemanticContext::Resolved`)**:
   - `validation_status == "validated"` and `representations` section is present.
   - SXR extracts `representation_json`, computes `content_hash` and `semantic_closure_hash`, and extracts declaration keys (`DeclKey`).
2. **Validation Failed (`SemanticContext::ValidationFailed`)**:
   - `conformance_status == "non_conformant"`, no representations section, and an explicit omission record is carried.
   - SXR binds the checkpoint hash to the exact failed inputs digest. **It never synthesizes placeholder hashes.**
3. **Malformed Output**:
   - If the emitted envelope violates schema or does not conform to either shape, the adapter fails closed with `SemanticError::MalformedEnvelope`.

### CAS Representation Materialization
When `cas_root` is supplied in the request and DomainForge returns a large representation with a `content_ref` URI (`cas://sha256:<hex>`), the adapter writes the verified representation bytes directly into `<cas_root>/sha256/` and ensures its content hash matches.

---

## 7. State
Stateless. Spawns ephemeral `domainforge` processes and optionally writes blobs into the designated CAS directory.

---

## 8. Lifecycle
Invoked when `sxr snapshot create` is executed. The path to the `domainforge` binary is determined by the `SXR_DOMAINFORGE_BIN` environment variable or resolved from system `PATH`.

---

## 9. Failure Modes & Diagnostic Codes
- `SemanticError::BinaryNotFound`: The `domainforge` binary was not found or is not executable.
- `SemanticError::MalformedEnvelope`: DomainForge produced JSON that does not parse as a CEP envelope.
- `SemanticError::HashMismatch`: The recomputed hash of representation $D$ does not match the hash declared in the envelope.

---

## 10. Extension Points
- Implement alternative `SemanticSourcePort` adapters (e.g. an in-process Rust compiler or WebAssembly bridge) returning `SemanticContext`.

---

## 11. Source Trail
- [`sxr-df/src/lib.rs`](file:///home/sprime01/projects/sxr/sxr-df/src/lib.rs) — Subsystem exports.
- [`sxr-df/src/adapter.rs`](file:///home/sprime01/projects/sxr/sxr-df/src/adapter.rs) — Subprocess invocation, envelope parsing, and context classification.
- [`sxr-df/src/error.rs`](file:///home/sprime01/projects/sxr/sxr-df/src/error.rs) — Error definitions.
- [`sxr-df/tests/real_binary_tests.rs`](file:///home/sprime01/projects/sxr/sxr-df/tests/real_binary_tests.rs) — Integration tests with the real DomainForge binary.
- [`sxr-df/tests/fixture_tests.rs`](file:///home/sprime01/projects/sxr/sxr-df/tests/fixture_tests.rs) — Offline fixture tests for valid and invalid envelope shapes.

# RealityTrace

**RealityTrace**: Cryptographic observation, evidence, and settlement runtime.

RealityTrace is an append-only, cryptographic audit and settlement runtime designed for software engineering verification and autonomous developer agent telemetry. It records what actually happened during software development and testing, verifies event streams against strict semantic envelope contracts, maintains tamper-evident hash-chained ledgers, and deterministically evaluates whether verification obligations have been settled.

---

## What Problem It Solves

Modern agentic engineering tools and automated CI pipelines suffer from severe **epistemological collapse**:
- **Tests treated as truth**: When a test passes, an agent assumes the feature works; when a flaky test fails, the agent assumes the code is wrong.
- **Silent history loss**: Test retries overwrite prior failures, destroying evidence of flakiness or environmental corruption.
- **Premature model blame**: When automated verification fails, developer agents typically assume their code or architecture model is broken and hallucinate refactorings, even when the real defect is an unconfigured test environment, network timeout, or faulty test harness.
- **Unearned settlement**: Verification is declared "successful" simply because a process exited with code `0`, without proving that the observations satisfied pre-registered acceptance criteria.

RealityTrace provides an immutable, verifiable substrate that enforces **anti-collapse boundaries**:
- Raw execution output is preserved as an **Observation**, not proof.
- Observations are transformed into **Evidence** only through explicit, pre-declared derivation rules.
- **Settlement** requires evaluating strictly earlier evidence against a pre-registered **Threshold**.
- A 9-locus **Diagnostic Attribution Loop** prevents agents from modifying code or domain models until infrastructure, verifier, and environmental failure loci have been systematically excluded with evidence.

---

## Installation

### From crates.io

Install the RealityTrace CLI:

```bash
cargo install realitytrace
```

### From Source

```bash
git clone https://github.com/GodSpeedAI/RealityTrace.git
cd RealityTrace
cargo build --release
```

The resulting binary will be located at `target/release/realitytrace`.

---

## CLI Usage

```bash
# Enroll a repository in RealityTrace
realitytrace repo enroll

# Inspect repository identity and status
realitytrace repo status

# Build a verification plan
realitytrace plan build --repo <REPO_ID> --question <QUESTION_REF> --threshold <THRESHOLD_REF>

# Execute a plan with verification telemetry
realitytrace verify run --plan <PLAN_JSON>

# Verify cryptographic hash chain integrity of the ledger
realitytrace ledger verify --repo <REPO_ID>

# Export projections and anti-collapse corpus
realitytrace projections export --repo <REPO_ID> --canonical-json
```

---

## Architecture & Subsystems

The workspace consists of focused Rust crates:

- **`realitytrace`** (`sxr-cli`): The command-line interface, daemon, and lifecycle commands.
- **`sxr-core`**: Core domain types, canonical JSON serialization, cryptographic hash-chain algebra, and CEP-0008 semantic envelope validation. Zero I/O.
- **`sxr-ledger`**: SQLite dense hash-chained append-only storage, commit queue, and durability management.
- **`sxr-git`**: Git worktree discovery, lineage verification, and ULID repository identity.
- **`sxr-df`**: Semantic model extraction adapter (DomainForge integration).
- **`sxr-verify`**: Process sandbox execution, environment isolation, and test runner telemetry parsers.

*(Note: `sxr` was the internal development codename for the engine; public releases and user-facing tooling are branded as **RealityTrace**).*

---

## Documentation

Comprehensive architecture, tutorials, and specifications are in the [`docs/`](./docs) directory:
- [System Overview & Orientation](./docs/index.md)
- [Conceptual Mental Model](./docs/mental-model.md)
- [Architecture Specification](./architecture.md)
- [Documentation Map](./documentation-map.md)
- [Source & Symbol Traceability Matrix](./source-map.md)

---

## License

Licensed under the [Apache License, Version 2.0](LICENSE).

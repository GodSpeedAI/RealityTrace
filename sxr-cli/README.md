# realitytrace

**RealityTrace**: Cryptographic observation, evidence, and settlement runtime CLI.

RealityTrace records what actually happens during software development and verification, validates event streams against strict semantic envelope contracts (CEP-0008), maintains tamper-evident hash-chained ledgers, and deterministically evaluates whether verification obligations have been settled.

## Installation

```bash
cargo install realitytrace
```

## Quick Start

```bash
# Enroll a repository
realitytrace repo enroll .

# Inspect enrolled repositories
realitytrace repo list

# Execute a verification run or ingest telemetry
realitytrace verify run --run <RUN_ID>

# Verify cryptographic ledger integrity
realitytrace ledger verify --repo <REPO_ID>

# Export projections
realitytrace projections export --repo <REPO_ID> --canonical-json
```

## Architecture

RealityTrace is composed of modular Rust crates:
- `realitytrace` (`sxr-cli`): The command-line interface and daemon.
- `sxr-core`: Pure domain types, canonical JSON serialization, and cryptographic hash-chain algebra.
- `sxr-ledger`: SQLite dense hash-chained append-only storage and commit queue.
- `sxr-git`: Git worktree discovery, lineage verification, and ULID repository identity.
- `sxr-df`: Semantic model extraction adapter (DomainForge).
- `sxr-verify`: Sandbox process execution and test telemetry parser.

## License

Licensed under the [Apache License, Version 2.0](../LICENSE).


# SXR Documentation Map

This document establishes the information architecture of the **Semantic Exhaust Runtime (SXR / RealityTrace)** documentation system. Each page is classified by its primary audience need, Diátaxis quadrant, prerequisites, and relationships.

---

## Structure Overview

The documentation is organized in nine progressive layers (Layer 0 to Layer 8):

| Layer | Section | Primary Reader Need | Diátaxis Category |
| :--- | :--- | :--- | :--- |
| **Layer 0** | [Orientation](#layer-0--orientation) | "What is this project, why does it exist, and how do I navigate it?" | Orientation / Gateway |
| **Layer 1** | [Mental Model](#layer-1--mental-model) | "How should I think about this system conceptually before seeing code?" | Explanation |
| **Layer 2** | [Architecture](#layer-2--architecture) | "What is the structural and physical topology of the system?" | Explanation / Architecture |
| **Layer 3** | [Subsystems](#layer-3--subsystems) | "How does each individual crate/subsystem operate internally?" | Reference & Explanation |
| **Layer 4** | [Workflows](#layer-4--workflows-and-execution-traces) | "What actually happens step-by-step when I perform operation X?" | Explanation & How-To |
| **Layer 5** | [Explanation](#layer-5--explanation--design-rationale) | "Why was it built this way, and what constraints/tradeoffs govern it?" | Explanation |
| **Layer 6** | [Tutorials](#layer-6--tutorials) | "Teach me the core concepts through a guided, reproducible hands-on task." | Tutorial |
| **Layer 7** | [How-To Guides](#layer-7--how-to-guides) | "Show me how to solve a specific problem or perform an operational task." | How-To Guide |
| **Layer 8** | [Reference](#layer-8--reference) | "Give me exact, complete technical specifications, schemas, and catalogs." | Reference |

---

## Page Catalog

### Root Synthesis Documents

#### [`architecture.md`](./architecture.md)
- **Purpose**: Canonical high-level architectural specification and subsystem decomposition of SXR.
- **Audience Need**: System-level structural comprehension for architects and engineers preparing to contribute.
- **Diátaxis Type**: Explanation / Architecture.
- **Prerequisites**: None (conceptual awareness of verification and ledgers is helpful).
- **Related Pages**: [`docs/index.md`](./docs/index.md), [`docs/mental-model.md`](./docs/mental-model.md), [`docs/architecture/logical.md`](./docs/architecture/logical.md).

#### [`documentation-map.md`](./documentation-map.md)
- **Purpose**: Structural directory and meta-guide for the documentation tree.
- **Audience Need**: Discovery and navigation across the knowledge system.
- **Diátaxis Type**: Meta-Reference.
- **Prerequisites**: None.
- **Related Pages**: [`docs/index.md`](./docs/index.md), [`source-map.md`](./source-map.md).

#### [`source-map.md`](./source-map.md)
- **Purpose**: Traceability matrix mapping capabilities, concepts, and invariants to implementation symbols and tests.
- **Audience Need**: Direct code lookup, symbol tracing, and verification of claims.
- **Diátaxis Type**: Reference / Index.
- **Prerequisites**: Familiarity with repository concepts.
- **Related Pages**: [`architecture.md`](./architecture.md), Subsystem guides in `docs/subsystems/`.

---

### Layer 0 — Orientation

#### [`docs/index.md`](./docs/index.md)
- **Purpose**: Entry point and orientation portal for all audiences.
- **Audience Need**: Understand system identity, purpose, high-level mechanics, core vocabulary, and navigation.
- **Diátaxis Type**: Orientation.
- **Prerequisites**: None.
- **Related Pages**: [`docs/mental-model.md`](./docs/mental-model.md), [`docs/tutorials/quickstart.md`](./docs/tutorials/quickstart.md).

---

### Layer 1 — Mental Model

#### [`docs/mental-model.md`](./docs/mental-model.md)
- **Purpose**: Conceptual framing of SXR's epistemological pipeline and anti-collapse axioms.
- **Audience Need**: Form a clear mental representation of declarations, observations, evidence, thresholds, and settlements.
- **Diátaxis Type**: Explanation.
- **Prerequisites**: [`docs/index.md`](./docs/index.md).
- **Related Pages**: [`docs/explanation/anti-collapse-epistemology.md`](./docs/explanation/anti-collapse-epistemology.md), [`docs/architecture/logical.md`](./docs/architecture/logical.md).

---

### Layer 2 — Architecture

#### [`docs/architecture/logical.md`](./docs/architecture/logical.md)
- **Purpose**: Decomposition into logical layers, core domain boundaries, and port traits.
- **Audience Need**: Understand conceptual component responsibilities and data flow boundaries.
- **Diátaxis Type**: Explanation / Architecture.
- **Prerequisites**: [`docs/mental-model.md`](./docs/mental-model.md).
- **Related Pages**: [`docs/architecture/runtime.md`](./docs/architecture/runtime.md), [`docs/architecture/dependency.md`](./docs/architecture/dependency.md).

#### [`docs/architecture/runtime.md`](./docs/architecture/runtime.md)
- **Purpose**: Concrete execution topologies: standalone CLI vs background UNIX daemon.
- **Audience Need**: Understand active operating system processes, IPC sockets, thread models, and concurrency.
- **Diátaxis Type**: Explanation / Architecture.
- **Prerequisites**: [`docs/architecture/logical.md`](./docs/architecture/logical.md).
- **Related Pages**: [`docs/workflows/daemon-and-durability.md`](./docs/workflows/daemon-and-durability.md), [`docs/how-to/operate-daemon.md`](./docs/how-to/operate-daemon.md).

#### [`docs/architecture/data-storage.md`](./docs/architecture/data-storage.md)
- **Purpose**: Detail storage formats: SQLite STRICT schema, hash chains, transactional outbox, CAS, and commit queues.
- **Audience Need**: Understand how data persists, integrity is cryptographically enforced, and projections are maintained.
- **Diátaxis Type**: Reference / Architecture.
- **Prerequisites**: [`docs/architecture/logical.md`](./docs/architecture/logical.md).
- **Related Pages**: [`docs/reference/schemas.md`](./docs/reference/schemas.md), [`docs/subsystems/sxr-ledger.md`](./docs/subsystems/sxr-ledger.md).

#### [`docs/architecture/dependency.md`](./docs/architecture/dependency.md)
- **Purpose**: Crate dependency DAG, internal rules (e.g. zero-I/O `sxr-core`), and external crate footprints.
- **Audience Need**: Understand build dependencies, crate boundaries, and architectural isolation invariants.
- **Diátaxis Type**: Explanation / Architecture.
- **Prerequisites**: [`docs/architecture/logical.md`](./docs/architecture/logical.md).
- **Related Pages**: [`source-map.md`](./source-map.md).

#### [`docs/architecture/security-trust.md`](./docs/architecture/security-trust.md)
- **Purpose**: Trust boundaries, non-mutation invariants (AC-SECURITY-001), XDG sandboxing, and identity isolation.
- **Audience Need**: Evaluate security properties, file system integrity guarantees, and multi-tenant repository safety.
- **Diátaxis Type**: Explanation / Architecture.
- **Prerequisites**: [`docs/architecture/runtime.md`](./docs/architecture/runtime.md).
- **Related Pages**: [`docs/explanation/isolation-invariants.md`](./docs/explanation/isolation-invariants.md).

---

### Layer 3 — Subsystems

#### [`docs/subsystems/sxr-core.md`](./docs/subsystems/sxr-core.md)
- **Purpose**: Deep dive into `sxr-core` (pure types, CEP validation, 26 record kinds, threshold algebra, clock, proof/evidence gates).
- **Audience Need**: Understand domain contracts, zero-I/O rules, and cryptographic chain formulas.
- **Diátaxis Type**: Subsystem Guide (Explanation & Reference).
- **Prerequisites**: [`docs/architecture/logical.md`](./docs/architecture/logical.md).
- **Related Pages**: [`docs/reference/record-kinds.md`](./docs/reference/record-kinds.md).

#### [`docs/subsystems/sxr-ledger.md`](./docs/subsystems/sxr-ledger.md)
- **Purpose**: Deep dive into `sxr-ledger` (SQLite adapter, flock locking, outbox watermark, generation tracking, CAS storage).
- **Audience Need**: Understand durable append operations, concurrency control, and storage verification.
- **Diátaxis Type**: Subsystem Guide.
- **Prerequisites**: [`docs/architecture/data-storage.md`](./docs/architecture/data-storage.md).
- **Related Pages**: [`docs/workflows/daemon-and-durability.md`](./docs/workflows/daemon-and-durability.md).

#### [`docs/subsystems/sxr-git.md`](./docs/subsystems/sxr-git.md)
- **Purpose**: Deep dive into `sxr-git` (RepositoryDiscoveryPort, minted ULID identity vs git commit hash, rev-parse discovery).
- **Audience Need**: Understand how repositories are enrolled, identified, and matched across relocations.
- **Diátaxis Type**: Subsystem Guide.
- **Prerequisites**: [`docs/architecture/logical.md`](./docs/architecture/logical.md).
- **Related Pages**: [`docs/workflows/repo-enrollment.md`](./docs/workflows/repo-enrollment.md).

#### [`docs/subsystems/sxr-df.md`](./docs/subsystems/sxr-df.md)
- **Purpose**: Deep dive into `sxr-df` (SemanticSourcePort, CliDomainForgeAdapter, .sea model resolution).
- **Audience Need**: Understand how declarative semantics and snapshots are extracted from DomainForge.
- **Diátaxis Type**: Subsystem Guide.
- **Prerequisites**: [`docs/architecture/logical.md`](./docs/architecture/logical.md).
- **Related Pages**: [`docs/workflows/semantic-snapshot.md`](./docs/workflows/semantic-snapshot.md).

#### [`docs/subsystems/sxr-verify.md`](./docs/subsystems/sxr-verify.md)
- **Purpose**: Deep dive into `sxr-verify` (VerifierPort, CargoTestVerifier, cargo-json parsing, observation generation).
- **Audience Need**: Understand how test processes run, how outputs are parsed into observations without premature evidence claims.
- **Diátaxis Type**: Subsystem Guide.
- **Prerequisites**: [`docs/architecture/logical.md`](./docs/architecture/logical.md).
- **Related Pages**: [`docs/workflows/verification-run.md`](./docs/workflows/verification-run.md), [`docs/how-to/add-verifier.md`](./docs/how-to/add-verifier.md).

#### [`docs/subsystems/sxr-cli.md`](./docs/subsystems/sxr-cli.md)
- **Purpose**: Deep dive into `sxr-cli` (CLI commands, state management, projections engine, background daemon, durability scanner).
- **Audience Need**: Understand user-facing orchestration, relational projection synchronization, and daemon mechanics.
- **Diátaxis Type**: Subsystem Guide.
- **Prerequisites**: [`docs/architecture/runtime.md`](./docs/architecture/runtime.md).
- **Related Pages**: [`docs/reference/cli.md`](./docs/reference/cli.md).

#### [`docs/subsystems/ablation.md`](./docs/subsystems/ablation.md)
- **Purpose**: Deep dive into the `ablation/` scientific study suite, evidence revisions (ER001/ER002), and execution runs.
- **Audience Need**: Understand how SXR's empirical hypothesis attribution and anti-collapse guarantees are tested against real LLMs.
- **Diátaxis Type**: Subsystem Guide / Explanation.
- **Prerequisites**: [`docs/mental-model.md`](./docs/mental-model.md).
- **Related Pages**: [`docs/explanation/asymmetric-diagnostic-friction.md`](./docs/explanation/asymmetric-diagnostic-friction.md).

---

### Layer 4 — Workflows and Execution Traces

#### [`docs/workflows/repo-enrollment.md`](./docs/workflows/repo-enrollment.md)
- **Purpose**: Execution trace of repository enrollment and semantic configuration.
- **Audience Need**: Trace data structures and files created during `repo enroll` and `semantic configure`.
- **Diátaxis Type**: Workflow Trace.
- **Prerequisites**: [`docs/subsystems/sxr-git.md`](./docs/subsystems/sxr-git.md).
- **Related Pages**: [`docs/how-to/enroll-and-configure.md`](./docs/how-to/enroll-and-configure.md).

#### [`docs/workflows/semantic-snapshot.md`](./docs/workflows/semantic-snapshot.md)
- **Purpose**: Execution trace of `snapshot create`, model validation, and omission record generation.
- **Audience Need**: Follow how `.sea` files become immutable `semantic.snapshot` ledger records.
- **Diátaxis Type**: Workflow Trace.
- **Prerequisites**: [`docs/subsystems/sxr-df.md`](./docs/subsystems/sxr-df.md).
- **Related Pages**: [`docs/explanation/anti-collapse-epistemology.md`](./docs/explanation/anti-collapse-epistemology.md).

#### [`docs/workflows/verification-run.md`](./docs/workflows/verification-run.md)
- **Purpose**: Complete trace of a verification run (`run begin` $\to$ `plan create` $\to$ `verify run` $\to$ `evidence create` $\to$ `settlement register` $\to$ `run finish`).
- **Audience Need**: Follow the exact sequence of records and invariants guarding settlement registration.
- **Diátaxis Type**: Workflow Trace.
- **Prerequisites**: [`docs/mental-model.md`](./docs/mental-model.md).
- **Related Pages**: [`docs/tutorials/quickstart.md`](./docs/tutorials/quickstart.md), [`docs/how-to/execute-verification-run.md`](./docs/how-to/execute-verification-run.md).

#### [`docs/workflows/graph-projections.md`](./docs/workflows/graph-projections.md)
- **Purpose**: Trace how ledger records are projected into $G_{declared}$, $G_{observed}$, and contentless FTS5 search.
- **Audience Need**: Understand how relational and graph queries are resolved and rebuilt deterministically.
- **Diátaxis Type**: Workflow Trace.
- **Prerequisites**: [`docs/architecture/data-storage.md`](./docs/architecture/data-storage.md).
- **Related Pages**: [`docs/how-to/rebuild-projections.md`](./docs/how-to/rebuild-projections.md).

#### [`docs/workflows/diagnostic-loop.md`](./docs/workflows/diagnostic-loop.md)
- **Purpose**: Trace the Slice 5 failure attribution loop: residual computation, 9 candidate loci hypotheses, and 4 settlement conditions.
- **Audience Need**: Follow how contradictory evidence is diagnosed without premature model refactoring.
- **Diátaxis Type**: Workflow Trace.
- **Prerequisites**: [`docs/mental-model.md`](./docs/mental-model.md).
- **Related Pages**: [`docs/how-to/diagnose-failures.md`](./docs/how-to/diagnose-failures.md), [`docs/explanation/asymmetric-diagnostic-friction.md`](./docs/explanation/asymmetric-diagnostic-friction.md).

#### [`docs/workflows/daemon-and-durability.md`](./docs/workflows/daemon-and-durability.md)
- **Purpose**: Trace asynchronous queue ingestion, fsynced segment receipts, batch committing, and recovery.
- **Audience Need**: Understand durability guarantees under process termination (`kill -9`) and spool fallback.
- **Diátaxis Type**: Workflow Trace.
- **Prerequisites**: [`docs/architecture/runtime.md`](./docs/architecture/runtime.md).
- **Related Pages**: [`docs/how-to/operate-daemon.md`](./docs/how-to/operate-daemon.md).

---

### Layer 5 — Explanation / Design Rationale

#### [`docs/explanation/anti-collapse-epistemology.md`](./docs/explanation/anti-collapse-epistemology.md)
- **Purpose**: Theoretical foundation of the seven collapse traps from CEP-0008 §53 and why SXR enforces rigid boundaries.
- **Audience Need**: Deeply understand why observations are not evidence, why completions are not settlements, etc.
- **Diátaxis Type**: Explanation.
- **Prerequisites**: [`docs/mental-model.md`](./docs/mental-model.md).
- **Related Pages**: [`docs/subsystems/sxr-core.md`](./docs/subsystems/sxr-core.md).

#### [`docs/explanation/cryptographic-ledger.md`](./docs/explanation/cryptographic-ledger.md)
- **Purpose**: Rationale for using append-only SQLite with SHA-256 hash chains and generation trees.
- **Audience Need**: Understand trade-offs between flat files, standard relational databases, and blockchain-like chains.
- **Diátaxis Type**: Explanation.
- **Prerequisites**: [`docs/architecture/data-storage.md`](./docs/architecture/data-storage.md).
- **Related Pages**: [`docs/subsystems/sxr-ledger.md`](./docs/subsystems/sxr-ledger.md).

#### [`docs/explanation/projections-and-outbox.md`](./docs/explanation/projections-and-outbox.md)
- **Purpose**: Rationale for CQRS and the transactional outbox pattern in a single SQLite database.
- **Audience Need**: Understand why read models ($G_{declared}$, $G_{observed}$, FTS) are decoupled from the write ledger.
- **Diátaxis Type**: Explanation.
- **Prerequisites**: [`docs/architecture/data-storage.md`](./docs/architecture/data-storage.md).
- **Related Pages**: [`docs/workflows/graph-projections.md`](./docs/workflows/graph-projections.md).

#### [`docs/explanation/asymmetric-diagnostic-friction.md`](./docs/explanation/asymmetric-diagnostic-friction.md)
- **Purpose**: Rationale for enforcing asymmetric friction against model-locus proposals (AC-DIAG-001).
- **Audience Need**: Understand why infrastructure and verifier loci must be evidence-excluded before proposing model changes.
- **Diátaxis Type**: Explanation.
- **Prerequisites**: [`docs/workflows/diagnostic-loop.md`](./docs/workflows/diagnostic-loop.md).
- **Related Pages**: [`docs/subsystems/ablation.md`](./docs/subsystems/ablation.md).

#### [`docs/explanation/isolation-invariants.md`](./docs/explanation/isolation-invariants.md)
- **Purpose**: Rationale for never writing inside the enrolled git repository (AC-SECURITY-001) and divorcing identity from Git.
- **Audience Need**: Understand the architectural necessity of repository isolation and immutable ULIDs.
- **Diátaxis Type**: Explanation.
- **Prerequisites**: [`docs/architecture/security-trust.md`](./docs/architecture/security-trust.md).
- **Related Pages**: [`docs/subsystems/sxr-git.md`](./docs/subsystems/sxr-git.md).

---

### Layer 6 — Tutorials

#### [`docs/tutorials/quickstart.md`](./docs/tutorials/quickstart.md)
- **Purpose**: Step-by-step hands-on guide from empty workspace to verified settlement.
- **Audience Need**: Run SXR end-to-end for the first time and verify output.
- **Diátaxis Type**: Tutorial.
- **Prerequisites**: Basic command-line familiarity and Rust toolchain installed.
- **Related Pages**: [`docs/how-to/enroll-and-configure.md`](./docs/how-to/enroll-and-configure.md), [`docs/how-to/execute-verification-run.md`](./docs/how-to/execute-verification-run.md).

---

### Layer 7 — How-To Guides

#### [`docs/how-to/enroll-and-configure.md`](./docs/how-to/enroll-and-configure.md)
- **Purpose**: Practical procedure for enrolling git repositories and configuring semantic namespaces.
- **Audience Need**: Set up a repository for SXR tracking.
- **Diátaxis Type**: How-To Guide.
- **Prerequisites**: Enrolled repository with git.
- **Related Pages**: [`docs/workflows/repo-enrollment.md`](./docs/workflows/repo-enrollment.md).

#### [`docs/how-to/execute-verification-run.md`](./docs/how-to/execute-verification-run.md)
- **Purpose**: Practical procedure for running verification test suites and deriving question-bound evidence.
- **Audience Need**: Execute verification runs and register settlements.
- **Diátaxis Type**: How-To Guide.
- **Prerequisites**: Configured repository with passing/failing tests.
- **Related Pages**: [`docs/workflows/verification-run.md`](./docs/workflows/verification-run.md).

#### [`docs/how-to/diagnose-failures.md`](./docs/how-to/diagnose-failures.md)
- **Purpose**: Practical procedure for diagnosing contradictory evidence and isolating the true failure locus.
- **Audience Need**: Resolve verification failures through the 9-locus diagnostic loop.
- **Diátaxis Type**: How-To Guide.
- **Prerequisites**: A verification run with contradictory evidence.
- **Related Pages**: [`docs/workflows/diagnostic-loop.md`](./docs/workflows/diagnostic-loop.md).

#### [`docs/how-to/operate-daemon.md`](./docs/how-to/operate-daemon.md)
- **Purpose**: Practical procedure for managing the `sxr daemon`, inspecting queues, and recovering from crashes.
- **Audience Need**: Run SXR in high-throughput daemon mode.
- **Diátaxis Type**: How-To Guide.
- **Prerequisites**: Built `sxr` binary.
- **Related Pages**: [`docs/workflows/daemon-and-durability.md`](./docs/workflows/daemon-and-durability.md).

#### [`docs/how-to/rebuild-projections.md`](./docs/how-to/rebuild-projections.md)
- **Purpose**: Practical procedure for rebuilding projections and verifying byte-identical replay.
- **Audience Need**: Recover projection databases or export anti-collapse corpora.
- **Diátaxis Type**: How-To Guide.
- **Prerequisites**: Existing repository ledger.
- **Related Pages**: [`docs/workflows/graph-projections.md`](./docs/workflows/graph-projections.md).

#### [`docs/how-to/add-verifier.md`](./docs/how-to/add-verifier.md)
- **Purpose**: Guide for developers extending `sxr-verify` with a new test runner or static analysis port.
- **Audience Need**: Add support for non-cargo verifiers (e.g. pytest, eslint, custom runners).
- **Diátaxis Type**: How-To Guide.
- **Prerequisites**: Rust development environment.
- **Related Pages**: [`docs/subsystems/sxr-verify.md`](./docs/subsystems/sxr-verify.md).

---

### Layer 8 — Reference

#### [`docs/reference/cli.md`](./docs/reference/cli.md)
- **Purpose**: Complete manual for the `sxr` command-line interface.
- **Audience Need**: Look up command syntax, flags, inputs, and JSON output structures.
- **Diátaxis Type**: Reference.
- **Prerequisites**: None.
- **Related Pages**: [`docs/reference/exit-codes.md`](./docs/reference/exit-codes.md).

#### [`docs/reference/record-kinds.md`](./docs/reference/record-kinds.md)
- **Purpose**: Definitive catalog of all 26 ledger record kinds from `record_kinds.csv`.
- **Audience Need**: Look up envelope mappings, allowed predecessors, required subject refs, and authority classes.
- **Diátaxis Type**: Reference.
- **Prerequisites**: [`docs/subsystems/sxr-core.md`](./docs/subsystems/sxr-core.md).
- **Related Pages**: [`docs/reference/schemas.md`](./docs/reference/schemas.md).

#### [`docs/reference/schemas.md`](./docs/reference/schemas.md)
- **Purpose**: Verbatim SQL DDL for ledger, outbox, and projection tables, plus JSON schemas.
- **Audience Need**: Exact database table definitions, column types, and constraints.
- **Diátaxis Type**: Reference.
- **Prerequisites**: None.
- **Related Pages**: [`docs/architecture/data-storage.md`](./docs/architecture/data-storage.md).

#### [`docs/reference/environment-and-xdg.md`](./docs/reference/environment-and-xdg.md)
- **Purpose**: Comprehensive list of environment variables (`XDG_*`, `SXR_*`) and filesystem paths.
- **Audience Need**: Configure SXR runtime directories, daemon sockets, and test seams.
- **Diátaxis Type**: Reference.
- **Prerequisites**: None.
- **Related Pages**: [`docs/architecture/runtime.md`](./docs/architecture/runtime.md).

#### [`docs/reference/exit-codes.md`](./docs/reference/exit-codes.md)
- **Purpose**: Definitive mapping of §15 exit codes (0, 2, 3, 4, 5, 6, 75) and diagnostic error strings.
- **Audience Need**: Interpret shell return codes in scripts and automated pipelines.
- **Diátaxis Type**: Reference.
- **Prerequisites**: None.
- **Related Pages**: [`docs/reference/cli.md`](./docs/reference/cli.md).

# Current status

## Objective
Evidence Revision 005 is implemented as the minimal authorization successor to frozen exec004. It retains ER002 scientific design and exec004 transport exactly, but adds a per-FULL-run immutable authority gate. No provider, qualification, or scientific model call was made.

## Completed
- Added `ablation/eval/v2/evidence_revision_002_exec005.py`, reusing the unchanged local OpenCode runtime adapter and preserving the 16-shard/4-worker, raw-before-score, ambiguity, and outcome-blind replacement behavior.
- Added a hash-bound `evidence-revision-002-exec005-full-authority-v1` authority artifact. It binds exact science/executable/implementation/matrix/plan, reviewed candidate commit, qualification/provenance artifact bytes, provider/model/agent/runtime, owner intent, timestamp, and deterministic authority hash.
- `authorize-full` validates fresh exec005 qualification **contents** (20/20 delivered, zero ambiguity, synthetic-only) and provenance contents (20 sessions, routes/agent matches, no tools, PASS), not merely stated hashes. Exec004 qualification is recorded as immutable lineage only and cannot authorize exec005.
- `run-shard` validates sealed run authority before it constructs `OpenCodeRuntimeClient`; offline fake execution remains authority-free for tests.
- Generated eight frozen `evidence-revision-002-exec005-*` artifacts. They remain `FROZEN_NOT_AUTHORIZED`, qualification-required, and FULL-unauthorized.

## Verification
- Required command: `uvx --from pytest pytest eval/tests/test_v2_evidence_revision_002_exec005.py -q`: **19 passed**.
- Complete offline ablation suite: **472 passed** in 12m30s.
- Exec005 preflight: PASS; `full_execution_authorized=false`.
- Help/parser and preflight only were run; no live transport command was run.

## Frozen identities
- ER002 scientific fingerprint: `056284062a2ef6b2a6f6dc54ae63eceeb8802b66341d42ced7d305067d9e1d99`.
- exec005 executable fingerprint: `ee58587fca10d2e7d7e45676965873fd24afaf4c64f593f12998f2236443dd50`.
- exec005 matrix SHA-256: `41c4b7d81b9de0d22f093601f5a0646514e3b4d369379eea1c5a7910d4b33d5d`.
- exec005 shard-plan SHA-256: `fc9583bc372cb7dc27fdac741f4299baa08cc5014eb563a5ef9ed7facf90ec4e`.
- 430 coordinates are exactly equal to exec004; all exec005 unit IDs are disjoint from exec004 IDs.

## Next authorization transition (not run)
After commit/review, a fresh synthetic exec005 qualification and matching provenance audit must pass. Mint authority only with:
`uv run --offline --python 3.12 -m eval.v2.evidence_revision_002_exec005 authorize-full --owner-authorized --qualification <fresh-exec005-qualification.json> --provenance-audit <fresh-exec005-provenance-audit.json> --candidate-commit <reviewed-40-hex-commit> --output <authority.json>`
Then initialize/run a shard only with the sealed evidence paths and that authority. FULL remains blocked absent that transition.

## Safety boundary
Exec004 code/artifacts and pre-existing aborted run directories were not modified or inspected. ZERO live model/provider/qualification calls occurred.

## DomainForge 0.18.2 convergence (2026-10-04)
- Expected: every semantic consumer runs the same DomainForge release; a stale compiler binary must not silently resolve a different world.
- Observed: installed `domainforge` was 0.18.1 while the release is 0.18.2; `sxr-df` did not check the binary version.
- Changed: `CliDomainForgeAdapter::with_required_version` probes `domainforge --version` and fails closed (`SemanticError::UnsupportedVersion`) before `envelope` runs; `snapshot create` opts in with `sxr_df::PINNED_DOMAINFORGE_VERSION = "0.18.2"`; the CLI test mock answers `--version`.
- Verified: `cargo test --workspace` 0 failures; `cargo clippy --workspace --all-targets -D warnings` clean; `sxr-df` real-binary tests (3) pass against 0.18.2 with `SXR_TEST_DOMAINFORGE_BIN`, `SXR_TEST_DOMAINFORGE_ROOT`, `SXR_TEST_LARGE_SEA_DIR` set.
- Next: bump `PINNED_DOMAINFORGE_VERSION` with each DomainForge convergence.

## DomainForge 0.19.0 bump (2026-10-04)
- DomainForge 0.19.0 is published (crates.io, PyPI, npm). `sxr_df::PINNED_DOMAINFORGE_VERSION` is now `0.19.0`; installed CLI upgraded to 0.19.0.
- Verified: `cargo test --workspace` 0 failures; the 3 `sxr-df` real-binary tests pass against the 0.19.0 binary (env: `SXR_TEST_DOMAINFORGE_BIN`, `SXR_TEST_DOMAINFORGE_ROOT`, `SXR_TEST_LARGE_SEA_DIR`).
- Note: 0.19.0 adds opt-in `--world-name` to `domainforge envelope`; sxr does not use it yet (Stage 8 evidence path).

## DomainForge 0.21.1 bump (2026-10-21)
- `sxr_df::PINNED_DOMAINFORGE_VERSION` is now `0.21.1`, matching SEA-Forge, Cognate and Horizon. DomainForge world identity includes `producer_version`, so a stack split across 0.19.0 / 0.21.0 / 0.21.1 names the same source as different worlds; before this change RealityTrace refused the 0.21.1 CLI (`unsupported domainforge version: expected 0.19.0, found 0.21.1`).
- `proof/{slice3,slice4,slice5,red-green}.sh` now write the current registry format (`version = 1`, `[[namespaces]]`); the retired `[namespaces.logistics]` map form is rejected by DomainForge (`invalid type: map, expected a sequence`). These scripts are not in CI.
- Verified: `cargo test --workspace` 209 passed, 0 failed; the 3 `sxr-df` real-binary tests pass against the released 0.21.1 CLI (`SXR_TEST_DOMAINFORGE_BIN`, `SXR_TEST_DOMAINFORGE_ROOT=~/projects/domainforge`, `SXR_TEST_LARGE_SEA_DIR` = a dir with a >64 KiB interaction-model.sea); `proof/red-green.sh` passes (RED unsettled + missing + contradiction, GREEN settled); godspeed-readiness R02 passes on this branch.
- Next: keep `PINNED_DOMAINFORGE_VERSION` in lockstep with the stack (godspeed-readiness gate C01 checks it).

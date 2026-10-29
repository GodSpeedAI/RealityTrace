# Native Binary Distribution — Verification Report

Date: 2026-10-29
Branch: `feat/native-binary-distribution`
Plan: `.agents/plans/native-binary-distribution-20261028.md`

## Expected state

Five-platform precompiled distribution for the `realitytrace` CLI, bound to the
`realitytrace-v*` release identity, gated on glibc 2.35 (NVIDIA Jetson AGX Orin),
with packaged-binary tests that assert evidence/ledger semantics rather than
exit codes, and a DomainForge 0.21.2 pin exercised against the real compiler.

## Observed state (local, this host)

Host: Ubuntu 26.04, glibc 2.43. Because the host is newer than the 2.35
baseline, the ABI gate correctly refuses a locally built binary; the 2.35
artifacts must come from the `ubuntu-22.04` / `ubuntu-22.04-arm` CI runners.

### Source verification

- `cargo fmt --all --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test --workspace --all-targets`: every suite `ok`, 0 failed
  (27 result lines; 3 ignored = the opt-in real-binary tests).
- `packaging/version.sh`: prints `0.3.0`; refuses `v0.2.1`.
- `sh -n` on every shell script: clean.
- YAML parse of all three workflows: clean.

### DomainForge 0.21.2

- `SXR_TEST_DOMAINFORGE_BIN=$HOME/.cargo/bin/domainforge`
  `SXR_TEST_DOMAINFORGE_ROOT=$HOME/projects/domainforge`
- `cargo test -p sxr-df --test real_binary_tests -- --ignored`
  - `real_domainforge_resolves_the_dogfood_entry`: passed
  - `real_binary_composition_and_settlement_authorization`: passed
  - `real_domainforge_materializes_large_cas_carried_representation`: **Not
    Tested** — panics on the missing `SXR_TEST_LARGE_SEA_DIR` fixture
    (`interaction-model.sea`, not in this repo or DomainForge). This is an
    environment precondition, not a 0.21.2 compatibility failure. It never ran
    for 0.21.1 either.
- `sxr-df/src/lib.rs` pin is `0.21.2` with a regression test asserting it.

### Packaging (local, logic validation)

- `.deb` built and inspected: `Package: realitytrace`, `Version: 0.3.0`,
  `Architecture: amd64`, `Depends: libc6 (>= 2.39), libgcc-s1 (>= 4.2)` on this
  host (2.35 on the CI runner), payload `/usr/bin/realitytrace`,
  `/usr/lib/systemd/user/realitytrace.service`, `/usr/share/doc/realitytrace/copyright`.
  No system `libsqlite3` dependency.
- Tarball built twice: identical SHA-256
  (`f2f4de55…fdf588a`), i.e. byte-reproducible.
- Extracted tarball binary passed the full functional smoke, including
  supporting→settled, contradicting→unsettled, tamper rejection, repository
  non-mutation, and fail-closed on a deleted ledger.
- The `.deb` payload was extracted (no local sudo) and its layout and unit file
  verified; the CI install test performs the real apt install/remove.

### ABI gate

`packaging/check-linux-binary.sh` on the local binary reported
`requires GLIBC_2.39, above the 2.35 baseline` and exited 1. The gate works; a
2.35-passing binary is produced only on the 22.04 runners.

## Unresolved / pending

- CI has not yet run; no artifacts have been published.
- Windows/macOS packaging is written but cannot be executed on this Linux host;
  their CI jobs are the first real execution.
- The large-CAS real-binary test remains Not Tested (fixture absent).

## Next authorized move

Commit, push the branch, run the Mode A packaging workflow, verify the ARM64
artifacts and hashes, then open the PR.

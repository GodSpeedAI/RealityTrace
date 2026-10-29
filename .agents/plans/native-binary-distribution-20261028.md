# Native Binary Distribution Plan — 2026-10-28

## Mission

Distribute `realitytrace` as precompiled native binaries across five targets
(Linux amd64/arm64 deb+tar.gz, Windows x64 zip+setup.exe, macOS universal
pkg+dmg), extend clean-install CI with packaged-binary tests, bind binary
publication to the `realitytrace-v*` CLI release identity, pin DomainForge
0.21.2, and deliver an installable ARM64 artifact for the Jetson AGX Orin
(Ubuntu 22.04, glibc 2.35).

## Expected state → observed state (start)

- Expected: packaged binary distribution pipeline exists; ARM64 artifacts downloadable.
- Observed: no binary packaging at all (only source install via clean-install.yml);
  CLI release `realitytrace-v0.3.0` has zero assets; workspace is Unix-only at
  source level (libc::flock in sxr-ledger, Unix sockets in sxr-cli daemon);
  DomainForge pin is 0.21.1 vs required 0.21.2.
- Difference: everything in the mission is yet to be implemented.

## Work breakdown

1. **Portability (src)** — smallest correct platform adaptations, no semantic change:
   - `sxr-ledger/src/writer_lock.rs` + `capability.rs`: replace raw
     `libc::flock` with std `File::try_lock`/`unlock` (stable since 1.89;
     flock on Unix, LockFileEx on Windows; whole-file semantics identical for
     our single-writer use). Removes the Unix-only dependency in core ledger.
   - `sxr-cli/src/state.rs`: HOME fallback to USERPROFILE (Windows has no HOME).
   - `sxr-cli/src/daemon.rs`: gate socket transport behind `#[cfg(unix)]`;
     keep queue/recovery core cross-platform; on non-Unix `daemon_run`
     explicitly refuses. Client paths fall back to the designed F-15 direct
     path (semantically identical by §16.3 parity, not degradation).
   - `sxr-verify/src/digest.rs` tests, `sxr-df/tests/process_tests.rs`:
     cfg(unix)-gate symlink/PermissionsExt test code.
2. **DomainForge 0.21.2** — bump `PINNED_DOMAINFORGE_VERSION`, audit all
   references, run real-binary sxr-df tests locally against installed
   domainforge 0.21.2, add regression test that the pin is enforced.
3. **Packaging** — build .deb/.tar.gz (Linux amd64+arm64 in ubuntu:22.04
   containers → glibc 2.35 baseline), Windows zip + Inno Setup setup.exe,
   macOS universal (lipo) pkg+dmg. Reuse SEA-Forge/DomainForge mechanisms.
4. **CI** — reusable native packaging workflow (Mode A branch artifacts,
   Mode B CLI-tag release publication gated on `sxr-cli--release_created`),
   checksums + actions/attest-build-provenance, packaged-binary clean-install
   tests (deb install on 22.04 amd64 + arm64, tarball, macOS pkg, Windows zip).
5. **Docs** — Jetson install/verify runbook incl. DomainForge discovery.
6. **Delivery** — push branch, run Mode A, verify artifacts + hashes, PR,
   adversarial review, handoff.

## Non-negotiable invariants (unchanged)

Expectation-before-observation, immutable evidence, raw-before-score,
CEP-0008 envelope validation, hash-chained ledger, deterministic replay,
direct/daemon parity where the daemon exists, fail-closed verification
(deleted/corrupt ledger must fail). Packaging tests must assert evidence
semantics, not just exit codes.

## Identity / governance

- Binary publisher must build from the CLI release tag, refuse version
  mismatch, never fire for non-CLI component releases.
- No crates.io publication changes; no force-push; no history rewrite.
- Historical evidence untouched; feature branch only.

## Verification gates

- Local: `cargo test --workspace` on x86_64 Linux (existing + new), real
  binary sxr-df tests vs domainforge 0.21.2, local deb build + fresh-HOME
  install + full ledger loop + negative tests.
- CI: all platform jobs green; packaged-binary install tests green; Mode A
  artifacts uploaded; hashes recorded in handoff.

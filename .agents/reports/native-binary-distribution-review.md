# Adversarial Review — Native Binary Distribution

Date: 2026-10-29
Scope: the diff of `feat/native-binary-distribution` (packaging, workflows,
portability changes).
Method: independent read-only review by a separate agent, focused on release
safety, shell correctness, packaging correctness, and regression risk.

## Outcome

No critical finding. The reviewer could not construct a path where artifacts
built from commit A are published under tag B, where a tag is moved or deleted,
or where a non-CLI component release triggers binary publication.

Independently confirmed sound:

- Binary publication gates on `sxr-cli--release_created`, not
  `releases_created`; `tag_name`/`version` come from the CLI component, matching
  the release-please config keys.
- Identity binding is real: the version is re-derived from `sxr-cli/Cargo.toml`
  at the checked-out ref, a non-matching tag is refused, and the publisher
  refuses to upload unless all four `built-from-*.txt` match the tag commit.
  `gh release upload --clobber` never moves a tag.
- `inputs.ref` / `inputs.version` / `inputs.upload` resolve correctly across
  `workflow_call`, `workflow_dispatch`, and `push`.
- `compgen -G` usage fails closed; macOS arm64-only names match the builder and
  the publisher's fallback branch; tarball layout matches the publisher.
- `.deb` `Depends` genuinely comes from `dpkg-shlibdeps`; no `postinst`; the
  unit is installed to `/usr/lib/systemd/user/` and never enabled.
- Windows `RemovePathEntry` PATH logic is correct for empty/trailing/bare
  entries and dedupe.
- `smoke-realitytrace.sh`: every assertion has a non-empty literal on at least
  one side; `jq_py` extractions abort under `set -e`; both negative checks are
  correctly inverted. No vacuous pass found.
- `File::try_lock` swap: std uses `flock` on Unix, so semantics are identical;
  a read-only exclusive lock is legal for `LockFileEx`; a second handle in the
  same process still conflicts, so the writer lock stays valid on Windows.

## Findings and disposition

| # | Severity | Finding | Disposition |
|---|---|---|---|
| 1 | High | A manual `package.yml` dispatch against an existing tag shares the concurrency group with a release build and could cancel it, leaving the Release created with no assets and `publish-binaries` skipped (not failed), so the recovery notice never ran. | **Fixed** in `aae1ab0`: release builds are never cancelled and get a group a manual run cannot collide with. |
| 2 | Medium | Preinstalled Inno Setup version is never checked; `x64compatible` needs >= 6.3, and hosted images can ship older, failing the Windows job after the full Rust build. | **Fixed**: the version is read and upgraded when < 6.3, with `$ErrorActionPreference='Stop'`. |
| 3 | Medium | No CI job ran `cargo test` on Windows, so the `LockFileEx` path was never exercised. | **Fixed**: `cargo test -p sxr-ledger --lib` added to the Windows job. |
| 4 | Medium | `#![cfg(unix)]` on `sxr-df/tests/process_tests.rs` removes all Windows coverage of the DomainForge process adapter (argv construction, exit codes, stderr/empty stdout). | **Accepted, documented.** Porting the `#!/bin/sh` fixtures to `.cmd` is a larger change with its own risk and no demonstrated defect. Windows still gets end-to-end CLI coverage from `smoke.ps1`, and `real_binary_tests` remains the adapter contract test wherever a real binary exists. Recorded as a known coverage gap rather than claimed as covered. |
| 5 | Low | `sha256sum $(ls ...)` splits on whitespace and would vacuously verify an empty sums file. | **Fixed**: NUL-delimited `find \| xargs`, plus a non-empty assertion. |
| 6 | Low | The `push: branches: [feat/native-binary-distribution]` trigger was a development leftover. | **Fixed**: replaced with a path-filtered `pull_request` trigger. |
| 7 | Low | The "package must not enable the daemon" assertion was vacuous without a systemd user manager. | **Fixed**: asserts no `postinst` and no enablement symlink. |
| 8 | Low | `built-from-*.txt` proved the checkout, not the uploaded bytes. | **Fixed**: the publisher now also verifies each builder's `SHA256SUMS-<os>`. |
| 9 | Low | `libc` became an unused dependency of `sxr-ledger`. | **Fixed**: removed from the crate and the workspace; `Cargo.lock` updated. |
| 10 | Low | The docs used `sha256sum` on macOS and advertised attestation unconditionally. | **Fixed**: `shasum -a 256 -c` documented for macOS; attestation marked as present only when the publishing run built the tag. |
| 11 | Low | Prerelease versions pass `version.sh` but fail late in `pkgbuild`/Inno Setup. | **Fixed**: `version.sh` now requires exactly numeric `x.y.z` and refuses at the first gate. |

## Residual, explicitly not claimed

- Windows/macOS have no real-hardware test beyond the GitHub-hosted runners.
- Packages are unsigned: no Apple Developer ID, no Authenticode. No notarization
  is claimed anywhere.
- Finding 4 (DomainForge adapter coverage on Windows) remains open by decision.

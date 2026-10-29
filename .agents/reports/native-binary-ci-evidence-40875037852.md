# Native Binary CI Evidence — run 40875037852

Date: 2026-10-29
Workflow: `Package` (`.github/workflows/package.yml`, Mode A push)
Run: https://github.com/GodSpeedAI/RealityTrace/actions/runs/40875037852
Commit built: `bdbf929ef35eb17a60a15955f9a5516ab92ede45`

This run is the first fully green execution of all five jobs. Artifacts were
downloaded from it and re-verified locally; the hashes below are the bytes that
were actually inspected.

## Job outcome

| Job | Result | Duration | Notes |
|---|---|---|---|
| `package (linux amd64)` | success | 1m38s | built, ABI-gated, installed via apt, removed, plus clean `ubuntu:22.04` + `debian:12` container installs |
| `package (linux arm64)` | success | 1m34s | native `ubuntu-22.04-arm`; same gates, executed on aarch64 |
| `package (macos universal)` | success | 4m55s | both slices built; `lipo -archs` → `x86_64 arm64`; pkg installed/removed; dmg payload pkg byte-identical |
| `package (windows x86_64)` | success | 5m08s | per-user install, fresh-process PATH check, functional smoke, uninstall + user-PATH removal |
| `domainforge 0.21.2 real-binary compatibility` | success | 2m33s | 2 real-binary tests pass against a live 0.21.2 built from crates.io |

Earlier runs, kept for lineage: `40874273848` (`298b6b0`) failed on the Windows
inline-Python quoting and an empty `env.HOME` in the DomainForge job; both were
fixed in `bdbf929` and are green above.

## Artifact identity

All four `built-from-*.txt` files contain
`bdbf929ef35eb17a60a15955f9a5516ab92ede45`.

### SHA-256

| Asset | SHA-256 |
|---|---|
| `realitytrace_0.3.0_arm64.deb` | `d947e82668cf6c8f6b066dbd2c9db53b627b3dfd5de5632fd0e8235261bf6477` |
| `realitytrace-0.3.0-aarch64-unknown-linux-gnu.tar.gz` | `b3196ad11d11956bd0147f1e41e865e376ef1b1c3ce00f71fe77cf9040022970` |
| `realitytrace_0.3.0_amd64.deb` | `374cb4f9c7b42829e8a840554e92e6181a48d433456e96d2208d2105f94e8c29` |
| `realitytrace-0.3.0-x86_64-unknown-linux-gnu.tar.gz` | `220c1ee973d4d1e3dad9771502f765edc524dfb0fc0e59ebea2b82067221ce7c` |
| `realitytrace-0.3.0-x86_64-pc-windows-msvc.zip` | `e3d29f70474460274a7ccd1a6e3443064ad7bf2e1ed884e768fd6d8e42c2b682` |
| `realitytrace-0.3.0-x86_64-pc-windows-msvc-setup.exe` | `e5dd43e6a85a9d68d77689479914969c50432cb1299cf88106e2368c25cfd27f` |
| `realitytrace-0.3.0-macos-universal.pkg` | `7feab3d8e29caeeb6e79fcc9f55c29ac46c81e05d3462da251a735289e53e8fa` |
| `realitytrace-0.3.0-macos-universal.dmg` | `48c52f725262cd5334536484de1d56b2f36dfe6523e65fcd687225653a25fe75` |

Each platform's `SHA256SUMS-<os>` was re-checked locally (`sha256sum -c` /
`shasum -a 256 -c`): all OK.

## ARM64 artifact inspection (the Jetson target)

Extracted `realitytrace-0.3.0-aarch64-unknown-linux-gnu.tar.gz`:

- `file`: `ELF 64-bit LSB pie executable, ARM aarch64, dynamically linked,
  interpreter /lib/ld-linux-aarch64.so.1, for GNU/Linux 3.7.0`.
- Maximum versioned symbol: `GLIBC_2.34` (versions present: 2.17, 2.18, 2.25,
  2.28, 2.29, 2.30, 2.32, 2.33, 2.34) — **within the 2.35 baseline**, so it
  runs on Ubuntu 22.04 / glibc 2.35.
- `NEEDED`: `libgcc_s.so.1`, `libm.so.6`, `libc.so.6`, `ld-linux-aarch64.so.1`.
  No system `libsqlite3` (bundled).
- No `RPATH`/`RUNPATH`.
- Tarball layout: `LICENSE`, `README.txt`, `realitytrace` under
  `realitytrace-0.3.0-aarch64-unknown-linux-gnu/`.

`realitytrace_0.3.0_arm64.deb`:

- `Package: realitytrace`, `Version: 0.3.0`, `Architecture: arm64`
- `Depends: libc6 (>= 2.34), libgcc-s1 (>= 4.2)` (from `dpkg-shlibdeps`, not
  hardcoded)
- Payload: `/usr/bin/realitytrace`, `/usr/lib/systemd/user/realitytrace.service`,
  `/usr/share/doc/realitytrace/copyright`
- No maintainer scripts at all — the daemon is never enabled by the package.

## Functional evidence (from the run logs)

Linux arm64, installed from the `.deb` on the native aarch64 runner — all 13
checks passed in order: version, doctor roots, no system libsqlite3, no
RPATH, enroll, identity, expectation-before-observation,
supports→settled, contradicts→unsettled, ledger verify (13 entries), tamper
rejected, repository unmodified, missing ledger fails. Then the package was
removed and `/usr/bin/realitytrace` + the unit were confirmed gone.

Same 13 checks passed for the extracted tarball, the container installs, the
macOS `.pkg`, and the Windows per-user install. Windows additionally asserted
`daemon` exits non-zero with a "not supported" explanation.

## Not tested

- `real_domainforge_materializes_large_cas_carried_representation` —
  `SXR_TEST_LARGE_SEA_DIR` / `interaction-model.sea` fixture is not in this
  repository. Never ran for 0.21.1 either. Reported Not Tested, not a
  compatibility failure.
- No real macOS/Windows hardware other than the GitHub-hosted runners; no
  notarization or Authenticode signing is claimed (packages are unsigned).
- No install performed on the actual Jetson AGX Orin hardware. The glibc and
  architecture preconditions are proven (2.34 ≤ 2.35, aarch64, deb depends).

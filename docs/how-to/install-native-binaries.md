# How-To: Install Precompiled RealityTrace Binaries

This guide installs a released `realitytrace` binary — no Rust toolchain, Cargo,
or SQLite development headers. It also covers the NVIDIA Jetson AGX Orin
(Ubuntu 22.04, glibc 2.35), which is the oldest Linux ABI the release supports.

Every release of the CLI carries these assets on the GitHub Release tagged
`realitytrace-v<version>`:

| Platform | Assets |
|---|---|
| Linux amd64 | `realitytrace_<version>_amd64.deb`, `realitytrace-<version>-x86_64-unknown-linux-gnu.tar.gz` |
| Linux arm64 | `realitytrace_<version>_arm64.deb`, `realitytrace-<version>-aarch64-unknown-linux-gnu.tar.gz` |
| macOS | `realitytrace-<version>-macos-universal.pkg` / `.dmg` (or `-macos-arm64` if the Intel slice was unavailable) |
| Windows x64 | `realitytrace-<version>-x86_64-pc-windows-msvc-setup.exe`, `.zip` |
| All | `SHA256SUMS` |

Packages are **unsigned**: there is no Apple Developer ID and no Authenticode
certificate. Verify the checksum before installing.

---

## Prerequisites

- A Linux host with glibc 2.35 or newer (Ubuntu 22.04+), macOS, or Windows 10/11 x64.
- `SHA256SUMS` from the same release.

The optional daemon uses a Unix-domain socket. It works on Linux and macOS and
is **not supported on Windows**; there `realitytrace daemon` refuses explicitly
rather than degrading silently, and all direct CLI/ledger operation works.

---

## Verify the download

Linux and Windows (Git Bash / WSL):

```sh
sha256sum -c SHA256SUMS --ignore-missing
```

macOS (`sha256sum` is not part of a stock install):

```sh
shasum -a 256 -c SHA256SUMS --ignore-missing
```

Where GitHub CLI is available, you may also check the build provenance that was
attached at publish time:

```sh
gh attestation verify <asset> --repo GodSpeedAI/RealityTrace
```

This only succeeds when the release carries provenance. Provenance is attached
automatically when the publishing run itself built the release tag; a release
whose binaries were re-attached later via `attach_binaries_tag` intentionally
has none (the attestation would name the wrong commit). `SHA256SUMS` is always
present and is the baseline check.

---

## Linux — .deb (recommended)

```sh
sudo apt-get install -y ./realitytrace_<version>_amd64.deb   # or _arm64.deb
realitytrace --version
```

The package installs `/usr/bin/realitytrace` and a **vendor** systemd user unit
at `/usr/lib/systemd/user/realitytrace.service`. It does **not** enable the
daemon and has no postinst. Enable it explicitly if you want it:

```sh
systemctl --user daemon-reload
systemctl --user enable --now realitytrace
```

A unit of the same name in `~/.config/systemd/user/` overrides the vendor copy.

## Linux — tarball (no package manager)

```sh
tar -xzf realitytrace-<version>-x86_64-unknown-linux-gnu.tar.gz
install -m 0755 realitytrace-<version>-x86_64-unknown-linux-gnu/realitytrace ~/.local/bin/
realitytrace --version
```

For the daemon from a tarball install, copy `packaging/realitytrace.service`
from the source tree to `~/.config/systemd/user/`, confirm `ExecStart` points at
the installed binary, then enable it.

---

## NVIDIA Jetson AGX Orin (Ubuntu 22.04, arm64, glibc 2.35)

The arm64 artifacts are built natively on Ubuntu 22.04 and gated on
`GLIBC_2.35`; they need no cross-environment setup on the Jetson.

### Install

```sh
cd ~/Downloads
sha256sum -c SHA256SUMS --ignore-missing
sudo apt-get install -y ./realitytrace_0.3.0_arm64.deb
realitytrace --version          # expect: realitytrace 0.3.0
realitytrace doctor --json
```

The tarball is equivalent if you prefer no package manager:

```sh
tar -xzf realitytrace-0.3.0-aarch64-unknown-linux-gnu.tar.gz
install -m 0755 realitytrace-0.3.0-aarch64-unknown-linux-gnu/realitytrace ~/.local/bin/
```

### Verify the binary end to end

```sh
# A disposable ledger loop: enroll, verify, and confirm a lost ledger fails.
export HOME=$(mktemp -d)
unset XDG_DATA_HOME XDG_STATE_HOME
mkdir -p "$HOME/repo" && cd "$HOME/repo"
echo fixture > README && git init -q && git add -A
git -c user.email=ci@example.invalid -c user.name=ci commit -qm init
rid=$(realitytrace repo enroll "$HOME/repo" --json | python3 -c 'import json,sys;print(json.load(sys.stdin)["repo_id"])')
realitytrace ledger verify --repo "$rid" --json
rm -f "$HOME/.local/share/sxr/repos/$rid"/ledger.db*
! realitytrace ledger verify --repo "$rid" --json && echo "fail-closed OK"
```

### DomainForge integration (semantic snapshots only)

RealityTrace pins **DomainForge 0.21.2** and enforces an exact version match when
semantic context is configured. DomainForge is distributed through crates.io:

```sh
cargo install domainforge-core --version 0.21.2 --features cli --locked
domainforge --version            # expect: domainforge 0.21.2
```

Discovery order: `REALITYTRACE_DOMAINFORGE_BIN`, then `SXR_DOMAINFORGE_BIN`, then
`domainforge` on `PATH`. To pin an explicit binary:

```sh
export REALITYTRACE_DOMAINFORGE_BIN="$HOME/.cargo/bin/domainforge"
```

A different DomainForge version is refused, not silently accepted.

### Rollback

```sh
sudo apt-get remove -y realitytrace          # keeps your ~/.local/share/sxr data
# or restore a previous .deb:
sudo apt-get install -y ./realitytrace_<previous-version>_arm64.deb
```

Removing the package never deletes ledger data under `~/.local/share/sxr` or
`~/.local/state/sxr`; remove those directories yourself only when you intend to.

---

## macOS

```sh
sudo installer -pkg realitytrace-<version>-macos-universal.pkg -target /
/usr/local/bin/realitytrace --version
```

The package is unsigned and not notarized. If Gatekeeper refuses it, Control-click
the package and choose Open, or `xattr -d com.apple.quarantine <pkg>`.

Uninstall:

```sh
sudo rm -f /usr/local/bin/realitytrace
sudo rm -rf /usr/local/share/doc/realitytrace
sudo pkgutil --forget ai.godspeed.realitytrace
```

---

## Windows

Run `realitytrace-<version>-x86_64-pc-windows-msvc-setup.exe`. It installs per
user into `%LOCALAPPDATA%\Programs\RealityTrace`, adds that directory to your
user `PATH` (a new terminal picks it up), and removes both on uninstall.

```powershell
realitytrace --version
realitytrace doctor --json
```

The `.zip` is a portable alternative: extract `realitytrace.exe` anywhere.

The daemon is not available on Windows:

```text
> realitytrace daemon
Error: the RealityTrace daemon is not supported on this platform ...
```

That refusal is intentional; direct CLI and ledger operation are unaffected.

---

## Troubleshooting

| Symptom | Cause | Action |
|---|---|---|
| `GLIBC_2.3x not found` | Binary newer than the host | Use the artifact for your distribution; releases target glibc 2.35+ |
| `DomainForge binary not found` | Semantic snapshot configured without the compiler | Install 0.21.2 and set `REALITYTRACE_DOMAINFORGE_BIN` |
| `unsupported DomainForge version` | Version drift | Install exactly 0.21.2 |
| Gatekeeper / SmartScreen warning | Unsigned package | Verify `SHA256SUMS`, then allow explicitly |
| `daemon is not supported` (Windows) | Unix-socket transport absent | Use direct CLI commands |

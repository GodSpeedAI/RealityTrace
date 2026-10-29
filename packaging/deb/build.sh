#!/bin/sh
# Build realitytrace_<version>_<arch>.deb from the native release binary.
#
# Usage: packaging/deb/build.sh <version> <amd64|arm64> <bindir> <outdir>
#
# Depends is computed from the binary with dpkg-shlibdeps against the build
# host's package database, so the package declares real library dependencies
# instead of hand-written guesses. Build on Ubuntu 22.04 (glibc 2.35) for the
# widest supported ABI (the NVIDIA Jetson AGX Orin baseline).
#
# The package installs:
#   /usr/bin/realitytrace
#   /usr/lib/systemd/user/realitytrace.service   (vendor unit, NOT enabled)
#   /usr/share/doc/realitytrace/copyright
#
# Installing the unit does not activate the daemon: enabling it is an explicit
# user action (`systemctl --user enable --now realitytrace`), and a user's own
# unit in ~/.config/systemd/user/realitytrace.service always takes precedence
# over the vendor copy. There is no postinst.
#
# Pattern adapted from DomainForge packaging/deb/build.sh.
set -eu

if [ "$#" -ne 4 ]; then
    echo "usage: $0 <version> <amd64|arm64> <bindir> <outdir>" >&2
    exit 2
fi

version=$1
arch=$2
bindir=$(CDPATH= cd -- "$3" && pwd)
outdir=$4

case "$version" in
    '' | *[!0-9A-Za-z.+~-]*)
        echo "error: invalid version '$version'" >&2
        exit 2
        ;;
esac
case "$arch" in
    amd64 | arm64) ;;
    *)
        echo "error: unsupported architecture '$arch' (expected amd64 or arm64)" >&2
        exit 2
        ;;
esac

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
binary="$bindir/realitytrace"
if [ ! -x "$binary" ]; then
    echo "error: missing executable $binary" >&2
    exit 1
fi

sh "$repo_root/packaging/check-linux-binary.sh" "$binary" "$arch"
command -v dpkg-shlibdeps >/dev/null 2>&1 || {
    echo "error: dpkg-shlibdeps is required (install dpkg-dev)" >&2
    exit 1
}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/shlibs/debian"
cat > "$work/shlibs/debian/control" <<EOF
Source: realitytrace
Section: utils
Priority: optional
Maintainer: GodSpeedAI <https://github.com/GodSpeedAI/RealityTrace>
Package: realitytrace
Architecture: $arch
Description: shlibs probe
 probe
EOF

(
    cd "$work/shlibs"
    dpkg-shlibdeps -O -e"$binary" > "$work/shlibs.out" 2> "$work/shlibs.err"
) || {
    cat "$work/shlibs.err" >&2
    exit 1
}

deps=$(sed -n 's/^shlibs:Depends=//p' "$work/shlibs.out")
case "$deps" in
    *libc6*) ;;
    *)
        echo "error: dpkg-shlibdeps did not report libc6 (got '$deps')" >&2
        exit 1
        ;;
esac
# SQLite is bundled (rusqlite `bundled`); a system libsqlite3 dependency here
# would mean the binary was linked against the host library instead.
case "$deps" in
    *libsqlite3*) echo "error: package would depend on system libsqlite3 ('$deps')" >&2; exit 1 ;;
esac

root="$work/realitytrace_${version}_${arch}"
mkdir -p "$root/DEBIAN" "$root/usr/bin" "$root/usr/lib/systemd/user" \
    "$root/usr/share/doc/realitytrace"
install -m 0755 "$binary" "$root/usr/bin/realitytrace"
install -m 0644 "$repo_root/LICENSE" "$root/usr/share/doc/realitytrace/copyright"
# The deb installs the binary at /usr/bin, so the vendor unit must name that
# path (the source-install unit under packaging/ targets ~/.local/bin).
cat > "$root/usr/lib/systemd/user/realitytrace.service" <<'EOF'
[Unit]
Description=RealityTrace optional daemon (commit queue service, spec section 16)
Documentation=https://github.com/GodSpeedAI/RealityTrace

[Service]
Type=notify
ExecStart=/usr/bin/realitytrace daemon --socket %t/realitytrace.sock
Restart=on-failure
RestartSec=2
# %t is $XDG_RUNTIME_DIR; socket mode is handled by the daemon itself so any
# enrolled repo's verifier can submit.

[Install]
WantedBy=default.target
EOF
chmod 0644 "$root/usr/lib/systemd/user/realitytrace.service"

installed_kib=$(du -sk "$root/usr" | cut -f1)
cat > "$root/DEBIAN/control" <<EOF
Package: realitytrace
Version: $version
Architecture: $arch
Maintainer: GodSpeedAI <https://github.com/GodSpeedAI/RealityTrace>
Installed-Size: $installed_kib
Depends: $deps
Section: utils
Priority: optional
Homepage: https://github.com/GodSpeedAI/RealityTrace
Description: cryptographic observation, evidence, and settlement runtime
 RealityTrace (SXR) records expectations before observations, keeps immutable
 evidence with a tamper-evident hash-chained ledger, and settles questions
 only when the evidence authorizes it. This package installs the standalone
 realitytrace command; it bundles SQLite and needs no Rust toolchain,
 Cargo, or SQLite development headers. An optional daemon is provided as a
 user-level systemd unit that is installed but not enabled.
EOF
chmod 0644 "$root/DEBIAN/control"

mkdir -p "$outdir"
deb="$outdir/realitytrace_${version}_${arch}.deb"
dpkg-deb --build --root-owner-group "$root" "$deb" >/dev/null

test "$(dpkg-deb --field "$deb" Package)" = "realitytrace"
test "$(dpkg-deb --field "$deb" Version)" = "$version"
test "$(dpkg-deb --field "$deb" Architecture)" = "$arch"
dpkg-deb --contents "$deb" > "$work/contents.txt"
grep -q './usr/bin/realitytrace$' "$work/contents.txt"
grep -q './usr/lib/systemd/user/realitytrace.service$' "$work/contents.txt"
dpkg-deb --info "$deb" | sed -n '/Package:/,/Description:/p'
echo "OK: built $deb"

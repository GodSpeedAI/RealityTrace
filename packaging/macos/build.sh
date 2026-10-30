#!/bin/sh
# Build the macOS install package and disk image for realitytrace.
#
# Usage: packaging/macos/build.sh <version> <arm64dir> <x86dir> <outdir>
#
# <x86dir> may be "" to build an arm64-only package (named -macos-arm64).
# With both directories the binary is merged with lipo into a universal
# package (named -macos-universal). The package is UNSIGNED: no Developer ID
# certificate is used, and none is claimed anywhere. The binary is ad-hoc
# signed after merging, because arm64 macOS requires a valid signature.
#
# Pattern adapted from DomainForge/SEA-Forge packaging/macos/build.sh.
set -eu

if [ "$#" -ne 4 ]; then
    echo "usage: $0 <version> <arm64dir> <x86dir> <outdir>" >&2
    exit 2
fi
version=$1
arm_dir=$2
x86_dir=$3
outdir=$4

case "$version" in
    '' | *[!0-9A-Za-z.+~-]*)
        echo "error: invalid version '$version'" >&2
        exit 2
        ;;
esac

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
binary=realitytrace
if [ ! -x "$arm_dir/$binary" ]; then
    echo "error: missing executable $arm_dir/$binary" >&2
    exit 1
fi
if [ -n "$x86_dir" ] && [ ! -x "$x86_dir/$binary" ]; then
    echo "error: missing executable $x86_dir/$binary" >&2
    exit 1
fi

if [ -n "$x86_dir" ]; then
    label=universal
else
    label=arm64
fi
pkg_name="realitytrace-${version}-macos-${label}.pkg"
dmg_name="realitytrace-${version}-macos-${label}.dmg"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Payload: bin/ installs to /usr/local/bin; share/doc to /usr/local/share/doc.
stage="$work/stage"
mkdir -p "$stage/bin" "$stage/share/doc/realitytrace"
if [ -n "$x86_dir" ]; then
    lipo -create "$arm_dir/$binary" "$x86_dir/$binary" -output "$stage/bin/$binary"
else
    cp "$arm_dir/$binary" "$stage/bin/$binary"
fi
chmod 0755 "$stage/bin/$binary"
codesign --force --sign - "$stage/bin/$binary" >/dev/null
install -m 0644 "$repo_root/LICENSE" "$stage/share/doc/realitytrace/copyright"

mkdir -p "$outdir"
pkgbuild --root "$stage" \
    --identifier ai.godspeed.realitytrace \
    --version "$version" \
    --install-location /usr/local \
    "$outdir/$pkg_name" >/dev/null

# Disk image: the package plus licence and install notes.
dmg_src="$work/dmg"
mkdir -p "$dmg_src"
cp "$outdir/$pkg_name" "$dmg_src/"
cp "$repo_root/LICENSE" "$dmg_src/LICENSE"
cat > "$dmg_src/README-install.txt" <<EOF
RealityTrace ${version} for macOS (${label})

Install
  Double-click ${pkg_name}, or from Terminal:
    sudo installer -pkg ${pkg_name} -target /
  The command realitytrace is placed in /usr/local/bin.

Unsigned package
  This package is NOT signed or notarized (no Apple Developer ID is used).
  Gatekeeper may refuse to open it. Either:
    - Control-click the package, choose Open, and confirm; or
    - remove the quarantine flag after download:
        xattr -d com.apple.quarantine ${pkg_name}
  Verify the download before installing:
    shasum -a 256 -c SHA256SUMS
    gh attestation verify ${pkg_name} --repo GodSpeedAI/RealityTrace

Daemon
  The optional commit-queue daemon uses a Unix-domain socket and is available
  on macOS. It is not started by the package. Run it explicitly:
    realitytrace daemon --socket "\$TMPDIR/realitytrace.sock"

Uninstall
  sudo rm -f /usr/local/bin/realitytrace
  sudo rm -rf /usr/local/share/doc/realitytrace
  sudo pkgutil --forget ai.godspeed.realitytrace

Licence: see LICENSE in this image.
EOF
rm -f "$outdir/$dmg_name"
hdiutil create -volname "RealityTrace ${version}" -srcfolder "$dmg_src" \
    -format UDZO "$outdir/$dmg_name" >/dev/null

echo "built $outdir/$pkg_name and $outdir/$dmg_name"

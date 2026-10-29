#!/bin/sh
# Build a self-contained Linux tarball that does not depend on the .deb.
#
# Usage: packaging/linux/build-tarball.sh <version> <target-triple> <bindir> <outdir>
#
# The archive is reproducible: normalized ownership, timestamps, sort order,
# and gzip -n (no embedded name or mtime).
#
# Pattern adapted from DomainForge packaging/linux/build-tarball.sh.
set -eu

if [ "$#" -ne 4 ]; then
    echo "usage: $0 <version> <target-triple> <bindir> <outdir>" >&2
    exit 2
fi

version=$1
target=$2
bindir=$(CDPATH= cd -- "$3" && pwd)
outdir=$4

case "$version" in
    '' | *[!0-9A-Za-z.+~-]*)
        echo "error: invalid version '$version'" >&2
        exit 2
        ;;
esac
case "$target" in
    x86_64-unknown-linux-gnu | aarch64-unknown-linux-gnu) ;;
    *)
        echo "error: unsupported target '$target'" >&2
        exit 2
        ;;
esac

binary="$bindir/realitytrace"
if [ ! -x "$binary" ]; then
    echo "error: missing executable $binary" >&2
    exit 1
fi

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

name="realitytrace-${version}-${target}"
stage="$work/$name"
mkdir -p "$stage"
install -m 0755 "$binary" "$stage/realitytrace"
install -m 0644 "$repo_root/LICENSE" "$stage/LICENSE"
cat > "$stage/README.txt" <<EOF
RealityTrace ${version} (${target})

This archive contains the standalone realitytrace command. It needs a glibc
2.35-or-newer Linux system (Ubuntu 22.04 or newer, e.g. the NVIDIA Jetson
AGX Orin). No Rust toolchain, Cargo, SQLite development headers, or other
language runtime is required: SQLite is bundled into the executable.

Run:
  ./realitytrace --version
  ./realitytrace doctor --json

Install (optional):
  install -m 0755 realitytrace ~/.local/bin/realitytrace

Optional daemon (Unix only):
  A systemd user unit is in the .deb at /usr/lib/systemd/user/realitytrace.service.
  For a tarball install, copy packaging/realitytrace.service (source tree)
  to ~/.config/systemd/user/ and adjust ExecStart to this binary's path,
  then: systemctl --user enable --now realitytrace

DomainForge integration (optional, semantic snapshots only):
  RealityTrace pins DomainForge 0.21.2. It locates the compiler via
  REALITYTRACE_DOMAINFORGE_BIN, then SXR_DOMAINFORGE_BIN, then "domainforge"
  on PATH. Verify with: domainforge --version
  Override explicitly: REALITYTRACE_DOMAINFORGE_BIN=/path/to/domainforge

Licence: Apache-2.0, see LICENSE.
EOF

mkdir -p "$outdir"
archive="$outdir/${name}.tar.gz"
tar --sort=name --mtime='UTC 1970-01-01' --owner=0 --group=0 --numeric-owner \
    -C "$work" -cf - "$name" | gzip -9n > "$archive"

test -s "$archive"
tar -tzf "$archive" | grep -q "^${name}/realitytrace$"
echo "OK: built $archive"

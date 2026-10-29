#!/bin/sh
# Print the RealityTrace CLI version (the version the `realitytrace` binary
# reports), and refuse to continue when the CLI crate and the release identity
# have drifted.
#
# Usage: packaging/version.sh [requested-tag-or-version]
#
# The workspace version is NOT the CLI version (the workspace carries its own
# independent version), so the CLI crate is the single source of truth here.
# With an argument, the argument must name that exact release: either
# `<version>` or the component tag `realitytrace-v<version>`. Anything else is
# refused, which is what keeps the binary publisher from attaching artifacts
# built from one tag to a different release.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' "$root/sxr-cli/Cargo.toml" | head -1)
case "$version" in
    '' | *[!0-9A-Za-z.+~-]*)
        echo "error: invalid or missing CLI version '$version' in sxr-cli/Cargo.toml" >&2
        exit 1
        ;;
esac

if [ "$#" -gt 0 ] && [ -n "$1" ]; then
    requested=$1
    requested=${requested#realitytrace-v}
    requested=${requested#v}
    if [ "$requested" != "$version" ]; then
        echo "error: requested release '$1' does not name CLI version '$version'" >&2
        exit 1
    fi
fi

printf '%s\n' "$version"

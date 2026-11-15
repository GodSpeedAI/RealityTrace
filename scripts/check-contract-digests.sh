#!/usr/bin/env sh
# Fails when a schema in contracts (or $SXR_CONTRACTS_DIR) differs from its
# DIGESTS manifest, or is missing from it. Consumers vendor these schemas by
# digest, so a changed schema must come with a new manifest line in the same
# commit.
#
# Usage: scripts/check-contract-digests.sh [--update]
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
dir=${SXR_CONTRACTS_DIR:-$root/contracts}
manifest=$dir/DIGESTS

if [ "${1:-}" = "--update" ]; then
  (cd "$dir" && sha256sum -- *.v*.json) >"$manifest"
  exit 0
fi

[ -f "$manifest" ] || { echo "fail: $manifest is missing" >&2; exit 2; }
status=0
for f in "$dir"/*.json; do
  name=$(basename "$f")
  want=$(awk -v n="$name" '$2 == n { print $1 }' "$manifest")
  have=$(sha256sum "$f" | awk '{ print $1 }')
  if [ -z "$want" ]; then
    echo "DIGEST MISSING $name: not listed in DIGESTS"; status=1
  elif [ "$want" != "$have" ]; then
    echo "DIGEST MISMATCH $name: manifest=$want actual=$have"; status=1
  fi
done
for name in $(awk '{ print $2 }' "$manifest"); do
  [ -f "$dir/$name" ] || { echo "DIGEST STALE $name: listed but absent"; status=1; }
done
[ "$status" -eq 0 ] && echo "ok: $(wc -l <"$manifest" | tr -d ' ') contract digests match"
exit "$status"

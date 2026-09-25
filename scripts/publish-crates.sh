#!/usr/bin/env bash
set -euo pipefail

# Scripts to publish RealityTrace crates to crates.io in dependency order with
# idempotency guards and sparse index propagation retries.

DRY_RUN=""
if [[ "${1:-}" == "--dry-run" ]]; then
  DRY_RUN="--dry-run"
  echo "==> Running in dry-run mode"
fi

CRATES_IN_ORDER=(
  "sxr-core"
  "sxr-git"
  "sxr-ledger"
  "sxr-df"
  "sxr-verify"
  "realitytrace"
)

publish_crate() {
  local crate="$1"
  echo "=================================================="
  echo "==> Publishing $crate to crates.io..."
  echo "=================================================="
  local max_retries=6
  local attempt=1
  while [ $attempt -le $max_retries ]; do
    set +e
    out=$(cargo publish -p "$crate" $DRY_RUN --color never 2>&1)
    rc=$?
    set -e
    printf '%s\n' "$out"
    if [ "$rc" -eq 0 ]; then
      echo "::notice::Successfully published $crate"
      if [[ -z "$DRY_RUN" && "$crate" != "realitytrace" ]]; then
        echo "Waiting 15s for crates.io index propagation..."
        sleep 15
      fi
      return 0
    fi
    if printf '%s' "$out" | grep -qiE 'already (exists|uploaded|published)|is already published'; then
      echo "::notice::$crate is already published — skipping"
      return 0
    fi
    if printf '%s' "$out" | grep -qiE 'no matching package named|failed to select a version for the requirement'; then
      echo "Waiting for dependent crates to propagate in crates.io index (attempt $attempt/$max_retries)..."
      sleep 15
      attempt=$((attempt + 1))
      continue
    fi
    echo "::error::Failed to publish $crate (exit code $rc)"
    return "$rc"
  done
  echo "::error::Timed out waiting to publish $crate"
  return 1
}

for crate in "${CRATES_IN_ORDER[@]}"; do
  publish_crate "$crate"
done

echo "==> All RealityTrace crates published successfully!"

#!/usr/bin/env bash
set -euo pipefail

# Scripts to publish RealityTrace crates to crates.io in dependency order with
# idempotency guards and sparse index propagation retries.

DRY_RUN=""
if [[ "${1:-}" == "--dry-run" ]]; then
  DRY_RUN="--dry-run"
  echo "==> Running in dry-run mode"
fi

# crates.io rate-limits first-time publishes with a per-account token bucket
# (burst of 5 new crates, then one token per 10 minutes; see
# https://crates.io/docs/rate-limits). A rate-limited request fails with HTTP
# 429 and names the instant the next token becomes available, so wait for that
# instant instead of failing the whole release.
RATE_LIMIT_MARGIN_SECONDS=15
RATE_LIMIT_FALLBACK_SECONDS=600
RATE_LIMIT_MAX_WAIT_SECONDS=660
RATE_LIMIT_MAX_WAITS=${RATE_LIMIT_MAX_WAITS:-3}

# Seconds to wait before retrying a rate-limited publish, derived from the
# retry instant advertised by crates.io; clamped so that a missing or malformed
# hint cannot stall the job, and bounded by RATE_LIMIT_MAX_WAIT_SECONDS.
rate_limit_wait_seconds() {
  local output="$1"
  local hint epoch now seconds
  hint=$(printf '%s' "$output" |
    grep -oE 'try again after [^.]*GMT' |
    head -n 1 |
    sed -e 's/^try again after //' -e 's/[[:space:]]*$//' || true)
  epoch=""
  if [[ -n "$hint" ]]; then
    epoch=$(date -u -d "$hint" +%s 2>/dev/null || true)
  fi
  if [[ -z "$epoch" ]]; then
    printf '%s' "$RATE_LIMIT_FALLBACK_SECONDS"
    return 0
  fi
  now=$(date -u +%s)
  seconds=$((epoch - now + RATE_LIMIT_MARGIN_SECONDS))
  if ((seconds < 1)); then
    seconds=1
  fi
  if ((seconds > RATE_LIMIT_MAX_WAIT_SECONDS)); then
    seconds=$RATE_LIMIT_MAX_WAIT_SECONDS
  fi
  printf '%s' "$seconds"
}

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
  local rate_limit_waits=0
  local wait_seconds
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
    if printf '%s' "$out" | grep -qiE 'status 429|too many [a-z ]+crates'; then
      rate_limit_waits=$((rate_limit_waits + 1))
      if ((rate_limit_waits > RATE_LIMIT_MAX_WAITS)); then
        echo "::error::$crate is still rate-limited from crates.io after $RATE_LIMIT_MAX_WAITS waits"
        return 1
      fi
      wait_seconds=$(rate_limit_wait_seconds "$out")
      echo "::warning::$crate was rate-limited by crates.io (wait $rate_limit_waits/$RATE_LIMIT_MAX_WAITS); retrying in ${wait_seconds}s"
      sleep "$wait_seconds"
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

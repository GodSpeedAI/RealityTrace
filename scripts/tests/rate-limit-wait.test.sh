#!/usr/bin/env bash
#
# Regression test for crates.io HTTP 429 handling in scripts/publish-crates.sh.
#
# Observed failure (release v0.3.0, Release run 36140411607, 2026-09-25): five
# crates published, then crates.io rejected the sixth with
#
#   status 429 Too Many Requests: You have published too many new crates in a
#   short period of time. Please try again after Fri, 25 Sep 2026 13:24:28 GMT
#
# and the publish script aborted the entire release instead of waiting for the
# retry instant the server advertised.
#
# The helpers under test are extracted from the shipped script (everything
# before publish_crate()) rather than copied, so this exercises the code that
# actually runs in CI. If that layout changes, the test fails closed.
set -euo pipefail

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
PUBLISH_SCRIPT="$REPO_ROOT/scripts/publish-crates.sh"

failures=0
checks=0

check() { # check <description> <expected> <actual>
  checks=$((checks + 1))
  if [[ "$2" == "$3" ]]; then
    echo "ok: $1"
  else
    echo "FAIL: $1 (expected '$2', got '$3')"
    failures=$((failures + 1))
  fi
}

check_between() { # check_between <description> <low> <high> <actual>
  checks=$((checks + 1))
  if [[ "$4" -ge "$2" && "$4" -le "$3" ]]; then
    echo "ok: $1"
  else
    echo "FAIL: $1 (expected $2..$3, got '$4')"
    failures=$((failures + 1))
  fi
}

echo "==> Syntax of scripts/publish-crates.sh"
bash -n "$PUBLISH_SCRIPT"
if command -v shellcheck >/dev/null 2>&1; then
  shellcheck "$PUBLISH_SCRIPT"
  echo "ok: shellcheck clean"
fi

echo "==> Extracting helpers from the shipped script"
extracted=$(mktemp)
trap 'rm -f "$extracted"' EXIT
end=$(awk '/^publish_crate\(\)/{print NR - 1; exit}' "$PUBLISH_SCRIPT")
if [[ -z "$end" ]]; then
  echo "FAIL: could not locate publish_crate() in $PUBLISH_SCRIPT"
  exit 1
fi
sed -n "1,${end}p" "$PUBLISH_SCRIPT" >"$extracted"
# shellcheck disable=SC1090
source "$extracted"
for fn in rate_limit_wait_seconds is_rate_limited; do
  if ! declare -F "$fn" >/dev/null; then
    echo "FAIL: $fn is not defined before publish_crate(); extraction is stale"
    exit 1
  fi
done

# The exact text crates.io returned when it rejected realitytrace v0.3.0.
OBSERVED="error: failed to publish realitytrace v0.3.0 to registry at https://crates.io
  the remote server responded with an error (status 429 Too Many Requests): You have published too many new crates in a short period of time. Please try again after Fri, 25 Sep 2026 13:24:28 GMT and see https://crates.io/docs/rate-limits for more details."

echo "==> Classification"
check "the observed 429 is classified as rate limited" "yes" \
  "$(is_rate_limited "$OBSERVED" && echo yes || echo no)"
check "the existing-crate update limit is classified as rate limited" "yes" \
  "$(is_rate_limited 'You have published too many updates to existing crates in a short period of time' && echo yes || echo no)"
check "a 403 auth failure is not a rate limit" "no" \
  "$(is_rate_limited 'the remote server responded with an error (status 403 Forbidden): authentication failed' && echo yes || echo no)"
check "an index-propagation error is not a rate limit" "no" \
  "$(is_rate_limited 'no matching package named sxr-core found' && echo yes || echo no)"
check "an already-published error is not a rate limit" "no" \
  "$(is_rate_limited 'crate version 0.3.0 is already uploaded' && echo yes || echo no)"

echo "==> Wait computation"
check "a retry instant in the past clamps to the 1s floor" "1" \
  "$(rate_limit_wait_seconds "$OBSERVED")"

future=$(date -u -d '+120 seconds' '+%a, %d %b %Y %H:%M:%S GMT')
check_between "a retry instant 120s out is honoured plus margin" 125 145 \
  "$(rate_limit_wait_seconds "... (status 429 Too Many Requests) ... Please try again after $future GMT and see https://crates.io/docs/rate-limits")"

far=$(date -u -d '+3600 seconds' '+%a, %d %b %Y %H:%M:%S GMT')
check "a retry instant past the cap clamps to RATE_LIMIT_MAX_WAIT_SECONDS" \
  "$RATE_LIMIT_MAX_WAIT_SECONDS" "$(rate_limit_wait_seconds "... try again after $far GMT ...")"

check "a 429 without a retry instant falls back to RATE_LIMIT_FALLBACK_SECONDS" \
  "$RATE_LIMIT_FALLBACK_SECONDS" "$(rate_limit_wait_seconds '... (status 429 Too Many Requests) ...')"

check "an unparseable retry instant falls back instead of hanging" \
  "$RATE_LIMIT_FALLBACK_SECONDS" "$(rate_limit_wait_seconds '... try again after not-a-date GMT ...')"

echo
if ((failures == 0)); then
  echo "ALL $checks CHECKS PASSED"
else
  echo "$failures of $checks CHECKS FAILED"
fi
exit "$failures"

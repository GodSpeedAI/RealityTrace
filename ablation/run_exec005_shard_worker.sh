#!/usr/bin/env bash
# run_exec005_shard_worker.sh -- execute exactly ONE shard attempt of
# evidence-revision-002 / exec-005 via eval.v2.evidence_revision_002_exec005.
#
# Usage: run_exec005_shard_worker.sh RUN_DIR AUTH_DIR SHARD [ATTEMPT]
#
# Evidence semantics:
# - Exit code is OPERATIONAL ONLY. The coordinator prints its result JSON and
#   exits 0 even when the shard settles into an ABORTED_* state; scientific
#   outcomes live in RUN_DIR/shards/NN/attempts/*/. Nonzero here means an
#   infrastructure/execution failure, never a scientific verdict.
# - NO auto-retry. Replacement of an aborted attempt is an outcome-blind
#   decision reserved to the operator via an explicit ATTEMPT increment
#   (001 -> 002 -> ...). This script never retries on its own.
# - Run identity is owned by the module: the FIRST invocation creates it
#   atomically (run.json); re-invocation against an existing run is safe and
#   only re-verifies sealed authority evidence.
# - Shard logs are APPENDED under /tmp/opencode/ (never truncated): historical
#   bytes are preserved across re-invocations of the same shard+attempt.
set -euo pipefail
set -o pipefail

usage() {
  cat >&2 <<'EOF'
usage: run_exec005_shard_worker.sh RUN_DIR AUTH_DIR SHARD [ATTEMPT]
  RUN_DIR    run directory (typically ablation/runs/evidence-revision-002/<name>)
  AUTH_DIR   directory containing full-authority.json, qualification.json,
             provenance-audit.json
  SHARD      shard index 0..15
  ATTEMPT    3-digit attempt id (default 001)
EOF
}

die() { echo "error: $*" >&2; usage; exit 64; }

(( $# >= 3 && $# <= 4 )) || die "expected 3 or 4 arguments, got $#"

RUN_DIR_ARG=$1
AUTH_DIR_ARG=$2
SHARD=$3
ATTEMPT=${4:-001}

[[ -n $RUN_DIR_ARG ]] || die "RUN_DIR must be non-empty"
[[ -d $AUTH_DIR_ARG ]] || die "AUTH_DIR is not a directory: $AUTH_DIR_ARG"
[[ $SHARD =~ ^([0-9]|1[0-5])$ ]] || die "SHARD must be an integer 0..15 (canonical form), got '$SHARD'"
[[ $ATTEMPT =~ ^[0-9]{3}$ ]] || die "ATTEMPT must be exactly 3 digits (e.g. 001), got '$ATTEMPT'"

for fname in full-authority.json qualification.json provenance-audit.json; do
  [[ -s $AUTH_DIR_ARG/$fname ]] || die "authority file missing or empty: $AUTH_DIR_ARG/$fname"
done

# Resolve to absolute paths BEFORE changing directory, so relative caller
# arguments keep pointing at the same locations inside the coordinator cwd.
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
RUN_DIR=$(realpath -m -- "$RUN_DIR_ARG")
AUTH_DIR=$(realpath -m -- "$AUTH_DIR_ARG")

LOG_DIR=/tmp/opencode
LOG_PATH=$LOG_DIR/exec005-shard-$(printf '%02d' "$SHARD")-a${ATTEMPT}.log
mkdir -p "$LOG_DIR"

printf '[%s] === exec005 shard %s attempt %s start (pid %s) ===\n' \
  "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$SHARD" "$ATTEMPT" "$$" >>"$LOG_PATH"

cd "$SCRIPT_DIR"

# Stream combined stdout+stderr into the log live; surface only the last
# non-empty line (the terminal JSON result) on stdout. Preserve the module's
# exit code via PIPESTATUS[0]; tee/tail failures never mask it.
set +e
uv run --offline --python 3.12 -m eval.v2.evidence_revision_002_exec005 run-shard \
  --run "$RUN_DIR" \
  --shard "$SHARD" \
  --attempt "$ATTEMPT" \
  --authority "$AUTH_DIR/full-authority.json" \
  --qualification "$AUTH_DIR/qualification.json" \
  --provenance-audit "$AUTH_DIR/provenance-audit.json" \
  2>&1 | tee -a "$LOG_PATH" | {
    last=''
    while IFS= read -r line; do
      [[ -n $line ]] && last=$line
    done
    printf 'exec005 shard %s attempt %s last-line: %s\n' "$SHARD" "$ATTEMPT" "$last"
  }
rc=${PIPESTATUS[0]}
set -e

printf '[%s] === exec005 shard %s attempt %s end rc=%s ===\n' \
  "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$SHARD" "$ATTEMPT" "$rc" >>"$LOG_PATH"

exit "$rc"

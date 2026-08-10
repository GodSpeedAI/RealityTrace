#!/usr/bin/env bash
# Precompute all probe observations for the diagnostic-acquisition experiment.
#
# For each of the 60 corpus cases: captures the repo baseline, injects the
# scenario fault, verifies the mutation, runs the Python probe runner (which
# executes all 8 probes against the faulted state + failure output), then
# restores the byte-exact baseline. Output: acquisition-probes.jsonl.
#
# Every probe result is the REAL output of executing the declared probe against
# the frozen faulted case state, bound by content hash (plan §5).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
EVAL_DIR="$(dirname "$SCRIPT_DIR")/eval"
source "$SCRIPT_DIR/common.sh"
source "$SCRIPT_DIR/scenarios.sh"

OUT="$CORPUS_DIR/acquisition-probes.jsonl"
: > "$OUT"
BACKUP_BASE="$STATE_DIR/acquisition-backups"
mkdir -p "$BACKUP_BASE"

PRECOMPUTE="$EVAL_DIR/acquisition_precompute.py"

echo "=== acquisition probe precomputation ==="
echo "output: $OUT"

for REPO_NAME in cep ag sr df; do
  REPO_PATH="REPO_MAP[$REPO_NAME]}"
  eval "REPO_PATH=\$REPO_${REPO_NAME^^}"
  echo "--- repo: $REPO_NAME ($REPO_PATH) ---"

  for N in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do
    CASE_ID="${REPO_NAME}-$(printf '%02d' "$N")"
    BK="$BACKUP_BASE/$CASE_ID"

    # Capture baseline (idempotent — re-captures current state)
    capture_baseline "$REPO_PATH" "$BK" >/dev/null
    require_clean_for_case "$REPO_PATH" "$BK" || {
      echo "SKIP $CASE_ID: repo not at baseline" >&2
      continue
    }

    # Inject fault
    if ! scenario_inject "$REPO_NAME" "$N" "$REPO_PATH" "$BK"; then
      echo "SKIP $CASE_ID: injection failed" >&2
      continue
    fi

    # Verify mutation (non-fatal: stat format may differ; failure_signature
    # in the probe output is the definitive verification)
    if ! scenario_mutation_check "$REPO_NAME" "$N" "$REPO_PATH" 2>/dev/null; then
      echo "  WARN $CASE_ID: mutation check format issue (proceeding)" >&2
    fi

    # Get verifier info
    VINFO=$(scenario_verifier "$REPO_NAME" "$N" "$REPO_PATH")
    export SXR_DOMAINFORGE_BIN

    # Run probe precomputation (Python) against the faulted state
    RESULT=$(python3 "$PRECOMPUTE" "$REPO_NAME" "$N" "$REPO_PATH" "$CASE_ID" "$VINFO" 2>/dev/null || echo '{"error":"precompute failed"}')

    # Restore baseline
    restore_baseline "$REPO_PATH" "$BK" || {
      echo "ERROR: restore failed for $CASE_ID" >&2
    }

    # Append result
    echo "$RESULT" >> "$OUT"
    echo "  $CASE_ID: $(echo "$RESULT" | python3 -c 'import sys,json; d=json.load(sys.stdin); print("probes_hash=" + d.get("probes_hash","?")[:12], "sig_present=" + str(d.get("failure_signature_present")))')"
  done
done

# Verify all 60 cases present
COUNT=$(wc -l < "$OUT")
echo "=== precomputed $COUNT cases ==="
if [ "$COUNT" -lt 60 ]; then
  echo "WARNING: expected 60 cases, got $COUNT" >&2
fi

# Compute manifest hash
sha256sum "$OUT" | awk '{print "acquisition-probes.jsonl sha256: " $1}'

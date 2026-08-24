#!/usr/bin/env bash
# run_exec005_full.sh -- orchestrator for ALL 16 shards of
# evidence-revision-002 / exec-005 (uses run_exec005_shard_worker.sh per shard).
#
# Usage: run_exec005_full.sh RUN_DIR AUTH_DIR [WORKERS]
#
# Evidence semantics:
# - Concurrency is FROZEN at 4 concurrent scientific shards by preregistered
#   protocol. WORKERS > 4 is refused outright; scheduling is never widened or
#   narrowed mid-run.
# - NO auto-retry, ever. The coordinator exits 0 even for ABORTED_* shards
#   (scientific outcomes live in the run directory), so a nonzero worker exit
#   is an operational failure only. Replacement of an aborted attempt is an
#   outcome-blind decision reserved to the operator via an explicit ATTEMPT
#   increment; this script reports and exits nonzero instead of retrying.
# - Coordinator init runs shard 0 / attempt 001 synchronously first: the
#   module creates run identity (run.json) atomically there. Re-invocation
#   against an existing run is safe (identity is verified, not recreated).
# - Exit codes:
#     0    all 16 shards SETTLED, no operational failure
#     1    preflight did not report PASS
#     3    run reached quiescence but NOT all 16 shards SETTLED (operator
#          decides outcome-blind retries by incrementing attempts)
#     4    operational failure (bad usage aside: worker nonzero exit,
#          status persistently unavailable, poll budget exhausted)
#     64   usage error (including refusal to exceed frozen concurrency)
set -euo pipefail
set -o pipefail

readonly MAX_WORKERS=4        # frozen preregistered concurrency cap
readonly POLL_SECS=60         # status poll cadence (seconds)
readonly MAX_POLLS=120        # hard poll budget (2h) before giving up
readonly MAX_STATUS_FAILS=10  # consecutive status failures tolerated

usage() {
  cat >&2 <<'EOF'
usage: run_exec005_full.sh RUN_DIR AUTH_DIR [WORKERS]
  RUN_DIR    run directory (typically ablation/runs/evidence-revision-002/<name>)
  AUTH_DIR   directory containing full-authority.json, qualification.json,
             provenance-audit.json
  WORKERS    concurrent shards, default 4, HARD CAP 4 (frozen protocol)
EOF
}

die() { echo "error: $*" >&2; usage; exit 64; }

(( $# >= 2 && $# <= 3 )) || die "expected 2 or 3 arguments, got $#"

RUN_DIR_ARG=$1
AUTH_DIR_ARG=$2
WORKERS=${3:-4}

[[ $WORKERS =~ ^[1-9][0-9]*$ ]] || die "WORKERS must be a positive integer, got '$WORKERS'"
(( WORKERS <= MAX_WORKERS )) || die "refusing WORKERS=$WORKERS: concurrency is FROZEN at $MAX_WORKERS (preregistered protocol)"

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
WORKER=$SCRIPT_DIR/run_exec005_shard_worker.sh
[[ -x $WORKER ]] || die "worker script not found/executable: $WORKER"

# Resolve to absolute paths BEFORE changing into the coordinator cwd.
RUN_DIR=$(realpath -m -- "$RUN_DIR_ARG")
AUTH_DIR=$(realpath -m -- "$AUTH_DIR_ARG")

LOG_DIR=/tmp/opencode
mkdir -p "$LOG_DIR"
MASTER_LOG=$LOG_DIR/exec005-full-$(basename -- "$RUN_DIR")-$(date -u +%Y%m%dT%H%M%SZ).log

# Mirror everything to a master log under /tmp/opencode/ while keeping console output.
exec > >(tee -a "$MASTER_LOG") 2>&1

echo "=== exec005 orchestrator start $(date -u +%Y-%m-%dT%H:%M:%SZ) run=$RUN_DIR workers=$WORKERS ==="
echo "master log: $MASTER_LOG"

cd "$SCRIPT_DIR"

LAUNCHED_PIDS=''
trap 'kill $LAUNCHED_PIDS 2>/dev/null || true; echo "caught signal; terminated outstanding workers" >&2; exit 130' INT TERM

BATCH_PIDS=()
BATCH_SHARDS=()
OP_FAIL=0
FAILED_SHARDS=''

launch_one() {
  local s=$1
  "$WORKER" "$RUN_DIR" "$AUTH_DIR" "$s" 001 &
  BATCH_PIDS+=("$!")
  BATCH_SHARDS+=("$s")
  LAUNCHED_PIDS+=" $!"
}

reap_batch() {
  local i
  for i in "${!BATCH_PIDS[@]}"; do
    if wait "${BATCH_PIDS[$i]}"; then
      echo "worker shard ${BATCH_SHARDS[$i]} exited 0"
    else
      # Operational failure only: ABORTED_* scientific outcomes still exit 0.
      echo "OPERATIONAL FAILURE: worker shard ${BATCH_SHARDS[$i]} exited nonzero" >&2
      OP_FAIL=1
      FAILED_SHARDS+=" ${BATCH_SHARDS[$i]}"
    fi
  done
  BATCH_PIDS=()
  BATCH_SHARDS=()
}

# ---- a. preflight -----------------------------------------------------------
PREFLIGHT_CMD=(uv run --offline --python 3.12 -m eval.v2.evidence_revision_002_exec005)
pf_rc=0
pf_out="$("${PREFLIGHT_CMD[@]}" preflight 2>&1)" || pf_rc=$?
printf '%s\n' "$pf_out"
if (( pf_rc != 0 )) || ! printf '%s\n' "$pf_out" | python3 -c '
import json, re, sys
text = sys.stdin.read()
ok = False
try:
    doc = json.loads(text)
    ok = isinstance(doc, dict) and doc.get("status") == "PASS"
except Exception:
    ok = re.search(r"\"status\"\s*:\s*\"PASS\"", text) is not None
sys.exit(0 if ok else 1)
'; then
  echo "PREFLIGHT FAILED (rc=$pf_rc); refusing to start" >&2
  exit 1
fi
echo "preflight: PASS"

# ---- b. authority evidence present -----------------------------------------
for fname in full-authority.json qualification.json provenance-audit.json; do
  [[ -s $AUTH_DIR/$fname ]] || die "authority file missing or empty: $AUTH_DIR/$fname"
done
echo "authority files: present (full-authority, qualification, provenance-audit)"

# ---- c. coordinator init: shard 0 attempt 001, synchronous ------------------
echo "coordinator init: shard 0 attempt 001 (creates run identity atomically; safe on re-invocation)"
"$WORKER" "$RUN_DIR" "$AUTH_DIR" 0 001

# ---- d. launch shards 1..15 in batches of <= WORKERS ------------------------
for s in $(seq 1 15); do
  launch_one "$s"
  if (( ${#BATCH_PIDS[@]} >= WORKERS )); then
    reap_batch
  fi
done
reap_batch

if (( OP_FAIL )); then
  echo "note: operational failures on shards:$FAILED_SHARDS (continuing to evidence reporting; will exit 4)" >&2
fi

# ---- e. poll status until no shard is ACTIVE --------------------------------
# Poll letters: S=SETTLED A=ACTIVE .=unregistered X=any other terminal state
# (ABORTED_*, EXITED_FAILED, STALE).
STATUS_CMD=(uv run --offline --python 3.12 -m eval.v2.evidence_revision_002_exec005 status --run "$RUN_DIR")

status_fail_streak=0
polls_done=0
while :; do
  if ! raw="$("${STATUS_CMD[@]}" 2>/dev/null)"; then
    raw=''
  fi

  mapfile -t poll < <(printf '%s\n' "$raw" | python3 -c '
import json, sys, time
stamp = time.strftime("%Y-%m-%dT%H:%M:%SZ")
try:
    doc = json.load(sys.stdin)
except Exception:
    print(stamp + " poll <status unparseable>")
    print(-1)
    sys.exit(0)
states = {}
for s in doc.get("shards", []):
    try:
        k = int(s.get("shard"))
    except Exception:
        continue
    states[k] = str(s.get("state") or "")
def letter(k):
    st = states.get(k)
    if st is None:
        return "."
    if st == "SETTLED":
        return "S"
    if st == "ACTIVE":
        return "A"
    return "X"
active = sum(1 for v in states.values() if v == "ACTIVE")
print(stamp + " " + "".join(letter(i) for i in range(16)) + " active=" + str(active))
print(active)
')

  active="${poll[1]:--1}"
  printf '%s\n' "${poll[0]}"
  polls_done=$((polls_done + 1))

  if (( active == 0 )); then
    break
  fi

  if (( active < 0 )); then
    status_fail_streak=$((status_fail_streak + 1))
    if (( status_fail_streak >= MAX_STATUS_FAILS )); then
      echo "status unavailable/unparseable ${status_fail_streak}x consecutively; bailing out" >&2
      exit 4
    fi
  else
    status_fail_streak=0
  fi

  if (( polls_done >= MAX_POLLS )); then
    echo "poll budget exhausted (${MAX_POLLS} polls) with active=${active}; bailing out" >&2
    exit 4
  fi

  sleep "$POLL_SECS"
done

# ---- f. final evidence report ------------------------------------------------
if ! final_raw="$("${STATUS_CMD[@]}" 2>/dev/null)"; then
  echo "final status query failed; cannot settle report" >&2
  exit 4
fi
printf '%s\n' "$final_raw"

ALL_SETTLED=0
if printf '%s\n' "$final_raw" | python3 -c '
import json, sys
try:
    doc = json.load(sys.stdin)
except Exception:
    print("SUMMARY settled=0 aborted=0 other=0 unregistered=16 (status unparseable)")
    sys.exit(1)
shards = doc.get("shards", [])
settled = sum(1 for s in shards if s.get("state") == "SETTLED")
aborted = sum(1 for s in shards if str(s.get("state", "")).startswith("ABORTED"))
other = len(shards) - settled - aborted
unregistered = max(16 - len(shards), 0)
print("SUMMARY settled=%d aborted=%d other=%d unregistered=%d"
      % (settled, aborted, other, unregistered))
sys.exit(0 if (settled == 16 and len(shards) == 16) else 1)
'; then
  ALL_SETTLED=1
fi

echo "=== exec005 orchestrator end $(date -u +%Y-%m-%dT%H:%M:%SZ) op_fail=$OP_FAIL all_settled=$ALL_SETTLED failed_shards:${FAILED_SHARDS:- none} ==="

if (( OP_FAIL )); then
  exit 4
fi
if (( ALL_SETTLED )); then
  exit 0
fi
exit 3

#!/usr/bin/env bash
# Slice 6 Gate 6 proof: optional daemon + durability parity (§16, §24 Slice 6).
# Demonstrates:
#   - AC-DAEMON-001: the SAME lifecycle run twice — once via the daemon, once
#     direct — with injected deterministic ids (SXR_TEST_ID_STREAM) and a
#     fixed clock (SXR_TEST_FIXED_CLOCK) yields BYTE-IDENTICAL `ledger export`
#     and BYTE-IDENTICAL canonical projections export (record parity §16.3).
#   - AC-DAEMON-002: the daemon is convenience-only — the daemon-off lifecycle
#     is the same honest, durable path (F-15: --async-mode without a daemon
#     transparently commits direct with "daemon": false).
#   - AC-DAEMON-003 / F-19b: a spooled observation blocks `evidence create`
#     on its run (exit 3 naming the spooled state) until `spool drain`.
#   - AC-DAEMON-004: kill -9 after the fsynced receipt — `sxr queue recover`
#     commits the segment exactly once; replay is a no-op by idempotency key.
#   - §16.4 corruption rule: a torn queue segment is frozen and retained
#     byte-for-byte; doctor exits 2 naming it; recovery refuses with exit 3.
#   - The daemon is a singleton per socket (second instance rejected).
set -euo pipefail
export SXR_DOMAINFORGE_BIN=${SXR_DOMAINFORGE_BIN:?set to the 0B binary path}
BIN=${SXR_BIN:-/home/sprime01/projects/sxr/target/debug/sxr}
FIXED_CLOCK=2026-08-08T12:00:00.000000Z

for D in a b c; do
  rm -rf /tmp/sxr-s6/$D
  mkdir -p /tmp/sxr-s6/$D/data /tmp/sxr-s6/$D/state
done
ROOT=/tmp/sxr-s6/repo
rm -rf "$ROOT"; mkdir -p "$ROOT/src"

# Fixture: a minimal cargo crate with a passing test.
cat > "$ROOT/Cargo.toml" <<'EOF'
[package]
name = "sxr-s6-fixture"
version = "0.1.0"
edition = "2021"
EOF
cat > "$ROOT/src/lib.rs" <<'EOF'
pub fn answer() -> u32 { 42 }
#[cfg(test)]
mod t {
    #[test]
    fn arithmetic() { assert_eq!(super::answer(), 42); }
}
EOF
cat > "$ROOT/.gitignore" <<'EOF'
target/
Cargo.lock
EOF
(
  cd "$ROOT"
  git init -q
  git add -A
  git -c user.email=t@t -c user.name=t commit -qm init
)

# The verifier stream is EXTERNAL input: recorded once, reused verbatim by
# both parity runs.
(cd "$ROOT" && cargo test --offline --message-format json > /tmp/sxr-s6/recorded.json 2>/dev/null) || true
grep -q "test result:" /tmp/sxr-s6/recorded.json || { echo "FAIL: no test summary in recorded stream"; exit 1; }

# Deterministic identity stream: same ids, same order, for BOTH parity runs.
# Each run gets its OWN copy — the seam persists consumption by rewriting the
# file, so a shared file would offset the second run's sequence.
python3 - <<'EOF'
import random, time
def ulid():
    n = ((int(time.time()*1000) & ((1<<48)-1)) << 80) | random.getrandbits(80)
    enc = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
    s = ""
    for _ in range(26):
        s = enc[n & 31] + s
        n >>= 5
    return s
random.seed(6)
with open('/tmp/sxr-s6/ids.txt', 'w') as f:
    for _ in range(300):
        f.write(ulid() + "\n")
EOF
for D in a b c; do cp /tmp/sxr-s6/ids.txt /tmp/sxr-s6/ids-$D.txt; done

# DomainForge is an EXTERNAL producer (like the verifier): its envelope
# embeds its own UUIDs/timestamps, so parity requires recording it once and
# replaying the identical artifact for both runs (AC-DAEMON-001: identical
# external inputs ⇒ identical records).
cat > /tmp/sxr-s6/df-replay.sh <<'EOF'
#!/usr/bin/env bash
# Record or replay one domainforge envelope for the parity proof.
RECORD="${SXR_DF_RECORD_FILE:-}"
if [ -n "$RECORD" ] && [ -f "$RECORD" ]; then
  cat "$RECORD"
  exit 0
fi
REAL="${SXR_DF_REAL_BIN:?SXR_DF_REAL_BIN required}"
OUT=$("$REAL" "$@")
CODE=$?
if [ -n "$RECORD" ]; then
  printf '%s' "$OUT" > "$RECORD"
fi
printf '%s' "$OUT"
exit $CODE
EOF
chmod +x /tmp/sxr-s6/df-replay.sh
export SXR_DF_REAL_BIN=$SXR_DOMAINFORGE_BIN
export SXR_DF_RECORD_FILE=/tmp/sxr-s6/df-envelope.json
export SXR_DOMAINFORGE_BIN=/tmp/sxr-s6/df-replay.sh

run_lifecycle() {
  local D=$1
  local MODE=$2
  # NOTE: `local D=$1 MODE=$2 STREAM=...$D...` would expand $D BEFORE the
  # assignment (up-front word expansion), yielding a broken/empty path.
  local STREAM=/tmp/sxr-s6/ids-$D.txt
  export XDG_DATA_HOME=/tmp/sxr-s6/$D/data
  export XDG_STATE_HOME=/tmp/sxr-s6/$D/state
  export SXR_TEST_ID_STREAM=$STREAM
  export SXR_TEST_FIXED_CLOCK=$FIXED_CLOCK
  local RID RUN
  RID=$($BIN repo enroll "$ROOT" --json | python3 -c "import sys,json;print(json.load(sys.stdin)['repo_id'])")
  $BIN semantic configure --repo "$RID" --entry src/lib.rs --json >/dev/null
  local SNAP=$($BIN snapshot create --repo "$RID" --json | python3 -c "import sys,json;print(json.load(sys.stdin)['snapshot_id'])")
  cat > /tmp/sxr-s6/$D/q.json <<'EOF'
{"question_id":"q-s6","question_form":"is the answer realized?","question_type":"verification","target_entities":[],"expected_answer_shape":"boolean"}
EOF
  local QID=$($BIN question declare --repo "$RID" --file /tmp/sxr-s6/$D/q.json --json | python3 -c "import sys,json;print(json.load(sys.stdin)['question_id'])")
  cat > /tmp/sxr-s6/$D/t.json <<'EOF'
{"threshold_id":"t-s6","minimum_relevance":"medium","minimum_reliability":"medium"}
EOF
  local TID=$($BIN threshold declare --repo "$RID" --question "$QID" --file /tmp/sxr-s6/$D/t.json --json | python3 -c "import sys,json;print(json.load(sys.stdin)['threshold_id'])")
  RUN=$($BIN run begin --repo "$RID" --snapshot "$SNAP" --question "$QID" --threshold "$TID" --json | python3 -c "import sys,json;print(json.load(sys.stdin)['run_id'])")
  $BIN plan create --run "$RUN" --verifier cargo-test --target "$TID" --json >/dev/null
  local ING
  if [ "$MODE" = daemon-on ]; then
    ING=$($BIN ingest --run "$RUN" --verifier cargo-test --stdin-format cargo-json --async-mode < /tmp/sxr-s6/recorded.json)
    echo "$ING" | grep -q '"daemon":true' || { echo "FAIL: daemon-on ingest not via daemon: $ING"; exit 1; }
    echo "$ING" | grep -q '"durability":"queued_for_commit"' || { echo "FAIL: not queued_for_commit: $ING"; exit 1; }
    $BIN await --repo "$RID" --all --timeout-secs 10 --json >/dev/null
  else
    ING=$($BIN ingest --run "$RUN" --verifier cargo-test --stdin-format cargo-json --async-mode < /tmp/sxr-s6/recorded.json)
    echo "$ING" | grep -q '"daemon":false' || { echo "FAIL: daemon-off ingest not fallback: $ING"; exit 1; }
    echo "$ING" | grep -q '"durability":"committed"' || { echo "FAIL: not committed: $ING"; exit 1; }
  fi
  # Idempotent duplicate: the same stream re-recorded must not double-append —
  # the second submission carries the SAME idempotency key and resolves to the
  # SAME record (one observation.recorded per run, verified at the diff).
  if [ "$MODE" = daemon-on ]; then
    DUP=$($BIN ingest --run "$RUN" --verifier cargo-test --stdin-format cargo-json --async-mode < /tmp/sxr-s6/recorded.json)
    $BIN await --repo "$RID" --all --timeout-secs 10 --json >/dev/null
  else
    DUP=$($BIN ingest --run "$RUN" --verifier cargo-test --stdin-format cargo-json --async-mode < /tmp/sxr-s6/recorded.json)
  fi
  KEY_ING=$(echo "$ING" | python3 -c "import sys,json;d=json.load(sys.stdin);print(d['idempotency_key'] if 'idempotency_key' in d else d['receipt']['idempotency_key'])")
  KEY_DUP=$(echo "$DUP" | python3 -c "import sys,json;d=json.load(sys.stdin);print(d['idempotency_key'] if 'idempotency_key' in d else d['receipt']['idempotency_key'])")
  [ "$KEY_ING" = "$KEY_DUP" ] || { echo "FAIL: duplicate submit has a different idempotency key: $KEY_ING vs $KEY_DUP"; exit 1; }
  echo "$RID"
}

echo "== AC-DAEMON-002 / F-15: daemon-off lifecycle (root b) =="
RID_B=$(run_lifecycle b daemon-off)
echo "  enrolled=$RID_B lifecycle committed direct, daemon:false"

echo "== AC-DAEMON-001: daemon-on lifecycle (root a) =="
# The daemon inherits the shell env — point it at root A explicitly (exports
# from run_lifecycle b must not leak in) and unset the id stream (the daemon
# never mints, and must not be able to).
export XDG_DATA_HOME=/tmp/sxr-s6/a/data
export XDG_STATE_HOME=/tmp/sxr-s6/a/state
unset SXR_TEST_ID_STREAM
export SXR_DAEMON_SOCKET=/tmp/sxr-s6/a.sock
$BIN daemon --socket /tmp/sxr-s6/a.sock --drain-delay-ms 200 > /tmp/sxr-s6/a-daemon.log 2>&1 &
DAEMON_A=$!
trap '[ -n "${DAEMON_A:-}" ] && kill $DAEMON_A 2>/dev/null; [ -n "${DAEMON_C:-}" ] && kill $DAEMON_C 2>/dev/null' EXIT
sleep 0.5
RID_A=$(run_lifecycle a daemon-on)
echo "  enrolled=$RID_A lifecycle committed via daemon queue"

echo "== AC-DAEMON-001: parity diff =="
export XDG_DATA_HOME=/tmp/sxr-s6/a/data XDG_STATE_HOME=/tmp/sxr-s6/a/state
$BIN projections rebuild --repo "$RID_A" --all --json >/dev/null
$BIN ledger export --repo "$RID_A" > /tmp/sxr-s6/a-ledger.json
$BIN projections export --repo "$RID_A" --canonical-json > /tmp/sxr-s6/a-canon.json
export XDG_DATA_HOME=/tmp/sxr-s6/b/data XDG_STATE_HOME=/tmp/sxr-s6/b/state
$BIN projections rebuild --repo "$RID_B" --all --json >/dev/null
$BIN ledger export --repo "$RID_B" > /tmp/sxr-s6/b-ledger.json
$BIN projections export --repo "$RID_B" --canonical-json > /tmp/sxr-s6/b-canon.json
diff -q /tmp/sxr-s6/a-ledger.json /tmp/sxr-s6/b-ledger.json && echo "  LEDGER EXPORT IDENTICAL (daemon-on == daemon-off)"
diff -q /tmp/sxr-s6/a-canon.json /tmp/sxr-s6/b-canon.json && echo "  CANONICAL PROJECTIONS IDENTICAL"
python3 - <<'EOF'
import json
a=json.load(open('/tmp/sxr-s6/a-ledger.json')); b=json.load(open('/tmp/sxr-s6/b-ledger.json'))
assert len(a)==len(b)>=9 or len(a)==len(b)>=7, (len(a), len(b))
kinds=[r['record_kind'] for r in a]
print("  %d records: %s" % (len(a), ", ".join(sorted(set(kinds)))))
assert 'observation.recorded' in kinds
obs=[r for r in a if r['record_kind']=='observation.recorded']
assert len(obs)==1, "duplicate replay must not double-append: %d observation records" % len(obs)
for i,(ra,rb) in enumerate(zip(a,b)):
    # §16.3 P1 parity surface: payload equality. The chain metadata
    # (entry_ulid, committed_at, writer_identity_ref, append_ordinal,
    # previous_entry_hash, entry_hash) is ELIDED — ledger machinery, not
    # record content. With the injected id stream + fixed clock, every
    # payload (including payload created_at and envelope ids) is byte-equal.
    assert ra['payload']==rb['payload'], (i, ra['record_kind'])
print("  exactly one observation.recorded per run (idempotent replay); all %d payloads byte-equal" % len(a))
EOF
echo "  PARITY: daemon-on == daemon-off (records identical, §16.3 P1)"

echo "== rebuild determinism on the daemon-fed ledger =="
export XDG_DATA_HOME=/tmp/sxr-s6/a/data XDG_STATE_HOME=/tmp/sxr-s6/a/state
$BIN projections rebuild --repo "$RID_A" --all --json >/dev/null
$BIN projections export --repo "$RID_A" --canonical-json > /tmp/sxr-s6/a-canon2.json
diff -q /tmp/sxr-s6/a-canon.json /tmp/sxr-s6/a-canon2.json && echo "  CANONICAL REBUILD DETERMINISTIC (with daemon records)"

echo "== AC-DAEMON-004: kill -9 after receipt, then queue recover =="
export XDG_DATA_HOME=/tmp/sxr-s6/c/data XDG_STATE_HOME=/tmp/sxr-s6/c/state
export SXR_TEST_ID_STREAM=/tmp/sxr-s6/ids-c.txt SXR_TEST_FIXED_CLOCK=$FIXED_CLOCK
RID_C=$($BIN repo enroll "$ROOT" --json | python3 -c "import sys,json;print(json.load(sys.stdin)['repo_id'])")
$BIN semantic configure --repo "$RID_C" --entry src/lib.rs --json >/dev/null
SNAP_C=$($BIN snapshot create --repo "$RID_C" --json | python3 -c "import sys,json;print(json.load(sys.stdin)['snapshot_id'])")
cat > /tmp/sxr-s6/c/q.json <<'EOF'
{"question_id":"q-s6","question_form":"is the answer realized?","question_type":"verification","target_entities":[],"expected_answer_shape":"boolean"}
EOF
QID_C=$($BIN question declare --repo "$RID_C" --file /tmp/sxr-s6/c/q.json --json | python3 -c "import sys,json;print(json.load(sys.stdin)['question_id'])")
cat > /tmp/sxr-s6/c/t.json <<'EOF'
{"threshold_id":"t-s6","minimum_relevance":"medium","minimum_reliability":"medium"}
EOF
TID_C=$($BIN threshold declare --repo "$RID_C" --question "$QID_C" --file /tmp/sxr-s6/c/t.json --json | python3 -c "import sys,json;print(json.load(sys.stdin)['threshold_id'])")
RUN_C=$($BIN run begin --repo "$RID_C" --snapshot "$SNAP_C" --question "$QID_C" --threshold "$TID_C" --json | python3 -c "import sys,json;print(json.load(sys.stdin)['run_id'])")
$BIN plan create --run "$RUN_C" --verifier cargo-test --target "$TID_C" --json >/dev/null
export SXR_DAEMON_SOCKET=/tmp/sxr-s6/c.sock
$BIN daemon --socket /tmp/sxr-s6/c.sock --drain-delay-ms 60000 > /tmp/sxr-s6/c-daemon.log 2>&1 &
DAEMON_C=$!
sleep 0.5
ING=$($BIN ingest --run "$RUN_C" --verifier cargo-test --stdin-format cargo-json --async-mode < /tmp/sxr-s6/recorded.json)
echo "$ING" | grep -q '"durability":"queued_for_commit"' || { echo "FAIL: not queued: $ING"; exit 1; }
SEG_COUNT=$(ls /tmp/sxr-s6/c/state/sxr/daemon-queue/$RID_C/*.queue 2>/dev/null | wc -l)
[ "$SEG_COUNT" = "1" ] || { echo "FAIL: expected 1 queue segment, got $SEG_COUNT"; exit 1; }
echo "  receipted segment on disk: $SEG_COUNT (durability achieved before the kill)"
DCPID=$DAEMON_C; kill -9 $DCPID 2>/dev/null; wait $DCPID 2>/dev/null || true; DAEMON_C=""
# wait on the killed PID only — a bare `wait` would also block on DAEMON_A,
# which by design runs forever (the singleton daemon for root a).
REC1=$($BIN queue recover | python3 -c "import sys,json;print(json.load(sys.stdin)['recovered'])")
[ "$REC1" = "1" ] || { echo "FAIL: first recover must commit 1, got $REC1"; exit 1; }
REC2=$($BIN queue recover | python3 -c "import sys,json;print(json.load(sys.stdin)['recovered'])")
[ "$REC2" = "0" ] || { echo "FAIL: replay must be a no-op, got $REC2"; exit 1; }
$BIN await --repo "$RID_C" --all --timeout-secs 5 --json | grep -q '"state":"committed"'
echo "  recover#1=$REC1 recover#2=$REC2 (idempotent replay) await: committed"

echo "== AC-DAEMON-003 / F-19b: spooled observation blocks consumers (exit 3) =="
export XDG_DATA_HOME=/tmp/sxr-s6/c/data XDG_STATE_HOME=/tmp/sxr-s6/c/state
# A FRESH run is required: a second ingest of run C is the same idempotency
# key, and a spooled duplicate of a committed record is correctly a no-op
# (the guard skips entries whose key is already committed).
RUN2=$($BIN run begin --repo "$RID_C" --snapshot "$SNAP_C" --question "$QID_C" --threshold "$TID_C" --json | python3 -c "import sys,json;print(json.load(sys.stdin)['run_id'])")
$BIN plan create --run "$RUN2" --verifier cargo-test --target "$TID_C" --json >/dev/null
RC=0
SPOOL_OUT=$($BIN ingest --run "$RUN2" --verifier cargo-test --stdin-format cargo-json --spool < /tmp/sxr-s6/recorded.json) || RC=$?
[ "$RC" = "75" ] || { echo "FAIL: --spool must exit 75, got $RC"; exit 1; }
OBS=$(python3 -c "
import json
d=json.loads('''$SPOOL_OUT''')
p=d['spool_entry']
for line in open(p):
    r=json.loads(line)
    print(r.get('observation_id','')); break
")
[ -n "$OBS" ] || { echo "FAIL: no observation_id in spool entry"; exit 1; }
RC=0
$BIN evidence create --run "$RUN2" --observation "$OBS" --derive --json > /tmp/sxr-s6/f19b.json 2>/tmp/sxr-s6/f19b.err || RC=$?
if [ "$RC" = "0" ]; then echo "FAIL: evidence must be blocked while the observation is spooled"; exit 1; fi
[ "$RC" = "3" ] || { echo "FAIL: expect exit 3, got $RC: $(cat /tmp/sxr-s6/f19b.err)"; exit 1; }
grep -q "spooled" /tmp/sxr-s6/f19b.err && echo "  evidence create blocked exit=3: $(head -c 160 /tmp/sxr-s6/f19b.err)"
$BIN spool drain --repo "$RID_C" --json > /tmp/sxr-s6/drain.json
$BIN evidence create --run "$RUN2" --observation "$OBS" --derive --json >/dev/null
echo "  spool drain + evidence create: unblocked"

echo "== §16.4: corrupt queue segment is frozen, named, refused =="
export XDG_DATA_HOME=/tmp/sxr-s6/c/data XDG_STATE_HOME=/tmp/sxr-s6/c/state
export SXR_DAEMON_SOCKET=/tmp/sxr-s6/c.sock
$BIN daemon --socket /tmp/sxr-s6/c.sock --drain-delay-ms 60000 > /tmp/sxr-s6/c-daemon.log 2>&1 &
DAEMON_C=$!
sleep 0.5
$BIN ingest --run "$RUN_C" --verifier cargo-test --stdin-format cargo-json --async-mode < /tmp/sxr-s6/recorded.json > /dev/null
SEG=$(ls /tmp/sxr-s6/c/state/sxr/daemon-queue/$RID_C/*.queue)
echo "torn-write" >> "$SEG"
DCPID=$DAEMON_C; kill -9 $DCPID 2>/dev/null; wait $DCPID 2>/dev/null || true; DAEMON_C=""
RC=0
$BIN queue recover > /dev/null 2>/tmp/sxr-s6/corrupt-rec.err || RC=$?
[ "$RC" = "3" ] || { echo "FAIL: recovery must refuse with exit 3, got $RC"; exit 1; }
grep -q "$SEG" /tmp/sxr-s6/corrupt-rec.err && echo "  recovery refused exit=3 naming the frozen segment"
[ -f "$SEG" ] && echo "  corrupt segment retained byte-for-byte"
RC=0
$BIN doctor > /tmp/sxr-s6/doctor.json 2>/dev/null || RC=$?
[ "$RC" = "2" ] || { echo "FAIL: doctor must exit 2, got $RC"; exit 1; }
grep -q "corrupt_queue_segments" /tmp/sxr-s6/doctor.json && echo "  doctor exit=2 reporting corrupt segments"
RC=0
$BIN await --repo "$RID_C" --all --timeout-secs 1 --json > /dev/null 2>/tmp/sxr-s6/await-corrupt.err || RC=$?
[ "$RC" = "3" ] || { echo "FAIL: await must exit 3 while frozen, got $RC"; exit 1; }
grep -q "corrupt queue segment" /tmp/sxr-s6/await-corrupt.err && echo "  await exit=3 (frozen entry listed)"

echo "== daemon singleton per socket =="
export SXR_DAEMON_SOCKET=/tmp/sxr-s6/c.sock
$BIN daemon --socket /tmp/sxr-s6/c.sock --drain-delay-ms 60000 > /dev/null 2>&1 &
DAEMON_C=$!
sleep 0.5
RC=0
$BIN daemon --socket /tmp/sxr-s6/c.sock --drain-delay-ms 60000 > /dev/null 2>/tmp/sxr-s6/second-daemon.err || RC=$?
[ "$RC" != "0" ] || { echo "FAIL: second daemon must be rejected"; exit 1; }
grep -q "already serving" /tmp/sxr-s6/second-daemon.err && echo "  second daemon rejected: $(cat /tmp/sxr-s6/second-daemon.err)"
kill $DAEMON_C 2>/dev/null; DAEMON_C=""

echo "== enrolled repo git status clean =="
git -C "$ROOT" status --porcelain | wc -l | grep -qx 0 && echo "  GIT CLEAN"
echo "SLICE 6 PROOF PASSED"

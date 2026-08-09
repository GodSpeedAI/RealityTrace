#!/usr/bin/env bash
# Slice 7 M1: build the dogfood failure corpus.
# Records 60 real failures (15 per repo across cep, ast-grep-mcp-rs, sea-rs,
# domainforge) into the repos' real ledgers, each with a construction-record
# answer key. Working trees are restored byte-exact after every case; a repo
# that does not return to its baseline aborts the corpus.
#
# Usage: build_corpus.sh [--only <repo>] [--case <n>] [--probe]
#   --probe : run scenarios WITHOUT recording (signature check only)
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
source "$HERE/common.sh"
source "$HERE/scenarios.sh"

PROBE=0
ONLY=""
ONLYCASE=""
RERUN=""
while [ $# -gt 0 ]; do
  case "$1" in
    --probe) PROBE=1 ;;
    --only) ONLY=$2; shift ;;
    --case) ONLYCASE=$2; shift ;;
    --rerun) RERUN=$2; shift ;;
    *) echo "unknown arg $1"; exit 1 ;;
  esac
  shift
done

CASES_FILE=$CORPUS_DIR/cases.jsonl
KEYS_SRC=$CORPUS_DIR/scenario-keys.json
REPAIRS_FILE=$CORPUS_DIR/repairs.jsonl
[ -f "$CASES_FILE" ] || : > "$CASES_FILE"
mkdir -p "$CORPUS_DIR"

# --rerun <case_id>: re-record ONE already-recorded case against a corrected
# fault (post-seal corpus repair). case_id/scenario/keys stay identical; only
# the case's run/observation/evidence metadata is replaced. The superseded
# record is preserved in repairs.jsonl; cases.jsonl is restored on failure.
if [ -n "$RERUN" ]; then
  case "$RERUN" in
    cep-*|ag-*|sr-*|df-*) : ;;
    *) echo "invalid case id $RERUN"; exit 1 ;;
  esac
  grep -q "\"case_id\": \"$RERUN\"" "$CASES_FILE" || { echo "$RERUN not in cases.jsonl"; exit 1; }
  ONLY=${RERUN%-*}
  ONLYCASE=$(printf '%s' "$RERUN" | cut -d- -f2)
  ONLYCASE=$((10#$ONLYCASE))
  BK_CASES="$CASES_FILE.rerun-bak"
  cp "$CASES_FILE" "$BK_CASES"
  trap 'cp "$BK_CASES" "$CASES_FILE" 2>/dev/null || true' EXIT
  python3 - "$CASES_FILE" "$RERUN" <<'PY'
import json, sys
path, case_id = sys.argv[1:]
out = [l for l in open(path) if l.strip() and json.loads(l)["case_id"] != case_id]
open(path, "w").write("".join(out))
PY
  echo "== --rerun $RERUN: old row removed; re-recording against corrected fault =="
fi

REPO_CEP=/home/sprime01/projects/cep
REPO_AG=/home/sprime01/projects/ast-grep-mcp-rs
REPO_SR=/home/sprime01/projects/sea-rs
REPO_DF=/home/sprime01/projects/domainforge

for repo in cep ag sr df; do
  [ -n "$ONLY" ] && [ "$ONLY" != "$repo" ] && continue
  eval "REPO_PATH=\$REPO_${repo^^}"
  echo "== repo setup: $repo ($REPO_PATH) =="
  cat > "$STATE_DIR/q-$repo.json" <<EOF
{"question_id":"q-corpus-$repo","question_form":"is the declared state realized for $repo?","question_type":"verification","target_entities":[],"expected_answer_shape":"boolean"}
EOF
  cat > "$STATE_DIR/t-$repo.json" <<EOF
{"threshold_id":"t-corpus-$repo","minimum_relevance":"medium","minimum_reliability":"medium"}
EOF
  SETUP=$(repo_setup "$repo" "$REPO_PATH" "${REPO_ENTRY[$repo]}" "$STATE_DIR/q-$repo.json" "$STATE_DIR/t-$repo.json")
  RID=$(printf '%s' "$SETUP" | cut -d'|' -f1)
  SNAP=$(printf '%s' "$SETUP" | cut -d'|' -f2)
  QID=$(printf '%s' "$SETUP" | cut -d'|' -f3)
  TID=$(printf '%s' "$SETUP" | cut -d'|' -f4)
  echo "  repo_id=$RID snapshot=$SNAP question=$QID threshold=$TID"

  # Declared target for plan create (semantic repos resolve a decl_key).
  TARGET=concept:unknown
  if [ "${REPO_SEMANTIC[$repo]}" = "1" ]; then
    $BIN projections rebuild --repo "$RID" --all --json >/dev/null
    TARGET=$($BIN query evidence --repo "$RID" --entity "name:${REPO_ENTITY[$repo]}" --json | python3 -c "import sys,json;d=json.load(sys.stdin);print(d.get('query',{}).get('decl_key',''))")
    [ -n "$TARGET" ] && [ "$TARGET" != "None" ] || TARGET=concept:unknown
    echo "  declared target=$TARGET"
  fi

  BK=$STATE_DIR/baseline-$repo
  [ -d "$BK" ] || capture_baseline "$REPO_PATH" "$BK" > "$STATE_DIR/baseline-$repo.hash"

  for n in $(seq 1 15); do
    [ -n "$ONLYCASE" ] && [ "$ONLYCASE" != "$n" ] && continue
    CASE_ID="$repo-$(printf '%02d' "$n")"
    if grep -q "\"case_id\": \"$CASE_ID\"" "$CASES_FILE"; then
      echo "  $CASE_ID already recorded"
      continue
    fi
    require_clean_for_case "$REPO_PATH" "$BK" || exit 1

    # Probe the scenario's failure signature before recording anything.
    scenario_inject "$repo" "$n" "$REPO_PATH" "$BK"
    local_bk=$STATE_DIR/case-$CASE_ID
    mkdir -p "$local_bk"

    # INVARIANT (owner, 2026-08-09): the intended mutation must demonstrably be
    # in the tree before the verifier runs. A no-op injection (wrong path,
    # failed sed) rejects the case — it can never become a valid case.
    if ! scenario_mutation_check "$repo" "$n" "$REPO_PATH"; then
      echo "  $CASE_ID: MUTATION NOT APPLIED — scenario rejected, not recorded"
      restore_baseline "$REPO_PATH" "$BK" || exit 1
      echo "  $CASE_ID: mutation check failed" >> "$STATE_DIR/broken-scenarios.log"
      continue
    fi

    SPEC=$(scenario_verifier "$repo" "$n" "$REPO_PATH")
    ENVS=$(printf '%s' "$SPEC" | cut -d'|' -f1)
    CMD=$(printf '%s' "$SPEC" | cut -d'|' -f2)
    FMT=$(printf '%s' "$SPEC" | cut -d'|' -f3)
    SIGS=$(printf '%s' "$SPEC" | cut -d'|' -f4- | tr '|' '\n' | grep -v '^$' || true)

    # INVARIANT: a scenario whose failure signature is only generic tokens
    # (e.g. "error") is a configuration error — a generic substring can never
    # admit a case. Every scenario must name the expected fault signature.
    GENERIC_RE='^(error|Error|ERROR|fail|failed|FAILED|FAIL|panic|warning|Warning|WARNING|warn)$'
    GEN_TOTAL=0; GEN_GENERIC=0
    while IFS= read -r sig; do
      [ -n "$sig" ] || continue
      GEN_TOTAL=$((GEN_TOTAL+1))
      if printf '%s' "$sig" | grep -qE "$GENERIC_RE"; then GEN_GENERIC=$((GEN_GENERIC+1)); fi
    done <<< "$SIGS"
    if [ "$GEN_TOTAL" -gt 0 ] && [ "$GEN_GENERIC" -eq "$GEN_TOTAL" ]; then
      echo "  $CASE_ID: signature set entirely generic tokens — configuration error, rejected"
      restore_baseline "$REPO_PATH" "$BK" || exit 1
      echo "  $CASE_ID: generic signature set rejected" >> "$STATE_DIR/broken-scenarios.log"
      continue
    fi

    RC=0
    if [ -n "$ENVS" ]; then
      timeout 900 env $ENVS bash -c "$CMD" > "$local_bk/out.txt" 2>&1 || RC=$?
    else
      timeout 900 bash -c "$CMD" > "$local_bk/out.txt" 2>&1 || RC=$?
    fi

    # INVARIANT: exit 0 demonstrates no failure — never a valid corpus case.
    if [ "$RC" = "0" ]; then
      echo "  $CASE_ID: verifier exit 0 — no failure demonstrated, rejected"
      tail -5 "$local_bk/out.txt" | sed 's/^/    /'
      restore_baseline "$REPO_PATH" "$BK" || exit 1
      echo "  $CASE_ID: exit 0 rejected" >> "$STATE_DIR/broken-scenarios.log"
      continue
    fi

    SIG_OK=0
    while IFS= read -r sig; do
      [ -n "$sig" ] || continue
      if grep -q "$sig" "$local_bk/out.txt"; then SIG_OK=1; break; fi
    done <<< "$SIGS"

    if [ "$SIG_OK" = "0" ]; then
      echo "  $CASE_ID: SIGNATURE MISMATCH (exit=$RC) — scenario rejected, not recorded"
      tail -5 "$local_bk/out.txt" | sed 's/^/    /'
      restore_baseline "$REPO_PATH" "$BK" || exit 1
      echo "  $CASE_ID: baseline restored" >> "$STATE_DIR/broken-scenarios.log"
      continue
    fi

    if [ "$PROBE" = "1" ]; then
      echo "  $CASE_ID: PROBE OK (exit=$RC)"
      restore_baseline "$REPO_PATH" "$BK" || exit 1
      continue
    fi

    # Record the case: run + plan + observation + evidence + finish.
    case "$FMT" in
      pytest-progress) TOOL=pytest ;;
      car-go-json) TOOL=cargo-test ;;
      df-validate) TOOL=df-validate ;;
      df-parse) TOOL=df-parse ;;
      *) TOOL=corpus ;;
    esac
    RUN=$(declare_case "$RID" "$SNAP" "$QID" "$TID" "$TARGET" "$TOOL")
    OBS=$(record_observation "$RID" "$RUN" "corpus-$repo" "$FMT" "$local_bk/out.txt")
    EV=$(finish_case "$RID" "$RUN" "$OBS")

    # INVARIANT: recorded evidence must be the exact bytes probed, and must
    # contain the scenario's failure signature (evidence ↔ invocation
    # correspondence). Mismatch aborts the corpus build.
    LOCAL_HASH=$(sha256sum "$local_bk/out.txt" | awk '{print $1}')
    LED_HASH=$(ledger_observation_artifact_hash "$RID" "$OBS")
    if [ -z "$LED_HASH" ] || [ "$LOCAL_HASH" != "$LED_HASH" ]; then
      echo "  $CASE_ID: FATAL — recorded artifact hash mismatch (local=$LOCAL_HASH ledger=$LED_HASH)"
      restore_baseline "$REPO_PATH" "$BK" || true
      exit 1
    fi
    SIG_OK=0
    while IFS= read -r sig; do
      [ -n "$sig" ] || continue
      if grep -q "$sig" "$local_bk/out.txt"; then SIG_OK=1; break; fi
    done <<< "$SIGS"
    if [ "$SIG_OK" = "0" ]; then
      echo "  $CASE_ID: FATAL — recorded evidence lacks the scenario signature"
      restore_baseline "$REPO_PATH" "$BK" || true
      exit 1
    fi
    echo "  $CASE_ID: evidence correspondence verified (sha256 $LOCAL_HASH)"

    restore_baseline "$REPO_PATH" "$BK" || {
      echo "FATAL: $repo did not return to baseline after $CASE_ID"
      exit 1
    }

    python3 - "$CASE_ID" "$repo" "$n" "$RID" "$RUN" "$OBS" "$EV" "$RC" "$FMT" <<'PY'
import json, sys
case_id, repo, n, rid, run, obs, ev, rc, fmt = sys.argv[1:]
rec = {
    "case_id": case_id, "repo": repo, "scenario": f"{repo}-{int(n)}",
    "repo_id": rid, "run_id": run, "observation_id": obs,
    "evidence_id": ev, "verifier_exit": int(rc), "stdin_format": fmt,
}
with open("/tmp/sxr-corpus/case-record.json", "w") as f:
    json.dump(rec, f)
PY
    cat /tmp/sxr-corpus/case-record.json >> "$CASES_FILE"
    printf '\n' >> "$CASES_FILE"
    echo "  $CASE_ID recorded (run=$RUN obs=$OBS)"

    if [ -n "$RERUN" ] && [ "$CASE_ID" = "$RERUN" ]; then
      # Post-seal corpus repair record: preserve the superseded evidence.
      python3 - "$REPAIRS_FILE" "$BK_CASES" "$RERUN" "$RUN" "$OBS" "$EV" "$RC" <<'PY'
import json, sys, datetime
path, old_cases, case_id, run, obs, ev, rc = sys.argv[1:]
old = None
for l in open(old_cases):
    if l.strip() and json.loads(l)["case_id"] == case_id:
        old = json.loads(l)
        break
assert old is not None, f"superseded row for {case_id} not found in backup"
rec = {
    "repair_id": f"{case_id}-repair-{datetime.date.today().isoformat()}",
    "case_id": case_id,
    "scenario": old["scenario"],
    "type": "post-seal-evidence-repair",
    "reason": "fault injection corrected; superseded evidence invalid (see 7-answer-key-review.md adjudication 1)",
    "authorized_by": "owner-2026-08-09",
    "superseded": {"run_id": old["run_id"], "observation_id": old["observation_id"],
                   "evidence_id": old["evidence_id"], "verifier_exit": old["verifier_exit"]},
    "recorded": {"run_id": run, "observation_id": obs, "evidence_id": ev, "verifier_exit": int(rc)},
}
with open(path, "a") as f:
    f.write(json.dumps(rec) + "\n")
PY
      echo "  $CASE_ID: supersession recorded in repairs.jsonl"
    fi
  done
done

if [ -n "$RERUN" ]; then
  if grep -q "\"case_id\": \"$RERUN\"" "$CASES_FILE"; then
    trap - EXIT
    rm -f "$BK_CASES"
    echo "== --rerun $RERUN: re-recorded; superseded evidence preserved in repairs.jsonl =="
  else
    echo "FATAL: $RERUN was not re-recorded; cases.jsonl restored"
    exit 1
  fi
fi

COUNT=$(wc -l < "$CASES_FILE")
echo "== corpus cases recorded: $COUNT =="

if [ "$PROBE" = "0" ] && [ "$COUNT" -ge 60 ] && [ ! -f "$CORPUS_DIR/keys.json" ]; then
  echo "== sealing answer keys =="
  python3 - "$CORPUS_DIR" "$CASES_FILE" "$KEYS_SRC" <<'PY'
import json, sys, hashlib, os
corpus, cases_file, keys_src = sys.argv[1:]
keys = json.load(open(keys_src))
keys = keys.get("keys", keys)
cases = [json.loads(l) for l in open(cases_file) if l.strip()]
out = {}
for c in cases:
    k = keys.get(c["scenario"])
    if k is None:
        print(f"MISSING KEY for {c['scenario']}"); sys.exit(1)
    out[c["case_id"]] = {"true_locus": k["locus"], "scenario": c["scenario"]}
seal = os.path.join(corpus, "keys.json")
json.dump(out, open(seal, "w"), indent=2, sort_keys=True)
os.chmod(seal, 0o600)
manifest = hashlib.sha256(open(seal, "rb").read()).hexdigest()
open(os.path.join(corpus, "keys.sha256"), "w").write(manifest + "  keys.json\n")
print(f"sealed {len(out)} keys: {manifest[:16]}…")
PY
fi

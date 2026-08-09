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
while [ $# -gt 0 ]; do
  case "$1" in
    --probe) PROBE=1 ;;
    --only) ONLY=$2; shift ;;
    --case) ONLYCASE=$2; shift ;;
    *) echo "unknown arg $1"; exit 1 ;;
  esac
  shift
done

CASES_FILE=$CORPUS_DIR/cases.jsonl
KEYS_SRC=$CORPUS_DIR/scenario-keys.json
[ -f "$CASES_FILE" ] || : > "$CASES_FILE"
mkdir -p "$CORPUS_DIR"

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
    SPEC=$(scenario_verifier "$repo" "$n" "$REPO_PATH")
    ENVS=$(printf '%s' "$SPEC" | cut -d'|' -f1)
    CMD=$(printf '%s' "$SPEC" | cut -d'|' -f2)
    FMT=$(printf '%s' "$SPEC" | cut -d'|' -f3)
    SIGS=$(printf '%s' "$SPEC" | cut -d'|' -f4- | tr '|' '\n' | grep -v '^$' || true)
    RC=0
    if [ -n "$ENVS" ]; then
      timeout 900 env $ENVS bash -c "$CMD" > "$local_bk/out.txt" 2>&1 || RC=$?
    else
      timeout 900 bash -c "$CMD" > "$local_bk/out.txt" 2>&1 || RC=$?
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
  done
done

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

#!/usr/bin/env bash
# Slice 7 M1 closure: §26 settlement coverage + §12.5a residual substrate.
#
# Phase 1 — one honest settlement per recorded corpus case. Corpus ingest
# evidence carries no exit code, so no evidence supports: every case settles
# `unsettled` (the 60-case corpus alone can only produce that status).
#
# Phase 2 — semantic repos (sea-rs, domainforge): commit the residual +
# hypothesis substrate once (residual.computed + diagnostic.hypothesis per
# candidate failure locus). Deterministic ids make re-runs no-ops.
#
# Phase 3 — domainforge specials (valid snapshot, green and fast suite):
#   a. clean `verify run` (cargo-test exit 0 -> supports) -> `settled`
#   b. clean `verify run` + --debt -> `settled_with_debt` (+ debt.recorded)
#   c. df-1 injected (failing cargo test -> contradicts) -> `unsettled` with
#      unresolved_contradictions populated; working tree restored after.
# The diagnostic loop is opened (residuals + hypotheses recorded) but never
# settled: corpus evidence is not discriminating, and the harness never
# manufactures discriminating evidence — attribution is the ablation's job.
#
# Phase 4 — gate: ledger verify x4, projections rebuild byte-identical, git
# status clean, settlement distribution printed.
#
# Resumable: phase 1 skips runs already settled; phases 2-3 use marker files.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
source "$HERE/common.sh"
source "$HERE/scenarios.sh"

CASES_FILE=$CORPUS_DIR/cases.jsonl
[ -f "$CASES_FILE" ] || { echo "no corpus yet; run build_corpus.sh first"; exit 1; }

corpus_setup() {
  # corpus_setup <repo> : re-run idempotent repo_setup; prints
  # "repo_id|snapshot_id|question_id|threshold_id".
  local repo=$1
  eval "REPO_PATH=\$REPO_${repo^^}"
  cat > "$STATE_DIR/q-$repo.json" <<EOF
{"question_id":"q-corpus-$repo","question_form":"is the declared state realized for $repo?","question_type":"verification","target_entities":[],"expected_answer_shape":"boolean"}
EOF
  cat > "$STATE_DIR/t-$repo.json" <<EOF
{"threshold_id":"t-corpus-$repo","minimum_relevance":"medium","minimum_reliability":"medium"}
EOF
  repo_setup "$repo" "$REPO_PATH" "${REPO_ENTRY[$repo]}" "$STATE_DIR/q-$repo.json" "$STATE_DIR/t-$repo.json"
}

# ── phase 1: one honest settlement per recorded case ─────────────────────────
echo "== phase 1: per-case settlements =="
settled=0
while IFS= read -r line; do
  [ -n "$line" ] || continue
  REPO=$(printf '%s' "$line" | python3 -c "import sys,json;print(json.loads(sys.stdin.read())['repo'])")
  RUN=$(printf '%s' "$line" | python3 -c "import sys,json;print(json.loads(sys.stdin.read())['run_id'])")
  RID=$(printf '%s' "$line" | python3 -c "import sys,json;print(json.loads(sys.stdin.read())['repo_id'])")
  if ledger_has_settlement_for_run "$RID" "$RUN"; then
    echo "  $REPO $RUN: already settled, skip"
    continue
  fi
  STATUS=$($BIN settlement register --run "$RUN" --question "q-corpus-$REPO" \
    --threshold "t-corpus-$REPO" --transformation transformation:verify --json \
    | python3 -c "import sys,json;print(json.load(sys.stdin)['settlement_status'])")
  settled=$((settled+1))
  echo "  $REPO $RUN: settlement=$STATUS"
done < "$CASES_FILE"
echo "  phase 1 done: $settled new settlements"

# ── phase 2: residual + hypothesis substrate (semantic repos) ────────────────
for repo in sr df; do
  if [ -f "$STATE_DIR/residuals-$repo.done" ]; then
    echo "== phase 2: $repo residuals already done =="
    continue
  fi
  echo "== phase 2: $repo residual substrate =="
  SETUP=$(corpus_setup "$repo")
  RID=$(printf '%s' "$SETUP" | cut -d'|' -f1)
  SNAP=$(printf '%s' "$SETUP" | cut -d'|' -f2)
  QID=$(printf '%s' "$SETUP" | cut -d'|' -f3)
  TID=$(printf '%s' "$SETUP" | cut -d'|' -f4)
  $BIN residual compute --repo "$RID" --question "$QID" --snapshot "$SNAP" \
    --json > "$STATE_DIR/residuals-$repo.json"
  python3 -c "
import json
d = json.load(open('$STATE_DIR/residuals-$repo.json'))
print(d.get('committed_count', 0))
print('\n'.join(r['residual_id'] for r in d.get('residuals', [])))
" | { read -r N; echo "  $repo: $N residuals committed"; while IFS= read -r res; do
    [ -n "$res" ] || continue
    $BIN diagnostic enumerate --repo "$RID" --question "$QID" \
      --snapshot "$SNAP" --residual "$res" --json >/dev/null
    echo "  $repo: hypotheses for $res"
  done; }
  touch "$STATE_DIR/residuals-$repo.done"
done

# ── phase 3: df special settlements ──────────────────────────────────────────
if [ -f "$STATE_DIR/df-specials.done" ]; then
  echo "== phase 3: df specials already done =="
else
  echo "== phase 3: df special settlements =="
  SETUP=$(corpus_setup df)
  RID=$(printf '%s' "$SETUP" | cut -d'|' -f1)
  SNAP=$(printf '%s' "$SETUP" | cut -d'|' -f2)
  QID=$(printf '%s' "$SETUP" | cut -d'|' -f3)
  TID=$(printf '%s' "$SETUP" | cut -d'|' -f4)
  eval "REPO_PATH=\$REPO_DF"
  BK=$STATE_DIR/baseline-df
  [ -d "$BK" ] || capture_baseline "$REPO_PATH" "$BK" > "$STATE_DIR/baseline-df.hash"
  require_clean_for_case "$REPO_PATH" "$BK" || exit 1
  $BIN projections rebuild --repo "$RID" --all --json >/dev/null
  TARGET=$($BIN query evidence --repo "$RID" --entity "name:${REPO_ENTITY[df]}" \
    --json | python3 -c "import sys,json;d=json.load(sys.stdin);print(d.get('query',{}).get('decl_key',''))")
  [ -n "$TARGET" ] && [ "$TARGET" != "None" ] || TARGET=concept:unknown

  # run_special <label> : begin run + plan + verify + evidence + finish; prints "run|obs".
  run_special() {
    local label=$1
    local RUN OBS
    RUN=$($BIN run begin --repo "$RID" --snapshot "$SNAP" --question "$QID" \
      --threshold "$TID" --json | jq_field run_id)
    $BIN plan create --run "$RUN" --verifier cargo-test --target "$TARGET" --json >/dev/null
    OBS=$($BIN verify run --run "$RUN" --json | python3 -c "import sys,json;print(json.load(sys.stdin)['observations'][0]['observation_id'])")
    local EV
    EV=$($BIN evidence create --run "$RUN" --observation "$OBS" --derive --json | jq_field evidence_id)
    $BIN run finish --run "$RUN" --json >/dev/null
    printf '%s|%s' "$RUN" "$OBS"
  }

  # (a) clean run -> settled
  if ! ledger_settlements_for_question "$RID" "$QID" | grep -q '|settled|'; then
    echo "  df: clean run (supports) -> settled"
    RUN=$(run_special df-settled | cut -d'|' -f1)
    $BIN settlement register --run "$RUN" --question "$QID" --threshold "$TID" \
      --transformation transformation:verify --json | python3 -c \
      "import sys,json;d=json.load(sys.stdin);print('    settlement_status='+d['settlement_status'])"
  else
    echo "  df: settled already present, skip"
  fi

  # (b) clean run + debt -> settled_with_debt (+ debt.recorded)
  if ! ledger_settlements_for_question "$RID" "$QID" | grep -q '|settled_with_debt|'; then
    echo "  df: clean run + --debt -> settled_with_debt"
    RUN=$(run_special df-debt | cut -d'|' -f1)
    $BIN settlement register --run "$RUN" --question "$QID" --threshold "$TID" \
      --transformation transformation:verify --debt "no integration evidence" --json \
      | python3 -c "import sys,json;d=json.load(sys.stdin);print('    settlement_status='+d['settlement_status']+' debt_id='+str(d['debt_id']))"
  else
    echo "  df: settled_with_debt already present, skip"
  fi

  # (c) df-1 injected failing run -> unsettled + unresolved_contradictions
  if [ -f "$STATE_DIR/df-contradicts.done" ]; then
    echo "  df: unresolved_contradictions already present, skip"
  else
    echo "  df: df-1 injected failing run -> unresolved_contradictions"
    require_clean_for_case "$REPO_PATH" "$BK" || exit 1
    scenario_inject df 1 "$REPO_PATH" "$BK"
    RUN=$(run_special df-contradicts | cut -d'|' -f1)
    restore_baseline "$REPO_PATH" "$BK" || {
      echo "FATAL: domainforge did not return to baseline after df-1 special"
      exit 1
    }
    $BIN settlement register --run "$RUN" --question "$QID" --threshold "$TID" \
      --transformation transformation:verify --json | python3 -c \
      "import sys,json;d=json.load(sys.stdin);print('    settlement_status='+d['settlement_status']+' unresolved_contradictions='+str(d['unresolved_contradictions']))"
    touch "$STATE_DIR/df-contradicts.done"
  fi

  touch "$STATE_DIR/df-specials.done"
fi

# ── phase 3b: wsl-rebuild enrollment + typed evidence (5th §26 repo) ────────
# wsl-rebuild is the plan's enroll-only §26 repo (no corpus scenarios). Two
# real runs record evidence types beyond `test_result`: `git fsck --full`
# (execution_log) and the repo's own `sxr ledger verify` (conformance_result)
# — genuine streams, unparsed (direction honest: unknown).
if [ -f "$STATE_DIR/wsl-dogfood.done" ]; then
  echo "== phase 3b: wsl-rebuild dogfood already done =="
else
  echo "== phase 3b: wsl-rebuild enrollment + typed evidence =="
  SETUP=$(corpus_setup wsl)
  RID=$(printf '%s' "$SETUP" | cut -d'|' -f1)
  SNAP=$(printf '%s' "$SETUP" | cut -d'|' -f2)
  QID=$(printf '%s' "$SETUP" | cut -d'|' -f3)
  TID=$(printf '%s' "$SETUP" | cut -d'|' -f4)
  eval "REPO_PATH=\$REPO_WSL"
  wsl_run() { # wsl_run <label> <evidence-type> <cmd...>
    local label=$1 evtype=$2
    shift 2
    local RUN OBS
    RUN=$($BIN run begin --repo "$RID" --snapshot "$SNAP" --question "$QID" \
      --threshold "$TID" --json | jq_field run_id)
    $BIN plan create --run "$RUN" --verifier corpus --target concept:unknown --json >/dev/null
    "$@" > "$STATE_DIR/wsl-$label.txt" 2>&1 || true
    OBS=$($BIN ingest --run "$RUN" --verifier corpus --stdin-format corpus \
      < "$STATE_DIR/wsl-$label.txt" | jq_field observation_id)
    $BIN evidence create --run "$RUN" --observation "$OBS" --derive \
      --evidence-type "$evtype" --json >/dev/null
    $BIN run finish --run "$RUN" --json >/dev/null
    echo "  wsl $label: run=$RUN evidence_type=$evtype"
  }
  wsl_run fsck execution_log git -C "$REPO_PATH" fsck --full
  wsl_run ledger conformance_result $BIN ledger verify --repo "$RID"
  touch "$STATE_DIR/wsl-dogfood.done"
fi

# ── phase 4: gate ────────────────────────────────────────────────────────────
echo "== phase 4: final gate =="
for repo in cep ag sr df wsl; do
  RID=$(repo_ids "$repo" | cut -d'|' -f1)
  eval "REPO_PATH=\$REPO_${repo^^}"
  $BIN ledger verify --repo "$RID" --json | python3 -c \
    "import sys,json;d=json.load(sys.stdin);print('  $repo ledger:', d.get('status', 'ok'))"
  [ -f "$STATE_DIR/rebuild-$repo.hash" ] || {
    $BIN projections rebuild --repo "$RID" --all --json >/dev/null
    $BIN projections export --repo "$RID" --canonical-json > "$STATE_DIR/rebuild-$repo.json"
    sha256sum "$STATE_DIR/rebuild-$repo.json" | awk '{print $1}' > "$STATE_DIR/rebuild-$repo.hash"
    echo "  $repo rebuild baseline captured"
  }
  $BIN projections rebuild --repo "$RID" --all --json >/dev/null
  $BIN projections export --repo "$RID" --canonical-json | sha256sum | awk '{print $1}' > "$STATE_DIR/rebuild-$repo.now"
  if diff -q "$STATE_DIR/rebuild-$repo.hash" "$STATE_DIR/rebuild-$repo.now" >/dev/null; then
    echo "  $repo rebuild: byte-identical"
  else
    echo "  $repo rebuild: MISMATCH"
    exit 1
  fi
  BK=$STATE_DIR/baseline-$repo
  # wsl-rebuild is enroll-only (no cases): capture its baseline at gate time.
  if [ ! -d "$BK" ]; then
    capture_baseline "$REPO_PATH" "$BK" > "$STATE_DIR/baseline-$repo.hash"
  fi
  if diff -q "$BK/status.txt" <(git -C "$REPO_PATH" status --porcelain | sort) >/dev/null; then
    echo "  $repo git status: clean"
  else
    echo "  $repo git status: DIRTY"
    git -C "$REPO_PATH" status --porcelain | head -10
    exit 1
  fi
done
echo "== settlement distribution (settlement.registered by status) =="
for repo in cep ag sr df; do
  RID=$(repo_ids "$repo" | cut -d'|' -f1)
  QID=$(repo_ids "$repo" | cut -d'|' -f2)
  echo "  $repo:"
  ledger_settlements_for_question "$RID" "$QID" | cut -d'|' -f2 | sort | uniq -c | sed 's/^/    /'
done
echo "== evidence types (evidence.created by type) =="
for repo in cep ag sr df wsl; do
  RID=$(repo_ids "$repo" | cut -d'|' -f1)
  $BIN ledger export --repo "$RID" 2>/dev/null | python3 -c "
import sys,json
from collections import Counter
d=json.load(sys.stdin)
recs=d if isinstance(d,list) else d.get('records',[])
c=Counter()
for r in recs:
    if r.get('record_kind')=='evidence.created':
        p=r['payload'] if isinstance(r['payload'],dict) else json.loads(r['payload'])
        c[p.get('evidence',[{}])[0].get('evidence_type','?')]+=1
print('  $repo:', ' '.join(f'{k}={v}' for k,v in sorted(c.items())))
"
done
echo "== closure complete =="

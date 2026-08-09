#!/usr/bin/env bash
# Slice 7 corpus harness — shared helpers (sourced by build_corpus.sh).
# Recording model: real dogfood against real §26 repos. Each case applies a
# deterministic fault to the repo's working tree, records the failing
# verification into the repo's real ledger, then restores the working tree to
# its byte-exact pre-case baseline. Baseline restoration is verified against a
# saved `git status --porcelain` snapshot — a mismatch aborts the corpus.
set -euo pipefail

BIN=${SXR_BIN:-/home/sprime01/projects/sxr/target/debug/sxr}
export XDG_DATA_HOME=${XDG_DATA_HOME:-$HOME/.local/share/sxr}
export XDG_STATE_HOME=${XDG_STATE_HOME:-$HOME/.local/state/sxr}
SXR_DOMAINFORGE_BIN=${SXR_DOMAINFORGE_BIN:-/home/sprime01/projects/domainforge-slice0b/target/debug/domainforge}
export SXR_DOMAINFORGE_BIN

CORPUS_DIR=${CORPUS_DIR:-/home/sprime01/projects/sxr/ablation/corpus}
STATE_DIR=${STATE_DIR:-/tmp/sxr-corpus}
mkdir -p "$CORPUS_DIR" "$STATE_DIR"

# ── baseline save/restore (byte-exact) ─────────────────────────────────────

# capture_baseline <repo> <backup-dir>: save git status, HEAD, staged diff,
# unstaged diff, and untracked non-ignored files. Returns a hash of the full
# state.
capture_baseline() {
  local repo=$1 bk=$2
  mkdir -p "$bk"
  git -C "$repo" status --porcelain | sort > "$bk/status.txt"
  git -C "$repo" rev-parse HEAD > "$bk/head.txt"
  git -C "$repo" diff --cached > "$bk/staged.patch" || true
  git -C "$repo" diff > "$bk/unstaged.patch" || true
  local n=0
  while IFS= read -r f; do
    [ -n "$f" ] || continue
    [ -e "$repo/$f" ] || { echo "WARN untracked missing: $f"; continue; }
    local rel
    rel=$(printf '%s' "$f" | sed 's/[^A-Za-z0-9._-]/_/g')
    mkdir -p "$bk/untracked"
    cp -a "$repo/$f" "$bk/untracked/$n-$rel"
    echo "$f" >> "$bk/untracked-list.txt"
    n=$((n+1))
  done < <(git -C "$repo" ls-files --others --exclude-standard)
  : > "$bk/injected-created.txt"
  : > "$bk/injected-modified.txt"
  : > "$bk/injected-chmod.txt"
  sha256sum "$bk/status.txt" "$bk/staged.patch" "$bk/unstaged.patch" "$bk/head.txt" 2>/dev/null | awk '{print $1}' | sha256sum | awk '{print $1}'
}

# restore_baseline <repo> <backup-dir>: revert every injected change, restore
# the exact pre-case state, and verify it matches the saved snapshot.
restore_baseline() {
  local repo=$1 bk=$2
  # Revert injected chmods FIRST (git checkout cannot overwrite a 000-mode
  # file or traverse a 000-mode directory).
  if [ -f "$bk/injected-chmod.txt" ]; then
    while IFS= read -r line; do
      [ -n "$line" ] || continue
      set -- $line
      chmod "$1" "$repo/$2" 2>/dev/null || true
    done < "$bk/injected-chmod.txt"
  fi
  # Revert injected changes to tracked files (index + worktree).
  git -C "$repo" restore --staged --worktree . 2>/dev/null || true
  # Delete files the scenario created.
  if [ -f "$bk/injected-created.txt" ]; then
    while IFS= read -r f; do
      [ -n "$f" ] || continue
      rm -rf "$repo/$f"
    done < "$bk/injected-created.txt"
  fi
  # Restore saved untracked files.
  if [ -f "$bk/untracked-list.txt" ]; then
    local i=0
    while IFS= read -r f; do
      [ -n "$f" ] || continue
      local rel
      rel=$(printf '%s' "$f" | sed 's/[^A-Za-z0-9._-]/_/g')
      mkdir -p "$repo/$(dirname "$f")"
      cp -a "$bk/untracked/$i-$rel" "$repo/$f" 2>/dev/null || true
      i=$((i+1))
    done < "$bk/untracked-list.txt"
  fi
  # Restore pre-existing STAGED modifications (index + worktree).
  if [ -s "$bk/staged.patch" ]; then
    (cd "$repo" && git apply --index "$bk/staged.patch" 2>/dev/null) || {
      echo "ERROR: could not re-apply staged diff in $repo"
      return 1
    }
  fi
  # Restore pre-existing UNSTAGED modifications (worktree only).
  if [ -s "$bk/unstaged.patch" ]; then
    (cd "$repo" && git apply "$bk/unstaged.patch" 2>/dev/null) || {
      echo "ERROR: could not re-apply unstaged diff in $repo"
      return 1
    }
  fi
  # Untracked-file mode+content restored by cp -a; gitignore files (target/
  # __pycache__) created by runs stay — they are invisible to git status and
  # ignored by the comparison below.
  local now
  git -C "$repo" status --porcelain | sort > "$bk/status-now.txt"
  if ! diff -q "$bk/status.txt" "$bk/status-now.txt" >/dev/null; then
    echo "ERROR: $repo working tree did not return to baseline:"
    diff "$bk/status.txt" "$bk/status-now.txt" | head -20
    return 1
  fi
  local head_now
  head_now=$(git -C "$repo" rev-parse HEAD)
  if [ "$head_now" != "$(cat "$bk/head.txt")" ]; then
    echo "ERROR: $repo HEAD moved during the case ($head_now)"
    return 1
  fi
  return 0
}

# require_clean_for_case <repo>: a repo may enter a case only if its baseline
# was captured and it is currently at that baseline (previous case restored).
require_clean_for_case() {
  local repo=$1 bk=$2
  if ! diff -q "$bk/status.txt" <(git -C "$repo" status --porcelain | sort) >/dev/null; then
    echo "ERROR: $repo not at baseline before case"
    return 1
  fi
  return 0
}

# ── sxr lifecycle helpers ───────────────────────────────────────────────────

jq_field() { python3 -c "import sys,json;d=json.load(sys.stdin);print(d.get('$1',''))"; }

# repo_setup <repo-name> <repo-path> <entry> <question-file> <threshold-file>
# Idempotent per repo: enroll / semantic configure / snapshot / question /
# threshold. Prints "repo_id|snapshot_id|question_id|threshold_id".
repo_setup() {
  local name=$1 path=$2 entry=$3 qfile=$4 tfile=$5
  local RID
  RID=$($BIN repo list --json | python3 -c "
import sys,json
d=json.load(sys.stdin)
for r in d.get('repos',[]):
    if r.get('filesystem_path') == '$path': print(r['repo_id']); break
")
  if [ -z "$RID" ]; then
    RID=$($BIN repo enroll "$path" --json | jq_field repo_id)
  fi
  local QID_STABLE TID_STABLE
  QID_STABLE=$($BIN ledger export --repo "$RID" 2>/dev/null | python3 -c "
import sys,json
d=json.load(sys.stdin)
recs=d if isinstance(d,list) else d.get('records',[])
for r in recs:
    if r.get('record_kind')=='question.declared':
        print(r['payload']['questions'][0]['question_id']); break
")
  TID_STABLE=$($BIN ledger export --repo "$RID" 2>/dev/null | python3 -c "
import sys,json
d=json.load(sys.stdin)
recs=d if isinstance(d,list) else d.get('records',[])
for r in recs:
    if r.get('record_kind')=='threshold.declared':
        print(r['payload']['states'][0]['state_id']); break
")
  $BIN semantic configure --repo "$RID" --entry "$entry" --json >/dev/null
  local SNAP
  SNAP=$($BIN snapshot create --repo "$RID" --json | jq_field snapshot_id)
  if [ -z "$QID_STABLE" ]; then
    $BIN question declare --repo "$RID" --file "$qfile" --json >/dev/null
    QID_STABLE=$(python3 -c "import json;print(json.load(open('$qfile'))['question_id'])")
  fi
  if [ -z "$TID_STABLE" ]; then
    $BIN threshold declare --repo "$RID" --question "$QID_STABLE" --file "$tfile" --json >/dev/null
    TID_STABLE=$(python3 -c "import json;print(json.load(open('$tfile'))['threshold_id'])")
  fi
  printf '%s|%s|%s|%s\n' "$RID" "$SNAP" "$QID_STABLE" "$TID_STABLE"
}

# declare_case <repo-id> <snapshot> <question> <threshold> <target> <verifier>
# Creates a fresh run + plan for one case. Prints "run_id".
declare_case() {
  local RID=$1 SNAP=$2 QID=$3 TID=$4 TARGET=$5 VERIFIER=$6
  local RUN
  RUN=$($BIN run begin --repo "$RID" --snapshot "$SNAP" --question "$QID" --threshold "$TID" --json | jq_field run_id)
  $BIN plan create --run "$RUN" --verifier "$VERIFIER" --target "$TARGET" --json >/dev/null
  printf '%s' "$RUN"
}

# record_observation <repo-id> <run> <verifier> <stdin-format> <file>
# Ingest a verifier stream; prints "observation_id|exit_of_ingest".
record_observation() {
  local RID=$1 RUN=$2 VERIFIER=$3 FMT=$4 FILE=$5
  local OUT RC=0
  OUT=$($BIN ingest --run "$RUN" --verifier "$VERIFIER" --stdin-format "$FMT" < "$FILE") || RC=$?
  local OBS
  OBS=$(printf '%s' "$OUT" | jq_field observation_id)
  printf '%s' "$OBS"
}

# finish_case <repo-id> <run> <observation> : evidence + run finish.
finish_case() {
  local RID=$1 RUN=$2 OBS=$3
  local EVIDENCE
  EVIDENCE=$($BIN evidence create --run "$RUN" --observation "$OBS" --derive --json | jq_field evidence_id)
  $BIN run finish --run "$RUN" --json >/dev/null
  printf '%s' "$EVIDENCE"
}

# ── corpus repo registry (shared with close_corpus.sh) ─────────────────────

REPO_CEP=/home/sprime01/projects/cep
REPO_AG=/home/sprime01/projects/ast-grep-mcp-rs
REPO_SR=/home/sprime01/projects/sea-rs
REPO_DF=/home/sprime01/projects/domainforge
REPO_WSL=/home/sprime01/wsl-rebuild

# repo config: name|path|entry|entity(name: for decl_key)|semantic(0/1)
declare -A REPO_ENTRY REPO_ENTITY REPO_SEMANTIC
REPO_ENTRY[cep]=README.md;    REPO_ENTITY[cep]="";          REPO_SEMANTIC[cep]=0
REPO_ENTRY[ag]=README.md;     REPO_ENTITY[ag]="";           REPO_SEMANTIC[ag]=0
REPO_ENTRY[sr]=.sea/interaction/interaction-model.sea; REPO_ENTITY[sr]="CanonicalJourney"; REPO_SEMANTIC[sr]=1
REPO_ENTRY[df]=examples/namespaces/finance/cashflow.sea; REPO_ENTITY[df]="Customer"; REPO_SEMANTIC[df]=1
REPO_ENTRY[wsl]=AGENTS.md;    REPO_ENTITY[wsl]="";          REPO_SEMANTIC[wsl]=0

# repo_ids <repo-name>: resolve enrolled repo + declared question/threshold
# WITHOUT touching the ledger (no snapshot create — idempotent). Prints
# "repo_id|question_id|threshold_id" or exits 1 when not enrolled yet.
repo_ids() {
  local name=$1
  eval "REPO_PATH=\$REPO_${name^^}"
  local RID
  RID=$($BIN repo list --json | python3 -c "
import sys,json
d=json.load(sys.stdin)
for r in d.get('repos',[]):
    if r.get('filesystem_path') == '$REPO_PATH': print(r['repo_id']); break
")
  [ -n "$RID" ] || return 1
  local QID TID
  QID=$($BIN ledger export --repo "$RID" 2>/dev/null | python3 -c "
import sys,json
d=json.load(sys.stdin)
recs=d if isinstance(d,list) else d.get('records',[])
for r in recs:
    if r.get('record_kind')=='question.declared':
        p=r['payload'] if isinstance(r['payload'],dict) else json.loads(r['payload'])
        print(p['questions'][0]['question_id']); break
")
  TID=$($BIN ledger export --repo "$RID" 2>/dev/null | python3 -c "
import sys,json
d=json.load(sys.stdin)
recs=d if isinstance(d,list) else d.get('records',[])
for r in recs:
    if r.get('record_kind')=='threshold.declared':
        p=r['payload'] if isinstance(r['payload'],dict) else json.loads(r['payload'])
        print(p['states'][0]['state_id']); break
")
  printf '%s|%s|%s\n' "$RID" "$QID" "$TID"
}

# ledger_observation_artifact_hash <repo-id> <observation-id>: prints the
# sha256 of the recorded observation's CAS artifact (representation content
# hash for rep:obs:<observation-id>), or nothing when absent. Used to prove
# recorded evidence corresponds byte-for-byte to the probed verifier stream.
ledger_observation_artifact_hash() {
  local rid=$1 obs=$2
  $BIN ledger export --repo "$rid" 2>/dev/null | python3 -c "
import sys, json
d = json.load(sys.stdin)
recs = d if isinstance(d, list) else d.get('records', [])
for r in recs:
    if r.get('record_kind') != 'observation.recorded':
        continue
    p = r['payload'] if isinstance(r['payload'], dict) else json.loads(r['payload'])
    for rep in p.get('representations', []):
        if rep.get('representation_id') == 'rep:obs:$obs':
            h = rep.get('content_hash', '')
            if h.startswith('sha256:'):
                h = h[len('sha256:'):]
            print(h)
            sys.exit(0)
sys.exit(1)
"
}

# ── ledger helpers (idempotency guards for closure) ────────────────────────

# ledger_has_settlement_for_run <repo-id> <run-id>: exit 0 if a
# settlement.registered record already names the run.
ledger_has_settlement_for_run() {
  local rid=$1 run=$2
  $BIN ledger export --repo "$rid" 2>/dev/null | python3 -c "
import sys, json
d = json.load(sys.stdin)
recs = d if isinstance(d, list) else d.get('records', [])
for r in recs:
    if r.get('record_kind') != 'settlement.registered':
        continue
    p = r['payload'] if isinstance(r['payload'], dict) else json.loads(r['payload'])
    if p.get('scope', {}).get('run_id') == '$run':
        sys.exit(0)
sys.exit(1)
"
}

# ledger_settlements_for_question <repo-id> <question-id>: prints
# "<settlement_id>|<settlement_status>|<run>" per settlement.registered naming
# the question, most recent last.
ledger_settlements_for_question() {
  local rid=$1 qid=$2
  $BIN ledger export --repo "$rid" 2>/dev/null | python3 -c "
import sys, json
d = json.load(sys.stdin)
recs = d if isinstance(d, list) else d.get('records', [])
for r in recs:
    if r.get('record_kind') != 'settlement.registered':
        continue
    p = r['payload'] if isinstance(r['payload'], dict) else json.loads(r['payload'])
    s = p.get('settlements', [{}])[0]
    if s.get('question_ref') != '$qid':
        continue
    print(s.get('settlement_id', ''), '|', s.get('settlement_status', ''), '|', p.get('scope', {}).get('run_id', ''), sep='')
"
}

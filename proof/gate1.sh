#!/usr/bin/env bash
set -euo pipefail
export XDG_DATA_HOME=/tmp/sxr-e2e/data
export XDG_STATE_HOME=/tmp/sxr-e2e/state
export SXR_DOMAINFORGE_BIN=${SXR_DOMAINFORGE_BIN:?set to the 0B binary path}
BIN=${SXR_BIN:-$(command -v sxr)}

RID=$($BIN repo list --json | python3 -c "import sys,json;print(json.load(sys.stdin)['repos'][0]['repo_id'])")
$BIN semantic configure --repo "$RID" --entry examples/namespaces/logistics/core.sea --json >/dev/null
SNAP=$($BIN snapshot create --repo "$RID" --json | python3 -c "import sys,json;print(json.load(sys.stdin)['snapshot_id'])")

cat > /tmp/q.json <<'EOF'
{"question_form":"is_true","question_type":"declared_semantics","target_entities":"concept:d842502d-ecbe-570c-93a1-941275af494b"}
EOF
QID=$($BIN question declare --repo "$RID" --file /tmp/q.json --json | python3 -c "import sys,json;print(json.load(sys.stdin)['question_id'])")

cat > /tmp/t.json <<'EOF'
{"threshold_id":"t1","kind":"AllEvidenceSupports","minimum_reliability":"medium"}
EOF
TID=$($BIN threshold declare --repo "$RID" --question "$QID" --file /tmp/t.json --json | python3 -c "import sys,json;print(json.load(sys.stdin)['threshold_id'])")

RUN=$($BIN run begin --repo "$RID" --snapshot "$SNAP" --question "$QID" --threshold "$TID" --json | python3 -c "import sys,json;print(json.load(sys.stdin)['run_id'])")
echo "RUN=$RUN"

$BIN plan create --run "$RUN" --verifier cargo-test --target concept:d842502d-ecbe-570c-93a1-941275af494b --json > /tmp/plan.json
TRANSFORM=$(python3 -c "import json;print(json.load(open('/tmp/plan.json'))['transformation_ref'])")
echo "TRANSFORM=$TRANSFORM"

echo "== verify run =="
$BIN verify run --run "$RUN" --json > /tmp/verify.json
cat /tmp/verify.json; echo
OBS=$(python3 -c "import json;print(json.load(open('/tmp/verify.json'))['observations'][0]['observation_id'])")
echo "OBS=$OBS"

echo "== evidence create --derive =="
$BIN evidence create --run "$RUN" --observation "$OBS" --derive --json > /tmp/ev.json
python3 -c "import json;d=json.load(open('/tmp/ev.json'));print('deriv_count',len(d['derivations']));print('all_precede',all(x['precedes_observation'] for x in d['derivations']));print('all_declared',all(x['declared_at']['append_ordinal'] for x in d['derivations']))"

echo "== settlement register =="
$BIN settlement register --run "$RUN" --question "$QID" --threshold "$TID" --transformation "$TRANSFORM" --json > /tmp/st.json
cat /tmp/st.json; echo

echo "== run finish =="
$BIN run finish --run "$RUN" --json

echo "== ledger verify =="
$BIN ledger verify --repo "$RID" --json | python3 -c "import sys,json;d=json.load(sys.stdin);print('ok',d['ok'])"

echo "== query evidence =="
$BIN query evidence --repo "$RID" --entity concept:d842502d-ecbe-570c-93a1-941275af494b --json

echo "== projections export/rebuild/export =="
$BIN projections export --repo "$RID" --canonical-json > /tmp/before.json
$BIN projections rebuild --repo "$RID" --all --json >/dev/null
$BIN projections export --repo "$RID" --canonical-json > /tmp/after.json
diff -q /tmp/before.json /tmp/after.json && echo "REBUILD IDENTICAL"

echo "== git status unchanged =="
git -C /tmp/sxr-e2e/target status --porcelain | wc -l | grep -qx 0 && echo "GIT CLEAN"
echo "ALL PROOF STEPS PASSED"
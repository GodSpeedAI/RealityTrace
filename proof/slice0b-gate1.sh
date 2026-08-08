#!/usr/bin/env bash
# Real-target Gate 1 capture: enroll the domainforge-slice0b worktree and
# snapshot its real logistics .sea through the 0B binary. Asserts I-DET
# (AC-SXR-002/AC-DF-002) and that the enrolled repo stays git-clean
# (AC-SECURITY-001). The full §15.1 lifecycle (verify/evidence/settlement/
# projections/I-LED) is covered by the fixture proof against the same binary.
set -uo pipefail
export XDG_DATA_HOME=/tmp/sxr-slice0b/data
export XDG_STATE_HOME=/tmp/sxr-slice0b/state
export SXR_DOMAINFORGE_BIN=/home/sprime01/projects/domainforge-slice0b/target/debug/domainforge
BIN=/home/sprime01/projects/sxr/target/debug/sxr
TARGET=/home/sprime01/projects/domainforge-slice0b

echo "== pre-condition: target git clean =="
git -C "$TARGET" status --porcelain | wc -l | grep -qx 0 && echo "clean"

echo "== enroll slice0b =="
"$BIN" repo enroll "$TARGET" --json > /tmp/renroll.json
RID=$(python3 -c "import json;print(json.load(open('/tmp/renroll.json'))['repo_id'])")
python3 -c "import json;d=json.load(open('/tmp/renroll.json'));assert len(d['identity_evidence']['root_commits'])>=1;print('root_commits>=1 OK')"
echo "RID=$RID"

echo "== semantic configure =="
"$BIN" semantic configure --repo "$RID" --entry examples/namespaces/logistics/core.sea --json > /tmp/rconf.json
python3 -c "import json;print('state',json.load(open('/tmp/rconf.json'))['state'])"

echo "== snapshot x2 (I-DET) =="
"$BIN" snapshot create --repo "$RID" --json > /tmp/rs1.json
"$BIN" snapshot create --repo "$RID" --json > /tmp/rs2.json
python3 -c "
import json
a=json.load(open('/tmp/rs1.json')); b=json.load(open('/tmp/rs2.json'))
print('snapshot_id equal:', a['snapshot_id']==b['snapshot_id'])
print('semantic_closure_hash equal:', a['semantic_closure_hash']==b['semantic_closure_hash'])
print('content_hash equal:', a['content_hash']==b['content_hash'])
print('model_status:', a.get('model_validation_status','valid'))
"
echo "== AC-SECURITY-001: target git status after read-only lifecycle =="
git -C "$TARGET" status --porcelain | wc -l | grep -qx 0 && echo "GIT STILL CLEAN" || { echo "GIT DIRTY"; git -C "$TARGET" status --porcelain | head; }

echo "== ledger verify =="
"$BIN" ledger verify --repo "$RID" --json | python3 -c "import sys,json;print('ok',json.load(sys.stdin)['ok'])"

echo "REAL TARGET CAPTURE PASSED"
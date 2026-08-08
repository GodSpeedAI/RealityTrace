#!/usr/bin/env bash
# Slice 2 Gate 2 RED/GREEN proof. Produces RED (fixture not realized), applies
# the fixture realization, produces GREEN, and asserts identity equality across
# the two runs (AC-VERIFY-001).
set -euo pipefail
export XDG_DATA_HOME=/tmp/sxr-rg2/data
export XDG_STATE_HOME=/tmp/sxr-rg2/state
export SXR_DOMAINFORGE_BIN=${SXR_DOMAINFORGE_BIN:?set to the 0B binary path}
BIN=${SXR_BIN:-/home/sprime01/projects/sxr/target/debug/sxr}
REPO=/tmp/sxr-rg2/repo
DF="$SXR_DOMAINFORGE_BIN"

rm -rf /tmp/sxr-rg2/data /tmp/sxr-rg2/state "$REPO"
mkdir -p "$REPO/.sea" "$REPO/src" "$REPO/tests"

cat > "$REPO/Cargo.toml" <<'EOF'
[package]
name = "sxr-rg2-fixture"
version = "0.1.0"
edition = "2021"
[lib]
path = "src/lib.rs"
EOF
cat > "$REPO/.sea-registry.toml" <<'EOF'
[namespaces.logistics]
patterns = [".sea/*.sea"]
EOF
cat > "$REPO/.sea/model.sea" <<'EOF'
Entity "Warehouse"
Entity "Factory"
Resource "Camera" units
Flow "Camera" from "Warehouse" to "Factory" quantity 100
EOF
cat > "$REPO/src/lib.rs" <<'EOF'
pub fn camera_flow_realized() -> bool {
    std::path::Path::new("REALIZED").exists()
}
EOF
cat > "$REPO/tests/realization.rs" <<'EOF'
#[test]
fn declared_camera_flow_is_realized() {
    assert!(sxr_rg2_fixture::camera_flow_realized(), "Camera flow from Warehouse to Factory is not realized");
}
EOF

cd "$REPO"
git init -q
git add -A
git -c user.email=t@t -c user.name=t commit -qm init

echo "== enroll =="
RID=$($BIN repo enroll "$REPO" --json | python3 -c "import sys,json;print(json.load(sys.stdin)['repo_id'])")
echo "RID=$RID"

echo "== semantic configure + snapshot =="
$BIN semantic configure --repo "$RID" --entry .sea/model.sea --json >/dev/null
$BIN snapshot create --repo "$RID" --json > /tmp/rg2-snap.json
SNAP=$(python3 -c "import json;print(json.load(open('/tmp/rg2-snap.json'))['snapshot_id'])")
echo "SNAP=$SNAP"

echo "== domainforge verification-contract + import =="
$DF envelope "$REPO/.sea/model.sea" --emit verification-contract 2>/dev/null > /tmp/rg2-contract.json
$BIN contract import --repo "$RID" --file /tmp/rg2-contract.json --json > /tmp/rg2-cimport.json
CID=$(python3 -c "import json;print(json.load(open('/tmp/rg2-cimport.json'))['contract_id'])")
echo "CID=$CID"

# Bind the FIRST obligation (the flow) for the proof.
OBS_ID=$(python3 -c "import json;o=json.load(open('/tmp/rg2-cimport.json'));print(o['claim_ids'][0])")
QID=$(python3 -c "import json;o=json.load(open('/tmp/rg2-cimport.json'));print(o['question_ids'][0])")
DECL=$(python3 -c "import json;o=json.load(open('/tmp/rg2-cimport.json'));print(o['declaration_refs'][0])")
echo "OBS_ID=$OBS_ID QID=$QID DECL=$DECL"

cat > /tmp/rg2-t.json <<EOF
{"threshold_id":"t-rg2","kind":"AnyEvidenceSupports","minimum_reliability":"medium"}
EOF
TID=$($BIN threshold declare --repo "$RID" --question "$QID" --file /tmp/rg2-t.json --json | python3 -c "import sys,json;print(json.load(sys.stdin)['threshold_id'])")
echo "TID=$TID"

# ── RED run (fixture NOT realized) ──
echo "== RED run =="
RUN_RED=$($BIN run begin --repo "$RID" --snapshot "$SNAP" --question "$QID" --threshold "$TID" --json | python3 -c "import sys,json;print(json.load(sys.stdin)['run_id'])")
$BIN plan create --run "$RUN_RED" --verifier cargo-test --target "$DECL" --from-contract "$CID" --obligation "$OBS_ID" --json > /tmp/rg2-plan-red.json
PLAN_RED=$(python3 -c "import json;print(json.load(open('/tmp/rg2-plan-red.json'))['plan_id'])")
TRANSFORM_RED=$(python3 -c "import json;print(json.load(open('/tmp/rg2-plan-red.json'))['transformation_ref'])")
$BIN verify run --run "$RUN_RED" --json > /tmp/rg2-red-verify.json
RED_OBS=$(python3 -c "import json;print(json.load(open('/tmp/rg2-red-verify.json'))['observations'][0]['observation_id'])")
RED_EXIT=$(python3 -c "import json;print(json.load(open('/tmp/rg2-red-verify.json'))['observations'][0]['exit_code'])")
echo "RED exit=$RED_EXIT"
$BIN evidence create --run "$RUN_RED" --observation "$RED_OBS" --derive --json > /tmp/rg2-red-ev.json
$BIN settlement register --run "$RUN_RED" --question "$QID" --threshold "$TID" --transformation "$TRANSFORM_RED" --json > /tmp/rg2-red-settle.json
RED_STATUS=$(python3 -c "import json;print(json.load(open('/tmp/rg2-red-settle.json'))['settlement_status'])")
RED_MISSING=$(python3 -c "import json;print(json.load(open('/tmp/rg2-red-settle.json'))['missing_evidence'])")
echo "RED status=$RED_STATUS missing=$RED_MISSING"
$BIN run finish --run "$RUN_RED" --json >/dev/null

# ── Apply realization ──
echo "== apply realization =="
touch "$REPO/REALIZED"

# ── GREEN run ──
echo "== GREEN run =="
RUN_GREEN=$($BIN run begin --repo "$RID" --snapshot "$SNAP" --question "$QID" --threshold "$TID" --json | python3 -c "import sys,json;print(json.load(sys.stdin)['run_id'])")
$BIN plan create --run "$RUN_GREEN" --verifier cargo-test --target "$DECL" --from-contract "$CID" --obligation "$OBS_ID" --json > /tmp/rg2-plan-green.json
PLAN_GREEN=$(python3 -c "import json;print(json.load(open('/tmp/rg2-plan-green.json'))['plan_id'])")
TRANSFORM_GREEN=$(python3 -c "import json;print(json.load(open('/tmp/rg2-plan-green.json'))['transformation_ref'])")
$BIN verify run --run "$RUN_GREEN" --json > /tmp/rg2-green-verify.json
GREEN_OBS=$(python3 -c "import json;print(json.load(open('/tmp/rg2-green-verify.json'))['observations'][0]['observation_id'])")
GREEN_EXIT=$(python3 -c "import json;print(json.load(open('/tmp/rg2-green-verify.json'))['observations'][0]['exit_code'])")
echo "GREEN exit=$GREEN_EXIT"
$BIN evidence create --run "$RUN_GREEN" --observation "$GREEN_OBS" --derive --json > /tmp/rg2-green-ev.json
$BIN settlement register --run "$RUN_GREEN" --question "$QID" --threshold "$TID" --transformation "$TRANSFORM_GREEN" --json > /tmp/rg2-green-settle.json
GREEN_STATUS=$(python3 -c "import json;print(json.load(open('/tmp/rg2-green-settle.json'))['settlement_status'])")
echo "GREEN status=$GREEN_STATUS"
$BIN run finish --run "$RUN_GREEN" --json >/dev/null

echo "== identity equality (AC-VERIFY-001) =="
python3 - <<'EOF'
import json
red=json.load(open('/tmp/rg2-plan-red.json'))['plan']
grn=json.load(open('/tmp/rg2-plan-green.json'))['plan']
for k in ['semantic_verification_contract_ref','question_ref','threshold_ref','snapshot_ref']:
    assert red[k]==grn[k], f"{k} differs: {red[k]}!={grn[k]}"
assert red['steps'][0]['obligation_ref']==grn['steps'][0]['obligation_ref'], "obligation differs"
assert red['steps'][0]['claim_ref']==grn['steps'][0]['claim_ref'], "claim differs"
assert red['steps'][0]['targets']==grn['steps'][0]['targets'], "declaration target differs"
print("  contract/obligation/claim/question/threshold/snapshot identical across RED/GREEN")
rs=json.load(open('/tmp/rg2-red-settle.json'))
gs=json.load(open('/tmp/rg2-green-settle.json'))
assert rs['settlement_status']=='unsettled', f"RED must be unsettled, got {rs['settlement_status']}"
assert rs['missing_evidence'], "RED must have populated missing_evidence"
assert gs['settlement_status']=='settled', f"GREEN must be settled, got {gs['settlement_status']}"
assert rs['unresolved_contradictions'], "RED must surface unresolved contradictions"
print("  RED=unsettled+missing+contradiction; GREEN=settled; only evidence/settlement differ")
EOF

echo "== ledger verify =="
$BIN ledger verify --repo "$RID" --json | python3 -c "import sys,json;assert json.load(sys.stdin)['ok']==True; print('  ledger ok')"

echo "== I-LED: every payload valid CEP =="
$BIN ledger export --repo "$RID" --json | python3 -c "import sys,json;[print(json.dumps(r['payload'])) for r in json.load(sys.stdin)]" > /tmp/rg2-payloads.txt
ok=1
while IFS= read -r p; do
  if ! echo "$p" | (cd /home/sprime01/projects/cep && uv run cep validate semantic-envelope --stdin) >/dev/null 2>&1; then
    echo "  FAILED payload: $p"; ok=0
  fi
done < /tmp/rg2-payloads.txt
[ "$ok" = 1 ] && echo "  all payloads pass cep validate"

echo "== projections rebuild byte-identical =="
$BIN projections rebuild --repo "$RID" --all --json >/dev/null
$BIN projections export --repo "$RID" --canonical-json > /tmp/rg2-before.json
$BIN projections rebuild --repo "$RID" --all --json >/dev/null
$BIN projections export --repo "$RID" --canonical-json > /tmp/rg2-after.json
diff -q /tmp/rg2-before.json /tmp/rg2-after.json && echo "  REBUILD IDENTICAL"

echo "== enrolled repo git status clean =="
git -C "$REPO" status --porcelain | wc -l | grep -qx 0 && echo "  GIT CLEAN"
echo "RED/GREEN PROOF PASSED exit=$RED_EXIT/$GREEN_EXIT"
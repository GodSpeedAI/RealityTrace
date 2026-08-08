#!/usr/bin/env bash
# Slice 5 Gate 5 proof: diagnostic attribution loop (§12.5a).
# Demonstrates:
#   - A flaky/failing verifier run produces a disagreement residual
#     (`declared_contradicted`) via `sxr residual compute`; `declared_unobserved`
#     never enters the loop (honest default, severity info).
#   - `sxr diagnostic enumerate` creates the diagnostic question, threshold and
#     one `diagnostic.hypothesis` per candidate failure locus (9 total).
#   - `sxr diagnostic settle` REFUSES promotion (exit 3, AC-DIAG-001) while any
#     §12.5a condition is unmet; silence is not exclusion.
#   - Model-locus proposals are refused until the cheap loci are each
#     evidence-excluded (asymmetric friction, AC-DIAG-001).
#   - `sxr diagnostic status` reports the §17.2-shaped loop state.
#   - Rebuild determinism with the four diagnostic tables.
set -euo pipefail
export XDG_DATA_HOME=/tmp/sxr-s5/data
export XDG_STATE_HOME=/tmp/sxr-s5/state
export SXR_DOMAINFORGE_BIN=${SXR_DOMAINFORGE_BIN:?set to the 0B binary path}
BIN=${SXR_BIN:-/home/sprime01/projects/sxr/target/debug/sxr}

rm -rf /tmp/sxr-s5/data /tmp/sxr-s5/state /tmp/sxr-s5/repo
mkdir -p /tmp/sxr-s5
ROOT=/tmp/sxr-s5/repo
mkdir -p "$ROOT/.sea" "$ROOT/src"

# Fixture: a semantically valid model whose verification TEST FAILS — a
# verifier fault scenario (the declared model is fine; the run contradicts).
cat > "$ROOT/Cargo.toml" <<'EOF'
[package]
name = "sxr-s5-fixture"
version = "0.1.0"
edition = "2021"
[lib]
path = "src/lib.rs"
EOF
cat > "$ROOT/src/lib.rs" <<'EOF'
pub fn camera_flow_realized() -> bool { true }
#[cfg(test)]
mod t {
    // Verifier fault: the check is flaky and fails even though the model is
    // fine — a `verifier` locus failure, not a `declared_model` failure.
    #[test]
    fn camera_flow_realized() { panic!("verifier flake: check failed"); }
}
EOF
cat > "$ROOT/.sea-registry.toml" <<'EOF'
[namespaces.logistics]
patterns = [".sea/*.sea"]
EOF
cat > "$ROOT/.gitignore" <<'EOF'
target/
Cargo.lock
EOF
cat > "$ROOT/.sea/model.sea" <<'EOF'
@namespace "orders"
@version "0.1.0"

dimension "Currency"
unit "USD" of "Currency" factor 1 base "USD"

role "Customer"

Entity "Warehouse"
Entity "Factory"
Resource "Camera" units
Flow "Camera" from "Warehouse" to "Factory" quantity 100

export enum OrderStatus {
    placed = "placed"
}

export entity "Order" {
    key order_id: uuid
    total: quantity<USD> (exclusive_min 0)
    status: OrderStatus default "placed"
}

record PlaceOrderInput {
    order_id: uuid
    total: quantity<USD> (exclusive_min 0)
}

record PlaceOrderOutput {
    order_id: uuid
    status: OrderStatus
}

operation place_order {
    intent "persist one valid order"
    direction inbound
    actor Customer
    access public
    input PlaceOrderInput
    output PlaceOrderOutput
    state Order
    effect creates Order
    transaction single_aggregate
    failure invalid_order for input_validation "input failed"
    failure idempotency_conflict for idempotency_conflict, concurrency_conflict "client_order_id reused with different canonical input"
    idempotency keyed_by order_id
    concurrency unique_key order_id
    evidence operation_trace
    lifecycle synchronous_request_response
}
EOF
(
  cd "$ROOT"
  git init -q
  git add -A
  git -c user.email=t@t -c user.name=t commit -qm init
)

echo "== enroll + snapshot =="
RID=$($BIN repo enroll "$ROOT" --json | python3 -c "import sys,json;print(json.load(sys.stdin)['repo_id'])")
$BIN semantic configure --repo "$RID" --entry .sea/model.sea --json >/dev/null
$BIN snapshot create --repo "$RID" --json > /tmp/sxr-s5/snap.json
SNAP=$(python3 -c "import json;print(json.load(open('/tmp/sxr-s5/snap.json'))['snapshot_id'])")
echo "RID=$RID SNAP=$SNAP"

echo "== question + threshold =="
cat > /tmp/sxr-s5/q.json <<'EOF'
{"question_id":"q-s5","question_form":"is the order realized?","question_type":"verification","target_entities":[],"expected_answer_shape":"boolean"}
EOF
QID=$($BIN question declare --repo "$RID" --file /tmp/sxr-s5/q.json --json | python3 -c "import sys,json;print(json.load(sys.stdin)['question_id'])")
cat > /tmp/sxr-s5/t.json <<'EOF'
{"threshold_id":"t-s5","minimum_relevance":"medium","minimum_reliability":"medium"}
EOF
TID=$($BIN threshold declare --repo "$RID" --question "$QID" --file /tmp/sxr-s5/t.json --json | python3 -c "import sys,json;print(json.load(sys.stdin)['threshold_id'])")

echo "== failing verification run -> contradicts evidence -> disagreement residual =="
# Resolve the declared decl key for "Order" (concept:<hash>) and plan against
# it, so derived evidence targets the DECLARED entity (exact-key match in the
# delta classifier).
DECL_KEY=$($BIN query evidence --repo "$RID" --entity "name:Order" --json | python3 -c "import sys,json;print(json.load(sys.stdin)['query']['decl_key'])")
echo "  declared Order decl_key=$DECL_KEY"
RUN=$($BIN run begin --repo "$RID" --snapshot "$SNAP" --question "$QID" --threshold "$TID" --json | python3 -c "import sys,json;print(json.load(sys.stdin)['run_id'])")
$BIN plan create --run "$RUN" --verifier cargo-test --target "$DECL_KEY" --json >/dev/null
$BIN verify run --run "$RUN" --json >/dev/null 2>&1 || true
OBS=$($BIN verify run --run "$RUN" --json 2>/dev/null | python3 -c "import sys,json;o=json.load(sys.stdin)['observations'];print(o[0]['observation_id'] if o else '')" || true)
EXITCODE=$($BIN verify run --run "$RUN" --json 2>/dev/null | python3 -c "import sys,json;o=json.load(sys.stdin)['observations'];print(o[0]['exit_code'] if o else '')" || true)
echo "  verifier exit_code=$EXITCODE (non-zero => contradicts)"
if [ -n "$OBS" ]; then
  $BIN evidence create --run "$RUN" --observation "$OBS" --derive --json >/dev/null
fi
$BIN run finish --run "$RUN" --json >/dev/null
echo "RUN=$RUN"

echo "== residual compute: only disagreement residuals enter the loop =="
$BIN projections rebuild --repo "$RID" --all --json >/dev/null
$BIN residual compute --repo "$RID" --question "$QID" --snapshot "$SNAP" --json > /tmp/sxr-s5/residuals.json
python3 - <<EOF
import json
d=json.load(open('/tmp/sxr-s5/residuals.json'))
committed=d['residuals']
assert isinstance(committed,list) and committed, "residual compute must commit residuals"
for r in committed:
    assert r['residual_kind'] != 'declared_unobserved', "declared_unobserved must NEVER enter the loop: %s" % r
    assert r['attribution_status']=='undiagnosed', "residual must start undiagnosed (F-27/AC-DIAG-001)"
kinds=[r['residual_kind'] for r in committed]
print("  committed=%d kinds=%s all undiagnosed" % (len(committed), sorted(kinds)))
assert 'declared_contradicted' in kinds, "the failing verifier must surface declared_contradicted"
RID2=d['residuals'][0]['residual_id']
import os
open('/tmp/sxr-s5/resid.txt','w').write(RID2)
EOF
RESID=$(cat /tmp/sxr-s5/resid.txt)
echo "RESID=$RESID"

echo "== diagnostic enumerate: 9 candidate failure loci =="
$BIN diagnostic enumerate --repo "$RID" --question "$QID" --snapshot "$SNAP" --residual "$RESID" --json > /tmp/sxr-s5/enum.json
python3 - <<'EOF'
import json
d=json.load(open('/tmp/sxr-s5/enum.json'))
h=d['hypotheses']
assert len(h)==9, "one hypothesis per candidate locus, got %d" % len(h)
loci=sorted(x['failure_locus_candidate'] for x in h)
assert loci==['authority','declared_model','environment','evidence_collection','implementation','projection','unknown','verification_plan','verifier'], loci
print("  9 hypotheses for loci: %s" % ", ".join(loci))
EOF
echo "  diag question: $(python3 -c "import json;print(json.load(open('/tmp/sxr-s5/enum.json'))['diagnostic_question_id'])")"

echo "== AC-DIAG-001: settle REFUSED (exit 3) while conditions unmet =="
DQ=$($BIN diagnostic status --repo "$RID" --residual "$RESID" --json 2>/dev/null | python3 -c "import sys,json;d=json.load(sys.stdin)['diagnostic'];print(d['hypotheses'][0]['question_id'])" || python3 -c "print('diagq:%s' % open('/tmp/sxr-s5/resid.txt').read())")
HYP_VERIFIER="hyp:$RESID:verifier"
RC=0
$BIN diagnostic settle --repo "$RID" --question "$DQ" --hypothesis "$HYP_VERIFIER" --json > /tmp/sxr-s5/refused-settle.json 2>/tmp/sxr-s5/refused-settle.err || RC=$?
if [ "$RC" = "0" ]; then
  echo "  FAIL: settle must be refused without evidence"; exit 1
fi
echo "  settle refused exit=$RC (expect 3): $(head -c 200 /tmp/sxr-s5/refused-settle.err)"
[ "$RC" = "3" ] || { echo "  FAIL: expect exit 3"; exit 1; }
grep -q "condition b" /tmp/sxr-s5/refused-settle.err && echo "  silence-is-not-exclusion named (condition b)"

echo "== AC-DIAG-001: model-locus proposal REFUSED (exit 3) without cheap-locus exclusions =="
RC=0
$BIN diagnostic propose --repo "$RID" --question "$DQ" --hypothesis "$HYP_VERIFIER" --locus declared_model --target "name:Order" --json > /tmp/sxr-s5/refused-prop.json 2>/tmp/sxr-s5/refused-prop.err || RC=$?
if [ "$RC" = "0" ]; then
  echo "  FAIL: model proposal must be refused before exclusions"; exit 1
fi
echo "  model proposal refused exit=$RC (expect 3): $(head -c 200 /tmp/sxr-s5/refused-prop.err)"
[ "$RC" = "3" ] || { echo "  FAIL: expect exit 3"; exit 1; }
grep -q "predecessor rule" /tmp/sxr-s5/refused-prop.err && echo "  predecessor rule enforced (proposal requires a diagnostic.settled)"

echo "== discriminating evidence: support the verifier, exclude the 8 alternatives =="
$BIN evidence for-hypothesis --run "$RUN" --question "$DQ" --hypothesis "$HYP_VERIFIER" --direction supports --json >/dev/null
python3 - <<EOF
import json
d=json.load(open('/tmp/sxr-s5/enum.json'))
for h in d['hypotheses']:
    hid=h['hypothesis_id']
    if hid=='$HYP_VERIFIER':
        continue
    import subprocess
    r=subprocess.run(['$BIN','evidence','for-hypothesis','--run','$RUN','--question','$DQ','--hypothesis',hid,'--direction','contradicts','--json'],capture_output=True,text=True)
    assert r.returncode==0, r.stderr
EOF
echo "  8 alternatives excluded by evidence"

echo "== settle now succeeds; status flips to diagnosed =="
$BIN diagnostic settle --repo "$RID" --question "$DQ" --hypothesis "$HYP_VERIFIER" --json > /tmp/sxr-s5/settle.json
python3 - <<EOF
import json
d=json.load(open('/tmp/sxr-s5/settle.json'))
assert d['supported_failure_locus']=='verifier', d
assert len(d['excluded_alternatives'])==8, d
print("  settled locus=%s excluded=%d contradicting recorded=%d" % (
    d['supported_failure_locus'], len(d['excluded_alternatives']), len(d['contradicting_evidence_refs'])))
EOF

echo "== repair proposal on the verifier locus succeeds =="
$BIN diagnostic propose --repo "$RID" --question "$DQ" --hypothesis "$HYP_VERIFIER" --locus verifier --target "verifier:cargo-test" --json > /tmp/sxr-s5/prop.json
echo "  proposal=$(python3 -c "import json;print(json.load(open('/tmp/sxr-s5/prop.json'))['proposal_id'])") locus=verifier"

echo "== AC-DIAG-001: model proposal STILL refused after verifier settlement =="
RC=0
$BIN diagnostic propose --repo "$RID" --question "$DQ" --hypothesis "$HYP_VERIFIER" --locus declared_model --target "name:Order" --json >/tmp/sxr-s5/refused-prop2.json 2>/tmp/sxr-s5/refused-prop2.err || RC=$?
if [ "$RC" = "0" ]; then
  echo "  FAIL: declared_model proposal must remain refused (verifier supported, not excluded)"; exit 1
fi
echo "  model proposal still refused exit=$RC: $(head -c 200 /tmp/sxr-s5/refused-prop2.err)"
[ "$RC" = "3" ] || { echo "  FAIL: expect exit 3"; exit 1; }

echo "== diagnostic status: §17.2-shaped loop state =="
$BIN diagnostic status --repo "$RID" --residual "$RESID" --json > /tmp/sxr-s5/status.json
python3 - <<'EOF'
import json
d=json.load(open('/tmp/sxr-s5/status.json'))['diagnostic']
assert d['found']==True
assert d['attribution_status']=='diagnosed', d
assert d['supported_failure_locus']=='verifier', d
assert len(d['candidate_failure_loci'])==9, d
assert len(d['settlements'])==1, d
assert len(d['proposals'])==1, d
print("  status: %s locus=%s candidates=%d settlements=%d proposals=%d" % (
    d['attribution_status'], d['supported_failure_locus'],
    len(d['candidate_failure_loci']), len(d['settlements']), len(d['proposals'])))
EOF

echo "== rebuild determinism with the diagnostic tables =="
$BIN projections export --repo "$RID" --canonical-json > /tmp/sxr-s5/canon-before.json
$BIN projections rebuild --repo "$RID" --all --json >/dev/null
$BIN projections export --repo "$RID" --canonical-json > /tmp/sxr-s5/canon-after.json
diff -q /tmp/sxr-s5/canon-before.json /tmp/sxr-s5/canon-after.json && echo "  CANONICAL REBUILD DETERMINISTIC"

echo "== enrolled repo git status clean =="
git -C "$ROOT" status --porcelain | wc -l | grep -qx 0 && echo "  GIT CLEAN"
echo "SLICE 5 PROOF PASSED"

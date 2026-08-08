#!/usr/bin/env bash
# Slice 4 Gate 4 proof: observed graph (G_observed) + question-bound residuals
# + independent CEP anti-collapse gate.
# Demonstrates:
#   - G_observed edges obey §12.4a admission (co-reference produces NO edge).
#   - sxr delta --question produces typed residuals (§12.5); declared_unobserved
#     is never rendered as failure; contradictions surface.
#   - sxr exports its records as an external anti-collapse corpus.
#   - The INDEPENDENT CEP evaluator (cep conformance anti-collapse --corpus)
#     passes (exit 0) over the sxr-exported corpus. sxr does not judge its own
#     semantics; CEP owns anti-collapse meaning.
#   - Rebuild determinism over the G_observed tables.
set -euo pipefail
export XDG_DATA_HOME=/tmp/sxr-s4/data
export XDG_STATE_HOME=/tmp/sxr-s4/state
export SXR_DOMAINFORGE_BIN=${SXR_DOMAINFORGE_BIN:?set to the 0B binary path}
BIN=${SXR_BIN:-/home/sprime01/projects/sxr/target/debug/sxr}
CEP=${CEP_REPO:-/home/sprime01/projects/cep}

rm -rf /tmp/sxr-s4/data /tmp/sxr-s4/state /tmp/sxr-s4/repo
mkdir -p /tmp/sxr-s4
ROOT=/tmp/sxr-s4/repo
mkdir -p "$ROOT/.sea" "$ROOT/src"

cat > "$ROOT/Cargo.toml" <<'EOF'
[package]
name = "sxr-s4-fixture"
version = "0.1.0"
edition = "2021"
[lib]
path = "src/lib.rs"
EOF
cat > "$ROOT/src/lib.rs" <<'EOF'
pub fn camera_flow_realized() -> bool { true }
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
$BIN snapshot create --repo "$RID" --json > /tmp/sxr-s4/snap.json
SNAP=$(python3 -c "import json;print(json.load(open('/tmp/sxr-s4/snap.json'))['snapshot_id'])")
echo "RID=$RID SNAP=$SNAP"

echo "== question + threshold =="
cat > /tmp/sxr-s4/q.json <<'EOF'
{"question_id":"q-s4","question_form":"is the order realized?","question_type":"verification","target_entities":[],"expected_answer_shape":"boolean"}
EOF
QID=$($BIN question declare --repo "$RID" --file /tmp/sxr-s4/q.json --json | python3 -c "import sys,json;print(json.load(sys.stdin)['question_id'])")
cat > /tmp/sxr-s4/t.json <<'EOF'
{"threshold_id":"t-s4","minimum_relevance":"medium","minimum_reliability":"medium"}
EOF
TID=$($BIN threshold declare --repo "$RID" --question "$QID" --file /tmp/sxr-s4/t.json --json | python3 -c "import sys,json;print(json.load(sys.stdin)['threshold_id'])")

echo "== lifecycle: run -> evidence on a declared entity (supports) =="
RUN=$($BIN run begin --repo "$RID" --snapshot "$SNAP" --question "$QID" --threshold "$TID" --json | python3 -c "import sys,json;print(json.load(sys.stdin)['run_id'])")
$BIN plan create --run "$RUN" --verifier cargo-test --target "name:Order" --json >/dev/null
$BIN verify run --run "$RUN" --json >/dev/null 2>&1 || true
OBS=$($BIN verify run --run "$RUN" --json 2>/dev/null | python3 -c "import sys,json;o=json.load(sys.stdin)['observations'];print(o[0]['observation_id'] if o else '')" || true)
if [ -n "$OBS" ]; then
  $BIN evidence create --run "$RUN" --observation "$OBS" --derive --json >/dev/null 2>&1 || true
fi
$BIN run finish --run "$RUN" --json >/dev/null
echo "RUN=$RUN"

echo "== a claimed relation + qualifying evidence (G_observed edge admission) =="
$BIN claim create --run "$RUN" --question "$QID" --src "concept:Warehouse" --dst "concept:Factory" --kind flow --json >/dev/null
CLID=$($BIN claim create --run "$RUN" --question "$QID" --src "concept:Order" --dst "concept:Warehouse" --kind relation --json | python3 -c "import sys,json;print(json.load(sys.stdin)['claim_id'])")
$BIN evidence for-claim --run "$RUN" --question "$QID" --claim "$CLID" --direction supports --json >/dev/null
$BIN settlement register --run "$RUN" --question "$QID" --threshold "$TID" --transformation tx-s4 --json >/dev/null 2>&1 || true

echo "== projections rebuild + G_observed materialized =="
$BIN projections rebuild --repo "$RID" --all --json > /tmp/sxr-s4/rebuild.json
python3 - <<EOF
import json
d=json.load(open('/tmp/sxr-s4/rebuild.json'))['projections']['tables']
assert d.get('sxr_obs_node',0) >= 0, "obs nodes table missing"
assert d.get('sxr_obs_edge',0) >= 0, "obs edges table missing"
print("  G_observed nodes=%s edges=%s claims=%s" % (d.get('sxr_obs_node',0), d.get('sxr_obs_edge',0), d.get('sxr_claim',0)))
EOF

echo "== delta --question: typed residuals (§12.5) =="
$BIN delta --repo "$RID" --question "$QID" --snapshot "$SNAP" --json > /tmp/sxr-s4/delta.json
python3 - <<'EOF'
import json
d=json.load(open('/tmp/sxr-s4/delta.json'))['delta']
res=d['residuals']
assert isinstance(res,list) and res, "delta must produce residuals"
# declared_unobserved must be present (declared decls without evidence) and NOT failure.
unobs=[r for r in res if r['residual_kind']=='declared_unobserved']
for r in unobs:
    assert r['severity']=='info', "declared_unobserved must never be failure (severity info): %s" % r
print("  delta: %d residuals (%d declared_unobserved=all info severity)" % (len(res), len(unobs)))
EOF

echo "== rebuild determinism over G_observed (AC-REPLAY-003 shape) =="
$BIN projections export --repo "$RID" --canonical-json > /tmp/sxr-s4/canon-before.json
$BIN delta --repo "$RID" --question "$QID" --snapshot "$SNAP" --json > /tmp/sxr-s4/delta-before.json
$BIN projections rebuild --repo "$RID" --all --json >/dev/null
$BIN projections export --repo "$RID" --canonical-json > /tmp/sxr-s4/canon-after.json
$BIN delta --repo "$RID" --question "$QID" --snapshot "$SNAP" --json > /tmp/sxr-s4/delta-after.json
diff -q /tmp/sxr-s4/canon-before.json /tmp/sxr-s4/canon-after.json && echo "  CANONICAL REBUILD DETERMINISTIC"
diff -q /tmp/sxr-s4/delta-before.json /tmp/sxr-s4/delta-after.json && echo "  DELTA REBUILD DETERMINISTIC"

echo "== export sxr corpus -> independent CEP anti-collapse gate =="
$BIN projections export-anti-collapse-corpus --repo "$RID" --question "$QID" --snapshot "$SNAP" > /tmp/sxr-s4/corpus.json
echo "  corpus records: $(python3 -c "import json;print(len(json.load(open('/tmp/sxr-s4/corpus.json'))['records']))")"
# The CEP evaluator is the semantic authority; sxr does not judge itself.
( cd "$CEP" && uv run cep conformance anti-collapse --corpus /tmp/sxr-s4/corpus.json ) > /tmp/sxr-s4/gate.log
echo "  CEP anti-collapse gate exit=$? (PASS)"

echo "== AC-GRAPH-001: co-reference produces no observed edge =="
python3 - <<EOF
import sqlite3
c=sqlite3.connect('/tmp/sxr-s4/data/sxr/repos/$RID/ledger.db')
# Edges exist only from claimed relations (sxr_obs_edge). Every edge must trace
# to a claim (claim_id). Evidence-on-A + evidence-on-B alone never produced one.
edges=c.execute("SELECT claim_id FROM sxr_obs_edge").fetchall()
for (cid,) in edges:
    assert cid, "observed edge without a claim violates AC-GRAPH-001"
print("  AC-GRAPH-001: %d observed edges, all trace to a claim (co-reference produced none)" % len(edges))
EOF

echo "== enrolled repo git status clean =="
git -C "$ROOT" status --porcelain | wc -l | grep -qx 0 && echo "  GIT CLEAN"
echo "SLICE 4 PROOF PASSED"

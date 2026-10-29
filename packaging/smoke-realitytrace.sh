#!/bin/sh
# Functional smoke of an INSTALLED (packaged) realitytrace binary.
#
# Usage: packaging/smoke-realitytrace.sh <realitytrace-binary> <expected-version>
#
# Runs with a disposable HOME and XDG roots, so it never touches the calling
# user's data. It exercises evidence semantics, not just startup:
#
#   - exact --version, valid `doctor --json`, data root inside the fresh HOME
#   - repository enrollment + identity
#   - question/threshold declaration BEFORE observation
#   - two real verifier runs (command verifier): one supports, one contradicts
#   - evidence derivation; the passing run settles, the failing run does NOT
#   - ledger verification, meaningful committed evidence in the export
#   - tamper-evidence: mutating a committed payload must fail verification
#   - the enrolled repository is not modified (AC-SECURITY-001)
#   - a deleted ledger must not verify as an empty intact chain (last test)
#
# Every assertion is checked; the script exits non-zero on the first failure.
set -eu

if [ "$#" -ne 2 ]; then
    echo "usage: $0 <realitytrace-binary> <expected-version>" >&2
    exit 2
fi

RT=$(CDPATH= cd -- "$(dirname -- "$1")" && pwd)/$(basename -- "$1")
VERSION=$2

if [ ! -x "$RT" ]; then
    echo "error: not executable: $RT" >&2
    exit 1
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/rt-smoke.XXXXXX")
cleanup() { rm -rf "$work"; }
trap cleanup EXIT

export HOME="$work/home"
export XDG_DATA_HOME="$work/xdg-data"
export XDG_STATE_HOME="$work/xdg-state"
unset XDG_RUNTIME_DIR SXR_DAEMON_SOCKET REALITYTRACE_DOMAINFORGE_BIN SXR_DOMAINFORGE_BIN 2>/dev/null || true
mkdir -p "$HOME" "$XDG_DATA_HOME" "$XDG_STATE_HOME"

jq_py() { python3 -c "import json,sys; d=json.load(sys.stdin); $1"; }

pass() { echo "smoke: PASS $1"; }
fail() {
    echo "smoke: FAIL $1" >&2
    exit 1
}

echo "smoke: binary $RT"
echo "smoke: fresh HOME $HOME"

# ── 1. version ───────────────────────────────────────────────────────────
got=$("$RT" --version)
test "$got" = "realitytrace $VERSION" || fail "version: got '$got', want 'realitytrace $VERSION'"
pass "version -> $got"

# ── 2. doctor --json ─────────────────────────────────────────────────────
"$RT" doctor --json > "$work/doctor.json"
python3 - "$work/doctor.json" "$XDG_DATA_HOME" "$XDG_STATE_HOME" <<'PY' || fail "doctor --json is not valid diagnostic JSON"
import json, sys
d = json.load(open(sys.argv[1]))
assert isinstance(d, dict), d
# The roots must live inside the disposable XDG roots, never the real HOME.
assert d.get("data_root", "").startswith(sys.argv[2]), d.get("data_root")
assert d.get("state_root", "").startswith(sys.argv[3]), d.get("state_root")
for key in ("daemon_active", "filesystem_class", "enrolled_repos"):
    assert key in d, (key, sorted(d))
PY
pass "doctor --json valid, data_root/state_root under disposable roots"

# ── 3. no system SQLite / compiler dependency ───────────────────────────
if command -v ldd >/dev/null 2>&1; then
    if ldd "$RT" 2>/dev/null | grep -q libsqlite3; then
        fail "binary links the system libsqlite3 (must be bundled)"
    fi
    pass "no system libsqlite3 linkage"
fi
if command -v readelf >/dev/null 2>&1; then
    ! readelf -d "$RT" | grep -Eq 'RPATH|RUNPATH' || fail "binary carries an RPATH/RUNPATH"
    pass "no RPATH/RUNPATH"
fi

# ── 4. enroll a disposable git repository ───────────────────────────────
repo="$work/repo"
mkdir -p "$repo/src"
printf 'pub fn answer() -> u32 { 42 }\n' > "$repo/src/lib.rs"
printf '# smoke repo\n' > "$repo/README.md"
git -C "$repo" init -q
git -C "$repo" add -A
git -C "$repo" -c user.email=ci@example.invalid -c user.name=ci commit -qm init

"$RT" repo enroll "$repo" --json > "$work/enroll.json"
rid=$(jq_py 'print(d["repo_id"])' < "$work/enroll.json")
test -n "$rid" || fail "repo enroll returned no repo_id"
pass "enrolled $rid"

"$RT" repo identity "$rid" --json > "$work/identity.json"
python3 - "$work/identity.json" "$rid" <<'PY' || fail "repository identity is incomplete"
import json, sys
d = json.load(open(sys.argv[1]))["identity"]
assert d["repo_id"] == sys.argv[2], d
assert d["checkout_id"], d
assert d["identity_evidence"], d
PY
pass "repository identity verified"

# Repository content before any workflow (AC-SECURITY-001: sxr never writes
# inside the enrolled repository).
before=$(git -C "$repo" status --porcelain --ignored)
test -z "$before" || fail "repo already dirty before workflow: $before"

# ── 5. declare expectation before observation ───────────────────────────
cat > "$work/question.json" <<'EOF'
{"question_id":"q-smoke","question_form":"is_true","question_type":"verification","target_entities":["concept:smoke"],"expected_answer_shape":"boolean"}
EOF
cat > "$work/threshold.json" <<'EOF'
{"threshold_id":"t-smoke","minimum_relevance":"medium","minimum_reliability":"medium"}
EOF
"$RT" question declare --repo "$rid" --file "$work/question.json" --json > /dev/null
"$RT" threshold declare --repo "$rid" --question q-smoke --file "$work/threshold.json" --json > /dev/null
pass "question and threshold declared before any observation"

run_query() {
    # $1 = argv JSON for the command verifier; echoes the run id
    "$RT" run begin --repo "$rid" --snapshot sha256:smoke --question q-smoke \
        --threshold t-smoke --json > "$work/run.json"
    run_id=$(jq_py 'print(d["run_id"])' < "$work/run.json")
    test -n "$run_id" || fail "run begin returned no run_id"
    "$RT" plan create --run "$run_id" --verifier command --target concept:smoke \
        --argv-json "$1" --cwd . --timeout-s 60 --json > /dev/null
    "$RT" verify run --run "$run_id" --json > "$work/verify.json"
    printf '%s\n' "$run_id"
}

# The durable direction of an evidence record, read from the committed ledger
# payload — not from a transient CLI echo.
ledger_direction() {
    # $1 = evidence id
    "$RT" ledger export --repo "$rid" > "$work/export-check.json"
    python3 - "$work/export-check.json" "$1" <<'PY'
import json, sys
rows = json.load(open(sys.argv[1]))
want = sys.argv[2]
for row in rows:
    if row["record_kind"] != "evidence.created":
        continue
    for ev in row["payload"]["evidence"]:
        if ev["evidence_id"] == want:
            print(ev["direction"])
            raise SystemExit(0)
raise SystemExit("evidence %s not found in the ledger" % want)
PY
}

# ── 6. supporting observation -> settled ────────────────────────────────
run_ok=$(run_query '["sh","-c","exit 0"]')
exit_ok=$(jq_py 'print(d["observations"][0]["exit_code"])' < "$work/verify.json")
test "$exit_ok" = "0" || fail "supporting verifier observation exit_code=$exit_ok"
obs_ok=$(jq_py 'print(d["observations"][0]["observation_id"])' < "$work/verify.json")
"$RT" evidence create --run "$run_ok" --observation "$obs_ok" --derive --json > "$work/evidence-ok.json"
ev_ok=$(jq_py 'print(d["evidence_id"])' < "$work/evidence-ok.json")
test -n "$ev_ok" || fail "evidence create returned no evidence_id"
dir_ok=$(ledger_direction "$ev_ok") || fail "supporting evidence missing from the ledger"
test "$dir_ok" = "supports" || fail "supporting evidence direction is '$dir_ok', expected supports"
"$RT" settlement register --run "$run_ok" --question q-smoke --threshold t-smoke \
    --transformation transformation:smoke --json > "$work/settle-ok.json"
status_ok=$(jq_py 'print(d["settlement_status"])' < "$work/settle-ok.json")
test "$status_ok" = "settled" || fail "supporting run settled as '$status_ok', expected settled"
pass "supporting observation -> evidence supports -> settlement settled"

# ── 7. contradicting observation must NOT settle ────────────────────────
run_bad=$(run_query '["sh","-c","exit 1"]')
exit_bad=$(jq_py 'print(d["observations"][0]["exit_code"])' < "$work/verify.json")
test "$exit_bad" = "1" || fail "failing verifier observation exit_code=$exit_bad, expected 1"
obs_bad=$(jq_py 'print(d["observations"][0]["observation_id"])' < "$work/verify.json")
"$RT" evidence create --run "$run_bad" --observation "$obs_bad" --derive --json > "$work/evidence-bad.json"
ev_bad=$(jq_py 'print(d["evidence_id"])' < "$work/evidence-bad.json")
test -n "$ev_bad" || fail "evidence create returned no evidence_id for the failing observation"
dir_bad=$(ledger_direction "$ev_bad") || fail "failing evidence missing from the ledger"
test "$dir_bad" = "contradicts" || fail "failing evidence direction is '$dir_bad', expected contradicts"
"$RT" settlement register --run "$run_bad" --question q-smoke --threshold t-smoke \
    --transformation transformation:smoke --json > "$work/settle-bad.json"
status_bad=$(jq_py 'print(d["settlement_status"])' < "$work/settle-bad.json")
test "$status_bad" = "unsettled" || fail "failing run settled as '$status_bad' — a failure must never be reported as success"
contradictions=$(jq_py 'print(len(d["unresolved_contradictions"]))' < "$work/settle-bad.json")
test "$contradictions" -ge 1 || fail "failing run has no unresolved contradiction"
pass "failing observation -> evidence contradicts -> settlement unsettled (not success)"

# ── 8. ledger integrity and meaningful committed evidence ───────────────
"$RT" ledger verify --repo "$rid" --json > "$work/ledger.json"
verified=$(jq_py 'print(d["verified_count"])' < "$work/ledger.json")
ok=$(jq_py 'print(d["ok"])' < "$work/ledger.json")
test "$ok" = "True" || fail "ledger verify ok=$ok"
test "$verified" -ge 8 || fail "verified_count=$verified, expected >= 8"
"$RT" ledger export --repo "$rid" > "$work/export.json"
python3 - "$work/export.json" <<'PY' || fail "ledger export lacks the required committed evidence chain"
import json, sys
rows = json.load(open(sys.argv[1]))
kinds = {r["record_kind"] for r in rows}
for want in ("observation.recorded", "evidence.created", "settlement.registered"):
    assert want in kinds, (want, sorted(kinds))
assert len(rows) >= 8, len(rows)
PY
pass "ledger verified ($verified entries) with observation/evidence/settlement records"

# ── 9. tamper evidence: mutate a committed payload ─────────────────────
ledger_dir="$XDG_DATA_HOME/sxr/repos/$rid"
db=$(ls "$ledger_dir"/ledger.db 2>/dev/null | head -1)
test -n "$db" || fail "no ledger.db under $ledger_dir"
python3 - "$db" <<'PY'
import sqlite3, sys
con = sqlite3.connect(sys.argv[1])
con.execute("UPDATE ledger SET payload = ? WHERE append_ordinal = (SELECT MAX(append_ordinal) FROM ledger)",
            ('{"tampered":true}',))
con.commit()
con.close()
PY
if "$RT" ledger verify --repo "$rid" --json > "$work/tampered.json" 2>&1; then
    fail "a tampered committed payload verified as intact"
fi
pass "tampered payload rejected by ledger verify"

# ── 10. enrolled repository untouched ──────────────────────────────────
after=$(git -C "$repo" status --porcelain --ignored)
test "$after" = "$before" || fail "workflow modified the enrolled repository: $after"
pass "enrolled repository unmodified"

# ── 11. missing ledger must not verify as an empty intact chain (last) ──
rm -f "$ledger_dir"/ledger.db*
if "$RT" ledger verify --repo "$rid" --json > /dev/null 2>&1; then
    fail "a deleted ledger verified as an empty, intact chain"
fi
pass "missing ledger fails verification"

echo "smoke: ALL CHECKS PASSED"
